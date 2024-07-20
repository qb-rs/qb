//! Ids are used to identify devices.

use std::{
    fmt,
    hash::{DefaultHasher, Hash, Hasher},
};

use bitcode::{Decode, Encode};
use lazy_static::lazy_static;
use rand::Rng;

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

impl fmt::Display for QBID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl AsRef<u64> for QBID {
    fn as_ref(&self) -> &u64 {
        &self.0
    }
}

impl QBID {
    /// Generate a new ID
    pub fn generate() -> QBID {
        let mut rng = rand::thread_rng();
        QBID(rng.gen::<u64>())
    }

    /// Get the string representation of this id in hex format
    pub fn to_string(&self) -> String {
        let id_vec = vec![self.0];
        let id_bytes = unsafe { id_vec.align_to::<u8>() }.1;
        hex::encode(id_bytes)
    }
}

lazy_static! {
    /// the default id
    pub static ref QBID_DEFAULT: QBID = QBID::default();
}
