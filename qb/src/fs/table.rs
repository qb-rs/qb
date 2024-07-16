use std::collections::HashMap;

use bitcode::{Decode, Encode};

use crate::common::hash::QBHash;

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
}
