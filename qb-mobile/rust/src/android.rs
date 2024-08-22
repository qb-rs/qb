use core::panic;

use bitcode::{Decode, Encode};
use qb_core::{device::QBDeviceId, fs::QBFS};
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

        Self {
            syncing: false,
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
                self.fs.changemap.append(changemap);
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
                        _ => unimplemented!(),
                    }
                },
            };
        }
    }
}
