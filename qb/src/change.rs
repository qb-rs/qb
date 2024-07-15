use core::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitcode::{Decode, Encode};
use time::macros::format_description;

use crate::{resource::QBResource, QBDiff, QBHash};

#[derive(Encode, Decode, Clone, Debug)]
pub struct QBChange {
    hash: QBHash,
    pub timestamp: u64,
    pub kind: QBChangeKind,
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

    /// Create a new entry from the current system time
    #[inline]
    pub fn now(kind: QBChangeKind, resource: QBResource) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self::new(ts, kind, resource)
    }

    #[inline]
    pub fn hash(&self) -> &QBHash {
        &self.hash
    }
}

// TODO: move filetree updates into fswrapper

#[derive(Encode, Decode, Clone, Debug)]
pub enum QBChangeKind {
    // CopyFrom,
    // CopyTo { from: QBResource }, // TODO: remove from
    // MoveFrom,
    // MoveTo { from: QBResource }, // TODO: remove from
    // Allow on: File
    Change { contents: Vec<u8> }, // TODO: diff file
    Diff { diff: QBDiff },
    // Allow on: File, Directory
    Create,
    // Allow on: File, empty Directory
    Delete,
}

impl QBChangeKind {
    pub fn additive(&self) -> bool {
        match self {
            // QBChangeKind::CopyTo { .. } => true,
            // QBChangeKind::MoveTo { .. } => true,
            QBChangeKind::Change { .. } => true,
            QBChangeKind::Diff { .. } => true,
            QBChangeKind::Create => true,
            _ => false,
        }
    }

    pub fn external(&self) -> bool {
        // TODO: add when move is here again
        false
    }
}
