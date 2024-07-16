use std::collections::HashMap;

use crate::common::resource::QBResource;

use super::QBChange;

#[derive(Debug)]
struct Entry {
    is_local: bool,
    change: QBChange,
}

/// A timesorted changemap
#[derive(Default, Debug)]
pub struct QBChangeMap {
    // bool indicates whether change is local
    changes: HashMap<QBResource, Vec<Entry>>,
}

impl QBChangeMap {
    pub fn push(&mut self, is_local: bool, change: QBChange) {
        self.changes
            .entry(change.resource.clone())
            .or_default()
            .push(Entry { is_local, change });
    }

    /// turn this changemap into a vec
    pub fn changes(self) -> Vec<QBChange> {
        let mut entries = self
            .changes
            .into_values()
            .flatten()
            .filter_map(|e| (!e.is_local).then(|| e.change))
            .collect::<Vec<_>>();
        Self::_sort(&mut entries);
        entries
    }

    #[inline]
    fn _sort(entries: &mut Vec<QBChange>) {
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }
}
