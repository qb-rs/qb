use std::{fmt, future::Future};

use bitcode::{Decode, Encode};
use msg::QBControlRequest;
use qb_core::{
    common::id::QBId,
    interface::protocol::{BridgeMessage, Message},
    QB,
};

// re-export qbis
pub use qbi_local;
pub mod msg;

pub trait ProcessQBControlRequest {
    fn process(&mut self, caller: QBId, request: QBControlRequest) -> impl Future<Output = ()>;
}

impl ProcessQBControlRequest for QB {
    fn process(&mut self, caller: QBId, request: QBControlRequest) -> impl Future<Output = ()> {
        request.process_to(self, caller)
    }
}

#[derive(Encode, Decode)]
pub enum QBControlResponse {
    // TODO: attach/detach success/error
    Bridge { msg: Vec<u8> },
}

impl fmt::Display for QBControlResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QBControlResponse::Bridge { msg } => {
                write!(
                    f,
                    "MSG_CONTROL_RESP_BRIDGE: {}",
                    simdutf8::basic::from_utf8(msg).unwrap_or("binary data")
                )
            }
        }
    }
}

impl QBControlRequest {
    pub async fn process_to(self, qb: &mut QB, caller: QBId) {
        // TODO: error handling
        match self {
            QBControlRequest::Start { id } => {
                // init.attach_to(qb, id).await;
                todo!()
            }
            QBControlRequest::Stop { id } => {
                qb.detach(&id).await.unwrap().join().unwrap();
            }
            QBControlRequest::Bridge { id, msg } => {
                qb.send(&id, Message::Bridge(BridgeMessage { caller, msg }))
                    .await;
            }
        }
    }
}
