use std::future::Future;

use bitcode::{Decode, Encode};
use qb_core::{common::id::QBID, interface::QBI, QB};
use qbi_local::{QBILocal, QBILocalInit};

// re-export the initialzation primitives
pub use qbi_local;

pub trait ProcessQBControlRequest {
    fn process(&mut self, request: QBControlRequest) -> impl Future<Output = ()>;
}

impl ProcessQBControlRequest for QB {
    fn process(&mut self, request: QBControlRequest) -> impl Future<Output = ()> {
        request.process_to(self)
    }
}

/// The initialzation struct for a QBI
#[derive(Encode, Decode)]
pub enum QBIInit {
    Local(QBILocalInit),
}

/// An attach request
#[derive(Encode, Decode)]
pub struct QBIAttach {
    id: QBID,
    init: QBIInit,
}

impl Into<QBControlRequest> for QBIAttach {
    fn into(self) -> QBControlRequest {
        QBControlRequest::Attach(self)
    }
}

/// A detach request
#[derive(Encode, Decode)]
pub struct QBIDetach {
    id: QBID,
}

impl Into<QBControlRequest> for QBIDetach {
    fn into(self) -> QBControlRequest {
        QBControlRequest::Detach(self)
    }
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
    Attach(QBIAttach),
    Detach(QBIDetach),
}

impl QBControlRequest {
    pub async fn process_to(self, qb: &mut QB) {
        match self {
            QBControlRequest::Attach(attach) => attach.init.attach_to(qb, attach.id).await,
            QBControlRequest::Detach(detach) => _ = qb.detach(&detach.id),
        }
    }
}
