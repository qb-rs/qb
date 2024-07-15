use core::{fmt, panic};

use bitcode::{Decode, Encode};

use crate::{QBPath, QBPathResult};

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
