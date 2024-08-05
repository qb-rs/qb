//! # What is a device id?
//!
//! Device ids are used to identify devices.
//! A device is a service which has the ability
//! to store files in some way.
//!
//! QBIs can be attached to devices to provide
//! them with the ability to synchronize with other
//! devices. A QBI can be identified by a pair of
//! device id and QBI kind. It is currently under
//! examination whether QBIIds are actually sensible,
//! as you probably don't want to connect to a device twice.

use std::hash::{DefaultHasher, Hasher};
use std::{fmt, hash::Hash};

use bitcode::{Decode, Encode};
use hex::FromHexError;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// A device identifier.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct QBDeviceId(u64);

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
