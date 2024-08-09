//! # change
//!
//! This module provides primitives for working with changes applied
//! to a filesystem.

use std::{collections::HashMap, fmt};

use bitcode::{Decode, Encode};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{diff::QBDiff, path::QBResource, time::QBTimeStampUnique};

/// This struct represents a change applied to some file.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub struct QBChange {
    /// The timestamp of when this change occured
    pub timestamp: QBTimeStampUnique,
    /// The kind of change
    pub kind: QBChangeKind,
}

impl fmt::Display for QBChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}", self.timestamp, self.kind)
    }
}

impl QBChange {
    /// Construct a new change.
    pub fn new(timestamp: QBTimeStampUnique, kind: QBChangeKind) -> Self {
        Self { timestamp, kind }
    }
}

/// The kind of change.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
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
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Default, Clone)]
pub struct QBChangeMap {
    changes: HashMap<QBResource, Vec<QBChange>>,
    head: QBTimeStampUnique,
}

impl QBChangeMap {
    /// Gets the changes since the timestamp.
    pub fn since_cloned(&self, since: &QBTimeStampUnique) -> QBChangeMap {
        // iterator magic
        let changes = self
            .changes
            .iter()
            .map(|(resource, entries)| {
                (
                    resource.clone(),
                    entries
                        .into_iter()
                        .filter(|e| &e.timestamp > since)
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            })
            .filter(|(_, entries)| !entries.is_empty())
            .collect::<HashMap<_, _>>();

        QBChangeMap {
            changes,
            head: self.head.clone(),
        }
    }

    /// Gets the changes since the timestamp.
    pub fn since(&mut self, since: &QBTimeStampUnique) -> QBChangeMap {
        // iterator magic
        let changes = self
            .changes
            .iter_mut()
            .filter_map(|(resource, entries)| {
                Some((
                    resource.clone(),
                    entries
                        .drain(entries.iter().position(|e| &e.timestamp > since)?..)
                        .collect(),
                ))
            })
            .collect::<HashMap<_, _>>();

        QBChangeMap {
            changes,
            head: self.head.clone(),
        }
    }

    /// Returns whether this changemap is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Iterate over the changes.
    pub fn iter(&self) -> impl Iterator<Item = (&QBResource, &QBChange)> {
        self.changes
            .iter()
            .map(|(resource, entries)| entries.into_iter().map(move |change| (resource, change)))
            .flatten()
            .sorted_by(|a, b| a.1.timestamp.cmp(&b.1.timestamp))
    }

    /// Return the head of this changemap (the last change).
    pub fn head(&self) -> &QBTimeStampUnique {
        &self.head
    }

    /// Gets the changes for a given resource from this changemap.
    #[inline(always)]
    pub fn entries(&mut self, resource: QBResource) -> &mut Vec<QBChange> {
        self.changes.entry(resource).or_default()
    }

    /// Sorts this changemap using each change's timestamp.
    pub fn sort(&mut self) {
        for entries in self.changes.values_mut() {
            Self::_sort(entries);
        }
    }

    #[inline(always)]
    fn _sort(entries: &mut [QBChange]) {
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    #[inline(always)]
    fn _sort_borrowed(entries: &mut [&QBChange]) {
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    /// Minifies this changemap.
    pub fn minify(&mut self) {
        for entries in self.changes.values_mut() {
            Self::_sort(entries);
            Self::_minify(entries);
        }
    }

    #[inline(always)]
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
