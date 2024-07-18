//! This module contains stuff related to QBIs (quixbyte interfaces).
//! A QBI is a modular adaptor for communicating with different services
//! and allowing to synchronize onto many different platforms.

pub mod communication;
pub mod protocol;

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

use bitcode::{Decode, Encode};
use communication::QBICommunication;
use lazy_static::lazy_static;

use crate::{change::QB_CHANGELOG_BASE, common::hash::QBHash};

/// trait which all quixbyte interfaces need to implement
///
/// if you need async support take a look at QBIAsync from
/// the qb-derive crate as well.
pub trait QBI<T> {
    /// Initialize this QBI.
    fn init(cx: T, com: QBICommunication) -> Self;
    /// main loop
    fn run(self);
}

/// struct which represents an id from a specific QBI connection
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
    /// the default id
    pub static ref QBID_DEFAULT: QBID = QBID::default();
}

/// struct that stores common changes and names for all connections
#[derive(Encode, Decode, Debug, Clone, Default)]
pub struct QBDevices {
    commons: HashMap<QBID, QBHash>,
    names: HashMap<QBID, String>,
}

impl QBDevices {
    /// Get the common hash of the connection with the id.
    pub fn get_common(&self, id: &QBID) -> &QBHash {
        self.commons.get(id).unwrap_or(QB_CHANGELOG_BASE.hash())
    }

    /// Set the common hash of the connection with the id.
    pub fn set_common(&mut self, id: &QBID, hash: QBHash) {
        self.commons.insert(id.clone(), hash);
    }

    /// Get the name of the connection with the id.
    pub fn get_name(&self, id: &QBID) -> &str {
        self.names.get(id).map(|a| a.as_str()).unwrap_or("untitled")
    }

    /// Set the name of the connection with the id.
    pub fn set_name(&mut self, id: &QBID, name: String) {
        self.names.insert(id.clone(), name);
    }
}
