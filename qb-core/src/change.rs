//! # change
//!
//! This module provides primitives for working with changes applied
//! to a filesystem.

use std::{collections::HashMap, fmt};

use bitcode::{Decode, Encode};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    diff::QBDiff,
    path::{QBPath, QBResource},
    time::QBTimeStampUnique,
};

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
    #[inline(always)]
    pub fn is_external(&self) -> bool {
        match self {
            QBChangeKind::CopyFrom | QBChangeKind::RenameFrom => true,
            _ => false,
        }
    }

    /// Returns whether this change removes the resource.
    #[inline(always)]
    pub fn is_subtractive(&self) -> bool {
        match self {
            QBChangeKind::Delete | QBChangeKind::RenameFrom => true,
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

    /// Append another changemap to this map.
    pub fn append(&mut self, other: Self) {
        if other.head > self.head {
            self.head = other.head;
        }
        for (resource, mut other_entries) in other.changes.into_iter() {
            let mut entries = self.entries(resource);
            entries.append(&mut other_entries);
            Self::_sort(&mut entries);
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

    /// Registers the change
    pub fn register(&mut self, change: &QBChange) -> bool {
        if change.timestamp > self.head {
            self.head = change.timestamp.clone();
            return true;
        }
        false
    }

    /// Push an entry.
    pub fn push(&mut self, resource: QBResource, change: QBChange) {
        let new_change = self.register(&change);
        let entries = self.entries(resource);
        entries.push(change);
        if !new_change {
            Self::_sort(entries);
        }
    }

    /// Gets the changes for a given resource from this changemap.
    /// [!] Pushing via this does not update the head nor does it assert sorting.
    #[inline(always)]
    fn entries(&mut self, resource: QBResource) -> &mut Vec<QBChange> {
        self.changes.entry(resource).or_default()
    }

    /// Sorts this changemap using each change's timestamp.
    /// this should not be necessary, changemap should be sorted.
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
        // really bad implementation currently. TODO: fix this
        for (resource, entries) in self.changes.clone().iter() {
            let mut remove_until = 0;

            let mut i = 0;
            while i < entries.len() {
                match &entries[i].kind {
                    // TODO: collapse diffs
                    kind if kind.is_external() => remove_until = i + 1,
                    QBChangeKind::Create => remove_until = i,
                    QBChangeKind::Delete => {
                        // remove unused, logged changes
                        if matches!(entries[remove_until].kind, QBChangeKind::Create) {
                            i += 1;
                        }

                        self.changes
                            .get_mut(resource)
                            .unwrap()
                            .drain(remove_until..i)
                            .len();

                        continue;
                    }
                    QBChangeKind::RenameFrom => {
                        if matches!(entries[remove_until].kind, QBChangeKind::Create) {
                            let mut changes = self
                                .changes
                                .get_mut(resource)
                                .unwrap()
                                .drain(remove_until..i + 1)
                                .collect::<Vec<_>>();
                            changes.pop();

                            let (index, resource) =
                                self.get_rename_to(&entries[i].timestamp).unwrap();

                            let to_entries = self.changes.get_mut(&resource.clone()).unwrap();
                            let mut head = to_entries.drain(index..).collect::<Vec<_>>();
                            to_entries.append(&mut changes);
                            to_entries.append(&mut head);
                        }
                    }
                    // TODO: collapse diffs using file table
                    _ => {}
                }

                i += 1;
            }
        }
    }

    pub fn get_rename_to(&self, timestamp: &QBTimeStampUnique) -> Option<(usize, &QBResource)> {
        self.changes.iter().find_map(|(resource, entries)| {
            entries
                .into_iter()
                .position(|change| &change.timestamp == timestamp)
                .map(|i| (i, resource))
        })
    }

    // TODO: collision detection
    // TODO: test whether merge(a, b) == merge(b, a)
    //
    /// merge two changelogs and return either a common changelog plus the changes
    /// required to each individual file system or a vec of merge conflicts.
    pub fn merge(&mut self, remote: Self) -> Result<Vec<(QBResource, QBChange)>, String> {
        let mut changes = Vec::new();
        for (resource, mut remote_entries) in remote.changes.into_iter() {
            if let Some(mut entries) = self.changes.get_mut(&resource) {
                // TODO: do this properly
                let mut rchanges = remote_entries.clone();
                changes.extend(&mut rchanges.into_iter().map(|e| (resource.clone(), e)));

                *entries = Self::_merge(remote_entries, &mut entries);
                // TODO: update head
            } else {
                changes.extend(
                    remote_entries
                        .iter()
                        .cloned()
                        .map(|e| (resource.clone(), e)),
                );
                self.register(remote_entries.last().unwrap());
                let entries = self.entries(resource);
                entries.append(&mut remote_entries);
                Self::_sort(entries);
            }
        }

        Ok(changes)
    }

    fn _merge(mut a: Vec<QBChange>, b: &mut Vec<QBChange>) -> Vec<QBChange> {
        a.append(b);
        Self::_sort(&mut a);

        let til = a
            .iter()
            .rposition(|e| !e.kind.is_subtractive())
            .unwrap_or(0);
        a.into_iter()
            .enumerate()
            .filter(|(i, e)| i >= &til || !e.kind.is_subtractive())
            .map(|(_, e)| e)
            .collect()
    }
}
