use std::collections::HashMap;

use bitcode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{diff::QBDiff, path::QBResource, time::QBTimeStampUnique};

/// This struct represents a change applied to some file.
#[derive(Encode, Decode, Serialize, Deserialize, Debug)]
pub struct QBChange {
    timestamp: QBTimeStampUnique,
    kind: QBChangeKind,
}

#[derive(Encode, Decode, Serialize, Deserialize, Debug)]
pub enum QBChangeKind {
    /// Create resource
    Create,
    /// Delete resource
    Delete,
    /// Update file contents (text)
    UpdateText(QBDiff),
    /// Update file contents (binary)
    #[serde(with = "serde_bytes")]
    UpdateBinary(Vec<u8>),
    /// Rename resource (destination)
    /// This change should have the same timestamp as the
    /// corresponding RenameFrom entry.
    RenameTo,
    /// Rename resource (source)
    /// This change should have the same timestamp as the
    /// corresponding RenameTo entry.
    RenameFrom,
    /// Copy resource (destination)
    /// This change should have the same timestamp as the
    /// corresponding CopyFrom entry.
    CopyTo,
    /// Copy resource (source)
    /// This change should have the same timestamp as the
    /// corresponding CopyTo entries.
    CopyFrom,
}

impl QBChangeKind {
    /// Returns whether this change has external changes that rely on it.
    #[inline]
    pub fn is_external(&self) -> bool {
        match self {
            QBChangeKind::CopyFrom | QBChangeKind::RenameFrom => true,
            _ => false,
        }
    }
}

/// This struct is a map which stores a collection of changes for each resource.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Default)]
pub struct QBChangeMap {
    changes: HashMap<QBResource, Vec<QBChange>>,
}

impl QBChangeMap {
    /// Gets the changes for a given resource from this changemap.
    #[inline]
    pub fn entries(&mut self, resource: QBResource) -> &mut Vec<QBChange> {
        self.changes.entry(resource).or_default()
    }

    /// Sorts this changemap using each change's timestamp.
    pub fn sort(&mut self) {
        for entries in self.changes.values_mut() {
            Self::_sort(entries);
        }
    }

    #[inline]
    fn _sort(entries: &mut [QBChange]) {
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    /// Minifies this changemap.
    pub fn minify(&mut self) {
        for entries in self.changes.values_mut() {
            Self::_sort(entries);
            Self::_minify(entries);
        }
    }

    #[inline]
    fn _minify(entries: &mut Vec<QBChange>) {
        let mut remove_until = 0;

        let mut i = 0;
        while i < entries.len() {
            match &entries[i].kind {
                // TODO: collapse diffs
                kind if kind.is_external() => remove_until = i + 1,
                QBChangeKind::Create => remove_until = i + 1,
                QBChangeKind::Delete => {
                    // remove unused, logged changes
                    i -= entries.drain(remove_until..i).len();

                    // remove direct create => delete chainsa
                    if i != 0 && matches!(entries[i - 1].kind, QBChangeKind::Create) {
                        debug_assert_eq!(entries.drain((i - 1)..(i + 1)).len(), 2);
                        i -= 1;
                    } else {
                        i += 1;
                    }

                    continue;
                }
                // TODO: collapse diffs using file table
                _ => {}
            }

            i += 1;
        }
    }
}
