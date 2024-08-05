use std::collections::HashMap;

use bitcode::DecodeOwned;
use qb_core::{
    interface::{QBIContext, QBIId},
    QB,
};
use qb_proto::QBPMessage;

pub struct SetupBlob {
    pub content_type: String,
    pub content: Vec<u8>,
}

impl SetupBlob {
    /// Deserialize this blob.
    ///
    /// This might throw an error if the content is malformed
    /// or the content type is not supported.
    pub fn deserialize<T>(&self) -> qb_proto::Result<T>
    where
        for<'a> T: QBPMessage<'a>,
    {
        match qb_proto::SUPPORTED_CONTENT_TYPES.get(&self.content_type) {
            Some(content_type) => content_type.from_bytes(&self.content),
            None => Err(qb_proto::Error::NegotiationFailed(format!(
                "{} not supported!",
                self.content_type
            ))),
        }
    }
}

pub type StartFn = Box<dyn Fn(&mut QB, QBIId, &[u8])>;
pub type SetupFn = Box<dyn Fn(SetupBlob) -> (QBIId, Vec<u8>)>;

pub struct QBDaemon {
    qbi_kinds: HashMap<QBIId, String>,
    start_fns: HashMap<String, StartFn>,
    setup_fns: HashMap<String, SetupFn>,
}

impl QBDaemon {
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
}
