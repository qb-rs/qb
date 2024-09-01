//! This module contains stuff related to the local filesystem.

// TODO: figure out, whether this really belongs in the core crate

pub mod table;
pub mod tree;
pub mod wrapper;

use std::{ffi::OsString, path::Path};

use thiserror::Error;
use tracing::{debug, info, warn};

use table::QBFileTable;
use tree::{QBFileTree, TreeFile};
use wrapper::QBFSWrapper;

use crate::{
    change::{QBChange, QBChangeKind, QBChangeMap},
    device::QBDeviceTable,
    diff::QBDiff,
    hash::QBHash,
    ignore::{QBIgnoreMap, QBIgnoreMapBuilder},
    path::{
        qbpaths::{
            self, INTERNAL_CHANGEMAP, INTERNAL_DEVICES, INTERNAL_FILETABLE, INTERNAL_FILETREE,
            INTERNAL_IGNORE,
        },
        QBPath, QBPathError, QBResource,
    },
};

/// struct describing an error that occured while dealing with the file system
#[derive(Error, Debug)]
pub enum Error {
    /// I/O error
    #[error("I/O error")]
    IO(#[from] std::io::Error),
    /// struct encoding/decoding error
    #[error("bitcode error")]
    Bitcode(#[from] bitcode::Error),
    /// path parsing error
    #[error("path error")]
    Path(#[from] QBPathError),
    /// string conversion error
    #[error("osstring conversion error: {0:?}")]
    OsString(OsString),
    /// file not found in filetree error
    #[error("file tree: not found")]
    NotFound,
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// struct describing a change that can be directly applied to the file system
///
/// this differs from [QBChange], as the diff stored in UpdateText
/// is already expanded, so no further processing is required.
#[derive(Debug)]
pub struct QBFSChange {
    /// the resource this change affects
    pub resource: QBResource,
    /// the kind of change
    pub kind: QBFSChangeKind,
}

/// enum describing the different kinds of changes
#[derive(Debug)]
pub enum QBFSChangeKind {
    /// update a file
    Update {
        /// the file content
        content: Vec<u8>,
        /// the hash of the content
        hash: QBHash,
    },
    /// create a file or directory
    Create,
    /// delete a file or directory
    Delete,
    /// rename a file or directory
    Rename {
        /// location
        from: QBPath,
    },
    /// copy a file or directory
    Copy {
        /// location
        from: QBPath,
    },
}

/// struct describing a text or binary diff of a file
#[derive(Debug)]
pub enum QBFileDiff {
    /// binary file
    Binary(Vec<u8>),
    /// text file
    Text(QBDiff),
}

/// struct representing a local file system
pub struct QBFS {
    /// the file system wrapper
    pub wrapper: QBFSWrapper,
    /// the file tree
    pub tree: QBFileTree,
    /// the file table
    pub table: QBFileTable,
    /// the changemap
    pub changemap: QBChangeMap,
    /// the devices
    pub devices: QBDeviceTable,
    /// the ignore builder
    pub ignore_builder: QBIgnoreMapBuilder,
    /// the ignore
    pub ignore: QBIgnoreMap,
}

impl QBFS {
    /// Initialize this file system
    pub async fn init(root: impl AsRef<Path>) -> Self {
        let wrapper = QBFSWrapper::new(root);
        wrapper.init().await.unwrap();

        let tree = wrapper.dload(INTERNAL_FILETREE.as_ref()).await;
        let table = wrapper.dload(INTERNAL_FILETABLE.as_ref()).await;
        let ignore_builder: QBIgnoreMapBuilder = wrapper.dload(INTERNAL_IGNORE.as_ref()).await;
        let ignore = ignore_builder.build(&table);
        let devices = wrapper.dload(INTERNAL_DEVICES.as_ref()).await;
        let changelog = wrapper.dload(INTERNAL_CHANGEMAP.as_ref()).await;

        debug!("loaded {}", ignore);

        Self {
            wrapper,
            tree,
            table,
            devices,
            changemap: changelog,
            ignore_builder,
            ignore,
        }
    }

    /// convert the given change to fs change
    pub fn to_fschanges(&mut self, changes: Vec<(QBResource, QBChange)>) -> Vec<QBFSChange> {
        // optimistic allocation
        let mut fschanges = Vec::with_capacity(changes.len());
        let mut source = None;
        for (resource, change) in changes {
            let kind = match &change.kind {
                QBChangeKind::Create => Some(QBFSChangeKind::Create),
                QBChangeKind::Delete => Some(QBFSChangeKind::Delete),
                QBChangeKind::UpdateBinary(content) => {
                    let hash = QBHash::compute(content);
                    Some(QBFSChangeKind::Update {
                        content: content.clone(),
                        hash,
                    })
                }
                QBChangeKind::UpdateText(diff) => {
                    let old = self.table.get(&diff.old_hash).to_string();
                    let contents = diff.apply(old);
                    let hash = QBHash::compute(&contents);
                    self.table.insert_hash(hash.clone(), contents.clone());
                    Some(QBFSChangeKind::Update {
                        content: contents.into(),
                        hash,
                    })
                }
                QBChangeKind::CopyFrom | QBChangeKind::RenameFrom => {
                    source = Some(resource.path.clone());
                    None
                }
                QBChangeKind::CopyTo => Some(QBFSChangeKind::Copy {
                    from: source.clone().unwrap(),
                }),
                QBChangeKind::RenameTo => Some(QBFSChangeKind::Rename {
                    from: source.clone().unwrap(),
                }),
            };

            if let Some(kind) = kind {
                fschanges.push(QBFSChange {
                    resource: resource.clone(),
                    kind,
                });
            }
        }

        fschanges
    }

    /// Process changes that were applied to the underlying file system
    pub fn notify_changes<'a>(&mut self, changes: impl Iterator<Item = &'a QBFSChange>) {
        for change in changes {
            self.notify_change(change);
        }
    }

    /// Applies changes to this filesystem.
    ///
    /// !!!Use with caution, Safety checks not yet implemented!!!
    pub async fn apply_changes(&mut self, changes: Vec<QBFSChange>) -> Result<()> {
        for change in changes {
            self.apply_change(change).await?;
        }

        Ok(())
    }

    /// Process change that was applied to the underlying file system
    pub fn notify_change(&mut self, change: &QBFSChange) {
        self.tree.notify_change(change);
        self.ignore_builder.notify_change(change);
        self.ignore.notify_change(change);
    }

    /// Applies a single change to this filesystem.
    ///
    /// !!!Use with caution, Safety checks not yet implemented!!!
    pub async fn apply_change(&mut self, change: QBFSChange) -> Result<()> {
        self.notify_change(&change);

        let kind = change.kind;
        let resource = change.resource;
        let contains = self.wrapper.contains(&resource).await;
        match kind {
            QBFSChangeKind::Update { content, .. } => {
                self.wrapper.write(&resource, &content).await.unwrap();
            }
            QBFSChangeKind::Delete => {
                if !contains {
                    // Think about returning an error?
                    warn!("fs: delete {}, but not found!", resource);
                    return Ok(());
                }

                let fspath = self.wrapper.fspath(&resource);
                match resource.is_dir() {
                    true => tokio::fs::remove_dir_all(&fspath).await?,
                    false => tokio::fs::remove_file(&fspath).await?,
                };
            }
            QBFSChangeKind::Create => {
                if contains {
                    // Think about returning an error?
                    warn!("fs: create {}, but exists!", resource);
                    return Ok(());
                }

                let fspath = self.wrapper.fspath(&resource);
                match resource.is_dir() {
                    true => {
                        tokio::fs::create_dir_all(fspath).await?;
                    }
                    false => {
                        drop(tokio::fs::File::create(fspath).await?);
                    }
                };
            }

            QBFSChangeKind::Copy { from } => {
                self.wrapper.copy(from, resource).await?;
            }
            QBFSChangeKind::Rename { from } => {
                // TODO: safe overwrites
                self.wrapper.rename(from, resource).await?;
            }
        }

        Ok(())
    }

    /// Compare the entry on the filesystem to the entry stored
    pub async fn diff(&mut self, path: impl AsRef<QBPath>) -> Result<Option<QBFileDiff>> {
        let contents = self.wrapper.read(&path).await?;
        let hash = QBHash::compute(&contents);

        info!("TREE: {} - {}", path.as_ref(), self.tree);
        let file = self
            .tree
            .get_or_insert_mut(&path, TreeFile::default().into())
            .unwrap()
            .file_mut();

        // no changes, nothing to do
        if file.hash == hash {
            return Ok(None);
        }

        match simdutf8::basic::from_utf8(&contents) {
            Ok(new) => {
                let new = new.to_string();
                let old = self.table.get(&file.hash).to_string();
                self.table.insert_hash(hash.clone(), new.clone());
                file.hash = hash;

                Ok(Some(QBFileDiff::Text(QBDiff::compute(old, new))))
            }
            Err(_) => Ok(Some(QBFileDiff::Binary(contents))),
        }
    }

    /// Save changelog to file system.
    pub async fn save_changelog(&self) -> Result<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_CHANGEMAP.as_ref(), &self.changemap)
            .await
    }

    /// Save devices to file system.
    pub async fn save_devices(&self) -> Result<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_DEVICES.as_ref(), &self.devices)
            .await
    }

    /// Save file tree to file system.
    pub async fn save_tree(&self) -> Result<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_FILETREE.as_ref(), &self.tree)
            .await
    }

    /// Save file table to file system.
    pub async fn save_table(&self) -> Result<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_FILETABLE.as_ref(), &self.table)
            .await
    }

    /// Save ignore builder to file system.
    pub async fn save_ignore(&self) -> Result<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_IGNORE.as_ref(), &self.ignore_builder)
            .await
    }

    /// Save state to file system.
    pub async fn save(&self) -> Result<()> {
        self.save_changelog().await?;
        self.save_devices().await?;
        self.save_tree().await?;
        self.save_ignore().await?;
        self.save_table().await
    }
}
