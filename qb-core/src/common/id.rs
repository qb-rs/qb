//! Ids are used to identify devices.

use std::hash::{DefaultHasher, Hash, Hasher};

use bitcode::{Decode, Encode};
use lazy_static::lazy_static;

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
