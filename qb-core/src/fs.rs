//! This module contains stuff related to the local filesystem.

pub mod table;
pub mod tree;
pub mod wrapper;

use std::{ffi::OsString, path::Path};

use thiserror::Error;
use tracing::warn;

use table::{QBFSChange, QBFSChangeKind, QBFileTable};
use tree::{QBFileTree, TreeFile};
use wrapper::QBFSWrapper;

use crate::{
    change::log::QBChangelog,
    common::{
        diff::QBDiff,
        hash::QBHash,
        ignore::{QBIgnoreMap, QBIgnoreMapBuilder},
        resource::{qbpaths, QBPath, QBPathError},
    },
    interface::QBDevices,
};

/// struct describing an error that occured while dealing with the file system
#[derive(Error, Debug)]
pub enum QBFSError {
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

pub(crate) type QBFSResult<T> = Result<T, QBFSError>;

/// struct describing a text or binary diff of a file
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
    /// the changelog
    pub changelog: QBChangelog,
    /// the devices
    pub devices: QBDevices,
    /// the ignore builder
    pub ignore_builder: QBIgnoreMapBuilder,
    /// the ignore
    pub ignore: QBIgnoreMap,
}

impl QBFS {
    /// Initialize this file system
    pub async fn init(root: impl AsRef<Path>) -> Self {
        let mut wrapper = QBFSWrapper::new(root);
        wrapper.init().await.unwrap();

        let tree = wrapper
            .load_or_default::<QBFileTree>(qbpaths::INTERNAL_FILETREE.as_ref())
            .await;
        let table = wrapper
            .load_or_default::<QBFileTable>(qbpaths::INTERNAL_FILETABLE.as_ref())
            .await;
        let ignore_builder = wrapper
            .load_or_default::<QBIgnoreMapBuilder>(qbpaths::INTERNAL_IGNORE.as_ref())
            .await;
        let ignore = ignore_builder.build(&table);
        let devices = wrapper
            .load_or_default::<QBDevices>(qbpaths::INTERNAL_DEVICES.as_ref())
            .await;
        let changelog = wrapper
            .load_or_default::<QBChangelog>(qbpaths::INTERNAL_CHANGELOG.as_ref())
            .await;

        println!("loaded {}", ignore);

        Self {
            wrapper,
            tree,
            table,
            devices,
            changelog,
            ignore_builder,
            ignore,
        }
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
    pub async fn apply_changes(&mut self, changes: Vec<QBFSChange>) -> QBFSResult<()> {
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
    pub async fn apply_change(&mut self, change: QBFSChange) -> QBFSResult<()> {
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
        }

        Ok(())
    }

    /// Compare the entry on the filesystem to the entry stored
    pub async fn diff(&mut self, path: impl AsRef<QBPath>) -> QBFSResult<Option<QBFileDiff>> {
        let contents = self.wrapper.read(&path).await?;
        let hash = QBHash::compute(&contents);

        let file = self
            .tree
            .get_or_insert(&path, TreeFile::default().into())
            .unwrap()
            .file();

        // no changes, nothing to do
        if file.hash == hash {
            return Ok(None);
        }

        match simdutf8::basic::from_utf8(&contents) {
            Ok(new) => {
                let new = new.to_string();
                let old = self.table.get(&file.hash).to_string();

                Ok(Some(QBFileDiff::Text(QBDiff::compute(old, new))))
            }
            Err(_) => Ok(Some(QBFileDiff::Binary(contents))),
        }
    }

    /// Save changelog to file system.
    pub async fn save_changelog(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_CHANGELOG.as_ref(), &self.changelog)
            .await
    }

    /// Save devices to file system.
    pub async fn save_devices(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_DEVICES.as_ref(), &self.devices)
            .await
    }

    /// Save file tree to file system.
    pub async fn save_tree(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_FILETREE.as_ref(), &self.tree)
            .await
    }

    /// Save file table to file system.
    pub async fn save_table(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_FILETABLE.as_ref(), &self.table)
            .await
    }

    /// Save ignore builder to file system.
    pub async fn save_ignore(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_IGNORE.as_ref(), &self.ignore_builder)
            .await
    }

    /// Save state to file system.
    pub async fn save(&self) -> QBFSResult<()> {
        self.save_changelog().await?;
        self.save_devices().await?;
        self.save_tree().await?;
        self.save_ignore().await?;
        self.save_table().await
    }
}
