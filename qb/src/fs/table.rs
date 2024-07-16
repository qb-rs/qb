use std::collections::HashMap;

use bitcode::{Decode, Encode};

use crate::{
    change::{QBChange, QBChangeKind},
    common::{hash::QBHash, resource::QBResource},
};

pub struct QBFSChange {
    pub resource: QBResource,
    pub kind: QBFSChangeKind,
}

pub enum QBFSChangeKind {
    Update { contents: Vec<u8>, hash: QBHash },
    Create,
    Delete,
}

/// used for storing previous file versions
#[derive(Encode, Decode, Debug, Clone, Default)]
pub struct QBFileTable {
    contents: HashMap<QBHash, String>,
}

impl QBFileTable {
    /// return the contents for this hash
    pub fn get<'a>(&'a self, hash: &QBHash) -> &'a str {
        self.contents.get(hash).map(|e| e.as_str()).unwrap_or("")
    }

    /// remove & return the contents for this hash
    pub fn remove(&mut self, hash: &QBHash) -> String {
        self.contents.remove(hash).unwrap_or_default()
    }

    /// insert contents for this file
    ///
    /// this will compute the contents hash
    pub fn insert(&mut self, contents: String) {
        self.contents.insert(QBHash::compute(&contents), contents);
    }

    /// insert contents for this file
    pub fn insert_hash(&mut self, hash: QBHash, contents: String) {
        self.contents.insert(hash, contents);
    }

    /// convert the given changes to fs changes
    pub fn to_fschanges(&mut self, changes: Vec<QBChange>) -> Vec<QBFSChange> {
        changes.into_iter().map(|e| self.to_fschange(e)).collect()
    }

    /// convert the given change to fs change
    pub fn to_fschange(&mut self, change: QBChange) -> QBFSChange {
        let resource = change.resource;
        let kind = change.kind;
        let kind = match kind {
            QBChangeKind::Create => QBFSChangeKind::Create,
            QBChangeKind::Delete => QBFSChangeKind::Delete,
            QBChangeKind::Change { contents } => {
                let hash = QBHash::compute(&contents);
                QBFSChangeKind::Update { contents, hash }
            }
            QBChangeKind::Diff { diff } => {
                let old = self.get(&diff.old_hash).to_string();
                let contents = diff.apply(old);
                let hash = QBHash::compute(&contents);
                self.insert_hash(hash.clone(), contents.clone());
                QBFSChangeKind::Update {
                    contents: contents.into(),
                    hash,
                }
            }
        };

        QBFSChange { resource, kind }
    }
}
