//! A filetable is a map which stores different text blobs
//! by their hash for applying diffs. We need this, as the
//! file stored on the file system might not always contain
//! the right content.

use core::panic;
use std::collections::HashMap;

use bitcode::{Decode, Encode};

use crate::{
    change::QBChange,
    hash::{QBHash, QB_HASH_EMPTY},
    path::QBResource,
};

/// struct describing a change that can be directly applied to the file system
///
/// this differs from [QBChange], as the diff stored in UpdateText
/// is already expanded, so no further processing is required.
pub struct QBFSChange {
    /// the resource this change affects
    pub resource: QBResource,
    /// the kind of change
    pub kind: QBFSChangeKind,
}

/// enum describing the different kinds of changes
pub enum QBFSChangeKind {
    /// update a file
    Update {
        /// the file content
        content: Vec<u8>,
        /// the hash of the content
        hash: QBHash,
    },
    /// create a file or directory
    Create,
    /// delete a file or directory
    Delete,
}

/// used for storing previous file versions
#[derive(Encode, Decode, Debug, Clone)]
pub struct QBFileTable {
    contents: HashMap<QBHash, String>,
}

impl Default for QBFileTable {
    fn default() -> Self {
        // add empty file content entry
        let mut contents = HashMap::new();
        contents.insert(QB_HASH_EMPTY.clone(), "".to_string());
        Self { contents }
    }
}

impl QBFileTable {
    /// return the contents for this hash
    pub fn get<'a>(&'a self, hash: &QBHash) -> &'a str {
        match self.contents.get(hash) {
            Some(val) => val.as_str(),
            None => panic!("could not find file table entry for hash {}", hash),
        }
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
    pub fn to_fschange(&mut self, _change: QBChange) -> QBFSChange {
        // TODO: refactor this

        todo!()

        /*
        let resource = change.resource;
        let kind = change.kind;
        let kind = match kind {
            QBChangeKind::Create => QBFSChangeKind::Create,
            QBChangeKind::Delete => QBFSChangeKind::Delete,
            QBChangeKind::UpdateBinary { contents } => {
                let hash = QBHash::compute(&contents);
                QBFSChangeKind::Update {
                    content: contents,
                    hash,
                }
            }
            QBChangeKind::UpdateText { diff } => {
                let old = self.get(&diff.old_hash).to_string();
                let contents = diff.apply(old);
                let hash = QBHash::compute(&contents);
                self.insert_hash(hash.clone(), contents.clone());
                QBFSChangeKind::Update {
                    content: contents.into(),
                    hash,
                }
            }
        };

        QBFSChange { resource, kind }
        */
    }
}
