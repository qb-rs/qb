//! Devices can be connected through with QBIs.

use std::{
    fmt,
    hash::{DefaultHasher, Hash, Hasher},
};

use bitcode::{Decode, Encode};
use hex::FromHexError;
use lazy_static::lazy_static;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// struct which represents an id from a specific device
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone, Default, Eq, PartialEq, Hash)]
pub struct QBId(pub(crate) u64);

impl From<&str> for QBId {
    fn from(value: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        QBId(hasher.finish())
    }
}

impl fmt::Display for QBId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl AsRef<u64> for QBId {
    fn as_ref(&self) -> &u64 {
        &self.0
    }
}

impl QBId {
    /// Generate a new ID
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        QBId(rng.gen::<u64>())
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

lazy_static! {
    /// the default id
    pub static ref QBID_DEFAULT: QBId = QBId::default();
}
