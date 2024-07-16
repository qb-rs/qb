// TODO: refactor this
// TODO: merge into change.rs

use std::{cmp::Ordering, collections::HashSet};

use bitcode::{Decode, Encode};

use crate::{change::map::QBChangeMap, common::hash::QBHash};

use super::{QBChange, QB_ENTRY_BASE};

#[derive(Encode, Decode, Clone, Debug)]
pub struct QBChangelog(pub Vec<QBChange>);

impl Default for QBChangelog {
    fn default() -> Self {
        Self(vec![QB_ENTRY_BASE.clone()])
    }
}

// TODO: test whether merge(a, b) == merge(b, a)
impl QBChangelog {
    /// Checks whether this changelog is valid.
    ///
    /// Changelog entries must be sorted according to their timestamp.
    /// TODO: convert to error for details
    pub fn is_valid(&self) -> bool {
        if self.0.is_empty() {
            return false;
        }

        if self.0.first().unwrap().hash() != QB_ENTRY_BASE.hash() {
            return false;
        }

        let mut current_ts = 0;
        for entry in self.0.iter() {
            if entry.timestamp < current_ts {
                return false;
            }

            current_ts = entry.timestamp;
        }

        return true;
    }

    /// Pushes an entry to this changelog.
    pub fn push(&mut self, entry: QBChange) -> bool {
        if self.0.iter().find(|e| e.hash() == entry.hash()).is_some() {
            return false;
        }

        self.0.push(entry);
        true
    }

    pub fn after(&mut self, hash: &QBHash) -> Option<Vec<QBChange>> {
        let index = self.0.iter().position(|e| e.hash() == hash)? + 1;
        Some(self.0.split_off(index))
    }

    pub fn after_cloned(&self, hash: &QBHash) -> Option<Vec<QBChange>> {
        let index = self.0.iter().position(|e| e.hash() == hash)? + 1;
        Some(self.0.iter().skip(index).map(|e| e.clone()).collect())
    }

    pub fn append(&mut self, entries: &mut Vec<QBChange>) {
        self.0.append(entries)
    }

    /// This is safe if changelog is valid.
    pub fn head(&self) -> QBHash {
        unsafe { self.0.last().unwrap_unchecked() }.hash().clone()
    }

    // TODO: work with errors instead of asserts to prevent runtime panics
    //
    /// Merge two changelogs and return either a common changelog plus the changes
    /// required to each individual file system or a vec of merge conflicts.
    pub fn merge(
        local: Vec<QBChange>,
        remote: Vec<QBChange>,
    ) -> Result<(Vec<QBChange>, Vec<QBChange>), String> {
        // TODO: this returns only the last change for a specific entry
        // TODO: fix this
        // TODO: rewrite
        let mut local_iter = local.into_iter().peekable();
        let mut remote_iter = remote.into_iter().peekable();

        let mut changemap = QBChangeMap::default();
        let mut entries = Vec::new();

        // skip common history
        while local_iter
            .peek()
            .zip(remote_iter.peek())
            .is_some_and(|(a, b)| a.hash() == b.hash())
        {
            _ = remote_iter.next();
            entries.push(unsafe { local_iter.next().unwrap_unchecked() });
        }

        loop {
            // unsafe code, yay
            let (entry, is_local) = match (local_iter.peek(), remote_iter.peek()) {
                (Some(_), None) => (unsafe { local_iter.next().unwrap_unchecked() }, true),
                (None, Some(_)) => (unsafe { remote_iter.next().unwrap_unchecked() }, false),
                (Some(a), Some(b)) if a.hash() == b.hash() => {
                    _ = remote_iter.next();
                    (unsafe { local_iter.next().unwrap_unchecked() }, true)
                }
                (Some(a), Some(b)) => match a.timestamp.cmp(&b.timestamp) {
                    Ordering::Less => (unsafe { local_iter.next().unwrap_unchecked() }, true),
                    Ordering::Greater => (unsafe { remote_iter.next().unwrap_unchecked() }, false),
                    // TODO: find a deterministic way of handling this case
                    Ordering::Equal => todo!(
                        "Two distinct entries from different changelogs may not have the same timestamp"
                    ),
                },
                _ => break,
            };

            //if changediff.try_push((is_local, entry.clone()))? {
            changemap.push(is_local, entry.clone());
            entries.push(entry);
            //}
        }

        // check that there are no duplicate hashes
        assert!({
            let mut uniq = HashSet::new();
            entries.iter().all(move |x| uniq.insert(x.hash().clone()))
        });

        Ok((entries, changemap.changes()))
    }
}
