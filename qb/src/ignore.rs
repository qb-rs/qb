use std::collections::HashMap;

use bitcode::{Decode, Encode};
/// TODO: add no std support by using a different ignore implementation
use thiserror::Error;
use tracing::warn;

use crate::{qbpaths, QBFileTree, QBPath, QBResource};

#[derive(Error, Debug)]
pub enum QBIgnoreError {
    #[error("gitignore error")]
    Gitignore(#[from] ignore::Error),
}

pub type QBIgnoreResult<T> = Result<T, QBIgnoreError>;

pub enum QBIgnoreGlob<'a> {
    GitIgnore(&'a ignore::gitignore::Glob),
    Internal,
}

impl<'a> From<&'a ignore::gitignore::Glob> for QBIgnoreGlob<'a> {
    fn from(value: &'a ignore::gitignore::Glob) -> Self {
        Self::GitIgnore(value)
    }
}

pub struct QBIgnore(ignore::gitignore::Gitignore);

impl QBIgnore {
    /// Match resource against this ignore file
    pub fn matched(&self, resource: &QBResource) -> ignore::Match<QBIgnoreGlob> {
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
            // TODO: error handling
            builder.add_line(None, line)?;
        }
        // TODO: error handling
        let ignore = builder.build()?;
        Ok(QBIgnore(ignore))
    }
}

#[derive(Encode, Decode, Clone, Default)]
pub struct QBIgnoreMapBuilder {
    ignores: Vec<(QBPath, usize)>,
}

impl QBIgnoreMapBuilder {
    /// Build the ignore map
    pub fn build(&self, filetree: &QBFileTree) -> QBIgnoreMap {
        let ignores = self
            .ignores
            .iter()
            .filter_map(|e| {
                let contents = &filetree.arena[e.1].file().contents;
                let ignore = QBIgnore::parse(&e.0, contents)
                    .inspect_err(|err| warn!("skipping ignore file for {}: {}", e.0, err))
                    .ok()?;
                Some((e.0.clone(), ignore))
            })
            .collect::<HashMap<QBPath, QBIgnore>>();

        QBIgnoreMap { ignores }
    }
}

pub struct QBIgnoreMap {
    ignores: HashMap<QBPath, QBIgnore>,
}

impl QBIgnoreMap {
    /// Match resource against this ignore map
    pub fn matched(&self, resource: &QBResource) -> ignore::Match<QBIgnoreGlob> {
        // ignore internal directories
        if qbpaths::INTERNAL.is_parent(resource) {
            return ignore::Match::Ignore(QBIgnoreGlob::Internal);
        }

        let mut curr = Some(resource.path.clone());
        while let Some(path) = curr {
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
