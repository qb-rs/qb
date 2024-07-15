use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

use bitcode::{Decode, Encode};
use lazy_static::lazy_static;

use crate::{QBHash, QBICommunication, QB_ENTRY_BASE};

pub trait QBI<T> {
    fn init(cx: T, com: QBICommunication) -> Self;
    fn run(self);
}

// TODO: common repository, id managing
#[derive(Encode, Decode, Debug, Clone, Default, Eq, PartialEq, Hash)]
pub struct QBID(pub(crate) u64);

impl From<&str> for QBID {
    fn from(value: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        QBID(hasher.finish())
    }
}

lazy_static! {
    pub static ref QBID_DEFAULT: QBID = QBID::default();
}

#[derive(Encode, Decode, Debug, Clone, Default)]
pub struct QBDevices {
    commons: HashMap<QBID, QBHash>,
    names: HashMap<QBID, String>,
}

impl QBDevices {
    pub fn get_common(&self, id: &QBID) -> &QBHash {
        self.commons.get(id).unwrap_or(QB_ENTRY_BASE.hash())
    }

    pub fn set_common(&mut self, id: &QBID, hash: QBHash) {
        self.commons.insert(id.clone(), hash);
    }

    pub fn get_name(&self, id: &QBID) -> &str {
        self.names.get(id).map(|a| a.as_str()).unwrap_or("untitled")
    }

    pub fn set_name(&mut self, id: &QBID, name: String) {
        self.names.insert(id.clone(), name);
    }
}
