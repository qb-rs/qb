use core::panic;
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bitcode::{Decode, Encode};
use notify::{
    event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind, RenameMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use qb_core::{
    change::{log::QBChangelog, transaction::QBTransaction, QBChange, QBChangeKind},
    common::device::QBDeviceId,
    fs::{QBFileDiff, QBFS},
    interface::{
        Message, QBIBridgeMessage, QBICommunication, QBIContext, QBIHostMessage, QBIId, QBISetup,
    },
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Encode, Decode, Serialize, Deserialize)]
pub struct QBILocal {
    pub path: String,
}

impl QBIContext for QBILocal {
    async fn run(self, host_id: QBDeviceId, com: QBICommunication) {
        Runner::init_async(self, host_id, com)
            .await
            .run_async()
            .await;
    }
}

impl<'a> QBISetup<'a> for QBILocal {
    async fn setup(self) -> QBIId {
        QBIId::generate(QBDeviceId::generate())
    }
}

pub struct Runner {
    com: QBICommunication,
    fs: QBFS,
    transaction: QBTransaction,
    syncing: bool,
    watcher_skip: Vec<PathBuf>,
    host_id: QBDeviceId,
}

impl Runner {
    async fn init_async(cx: QBILocal, host_id: QBDeviceId, com: QBICommunication) -> Self {
        let fs = QBFS::init(cx.path).await;

        com.send(Message::Common {
            common: fs.devices.get_common(&host_id).clone(),
        })
        .await;

        Self {
            syncing: false,
            watcher_skip: Vec::new(),
            transaction: Default::default(),
            host_id,
            fs,
            com,
        }
    }

    async fn on_message(&mut self, msg: Message) {
        info!("recv {}", msg);

        match msg {
            Message::Common { common } => {
                self.fs.devices.set_common(&self.host_id, common);
                self.fs.save_devices().await.unwrap();
            }
            Message::Sync { common, changes } => {
                assert!(self.fs.devices.get_common(&self.host_id).clone() == common);

                let local_entries = self.fs.changelog.after(&common).unwrap();

                // Apply changes
                let (mut entries, fschanges) =
                    QBChangelog::merge(local_entries.clone(), changes).unwrap();
                self.watcher_skip.append(
                    &mut fschanges
                        .iter()
                        .map(|e| self.fs.wrapper.fspath(&e.resource))
                        .collect(),
                );

                self.fs.changelog.append(&mut entries);

                let fschanges = self.fs.table.to_fschanges(fschanges);
                self.fs.apply_changes(fschanges).await.unwrap();

                let new_common = self.fs.changelog.head();
                self.fs.devices.set_common(&self.host_id, new_common);

                // Send sync to remote
                if !self.syncing {
                    self.com
                        .send(Message::Sync {
                            common,
                            changes: local_entries,
                        })
                        .await;
                }

                self.syncing = false;

                // save the changes applied
                self.fs.save().await.unwrap();
            }
            Message::Broadcast { msg } => println!("BROADCAST: {}", msg),
        }
    }

    // TODO: filter events caused by apply
    async fn on_watcher(&mut self, event: Event) {
        debug!("event {:?}", event);
        let fspath = &event.paths[0];
        let path = self.fs.wrapper.parse(fspath).unwrap();
        let resource = match event.kind {
            EventKind::Remove(RemoveKind::Folder) | EventKind::Create(CreateKind::Folder) => {
                path.dir()
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                match self.fs.tree.get(&path).map(|e| e.is_dir()) {
                    Some(true) => path.dir(),
                    Some(false) => path.file(),
                    None => return,
                }
            }
            EventKind::Create(CreateKind::File)
            | EventKind::Remove(RemoveKind::File)
            | EventKind::Access(AccessKind::Close(AccessMode::Write)) => path.file(),
            _ => return,
        };

        // skip ignored files
        if !self.fs.ignore.matched(&resource).is_none() {
            return;
        }

        if self.watcher_skip.iter().any(|e| e == fspath) {
            debug!("skip {:?}", resource);
            return;
        }

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let change = match event.kind {
            EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                let kind = self.fs.diff(&resource).await.unwrap();
                match kind {
                    Some(QBFileDiff::Text(diff)) => {
                        QBChange::new(ts, QBChangeKind::UpdateText { diff }, resource)
                    }
                    Some(QBFileDiff::Binary(contents)) => {
                        QBChange::new(ts, QBChangeKind::UpdateBinary { contents }, resource)
                    }
                    None => return,
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) | EventKind::Remove(..) => {
                QBChange::new(ts, QBChangeKind::Delete, resource)
            }
            EventKind::Create(..) => QBChange::new(ts, QBChangeKind::Create, resource),
            _ => panic!("this should not happen"),
        };

        // tree needs to be updated continously
        let fschange = self.fs.table.to_fschange(change.clone());
        self.fs.tree.notify_change(&fschange);

        self.transaction.push(change);
    }

    fn should_sync(&mut self) -> bool {
        !self.syncing && !self.transaction.complete().is_empty()
    }

    async fn sync(&mut self) {
        // TODO: minify entries vector
        info!("syncing");
        self.syncing = true;

        // Complete transaction
        let mut changes = std::mem::take(&mut self.transaction).complete_into();
        // TODO: embed this directly
        let fschanges = self.fs.table.to_fschanges(changes.clone());
        self.fs.notify_changes(fschanges.iter());
        self.fs.changelog.append(&mut changes.clone());

        // save the changes applied
        self.fs.save().await.unwrap();

        // notify remote
        self.com
            .send(Message::Sync {
                common: self.fs.devices.get_common(&self.host_id).clone(),
                changes: std::mem::take(&mut changes),
            })
            .await;
    }

    async fn run_async(mut self) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            tx.blocking_send(res).unwrap();
        })
        .unwrap();

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        watcher
            .watch(&self.fs.wrapper.root, RecursiveMode::Recursive)
            .unwrap();

        loop {
            //let msg = self.com.rx.recv().await.unwrap();
            tokio::select! {
                Some(msg) = self.com.rx.recv() => {
                    match msg {
                        QBIHostMessage::Message(msg) => self.on_message(msg).await,
                        QBIHostMessage::Bridge(QBIBridgeMessage { caller, .. }) =>
                            self.com
                                .send(QBIBridgeMessage {
                                    caller,
                                    msg: "unimplemented".into(),
                                })
                                .await,
                        QBIHostMessage::Stop => break
                    }
                },
                Some(Ok(event)) = rx.recv() => {
                    self.on_watcher(event).await;
                },
                _ = tokio::time::sleep(Duration::from_secs(1)), if self.should_sync() => {
                    self.sync().await;
                },
                _ = tokio::time::sleep(Duration::from_secs(1)), if !self.watcher_skip.is_empty() => {
                    self.watcher_skip.clear();
                },
            };
        }
    }
}
