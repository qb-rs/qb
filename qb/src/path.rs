use std::{fmt, path::PathBuf};

use bitcode::{Decode, Encode};
use thiserror::Error;

use crate::QBResource;

pub mod qbpaths {
    use lazy_static::lazy_static;

    use super::QBPath;

    lazy_static! {
        pub static ref ROOT: QBPath = unsafe { QBPath::new("") };
        pub static ref INTERNAL: QBPath = unsafe { QBPath::new(".qb") };
        pub static ref INTERNAL_CHANGELOG: QBPath = unsafe { QBPath::new(".qb/changelog") };
        pub static ref INTERNAL_FILETREE: QBPath = unsafe { QBPath::new(".qb/filetree") };
        pub static ref INTERNAL_COMMON: QBPath = unsafe { QBPath::new(".qb/common") };
    }
}

#[derive(Error, Debug)]
pub enum QBPathError {
    #[error("path exceeds maximum number of segments {0}")]
    MaxSegsExceeded(usize),
    #[error("directory traversal detected")]
    TraversalDetected,
}

pub type QBPathResult<T> = Result<T, QBPathError>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QBPath(String);

impl fmt::Display for QBPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "qb://{}", self.0)
    }
}

impl fmt::Debug for QBPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QBPath(qb://{})", self.0)
    }
}

impl AsRef<QBPath> for QBPath {
    fn as_ref(&self) -> &QBPath {
        self
    }
}

impl QBPath {
    const MAX_SEGS: usize = 50;

    /// Do not sanitize path and return QBPath instance
    ///
    /// Be careful when using this method, as it could lead
    /// to path traversal attacks.
    #[inline]
    pub unsafe fn new(path: impl Into<String>) -> Self {
        QBPath(path.into())
    }

    /// Sanitize path and return QBPath instance
    pub fn try_from(path: impl AsRef<str>) -> QBPathResult<Self> {
        if path.as_ref().starts_with("/") {
            return Err(QBPathError::TraversalDetected);
        }

        Ok(Self(Self::clean(path)?))
    }

    /// Convert this path into a resource
    ///
    /// Alias for QBResource::new_file(self)
    #[inline]
    pub fn file(self) -> QBResource {
        QBResource::new_file(self)
    }

    /// Convert this path into a resource
    ///
    /// Alias for QBResource::new_dir(self)
    #[inline]
    pub fn dir(self) -> QBResource {
        QBResource::new_dir(self)
    }

    /// Convert this path into a resource
    ///
    /// Alias for QBResource::new_symlink(self)
    #[inline]
    pub fn symlink(self) -> QBResource {
        QBResource::new_symlink(self)
    }

    /// Checks whether this path is parent of other
    #[inline]
    pub fn is_parent(&self, other: &QBPath) -> bool {
        other.0.starts_with(&self.0)
    }

    /// Checks whether this path is child of other
    ///
    /// Alias for other.is_parent(self)
    #[inline]
    pub fn is_child(&self, other: &QBPath) -> bool {
        other.is_parent(self)
    }

    /// Enter a relative path
    ///
    /// This allows the new path to be outside of the previous
    /// path if the target is something like "../abc".
    #[inline]
    pub fn rel(mut self, path: impl AsRef<str>) -> QBPathResult<Self> {
        self.0 = Self::clean(self.0 + "/" + path.as_ref())?;
        Ok(self)
    }

    /// Enter a substitute path
    ///
    /// This will throw an error if the new path lies outside
    /// of the previous path. [QBPathError::TraversalDetected]
    #[inline]
    pub fn sub(mut self, path: impl AsRef<str>) -> QBPathResult<Self> {
        if !self.0.is_empty() {
            self.0 += "/";
        }
        self.0 += Self::clean(path)?.as_str();
        Ok(self)
    }

    /// Clean and parse the path string
    ///
    /// If absolute, this will try to slice of the root path and if
    /// path does not start with the root path, an error is returned.
    pub fn parse(root: &str, path: impl AsRef<str>) -> QBPathResult<QBPath> {
        let path = Self::clean(path)?;

        if Self::is_absolute(&path) {
            if !path.starts_with(root) {
                return Err(QBPathError::TraversalDetected);
            }

            // slice off the base path
            return Ok(QBPath(path[root.len()..].to_string()));
        }

        Ok(QBPath(path))
    }

    /// Returns whether the given string refers to an absolute path
    #[inline]
    pub fn is_absolute(path: impl AsRef<str>) -> bool {
        path.as_ref().starts_with("/")
    }

    /// Return the segments of this path
    #[inline]
    pub fn segments<'a>(&'a self) -> std::str::Split<'a, &str> {
        self.0.split("/")
    }

    /// Convert into string
    #[inline]
    pub fn to_string(&self, root: &str) -> String {
        let path = &self.0;
        format!("{root}{path}")
    }

    /// Convert into path
    #[inline]
    pub fn to_path(&self, root: &str) -> PathBuf {
        self.to_string(root).into()
    }

    /// Cleans the given path string
    ///
    /// TODO: testing
    /// TODO: windows
    /// TODO: path escapes
    pub fn clean(path: impl AsRef<str>) -> QBPathResult<String> {
        let segs = path
            .as_ref()
            .splitn(Self::MAX_SEGS, "/")
            .collect::<Vec<_>>();
        if segs.len() == Self::MAX_SEGS {
            return Err(QBPathError::MaxSegsExceeded(Self::MAX_SEGS));
        }

        // Path stack
        let mut stack = Vec::new();
        for (i, seg) in segs.into_iter().enumerate() {
            if i != 0 && seg.is_empty() || seg == "." {
                continue;
            }

            if seg == ".." {
                stack.pop().ok_or(QBPathError::TraversalDetected)?;
                continue;
            }

            // TODO: sanitize segment, remove escapes etc.

            stack.push(seg);
        }

        Ok(stack.join("/"))
    }
}
