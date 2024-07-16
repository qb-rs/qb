use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use notify::{
    event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind, RenameMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use qb::{
    change::{log::QBChangelog, transaction::QBTransaction, QBChange, QBChangeKind},
    common::resource::qbpaths,
    fs::{QBFileDiff, QBFS},
    interface::{
        communication::QBICommunication,
        protocol::{QBIMessage, QBMessage},
        QBID_DEFAULT,
    },
};
use qb_derive::QBIAsync;
use tracing::{debug, info};

pub struct QBILocalInit {
    pub path: PathBuf,
}

#[derive(QBIAsync)]
#[context(QBILocalInit)]
pub struct QBILocal {
    com: QBICommunication,
    fs: QBFS,
    transaction: QBTransaction,
    syncing: bool,
    watcher_skip: Vec<PathBuf>,
}

impl QBILocal {
    async fn init_async(cx: QBILocalInit, com: QBICommunication) -> Self {
        let fs = QBFS::init(cx.path).await;

        com.tx
            .send(QBIMessage::Common {
                common: fs.devices.get_common(&QBID_DEFAULT).clone(),
            })
            .await
            .unwrap();

        Self {
            syncing: false,
            watcher_skip: Vec::new(),
            transaction: Default::default(),
            fs,
            com,
        }
    }

    async fn on_remote(&mut self, msg: QBMessage) {
        info!("recv {}", msg);

        match msg {
            QBMessage::Common { common } => {
                self.fs.devices.set_common(&QBID_DEFAULT, common);
                self.fs.save_devices().await.unwrap();
            }
            QBMessage::Sync { common, changes } => {
                assert!(self.fs.devices.get_common(&QBID_DEFAULT).clone() == common);

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
                self.fs.devices.set_common(&QBID_DEFAULT, new_common);

                // Send sync to remote
                if !self.syncing {
                    self.com
                        .tx
                        .send(QBIMessage::Sync {
                            common,
                            changes: local_entries,
                        })
                        .await
                        .unwrap();
                }

                self.syncing = false;

                // save the changes applied
                self.fs.save().await.unwrap();
            }
            QBMessage::Broadcast { msg } => println!("BROADCAST: {}", msg),
        }
    }

    // TODO: filter events caused by apply
    async fn on_watcher(&mut self, event: Event) {
        debug!("event {:?}", event);
        for (i, fspath) in event.paths.iter().enumerate() {
            let path = self.fs.wrapper.parse(fspath.to_str().unwrap()).unwrap();

            // skip internal files
            if qbpaths::INTERNAL.is_parent(&path) {
                continue;
            }

            if self.watcher_skip.iter().find(|e| e == &fspath).is_some() {
                debug!("skip {:?}", path);
                continue;
            }

            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
                + i as u64;

            let entry = match event.kind {
                EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                    let kind = self.fs.diff(&path).await.unwrap();
                    match kind {
                        Some(QBFileDiff::Text(diff)) => {
                            QBChange::new(ts, QBChangeKind::Diff { diff }, path.file())
                        }
                        Some(QBFileDiff::Binary(contents)) => {
                            QBChange::new(ts, QBChangeKind::Change { contents }, path.file())
                        }
                        None => continue,
                    }
                }
                EventKind::Modify(ModifyKind::Name(RenameMode::From))
                | EventKind::Remove(RemoveKind::File) => {
                    QBChange::new(ts, QBChangeKind::Delete, path.file())
                }
                EventKind::Remove(RemoveKind::Folder) => {
                    QBChange::new(ts, QBChangeKind::Delete, path.dir())
                }
                EventKind::Create(CreateKind::File) => {
                    QBChange::new(ts, QBChangeKind::Create, path.file())
                }
                EventKind::Create(CreateKind::Folder) => {
                    QBChange::new(ts, QBChangeKind::Create, path.dir())
                }
                _ => continue,
            };

            // TODO: embed this directly
            let fschange = self.fs.table.to_fschange(entry.clone());
            self.fs.notify_change(fschange);
            self.transaction.push(entry);
        }
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
        self.fs.changelog.append(&mut changes.clone());

        // save the changes applied
        self.fs.save().await.unwrap();

        // notify remote
        self.com
            .tx
            .send(QBIMessage::Sync {
                common: self.fs.devices.get_common(&QBID_DEFAULT).clone(),
                changes: std::mem::take(&mut changes),
            })
            .await
            .unwrap();
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
                    self.on_remote(msg).await;
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
                // TODO: sync response timeout
            };
        }
    }
}
