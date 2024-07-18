//! An ignore file is a file that specifies certain overrides for
//! which files to exclude or to include when syncing.

// TODO: add no std support by using a different ignore implementation

use std::{collections::HashMap, fmt};

use bitcode::{Decode, Encode};
use thiserror::Error;
use tracing::warn;

use crate::fs::table::{QBFSChange, QBFSChangeKind, QBFileTable};

use super::{
    hash::QBHash,
    resource::{qbpaths, QBPath, QBResource},
};

/// struct describing an error that occured when dealing with an ignore file
#[derive(Error, Debug)]
pub enum QBIgnoreError {
    /// parser error
    #[error("gitignore error")]
    Gitignore(#[from] ignore::Error),
}

pub(crate) type QBIgnoreResult<T> = Result<T, QBIgnoreError>;

/// struct describing where the ignore rule was defined
pub enum QBIgnoreGlob<'a> {
    /// in ignore file
    GitIgnore(&'a ignore::gitignore::Glob),
    /// in internal code
    Internal,
}

impl<'a> From<&'a ignore::gitignore::Glob> for QBIgnoreGlob<'a> {
    fn from(value: &'a ignore::gitignore::Glob) -> Self {
        Self::GitIgnore(value)
    }
}

/// struct describing an ignore file
pub struct QBIgnore(ignore::gitignore::Gitignore);

impl QBIgnore {
    /// Match resource against this ignore file
    pub fn matched(&self, resource: &QBResource) -> ignore::Match<QBIgnoreGlob> {
        // println!("MATCHING: {}", resource);
        self.0
            .matched(resource.path.as_fspath(), resource.is_dir())
            .map(|e| e.into())
    }

    /// Parse a QBIgnore from its contents
    ///
    /// path should be the path of the directory this ignore file is stored
    pub fn parse(path: impl AsRef<QBPath>, contents: impl AsRef<str>) -> QBIgnoreResult<QBIgnore> {
        let fspath = path.as_ref().as_fspath();
        let mut builder = ignore::gitignore::GitignoreBuilder::new(fspath);
        for line in contents.as_ref().split("\n") {
            builder.add_line(None, line)?;
        }
        // TODO: error handling
        let ignore = builder.build()?;
        Ok(QBIgnore(ignore))
    }
}

/// builder for [QBIgnoreMap]
#[derive(Encode, Decode, Clone, Default, Debug)]
pub struct QBIgnoreMapBuilder {
    ignores: HashMap<QBPath, QBHash>,
}

impl QBIgnoreMapBuilder {
    /// Notify this builder of a file system change
    pub fn notify_change(&mut self, change: &QBFSChange) {
        let resource = &change.resource;
        let kind = &change.kind;

        if resource.path.name() != Some(".qbignore") {
            return;
        }

        let path = resource.path.clone().parent().unwrap();

        match kind {
            QBFSChangeKind::Update { hash, .. } => {
                self.ignores.insert(path, hash.clone());
            }
            QBFSChangeKind::Delete => _ = self.ignores.remove(&path),
            QBFSChangeKind::Create => {}
        };
    }

    /// Build the ignore map
    pub fn build(&self, table: &QBFileTable) -> QBIgnoreMap {
        let ignores = self
            .ignores
            .iter()
            .filter_map(|(path, hash)| {
                let contents = table.get(hash);
                let ignore = QBIgnore::parse(path, contents)
                    .inspect_err(|err| warn!("skipping ignore file for {}: {}", path, err))
                    .ok()?;
                Some((path.clone(), ignore))
            })
            .collect::<HashMap<QBPath, QBIgnore>>();

        QBIgnoreMap { ignores }
    }
}

/// struct describing a collection of ignore files that cover a file system
pub struct QBIgnoreMap {
    ignores: HashMap<QBPath, QBIgnore>,
}

impl fmt::Display for QBIgnoreMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ignore map with {} file(s):", self.ignores.len())?;
        for (path, ignore) in self.ignores.iter() {
            write!(f, "- {} -> {} rules", path, ignore.0.num_ignores())?;
        }
        Ok(())
    }
}

impl QBIgnoreMap {
    /// Notify this ignore map of a file system change
    pub fn notify_change(&mut self, change: &QBFSChange) {
        let resource = &change.resource;
        let kind = &change.kind;

        if resource.path.name().unwrap() != ".qbignore" {
            return;
        }

        let path = resource.path.clone().parent().unwrap();

        match kind {
            QBFSChangeKind::Update { content, .. } => {
                match simdutf8::basic::from_utf8(content) {
                    Ok(str) => {
                        let ignore = match QBIgnore::parse(&path, str) {
                            Ok(ignore) => ignore,
                            Err(err) => {
                                warn!("skipping ignore file for {}: {}", path, err);
                                return;
                            }
                        };
                        self.ignores.insert(path, ignore);
                    }
                    Err(_) => {}
                };
            }
            QBFSChangeKind::Delete => _ = self.ignores.remove(&path),
            QBFSChangeKind::Create => {}
        };
    }

    /// Match resource against this ignore map
    ///
    /// TODO: unexpected behaviour when trying to ignore directories without /
    pub fn matched(&self, resource: &QBResource) -> ignore::Match<QBIgnoreGlob> {
        // ignore internal directories
        if qbpaths::INTERNAL.is_parent(resource) {
            return ignore::Match::Ignore(QBIgnoreGlob::Internal);
        }

        let mut curr = Some(resource.path.clone());
        while let Some(path) = curr {
            // println!("TRYING: {}", path);
            if let Some(ignore) = self.ignores.get(&path) {
                let m = ignore.matched(resource);
                if !m.is_none() {
                    return m;
                }
            }
            curr = path.parent();
        }

        ignore::Match::None
    }
}
