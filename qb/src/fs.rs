pub mod table;
pub mod tree;
pub mod wrapper;

use std::{ffi::OsString, path::Path};

use thiserror::Error;
use tracing::warn;

use table::{QBFSChange, QBFSChangeKind, QBFileTable};
use tree::QBFileTree;
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
    pub tree: QBFileTree,
    pub table: QBFileTable,
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
    /// not through the apply method.
    pub fn notify_change(&mut self, change: QBFSChange) {
        let kind = change.kind;
        let resource = change.resource;
        match kind {
            QBFSChangeKind::Update { hash, .. } => {
                self.tree.update(&resource, hash);
            }
            QBFSChangeKind::Delete => {
                self.tree.delete(&resource);
            }
            QBFSChangeKind::Create => {
                self.tree.create(&resource);
            }
        }
    }

    /// Applies changes to this filesystem.
    ///
    /// !!!Use with caution, Safety checks not yet implemented!!!
    ///
    /// TODO: add caution checks
    pub async fn apply_changes(&mut self, changes: Vec<QBFSChange>) -> QBFSResult<()> {
        for change in changes {
            let kind = change.kind;
            let resource = change.resource;
            let contains = self.wrapper.contains(&resource).await;
            match kind {
                QBFSChangeKind::Update { contents, hash } => {
                    self.tree.update(&resource, hash);
                    self.wrapper.write(&resource, &contents).await.unwrap();
                }
                QBFSChangeKind::Delete => {
                    self.tree.delete(&resource);

                    if !contains {
                        warn!("fs: delete {}, but not found!", resource);
                        continue;
                    }

                    let fspath = self.wrapper.fspath(&resource);
                    match resource.is_dir() {
                        true => tokio::fs::remove_dir_all(&fspath).await?,
                        false => tokio::fs::remove_file(&fspath).await?,
                    };
                }
                QBFSChangeKind::Create => {
                    self.tree.create(&resource);

                    if contains {
                        warn!("fs: create {}, but exists!", resource);
                        continue;
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
            };
        }

        Ok(())
    }

    /// Compare the entry on the filesystem to the entry stored
    pub async fn diff(&self, path: impl AsRef<QBPath>) -> QBFSResult<Option<QBFileDiff>> {
        let contents = self.wrapper.read(&path).await?;
        let hash = QBHash::compute(&contents);

        let file = self.tree.get(&path).ok_or(QBFSError::NotFound)?.file();

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
    pub async fn save_tree(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_FILETREE.as_ref(), &self.tree)
            .await
    }

    /// TODO: doc
    pub async fn save_table(&self) -> QBFSResult<()> {
        self.wrapper
            .save(qbpaths::INTERNAL_FILETABLE.as_ref(), &self.table)
            .await
    }

    pub async fn save(&self) -> QBFSResult<()> {
        self.save_changelog().await?;
        self.save_devices().await?;
        self.save_tree().await?;
        self.save_table().await
    }
}
