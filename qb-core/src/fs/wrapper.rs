//! A file system wrapper wraps the local file system and implements
//! functions like read, write or delete.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use bitcode::{DecodeOwned, Encode};

use crate::path::{qbpaths, QBPath, QBResource, QBResourceKind};

use super::{QBFSError, QBFSResult};

/// struct which wraps the local file system
#[derive(Clone)]
pub struct QBFSWrapper {
    /// the root path
    pub root: PathBuf,
    /// the root path (as a string)
    pub root_str: String,
}

impl QBFSWrapper {
    /// Create a new filesystem wrapper around a root path.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = std::path::absolute(root).unwrap();
        let mut root_str = root.to_str().unwrap().to_string();
        if root_str.ends_with('/') {
            root_str.pop();
        }

        Self { root_str, root }
    }

    /// Make sure the filesystem is properly setup.
    pub async fn init(&self) -> QBFSResult<()> {
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
    pub async fn dload<T: DecodeOwned + Default>(&self, path: impl AsRef<QBPath>) -> T {
        self.load(path).await.unwrap_or(Default::default())
    }

    /// Encode and save to a path
    pub async fn save(&self, path: impl AsRef<QBPath>, item: &impl Encode) -> QBFSResult<()> {
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

    /// Reads a directory asynchronously
    ///
    /// Stops processing entries once an error occured and returns this error.
    pub async fn read_dir(&self, path: impl AsRef<QBPath>) -> QBFSResult<Vec<QBResource>> {
        let fspath = self.fspath(&path);

        let mut entries = Vec::new();
        let mut iter = tokio::fs::read_dir(fspath).await?;
        while let Some(entry) = iter.next_entry().await? {
            let file_type = entry.file_type().await?;
            let file_name = Self::str(entry.file_name())?;

            let resource = QBResource::new(
                path.as_ref().clone().substitue(file_name)?,
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

    /// Write to a path asynchronously
    pub async fn write(
        &self,
        path: impl AsRef<QBPath>,
        contents: impl AsRef<[u8]>,
    ) -> QBFSResult<()> {
        tokio::fs::write(self.fspath(path), contents).await?;
        Ok(())
    }

    /// Returns the path to the given resource on this filesystem.
    pub fn fspath(&self, resource: impl AsRef<QBPath>) -> PathBuf {
        resource.as_ref().get_fspath(self.root_str.as_str())
    }

    /// Parse a local fs path to a quixbyte path.
    pub fn parse(&self, path: impl AsRef<Path>) -> QBFSResult<QBPath> {
        Ok(QBPath::parse(
            self.root_str.as_str(),
            Self::strref(path.as_ref().as_os_str())?,
        )?)
    }

    /// Parse a local fs path to a quixbyte path.
    pub fn parse_str(&self, path: impl AsRef<str>) -> QBFSResult<QBPath> {
        Ok(QBPath::parse(self.root_str.as_str(), path)?)
    }

    /// Utility for converting an osstring into a string
    #[inline]
    fn str(osstring: OsString) -> QBFSResult<String> {
        osstring.into_string().map_err(QBFSError::OsString)
    }

    /// Utility for converting an osstr into a str
    #[inline]
    fn strref(osstring: &OsStr) -> QBFSResult<&str> {
        osstring
            .to_str()
            .ok_or_else(|| QBFSError::OsString(osstring.to_owned()))
    }
}
