use std::{fmt, future::Future};

use bitcode::{Decode, Encode};
use qb_core::{
    interface::{QBIBridgeMessage, QBIHostMessage, QBIId},
    QB,
};

// re-export qbis
pub use qbi_local;

use qb_core::common::id::QBId;
use serde::{Deserialize, Serialize};

#[derive(Encode, Decode, Serialize, Deserialize)]
pub enum QBControlRequest {
    Start {
        id: QBIId,
    },
    Stop {
        id: QBIId,
    },
    /// Talk to the QBI
    Bridge {
        id: QBIId,
        msg: Vec<u8>,
    },
}

impl fmt::Display for QBControlRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QBControlRequest::Start { id, .. } => {
                write!(f, "MSG_CONTROL_REQ_START {}", id)
            }
            QBControlRequest::Stop { id } => {
                write!(f, "MSG_CONTROL_REQ_STOP {}", id)
            }
            QBControlRequest::Bridge { id, msg } => {
                write!(
                    f,
                    "MSG_CONTROL_REQ_BRIDGE {}: {}",
                    id,
                    simdutf8::basic::from_utf8(msg).unwrap_or("binary data")
                )
            }
        }
    }
}

pub trait ProcessQBControlRequest {
    fn process(&mut self, caller: QBId, request: QBControlRequest) -> impl Future<Output = ()>;
}

impl ProcessQBControlRequest for QB {
    fn process(&mut self, caller: QBId, request: QBControlRequest) -> impl Future<Output = ()> {
        request.process_to(self, caller)
    }
}

#[derive(Encode, Decode, Serialize, Deserialize)]
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
            QBControlRequest::Start { id: _id } => {
                // init.attach_to(qb, id).await;
                todo!()
            }
            QBControlRequest::Stop { id } => {
                qb.detach(&id).await.unwrap().await.unwrap();
            }
            QBControlRequest::Bridge { id, msg } => {
                qb.send(
                    &id,
                    QBIHostMessage::Bridge(QBIBridgeMessage { caller, msg }),
                )
                .await;
            }
        }
    }
}
