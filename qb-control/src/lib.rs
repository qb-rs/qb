use std::future::Future;

use bitcode::{Decode, Encode};
use qb_core::{common::id::QBID, interface::QBI, QB};
use qbi_local::{QBILocal, QBILocalInit};

// re-export qbis
pub use qbi_local;

pub trait ProcessQBControlRequest {
    fn process(
        &mut self,
        request: QBControlRequest,
    ) -> impl Future<Output = Option<QBControlResponse>>;
}

impl ProcessQBControlRequest for QB {
    fn process(
        &mut self,
        request: QBControlRequest,
    ) -> impl Future<Output = Option<QBControlResponse>> {
        request.process_to(self)
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

#[derive(Encode, Decode)]
pub enum QBControlResponse {
    // TODO: attach/detach success/error
    Bridge { id: QBID, msg: Vec<u8> },
}

impl QBControlRequest {
    pub async fn process_to(self, qb: &mut QB) -> Option<QBControlResponse> {
        // TODO: error handling
        match self {
            QBControlRequest::Attach { init, id } => {
                init.attach_to(qb, id).await;
                None
            }
            QBControlRequest::Detach { id } => {
                qb.detach(&id).await.unwrap().join().unwrap();
                None
            }
            QBControlRequest::Bridge { id, msg } => Some(QBControlResponse::Bridge {
                msg: qb.bridge(&id, msg).await.unwrap(),
                id,
            }),
        }
    }
}
