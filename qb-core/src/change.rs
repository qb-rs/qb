//! This module is responsible for everything that
//! has to do with tracking changes. That is for example
//! the [QBChange] structs themselves.
//!
//! TODO: need to work hard on this one

pub mod log;
pub mod map;
pub mod transaction;

use core::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitcode::{Decode, Encode};
use lazy_static::lazy_static;
use time::macros::format_description;

use crate::common::{
    diff::QBDiff,
    hash::QBHash,
    resource::{qbpaths, QBResource},
};

lazy_static! {
    /// This is the base entry that comes first at every changelog
    pub static ref QB_CHANGELOG_BASE: QBChange =
        QBChange::new(0, QBChangeKind::Create, qbpaths::ROOT.clone().dir());
}

/// This struct describes a change that has been done.
#[derive(Encode, Decode, Clone, Debug)]
pub struct QBChange {
    hash: QBHash,
    /// a unix timestamp describing when the change has been committed
    pub timestamp: u64,
    /// the kind of change
    pub kind: QBChangeKind,
    /// the resource this change affects
    pub resource: QBResource,
}

impl fmt::Display for QBChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let utc = time::OffsetDateTime::UNIX_EPOCH + Duration::from_millis(self.timestamp);
        let ft = format_description!("[day]-[month repr:short]-[year] [hour]:[minute]:[second]");

        write!(
            f,
            "{} {} {:?} {}",
            self.hash,
            utc.format(ft).unwrap(),
            self.kind,
            self.resource
        )
    }
}

impl QBChange {
    /// Create a new change.
    pub fn new(timestamp: u64, kind: QBChangeKind, resource: QBResource) -> Self {
        let mut ret = Self {
            hash: Default::default(),
            timestamp,
            kind,
            resource,
        };

        // TODO: figure out whether it makes sense to store the hash of each commit
        let binary = bitcode::encode(&ret);
        QBHash::compute_mut(&mut ret.hash, binary);

        ret
    }

    /// Create a new change with the current system time.
    #[inline]
    pub fn now(kind: QBChangeKind, resource: QBResource) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self::new(ts, kind, resource)
    }

    /// return the hash of this change
    #[inline]
    pub fn hash(&self) -> &QBHash {
        &self.hash
    }
}

/// an enum describing the different kinds of changes
#[derive(Encode, Decode, Clone, Debug)]
pub enum QBChangeKind {
    /// change a binary file
    UpdateBinary {
        /// the file's new contents
        contents: Vec<u8>,
    },
    /// change a text file
    UpdateText {
        /// a diff that when applied yields the new contents
        diff: QBDiff,
    },
    /// create a file or directory
    Create,
    /// delete a file or directory
    Delete,
}

impl QBChangeKind {
    /// currently unimplemented, will be interessting once move and copy are here again
    pub fn external(&self) -> bool {
        // TODO: add when move is here again
        false
    }
}
