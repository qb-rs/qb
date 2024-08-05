use std::collections::HashMap;

use bitcode::DecodeOwned;
use qb_core::{
    interface::{QBIContext, QBIId, QBISetup},
    QB,
};
use qb_proto::QBPBlob;

pub type StartFn = Box<dyn Fn(&mut QB, QBIId, &[u8])>;
pub type SetupFn = Box<dyn Fn(&mut QBDaemon, QBPBlob)>;

pub struct QBIDescriptior {
    name: String,
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
        let start = self.start_fns.get(&descriptor.name).unwrap();
        start(&mut self.qb, id, &descriptor.data);
    }

    /// Register a QBI kind.
    pub fn register<T>(&mut self, name: String)
    where
        for<'a> T: QBIContext + QBISetup<'a> + DecodeOwned,
    {
        self.start_fns.insert(
            name.clone(),
            Box::new(|qb, id, data| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                runtime.block_on(qb.attach(id, bitcode::decode::<T>(data).unwrap()));
            }),
        );
        let name_clone = name.clone();
        self.setup_fns.insert(
            name,
            Box::new(move |daemon, blob| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                let cx = blob.deserialize::<T>().unwrap();
                let data = bitcode::encode(&cx);
                let id = runtime.block_on(cx.setup());
                daemon.qbis.insert(
                    id,
                    QBIDescriptior {
                        name: name_clone.clone(),
                        data,
                    },
                );
            }),
        );
    }

    /// Register the default QBI kinds.
    pub fn register_default(&mut self) {
        // self.register::<QBILocal>("local");
        // self.register::<QBIGDrive>("gdrive");
    }
}
