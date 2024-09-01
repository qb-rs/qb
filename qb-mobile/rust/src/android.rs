use core::panic;
use std::time::Duration;

use bitcode::{Decode, Encode};
use qb_core::{
    change::{QBChange, QBChangeKind},
    device::QBDeviceId,
    fs::{QBFileDiff, QBFS},
    path::{qbpaths::INTERNAL, QBResource},
    time::QBTimeStampRecorder,
};
use qb_ext::{
    interface::{QBIChannel, QBIContext, QBIHostMessage, QBIMessage},
    QBExtSetup,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

#[derive(Encode, Decode, Serialize, Deserialize)]
pub struct QBIAndroid {
    pub path: String,
}

impl QBIContext for QBIAndroid {
    async fn run(self, host_id: QBDeviceId, com: QBIChannel) {
        Runner::init(self, host_id, com).await.run().await;
    }
}

impl QBExtSetup<QBIAndroid> for QBIAndroid {
    async fn setup(self) -> Self {
        info!("PATH: {}", self.path);
        let mut fs = QBFS::init(self.path.clone()).await;
        fs.devices.host_id = QBDeviceId::generate();
        fs.save().await.unwrap();
        self
    }
}

struct Runner {
    com: QBIChannel,
    fs: QBFS,
    syncing: bool,
    host_id: QBDeviceId,
    recorder: QBTimeStampRecorder,
}

impl Runner {
    async fn init(cx: QBIAndroid, host_id: QBDeviceId, com: QBIChannel) -> Self {
        let fs = QBFS::init(cx.path).await;

        com.send(QBIMessage::Device {
            device_id: fs.devices.host_id.clone(),
        })
        .await;
        com.send(QBIMessage::Common {
            common: fs.devices.get_common(&host_id).clone(),
        })
        .await;

        let recorder = QBTimeStampRecorder::from_device_id(fs.devices.host_id.clone());

        Self {
            syncing: false,
            recorder,
            host_id,
            fs,
            com,
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
                self.fs.apply_changes(fschanges).await.unwrap();

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

    async fn on_notification(&mut self, notification: NotifyAndroid) {
        let resource = notification.resource;
        let path = &resource.path;

        // skip internal files
        if INTERNAL.is_parent(&path) {
            return;
        }

        // skip ignored files
        if !self.fs.ignore.matched(&resource).is_none() {
            return;
        }

        let change = match notification.kind {
            NotifyKind::Write => {
                info!("KIND: {:?}", self.fs.wrapper.fspath(&resource));
                let kind = self.fs.diff(&resource).await;
                let kind = kind.unwrap();
                match kind {
                    Some(QBFileDiff::Text(diff)) => {
                        QBChange::new(self.recorder.record(), QBChangeKind::UpdateText(diff))
                    }
                    Some(QBFileDiff::Binary(contents)) => {
                        QBChange::new(self.recorder.record(), QBChangeKind::UpdateBinary(contents))
                    }
                    None => return,
                }
            }
        };

        self.fs.changemap.push((resource, change));
        info!("CHANGE ADDED: should_sync = {}", self.should_sync());
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                Some(msg) = self.com.recv() => {
                    match msg {
                        QBIHostMessage::Message(msg) => self.on_message(msg).await,
                        QBIHostMessage::Stop => {
                            info!("stopping...");
                            break
                        }
                        QBIHostMessage::Bridge(data) => {
                            info!("BRIDGE RECEIVED");
                            let notification = serde_json::from_slice::<NotifyAndroid>(&data).unwrap();
                            info!("notif: {notification:?}");
                            self.on_notification(notification).await;
                        }
                        _ => unimplemented!("unknown message: {msg:?}"),
                    }
                },
                _ = tokio::time::sleep(Duration::from_secs(3)), if self.should_sync() => {
                    self.sync().await;
                },
            };
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NotifyAndroid {
    kind: NotifyKind,
    resource: QBResource,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
enum NotifyKind {
    Write,
}
