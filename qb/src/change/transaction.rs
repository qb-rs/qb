//! A transaction contains changes that were not yet applied to a changelog.

use std::collections::HashMap;

use crate::common::resource::QBResource;

use super::{QBChange, QBChangeKind};

/// A timesorted changemap.
#[derive(Default, Debug)]
pub struct QBTransaction {
    pub(crate) changes: HashMap<QBResource, Vec<QBChange>>,
}

impl From<Vec<QBChange>> for QBTransaction {
    fn from(value: Vec<QBChange>) -> Self {
        Self::from_vec(value)
    }
}

impl From<HashMap<QBResource, Vec<QBChange>>> for QBTransaction {
    fn from(value: HashMap<QBResource, Vec<QBChange>>) -> Self {
        Self::from_map(value)
    }
}

impl Into<Vec<QBChange>> for QBTransaction {
    fn into(self) -> Vec<QBChange> {
        self.into_vec()
    }
}

impl Into<HashMap<QBResource, Vec<QBChange>>> for QBTransaction {
    fn into(self) -> HashMap<QBResource, Vec<QBChange>> {
        self.into_map()
    }
}

impl QBTransaction {
    /// Convert a vec of entries into a transaction.
    ///
    /// This will sort the individual entries.
    pub fn from_vec(value: Vec<QBChange>) -> Self {
        Self::from_map(Self::_group(value))
    }

    /// Convert a vec of entries into a transaction.
    ///
    /// [!] This will not sort the individual entries
    /// and therefore requires a sorted vec as input.
    pub unsafe fn from_vec_unchecked(value: Vec<QBChange>) -> Self {
        Self::from_map_unchecked(Self::_group(value))
    }

    /// Convert a map into a transaction.
    ///
    /// This will sort the individual entries.
    pub fn from_map(mut value: HashMap<QBResource, Vec<QBChange>>) -> Self {
        for entries in value.values_mut() {
            Self::_sort(entries);
        }

        Self { changes: value }
    }

    /// Convert a map into a transaction.
    ///
    /// [!] This will not sort the individual entries
    /// and therefore requires sorted entries as input.
    pub unsafe fn from_map_unchecked(value: HashMap<QBResource, Vec<QBChange>>) -> Self {
        Self { changes: value }
    }

    /// Convert a transaction into a map
    pub fn into_map(self) -> HashMap<QBResource, Vec<QBChange>> {
        self.changes
    }

    /// Convert a transaction into a vec
    pub fn into_vec(self) -> Vec<QBChange> {
        let mut vec = self.changes.into_values().flatten().collect::<Vec<_>>();
        Self::_sort(&mut vec);
        vec
    }

    /// Convert a transaction to a vec
    pub fn to_vec(&self) -> Vec<&QBChange> {
        let mut vec = self.changes.values().flatten().collect::<Vec<_>>();
        Self::_sort_borrowed(&mut vec);
        vec
    }

    /// Insert entry for resource, sort
    ///
    /// Makes sure that the entries for the resource
    /// are in correct order after inserting (sort).
    pub fn push(&mut self, entry: QBChange) {
        let entries = self._entries(entry.resource.clone());
        entries.push(entry);
        Self::_sort(entries);
    }

    /// Insert entry for resource, do not sort
    ///
    /// This method is marked as unsafe, as in release
    /// builds pushing entries not sorted by timestamp
    /// can cause issues when minifying.
    ///
    /// In debug mode this would cause a runtime panic.
    pub unsafe fn push_unchecked(&mut self, entry: QBChange) {
        let entries = self._entries(entry.resource.clone());
        debug_assert!(entries
            .last()
            .map(|e| e.timestamp <= entry.timestamp)
            .unwrap_or(true));
        entries.push(entry);
    }

    /// Minify the transaction [only local]
    ///
    /// This will merge redundant changes.
    pub fn minify(&mut self) {
        for entries in self.changes.values_mut() {
            Self::_minify(entries);
        }
    }

    /// Complete the transaction [only local]
    ///
    /// This will minify the transaction and then turn
    /// it into a vector.
    pub fn complete_into(mut self) -> Vec<QBChange> {
        self.minify();
        self.into_vec()
    }

    /// Complete the transaction [only local]
    ///
    /// This will minify the transaction and then turn
    /// it into a vector.
    pub fn complete(&mut self) -> Vec<&QBChange> {
        self.minify();
        self.to_vec()
    }

    // INTERNAL HELPER FUNCTIONS FOR DRY-ING

    #[inline]
    fn _entries(&mut self, resource: QBResource) -> &mut Vec<QBChange> {
        self.changes.entry(resource).or_default()
    }

    #[inline]
    fn _group(value: Vec<QBChange>) -> HashMap<QBResource, Vec<QBChange>> {
        value.into_iter().fold(
            HashMap::new(),
            |mut changes: HashMap<QBResource, Vec<QBChange>>, entry| {
                changes.entry(entry.resource).or_default();
                changes
            },
        )
    }

    #[inline]
    fn _sort(entries: &mut Vec<QBChange>) {
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    #[inline]
    fn _sort_borrowed(entries: &mut Vec<&QBChange>) {
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    #[inline]
    fn _minify(entries: &mut Vec<QBChange>) {
        let mut remove_until = 0;

        let mut i = 0;
        while i < entries.len() {
            match &entries[i].kind {
                // TODO: collapse diffs
                kind if kind.external() => remove_until = i + 1,
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
