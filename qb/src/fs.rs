use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use bitcode::{DecodeOwned, Encode};
use thiserror::Error;
use tracing::warn;

use crate::{
    qbpaths, QBChange, QBChangeKind, QBChangelog, QBDevices, QBDiff, QBFileTree, QBHash,
    QBIgnoreMap, QBIgnoreMapBuilder, QBPath, QBPathError, QBResource, QBResourceKind, TreeDir,
    TreeFile,
};

#[derive(Error, Debug)]
pub enum QBFSError {
    #[error("I/O error")]
    IO(#[from] std::io::Error),
    #[error("bitcode error")]
    Bitcode(#[from] bitcode::Error),
    #[error("path error")]
    Path(#[from] QBPathError),
    #[error("osstring conversion error: {0:?}")]
    OsString(OsString),
    #[error("file tree: not found")]
    NotFound,
}

pub type QBFSResult<T> = Result<T, QBFSError>;

pub enum QBFileDiff {
    Binary(Vec<u8>),
    Text(QBDiff),
}

pub struct QBFS {
    pub wrapper: QBFSWrapper,
    pub filetree: QBFileTree,
    pub changelog: QBChangelog,
    pub devices: QBDevices,
    pub ignore_builder: QBIgnoreMapBuilder,
    pub ignore: QBIgnoreMap,
}

impl QBFS {
    /// Initialize this file system
    pub async fn init(root: impl AsRef<Path>) -> Self {
        let mut wrapper = QBFSWrapper::new(root);
        wrapper.init().await.unwrap();

        let filetree = wrapper
            .load_or_default::<QBFileTree>(qbpaths::INTERNAL_FILETREE.as_ref())
            .await;
        let ignore_builder = wrapper
            .load_or_default::<QBIgnoreMapBuilder>(qbpaths::INTERNAL_IGNORE.as_ref())
            .await;
        let ignore = ignore_builder.build(&filetree);
        let devices = wrapper
            .load_or_default::<QBDevices>(qbpaths::INTERNAL_DEVICES.as_ref())
            .await;
        let changelog = wrapper
            .load_or_default::<QBChangelog>(qbpaths::INTERNAL_CHANGELOG.as_ref())
            .await;

        Self {
            wrapper,
            filetree,
            devices,
            changelog,
            ignore_builder,
            ignore,
        }
    }

    /// Process changes that were applied to the underlying file system
    /// not through the apply method.
    pub fn notify_change(&mut self, change: &QBChange) {
        let kind = &change.kind;
        let resource = &change.resource;
        match &kind {
            QBChangeKind::Diff { diff } => {
                // remove unnessecary clone
                assert!(resource.is_file());

                let file = self.filetree.get_mut(resource).unwrap().file_mut();
                let old = std::mem::take(&mut file.contents);
                let new = diff.apply(old);
                file.hash = QBHash::compute(&new);
                file.contents = new;
            }
            QBChangeKind::Change { contents } => {
                assert!(resource.is_file());

                let file = self.filetree.get_mut(resource).unwrap().file_mut();
                file.hash = QBHash::compute(&contents);
                file.contents = Default::default();
            }
            QBChangeKind::Delete => {
                self.filetree.remove(&resource);
            }
            QBChangeKind::Create => {
                match resource.is_dir() {
                    true => {
                        self.filetree.insert(resource, TreeDir::default());
                    }
                    false => {
                        self.filetree.insert(resource, TreeFile::default());
                    }
                };
            }
        }
    }

    /// Applies changes to this filesystem.
    ///
    /// !!!Use with caution, Safety checks not yet implemented!!!
    ///
    /// TODO: add caution checks
    pub async fn apply_changes(&mut self, changes: &Vec<QBChange>) -> QBFSResult<()> {
        for change in changes {
            let kind = &change.kind;
            let resource = &change.resource;
            let contains = self.wrapper.contains(&change.resource).await;
            match &kind {
                QBChangeKind::Diff { diff } => {
                    // TODO: remove unnessecary clone
                    assert!(resource.is_file());

                    let file = self.filetree.get_mut(resource).unwrap().file_mut();
                    let old = std::mem::take(&mut file.contents);
                    let new = diff.apply(old);
                    file.hash = QBHash::compute(&new);
                    file.contents = new.clone();

                    self.wrapper.write(resource, &new).await.unwrap();
                }
                QBChangeKind::Change { contents } => {
                    assert!(resource.is_file());

                    let file = self.filetree.get_mut(resource).unwrap().file_mut();
                    file.hash = QBHash::compute(&contents);
                    file.contents = Default::default();

                    self.wrapper.write(resource, &contents).await.unwrap();
                }
                QBChangeKind::Delete => {
                    if !contains {
                        warn!("delete requested but resource {} not found!", resource);
                        continue;
                    }

                    self.filetree.remove(&resource);
                    let fspath = self.wrapper.fspath(&change.resource);
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

                    let fspath = self.wrapper.fspath(&change.resource);
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
    pub async fn diff(&self, path: impl AsRef<QBPath>) -> QBFSResult<Option<QBFileDiff>> {
        let contents = self.wrapper.read(&path).await?;
        let hash = QBHash::compute(&contents);

        let file = self.filetree.get(&path).ok_or(QBFSError::NotFound)?.file();

        // no changes, nothing to do
        if file.hash == hash {
            return Ok(None);
        }

        match simdutf8::basic::from_utf8(&contents) {
            Ok(new) => {
                let new = new.to_string();
                let old = file.contents.clone();

                Ok(Some(QBFileDiff::Text(QBDiff::compute(old, new))))
            }
            Err(_) => Ok(Some(QBFileDiff::Binary(contents))),
        }
    }

    /// TODO: doc
    pub async fn save_changelog(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_CHANGELOG.as_ref(), &self.changelog)
            .await
    }

    /// TODO: doc
    pub async fn save_devices(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_DEVICES.as_ref(), &self.devices)
            .await
    }

    /// TODO: doc
    pub async fn save_filetree(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_FILETREE.as_ref(), &self.filetree)
            .await
    }
}

pub struct QBFSWrapper {
    pub root: PathBuf,
    pub root_str: String,
}

impl QBFSWrapper {
    /// Create a new filesystem wrapper around a root path.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = std::path::absolute(root).unwrap();
        let mut root_str = root.to_str().unwrap().to_string();
        if root_str.ends_with("/") {
            root_str.pop();
        }

        Self { root_str, root }
    }

    /// Make sure the filesystem is properly setup.
    pub async fn init(&mut self) -> QBFSResult<()> {
        tokio::fs::create_dir_all(self.fspath(qbpaths::INTERNAL.as_ref())).await?;
        Ok(())
    }

    /// Load and decode from a path
    pub async fn load<'a, T: DecodeOwned>(&self, path: impl AsRef<QBPath>) -> QBFSResult<T> {
        Ok(bitcode::decode(&self.read(path).await?)?)
    }

    /// Load and decode from a path
    ///
    /// returns the default value if an error is returned
    #[inline]
    pub async fn load_or_default<'a, T: DecodeOwned + Default>(
        &self,
        path: impl AsRef<QBPath>,
    ) -> T {
        self.load(path).await.unwrap_or(Default::default())
    }

    /// Encode and save to a path
    pub async fn save<'a, T: Encode>(&self, path: impl AsRef<QBPath>, item: &T) -> QBFSResult<()> {
        tokio::fs::write(self.fspath(path), bitcode::encode(item)).await?;
        Ok(())
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
