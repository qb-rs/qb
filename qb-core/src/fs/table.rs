//! A filetable is a map which stores different text blobs
//! by their hash for applying diffs. We need this, as the
//! file stored on the file system might not always contain
//! the right content.

use core::panic;
use std::collections::HashMap;

use bitcode::{Decode, Encode};

use crate::hash::{QBHash, QB_HASH_EMPTY};

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
}
