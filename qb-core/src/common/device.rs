//! # What is a device?
//!
//! A device is a service which has the ability
//! to store files in some way.
//!
//! # What are device ids?
//!
//! Device ids are used to identify devices.
//!
//! ----
//!
//! QBIs can be attached to devices to provide
//! them with the ability to synchronize with other
//! devices. A QBI can be identified by a pair of
//! device id and QBI kind. It is currently under
//! examination whether QBIIds are actually sensible,
//! as you probably don't want to connect to a device twice.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::{fmt, hash::Hash};

use bitcode::{Decode, Encode};
use hex::FromHexError;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::change::QB_CHANGELOG_BASE;

use super::hash::QBHash;

/// A device identifier.
#[derive(Encode, Decode, Serialize, Deserialize, Default, Clone, Eq, PartialEq, Hash)]
pub struct QBDeviceId(pub(crate) u64);

impl From<&str> for QBDeviceId {
    fn from(value: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        QBDeviceId(hasher.finish())
    }
}

impl fmt::Display for QBDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for QBDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QBDeviceId({})", self.to_hex())
    }
}

impl AsRef<u64> for QBDeviceId {
    fn as_ref(&self) -> &u64 {
        &self.0
    }
}

impl QBDeviceId {
    /// Generate a new ID
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        QBDeviceId(rng.gen::<u64>())
    }

    /// Get the string representation of this id in hex format
    pub fn to_hex(&self) -> String {
        let id_bytes = self.0.to_be_bytes();
        hex::encode(id_bytes)
    }

    /// Decode a hexadecimal string to an id
    pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, FromHexError> {
        let mut id_bytes: [u8; 8] = [0; 8];
        hex::decode_to_slice(hex.as_ref(), &mut id_bytes)?;
        Ok(Self(u64::from_be_bytes(id_bytes)))
    }
}

/// struct that stores common changes and names for all connections
#[derive(Encode, Decode, Debug, Clone)]
pub struct QBDeviceTable {
    /// The id of the device hosting this table
    pub host_id: QBDeviceId,
    commons: HashMap<QBDeviceId, QBHash>,
    names: HashMap<QBDeviceId, String>,
}

impl Default for QBDeviceTable {
    fn default() -> Self {
        Self {
            host_id: QBDeviceId::generate(),
            commons: Default::default(),
            names: Default::default(),
        }
    }
}

impl QBDeviceTable {
    /// Get the common hash of the connection with the id.
    pub fn get_common(&self, id: &QBDeviceId) -> &QBHash {
        self.commons.get(id).unwrap_or(QB_CHANGELOG_BASE.hash())
    }

    /// Set the common hash of the connection with the id.
    pub fn set_common(&mut self, id: &QBDeviceId, hash: QBHash) {
        self.commons.insert(id.clone(), hash);
    }

    /// Get the name of the connection with the id.
    pub fn get_name(&self, id: &QBDeviceId) -> &str {
        self.names.get(id).map(|a| a.as_str()).unwrap_or("untitled")
    }

    /// Set the name of the connection with the id.
    pub fn set_name(&mut self, id: &QBDeviceId, name: String) {
        self.names.insert(id.clone(), name);
    }
}
