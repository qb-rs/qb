use std::{fmt, future::Future};

use bitcode::{Decode, Encode};
use qb_core::{
    common::id::QBID,
    interface::{
        protocol::{BridgeMessage, Message},
        QBI,
    },
    QB,
};
use qbi_local::{QBILocal, QBILocalInit};

// re-export qbis
pub use qbi_local;

pub trait ProcessQBControlRequest {
    fn process(&mut self, caller: QBID, request: QBControlRequest) -> impl Future<Output = ()>;
}

impl ProcessQBControlRequest for QB {
    fn process(&mut self, caller: QBID, request: QBControlRequest) -> impl Future<Output = ()> {
        request.process_to(self, caller)
    }
}

/// The initialzation struct for a QBI
#[derive(Encode, Decode)]
pub enum QBIInit {
    Local(QBILocalInit),
}

impl QBIInit {
    /// Attach this interface to the master.
    pub async fn attach_to(self, qb: &mut QB, id: impl Into<QBID>) {
        match self {
            QBIInit::Local(cx) => qb.attach(id, QBILocal::init, cx).await,
        }
    }
}

#[derive(Encode, Decode)]
pub enum QBControlRequest {
    Attach {
        id: QBID,
        init: QBIInit,
    },
    Detach {
        id: QBID,
    },
    /// Talk to the QBI
    Bridge {
        id: QBID,
        msg: Vec<u8>,
    },
}

impl fmt::Display for QBControlRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QBControlRequest::Attach { id, .. } => {
                write!(f, "MSG_CONTROL_REQ_ATTACH {}", id)
            }
            QBControlRequest::Detach { id } => {
                write!(f, "MSG_CONTROL_REQ_DETACH {}", id)
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
    pub async fn process_to(self, qb: &mut QB, caller: QBID) {
        // TODO: error handling
        match self {
            QBControlRequest::Attach { init, id } => {
                init.attach_to(qb, id).await;
            }
            QBControlRequest::Detach { id } => {
                qb.detach(&id).await.unwrap().join().unwrap();
            }
            QBControlRequest::Bridge { id, msg } => {
                qb.send(&id, Message::Bridge(BridgeMessage { caller, msg }))
                    .await;
            }
        }
    }
}
