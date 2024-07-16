use std::{
    fmt, panic,
    path::{Path, PathBuf},
};

use bitcode::{Decode, Encode};

use thiserror::Error;

pub mod qbpaths {
    use lazy_static::lazy_static;

    use super::QBPath;

    lazy_static! {
        pub static ref ROOT: QBPath = unsafe { QBPath::new("/") };
        pub static ref INTERNAL: QBPath = unsafe { QBPath::new("/.qb") };
        pub static ref INTERNAL_CHANGELOG: QBPath = unsafe { QBPath::new("/.qb/changelog") };
        pub static ref INTERNAL_FILETREE: QBPath = unsafe { QBPath::new("/.qb/filetree") };
        pub static ref INTERNAL_FILETABLE: QBPath = unsafe { QBPath::new("/.qb/filetable") };
        pub static ref INTERNAL_IGNORE: QBPath = unsafe { QBPath::new("/.qb/ignore") };
        pub static ref INTERNAL_DEVICES: QBPath = unsafe { QBPath::new("/.qb/devices") };
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
        Ok(Self(Self::clean(path)?))
    }

    pub fn as_fspath(&self) -> &Path {
        Path::new(&self.0)
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
    pub fn is_parent(&self, other: impl AsRef<QBPath>) -> bool {
        other.as_ref().0.starts_with(&self.0)
    }

    /// Returns the parent path (if any)
    #[inline]
    pub fn parent(mut self) -> Option<Self> {
        let pos = self.0.chars().rev().position(|c| c == '/')?;
        let new_len = self.0.len() - pos - 1;
        self.0.truncate(new_len);
        Some(self)
    }

    /// Checks whether this path is child of other
    ///
    /// Alias for other.is_parent(self)
    #[inline]
    pub fn is_child(&self, other: impl AsRef<QBPath>) -> bool {
        other.as_ref().is_parent(self)
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
        self.0 += Self::clean(path)?.as_str();
        Ok(self)
    }

    /// Clean and parse the path string
    ///
    /// If absolute, this will try to slice of the root path and if
    /// path does not start with the root path, an error is returned.
    pub fn parse(root: &str, path: impl AsRef<str>) -> QBPathResult<QBPath> {
        assert!(!root.ends_with("/"));

        // TODO: windows and shit
        let mut path = path.as_ref();
        if path.starts_with(root) {
            path = &path[root.len()..];
        }
        let path = Self::clean(path)?;

        Ok(QBPath(path))
    }

    /// Return the segments of this path
    #[inline]
    pub fn segments<'a>(&'a self) -> std::iter::Skip<std::str::Split<'_, &str>> {
        // skip the first segment, as it is always empty
        self.0.split("/").skip(1)
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
        for seg in segs.into_iter() {
            if seg.is_empty() || seg == "." {
                continue;
            }

            if seg == ".." {
                stack.pop().ok_or(QBPathError::TraversalDetected)?;
                continue;
            }

            // TODO: sanitize segment, remove escapes etc.

            stack.push(seg);
        }

        Ok("/".to_string() + &stack.join("/"))
    }
}

#[derive(Encode, Decode, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QBResource {
    pub path: QBPath,
    pub kind: QBResourceKind,
}

#[derive(Encode, Decode, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QBResourceKind {
    File,
    Dir,
    Symlink,
}

impl QBResourceKind {
    /// Returns the resource kind from the metadata
    #[inline]
    pub fn from_file_type(file_type: std::fs::FileType) -> QBResourceKind {
        if file_type.is_file() {
            return QBResourceKind::File;
        }

        if file_type.is_dir() {
            return QBResourceKind::Dir;
        }

        if file_type.is_symlink() {
            return QBResourceKind::Symlink;
        }

        panic!("invalid file type: {:?}", file_type);
    }

    /// Returns the resource kind from the metadata
    #[inline]
    pub fn from_metadata(metadata: std::fs::Metadata) -> QBResourceKind {
        Self::from_file_type(metadata.file_type())
    }

    /// Checks whether this is a directory
    #[inline]
    pub fn is_dir(&self) -> bool {
        matches!(self, QBResourceKind::Dir)
    }

    /// Checks whether this is a file
    #[inline]
    pub fn is_file(&self) -> bool {
        matches!(self, QBResourceKind::File)
    }

    /// Checks whether this is a symlink
    #[inline]
    pub fn is_symlink(&self) -> bool {
        matches!(self, QBResourceKind::Symlink)
    }

    /// Checks whether this is of given file type
    #[inline]
    pub fn is_file_type(&self, file_type: std::fs::FileType) -> bool {
        match self {
            QBResourceKind::File => file_type.is_file(),
            QBResourceKind::Dir => file_type.is_dir(),
            QBResourceKind::Symlink => file_type.is_symlink(),
        }
    }

    /// Checks whether this matches the given metadata
    #[inline]
    pub fn is_metadata(&self, metadata: std::fs::Metadata) -> bool {
        Self::is_file_type(&self, metadata.file_type())
    }
}

impl fmt::Display for QBResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            QBResourceKind::File => write!(f, "file->")?,
            QBResourceKind::Dir => write!(f, "dir->")?,
            QBResourceKind::Symlink => write!(f, "symlink->")?,
        };
        fmt::Display::fmt(&self.path, f)
    }
}

impl AsRef<QBPath> for QBResource {
    fn as_ref(&self) -> &QBPath {
        &self.path
    }
}

impl QBResource {
    /// Creates a new QBResource instance
    #[inline]
    pub fn new(path: QBPath, kind: QBResourceKind) -> Self {
        Self { path, kind }
    }

    /// Creates a new QBResource instance
    ///
    /// Alias for Self::new(path, QBResourceKind::File)
    #[inline]
    pub fn new_file(path: QBPath) -> Self {
        Self::new(path, QBResourceKind::File)
    }

    /// Creates a new QBResource instance
    ///
    /// Alias for Self::new(path, QBResourceKind::Dir)
    #[inline]
    pub fn new_dir(path: QBPath) -> Self {
        Self::new(path, QBResourceKind::Dir)
    }

    /// Creates a new QBResource instance
    ///
    /// Alias for Self::new(path, QBResourceKind::Symlink)
    #[inline]
    pub fn new_symlink(path: QBPath) -> Self {
        Self::new(path, QBResourceKind::Symlink)
    }

    /// Parses path and creates a new QBResource instance
    ///
    /// If the path ends with a slash, a directory resource
    /// is returned. Otherwise a file resource is returned.
    #[deprecated(note = "use QBPath::parse()?.file() instead")]
    pub fn try_from(value: impl AsRef<str>) -> QBPathResult<Self> {
        let path = QBPath::try_from(&value)?;
        if value.as_ref().ends_with("/") {
            Ok(Self::new(path, QBResourceKind::Dir))
        } else {
            Ok(Self::new(path, QBResourceKind::File))
        }
    }

    /// Checks whether this resource is of given file type
    ///
    /// Alias for self.kind.is_file_type(file_type)
    #[inline]
    pub fn is_file_type(&self, file_type: std::fs::FileType) -> bool {
        self.kind.is_file_type(file_type)
    }

    /// Checks whether this resource matches the given metadata
    ///
    /// Alias for self.kind.is_metadata(metadata)
    #[inline]
    pub fn is_metadata(&self, metadata: std::fs::Metadata) -> bool {
        self.kind.is_metadata(metadata)
    }

    /// Checks whether this resource is a directory
    ///
    /// Alias for self.kind.is_dir()
    #[inline]
    pub fn is_dir(&self) -> bool {
        self.kind.is_dir()
    }

    /// Checks whether this resource is a file
    ///
    /// Alias for self.kind.is_file()
    #[inline]
    pub fn is_file(&self) -> bool {
        self.kind.is_file()
    }

    /// Checks whether this resource is a symlink
    ///
    /// Alias for self.kind.is_symlink()
    #[inline]
    pub fn is_symlink(&self) -> bool {
        self.kind.is_symlink()
    }
}
