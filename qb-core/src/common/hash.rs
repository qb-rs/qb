//! A hash is a sequence with constant length which gets mapped
//! to some other data. Here it is used for differentiating between files
//! without storing the file's contents.

use core::fmt;

use bitcode::{Decode, Encode};
use lazy_static::lazy_static;
use sha2::{digest::generic_array::GenericArray, Digest, Sha256};

/// struct which describes a hash
#[derive(Encode, Decode, PartialEq, Eq, Clone, Default, Hash, PartialOrd, Ord)]
pub struct QBHash(pub(crate) [u8; 32]);

impl fmt::Display for QBHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..", hex::encode(&self.0[0..8]))
    }
}

impl fmt::Debug for QBHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QBHash({})", hex::encode(self.0))
    }
}

lazy_static! {
    /// The hash for empty contents
    pub static ref QB_HASH_EMPTY: QBHash = QBHash::compute(vec![]);
}

impl QBHash {
    /// Compute the hash.
    pub fn compute(contents: impl AsRef<[u8]>) -> QBHash {
        let mut hash = QBHash::default();
        Self::compute_mut(&mut hash, contents);
        hash
    }

    /// Compute the hash.
    pub fn compute_mut(hash: &mut QBHash, contents: impl AsRef<[u8]>) {
        let mut hasher = Sha256::new();
        hasher.update(contents);
        hasher.finalize_into(GenericArray::from_mut_slice(&mut hash.0));
    }
}
