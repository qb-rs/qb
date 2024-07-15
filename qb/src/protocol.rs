use core::fmt;

use tokio::sync::mpsc;

use crate::{QBChange, QBHash};

/// Communication interface.
pub struct QBICommunication {
    pub tx: mpsc::Sender<QBIMessage>,
    pub rx: mpsc::Receiver<QBMessage>,
}

impl QBICommunication {
    // #[deprecated = "use-case unknown"]
    // pub async fn bridge_request_async<T>(&mut self, key: String) -> T
    // where
    //     T: 'static,
    // {
    //     self.tx
    //         .send(QBIMessage::BridgeRequest { key })
    //         .await
    //         .expect("Could not send request!");
    //     let resp = self.rx.recv().await.expect("Could not receive response!");
    //     match resp {
    //         HostMessage::BridgeRequest { val } => *val.downcast::<T>().expect("Type mismatch"),
    //         _ => panic!("Received response of wrong type!"),
    //     }
    // }

    // #[deprecated = "use-case unknown"]
    // pub fn bridge_request<T>(&mut self, key: String) -> T
    // where
    //     T: 'static,
    // {
    //     self.tx
    //         .blocking_send(QBIMessage::BridgeRequest { key })
    //         .expect("Could not send request!");
    //     let resp = self
    //         .rx
    //         .blocking_recv()
    //         .expect("Could not receive response!");
    //     match resp {
    //         HostMessage::BridgeRequest { val } => *val.downcast::<T>().expect("Type mismatch"),
    //         _ => panic!("Received response of wrong type!"),
    //     }
    // }
}

// TODO: figure out what to call this
#[derive(Debug, Clone)]
pub enum QBIMessage {
    // TODO: figure out which structs to send over
    // RTCConnectionOffer {},
    // RTCConnectionAnswer {},
    Broadcast {
        msg: String,
    },
    Common {
        common: QBHash,
    }, // When newest common entry gets updated
    Sync {
        common: QBHash,
        entries: Vec<QBChange>,
    },
    // SyncComplete is the same as Sync with empty entries
    //SyncComplete {
    //    common: QBHash,
    //},
    //BridgeRequest {
    //    key: String,
    //},
}

impl fmt::Display for QBIMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            QBIMessage::Sync { common, entries } => {
                writeln!(f, "MSG_SYNC common: {}", common)?;
                for entry in entries {
                    fmt::Display::fmt(entry, f)?;
                    writeln!(f)?;
                }
                Ok(())
            }
            QBIMessage::Common { common } => {
                write!(f, "MSG_COMMON {}", common)
            }
            QBIMessage::Broadcast { msg } => {
                write!(f, "MSG_BROADCAST {}", msg)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum QBMessage {
    // TODO: figure out which structs to send over
    // RTCConnectionOffer {},
    // RTCConnectionAnswer {},
    Broadcast {
        msg: String,
    },
    // Check if we even need this
    Common {
        common: QBHash,
    }, // When newest common entry gets updated
    Sync {
        common: QBHash,
        entries: Vec<QBChange>,
    },
    // SyncComplete is the same as Sync with empty entries
    //SyncComplete {
    //    common: QBHash,
    //},
    //BridgeRequest {
    //    val: Box<dyn Any + Send + Sync + 'static>,
    //},
}

impl fmt::Display for QBMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            QBMessage::Sync { common, entries } => {
                writeln!(f, "MSG_SYNC common: {}", common)?;
                for entry in entries {
                    fmt::Display::fmt(entry, f)?;
                    writeln!(f)?;
                }
                Ok(())
            }
            QBMessage::Common { common } => {
                write!(f, "MSG_COMMON {}", common)
            }
            QBMessage::Broadcast { msg } => {
                write!(f, "MSG_BROADCAST {}", msg)
            }
        }
    }
}
