//! This module contains helpers for the communication between
//! the QBI and the master.

use tokio::sync::mpsc;

use super::protocol::{QBIMessage, QBMessage};

/// struct describing the communication interface between QBI and master
pub struct QBICommunication {
    /// the transmission channel
    pub tx: mpsc::Sender<QBIMessage>,
    /// the receive channel
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
