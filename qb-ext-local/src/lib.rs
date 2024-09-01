use core::panic;
use std::{collections::HashMap, path::PathBuf, time::Duration};

use bitcode::{Decode, Encode};
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use qb_core::{
    change::{QBChange, QBChangeKind},
    device::QBDeviceId,
    fs::{QBFileDiff, QBFS},
    path::{qbpaths::INTERNAL, QBPath, QBResource},
    time::QBTimeStampRecorder,
};
use qb_ext::{
    interface::{QBIChannel, QBIContext, QBIHostMessage, QBIMessage},
    QBExtSetup,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

pub type QBILocalSetup = QBILocal;
#[derive(Encode, Decode, Serialize, Deserialize)]
pub struct QBILocal {
    pub path: String,
}

impl QBIContext for QBILocal {
    async fn run(self, host_id: QBDeviceId, com: QBIChannel) {
        Runner::init(self, host_id, com).await.run().await;
    }
}

impl QBExtSetup<QBILocal> for QBILocalSetup {
    async fn setup(self) -> QBILocal {
        let mut fs = QBFS::init(self.path.clone()).await;
        fs.devices.host_id = QBDeviceId::generate();
        fs.save().await.unwrap();
        self
    }
}

pub struct Runner {
    com: QBIChannel,
    fs: QBFS,
    syncing: bool,
    watcher_skip: Vec<PathBuf>,
    host_id: QBDeviceId,
    recorder: QBTimeStampRecorder,
    trackers: HashMap<usize, QBPath>,
}

impl Runner {
    async fn init(cx: QBILocal, host_id: QBDeviceId, com: QBIChannel) -> Self {
        let fs = QBFS::init(cx.path).await;

        com.send(QBIMessage::Device {
            device_id: fs.devices.host_id.clone(),
        })
        .await;
        com.send(QBIMessage::Common {
            common: fs.devices.get_common(&host_id).clone(),
        })
        .await;

        let recorder = QBTimeStampRecorder::from(fs.devices.host_id.clone());

        Self {
            syncing: false,
            watcher_skip: Vec::new(),
            trackers: Default::default(),
            host_id,
            fs,
            com,
            recorder,
        }
    }

    async fn on_message(&mut self, msg: QBIMessage) {
        debug!("recv {}", msg);

        match msg {
            QBIMessage::Common { common } => {
                self.fs.devices.set_common(&self.host_id, common);
                self.fs.save_devices().await.unwrap();
            }
            QBIMessage::Sync {
                common,
                changes: remote,
            } => {
                assert!(self.fs.devices.get_common(&self.host_id).clone() == common);

                let local = self.fs.changemap.since(&common);

                // Apply changes
                let mut changemap = local.clone();
                let changes = changemap.merge(remote).unwrap();
                self.fs.changemap.append_map(changemap);
                let fschanges = self.fs.to_fschanges(changes);
                self.watcher_skip.append(
                    &mut fschanges
                        .iter()
                        .map(|e| self.fs.wrapper.fspath(&e.resource))
                        .collect(),
                );
                self.fs.apply_changes(fschanges).await.unwrap();

                // TODO: implement conversion code
                //let fschanges = self.fs.table.to_fschanges(fschanges);
                //self.fs.apply_changes(fschanges).await.unwrap();

                let new_common = self.fs.changemap.head().clone();
                self.fs.devices.set_common(&self.host_id, new_common);

                // Send sync to remote
                if !self.syncing {
                    self.com
                        .send(QBIMessage::Sync {
                            common,
                            changes: local,
                        })
                        .await;
                }

                self.syncing = false;

                // save the changes applied
                self.fs.save().await.unwrap();
            }
            QBIMessage::Broadcast { msg } => debug!("BROADCAST: {}", msg),
            val => warn!("unexpected message: {}", val),
        }
    }

    // TODO: filter events caused by apply
    async fn on_watcher(&mut self, event: Event) {
        let fspath = &event.paths[0];
        let path = self.fs.wrapper.parse(fspath).unwrap();

        // skip internal files
        if INTERNAL.is_parent(&path) {
            return;
        }

        debug!("event {:?}", event);
        let resource = match event.kind {
            EventKind::Remove(RemoveKind::Folder) | EventKind::Create(CreateKind::Folder) => {
                path.dir()
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                self.trackers.insert(event.tracker().unwrap(), path);
                return;
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                self.fs.wrapper.to_resource(path).await.unwrap()
            }
            EventKind::Create(CreateKind::File)
            | EventKind::Remove(RemoveKind::File)
            | EventKind::Modify(ModifyKind::Data(_)) => path.file(),
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

        let entries = match event.kind {
            EventKind::Modify(ModifyKind::Data(_)) => {
                let kind = self.fs.diff(&resource).await.unwrap();
                match kind {
                    Some(QBFileDiff::Text(diff)) => {
                        vec![(
                            resource,
                            QBChange::new(self.recorder.record(), QBChangeKind::UpdateText(diff)),
                        )]
                    }
                    Some(QBFileDiff::Binary(contents)) => {
                        vec![(
                            resource,
                            QBChange::new(
                                self.recorder.record(),
                                QBChangeKind::UpdateBinary(contents),
                            ),
                        )]
                    }
                    None => return,
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                let ts = self.recorder.record();
                let previouspath = self.trackers.remove(&event.tracker().unwrap()).unwrap();
                vec![
                    (
                        QBResource::new(previouspath, resource.kind.clone()),
                        QBChange::new(ts.clone(), QBChangeKind::RenameFrom),
                    ),
                    (resource, QBChange::new(ts, QBChangeKind::RenameTo)),
                ]
            }
            EventKind::Remove(..) => {
                info!("DELETE {}", resource);
                vec![(
                    resource,
                    QBChange::new(self.recorder.record(), QBChangeKind::Delete),
                )]
            }
            EventKind::Create(..) => vec![(
                resource,
                QBChange::new(self.recorder.record(), QBChangeKind::Create),
            )],
            _ => panic!("this should not happen"),
        };

        let fschanges = self.fs.to_fschanges(entries.clone());
        self.fs.tree.notify_changes(fschanges.iter());
        self.fs.changemap.append(entries);
    }

    fn should_sync(&mut self) -> bool {
        !self.syncing && self.fs.changemap.head() != self.fs.devices.get_common(&self.host_id)
    }

    async fn sync(&mut self) {
        // TODO: minify entries vector
        info!("syncing");
        self.syncing = true;

        // Complete transaction
        let common = self.fs.devices.get_common(&self.host_id).clone();
        let mut changes = self.fs.changemap.since_cloned(&common);
        changes.minify();

        // save the changes applied
        self.fs.save().await.unwrap();

        // notify remote
        self.com.send(QBIMessage::Sync { common, changes }).await;
    }

    async fn run(mut self) {
        let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::channel(10);
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            watcher_tx.blocking_send(res).unwrap();
        })
        .unwrap();

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        watcher
            .watch(&self.fs.wrapper.root, RecursiveMode::Recursive)
            .unwrap();

        loop {
            tokio::select! {
                Some(msg) = self.com.recv() => {
                    match msg {
                        QBIHostMessage::Message(msg) => self.on_message(msg).await,
                        QBIHostMessage::Stop => {
                            info!("stopping...");
                            break
                        }
                        _ => unimplemented!("unknown message: {msg:?}"),
                    }
                },
                Some(Ok(event)) = watcher_rx.recv() => {
                    self.on_watcher(event).await;
                },
                _ = tokio::time::sleep(Duration::from_secs(3)), if self.should_sync() => {
                    self.sync().await;
                },
                _ = tokio::time::sleep(Duration::from_secs(1)), if !self.watcher_skip.is_empty() => {
                    self.watcher_skip.clear();
                },
            };
        }
    }
}
