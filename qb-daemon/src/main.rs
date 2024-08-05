use std::collections::HashMap;

use bitcode::DecodeOwned;
use qb_core::{
    interface::{QBIContext, QBIId},
    QB,
};
use qb_proto::QBPBlob;

pub type StartFn = Box<dyn Fn(&mut QB, QBIId, &[u8])>;
pub type SetupFn = Box<dyn Fn(QBPBlob) -> (QBIId, Vec<u8>)>;

pub struct QBIDescriptior {
    kind: String,
    data: Vec<u8>,
}

pub struct QBDaemon {
    qb: QB,
    qbis: HashMap<QBIId, QBIDescriptior>,
    start_fns: HashMap<String, StartFn>,
    setup_fns: HashMap<String, SetupFn>,
}

impl QBDaemon {
    /// Start a QBI by the given id.
    pub fn start(&mut self, id: QBIId) {
        let descriptor = self.qbis.get(&id).unwrap();
        let start = self.start_fns.get(&descriptor.kind).unwrap();
        start(&mut self.qb, id, &descriptor.data);
    }

    /// Register a QBI kind.
    pub fn register<T: QBIContext + DecodeOwned>(&mut self, name: String) {
        self.start_fns.insert(
            name,
            Box::new(|qb, id, data| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                runtime.block_on(qb.attach(id, bitcode::decode::<T>(data).unwrap()));
            }),
        );
    }

    /// Register the default QBI kinds.
    pub fn register_default(&mut self) {
        // self.register::<QBILocal>("local");
        // self.register::<QBIGDrive>("gdrive");
    }
}
