use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use bitcode::{DecodeOwned, Encode};
use thiserror::Error;
use tracing::warn;

use crate::{
    qbpaths, QBChange, QBChangeKind, QBDiff, QBFileTree, QBFileTreeError, QBHash, QBPath,
    QBPathError, QBResource, QBResourceKind, TreeDir, TreeFile,
};

#[derive(Error, Debug)]
pub enum QBFSError {
    #[error("I/O error")]
    IO(#[from] std::io::Error),
    #[error("bitcode error")]
    Bitcode(#[from] bitcode::Error),
    #[error("path error")]
    Path(#[from] QBPathError),
    #[error("file tree error")]
    FileTree(#[from] QBFileTreeError),
    #[error("osstring conversion error: {0:?}")]
    OsString(OsString),
}

pub type QBFSResult<T> = Result<T, QBFSError>;

pub enum QBFileDiff {
    Binary(Vec<u8>),
    Text(QBDiff),
}

pub struct QBFSWrapper {
    pub root: PathBuf,
    pub root_str: String,
    pub filetree: QBFileTree,
}

impl QBFSWrapper {
    /// Create a new filesystem wrapper around a root path.
    pub fn new<T: AsRef<Path>>(root: T) -> Self {
        let root = std::path::absolute(root).unwrap();
        let mut root_str = root.to_str().unwrap().to_string();
        if !root_str.ends_with("/") {
            root_str += "/";
        }

        Self {
            root_str,
            root,
            filetree: Default::default(),
        }
    }

    /// Make sure the filesystem is properly setup.
    pub async fn init(&mut self) -> QBFSResult<()> {
        tokio::fs::create_dir_all(self.fspath(qbpaths::INTERNAL.as_ref())).await?;
        tokio::fs::create_dir_all(self.fspath(qbpaths::INTERNAL_COMMON.as_ref())).await?;
        if let Ok(filetree) = self.load(qbpaths::INTERNAL_FILETREE.as_ref()).await {
            self.filetree = filetree;
        }
        Ok(())
    }

    /// Load and decode from a path
    pub async fn load<'a, T: DecodeOwned>(&self, path: impl AsRef<QBPath>) -> QBFSResult<T> {
        Ok(bitcode::decode(&self.read(path).await?)?)
    }

    /// Encode and save to a path
    pub async fn save<'a, T: Encode>(&self, path: impl AsRef<QBPath>, item: &T) -> QBFSResult<()> {
        tokio::fs::write(self.fspath(path), bitcode::encode(item)).await?;
        Ok(())
    }

    /// Applies changes to this filesystem.
    ///
    /// !!!Use with caution, Safety checks not yet implemented!!!
    ///
    /// TODO: add caution checks
    pub async fn apply(&mut self, changes: &Vec<QBChange>) -> QBFSResult<()> {
        for change in changes {
            let kind = &change.kind;
            let resource = &change.resource;
            let contains = self.contains(&change.resource).await;
            match &kind {
                QBChangeKind::Diff { diff } => {
                    // remove unnessecary clone
                    assert!(resource.is_file());

                    let file = self.filetree.get_mut(resource).unwrap().file_mut();
                    let old = std::mem::take(&mut file.contents);
                    let new = diff.apply(old);
                    file.hash = QBHash::compute(&new);
                    file.contents = new.clone();

                    self.write(resource, &new).await.unwrap();
                }
                QBChangeKind::Change { contents } => {
                    // remove unnessecary clone
                    assert!(resource.is_file());

                    let file = self.filetree.get_mut(resource).unwrap().file_mut();
                    file.hash = QBHash::compute(&contents);
                    file.contents = Default::default();

                    self.write(resource, &contents).await.unwrap();
                }
                QBChangeKind::Delete => {
                    if !contains {
                        warn!("delete requested but resource {} not found!", resource);
                        continue;
                    }

                    self.filetree.remove(&resource);
                    let fspath = self.fspath(&change.resource);
                    match resource.is_dir() {
                        true => tokio::fs::remove_dir_all(&fspath).await?,
                        false => tokio::fs::remove_file(&fspath).await?,
                    };
                }
                QBChangeKind::Create => {
                    if contains {
                        warn!("create requested but resource {} exists!", resource);
                        continue;
                    }

                    let fspath = self.fspath(&change.resource);
                    match resource.is_dir() {
                        true => {
                            self.filetree.insert(resource, TreeDir::default());
                            tokio::fs::create_dir_all(fspath).await?;
                        }
                        false => {
                            self.filetree.insert(resource, TreeFile::default());
                            drop(tokio::fs::File::create(fspath).await?);
                        }
                    };
                }
            };
        }

        Ok(())
    }

    /// Compare the entry on the filesystem to the entry stored
    pub async fn update(&mut self, path: impl AsRef<QBPath>) -> QBFSResult<Option<QBFileDiff>> {
        let contents = self.read(&path).await?;
        let hash = QBHash::compute(&contents);

        let file = self
            .filetree
            .get_mut(&path)
            .ok_or(QBFileTreeError::NotFound)?
            .file_mut();

        // no changes, nothing to do
        if file.hash == hash {
            return Ok(None);
        }
        file.hash = hash;

        match simdutf8::basic::from_utf8(&contents) {
            Ok(new) => {
                let new = new.to_string();
                let old = std::mem::replace(&mut file.contents, new.clone());

                Ok(Some(QBFileDiff::Text(QBDiff::compute(old, new))))
            }
            Err(_) => {
                file.contents = Default::default();
                Ok(Some(QBFileDiff::Binary(contents)))
            }
        }
    }

    /// Returns whether this filesystem contains the given resource
    pub async fn contains(&self, resource: &QBResource) -> bool {
        tokio::fs::metadata(self.fspath(resource))
            .await
            .map(|metadata| resource.is_file_type(metadata.file_type()))
            .unwrap_or(false)
    }

    /// Returns whether this filesystem contains the given resource
    pub fn contains_sync(&self, resource: &QBResource) -> bool {
        std::fs::metadata(self.fspath(resource))
            .map(|metadata| resource.is_file_type(metadata.file_type()))
            .unwrap_or(false)
    }

    /// Reads a directory asynchronously
    ///
    /// Stops processing entries once an error occured and returns this error.
    pub async fn read_dir(&self, path: impl AsRef<QBPath>) -> QBFSResult<Vec<QBResource>> {
        let fspath = self.fspath(&path);

        let mut entries = Vec::new();
        let mut iter = tokio::fs::read_dir(fspath).await?;
        while let Some(entry) = iter.next_entry().await? {
            let file_type = entry.file_type().await?;
            let file_name = Self::_str(entry.file_name())?;

            let resource = QBResource::new(
                path.as_ref().clone().sub(file_name)?,
                QBResourceKind::from_file_type(file_type),
            );

            entries.push(resource);
        }

        Ok(entries)
    }

    /// Reads a directory synchronously
    ///
    /// Stops processing entries once an error occured and returns this error.
    pub fn read_dir_sync(&self, path: impl AsRef<QBPath>) -> QBFSResult<Vec<QBResource>> {
        let fspath = self.fspath(&path);

        let mut entries = Vec::new();
        let mut iter = std::fs::read_dir(fspath)?;
        while let Some(entry) = iter.next() {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let file_name = Self::_str(entry.file_name())?;

            let resource = QBResource::new(
                path.as_ref().clone().sub(file_name)?,
                QBResourceKind::from_file_type(file_type),
            );

            entries.push(resource);
        }

        Ok(entries)
    }

    /// Read a path asynchronously
    pub async fn read(&self, path: impl AsRef<QBPath>) -> QBFSResult<Vec<u8>> {
        Ok(tokio::fs::read(self.fspath(path)).await?)
    }

    /// Read a path synchronously
    pub fn read_sync(&self, path: impl AsRef<QBPath>) -> QBFSResult<Vec<u8>> {
        Ok(std::fs::read(self.fspath(path))?)
    }

    /// Write to a path asynchronously
    pub async fn write(
        &self,
        path: impl AsRef<QBPath>,
        contents: impl AsRef<[u8]>,
    ) -> QBFSResult<()> {
        tokio::fs::write(self.fspath(path), contents).await?;
        Ok(())
    }

    /// Write to a path synchronously
    pub fn write_sync(
        &self,
        path: impl AsRef<QBPath>,
        contents: impl AsRef<[u8]>,
    ) -> QBFSResult<()> {
        std::fs::write(self.fspath(path), contents)?;
        Ok(())
    }

    /// Returns the path to the given resource on this filesystem.
    pub fn fspath(&self, resource: impl AsRef<QBPath>) -> PathBuf {
        resource.as_ref().to_path(self.root_str.as_str())
    }

    pub fn parse(&self, path: impl AsRef<str>) -> QBFSResult<QBPath> {
        Ok(QBPath::parse(self.root_str.as_str(), path)?)
    }

    /// Utility for converting an osstring into a string
    #[inline]
    fn _str(osstring: OsString) -> QBFSResult<String> {
        osstring
            .into_string()
            .map_err(|str| QBFSError::OsString(str))
    }
}
