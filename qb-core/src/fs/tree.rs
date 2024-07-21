//! A filetree is a tree of files (hence the name) used for detecting
//! socalled offline changes, that is, when changes occur while the
//! application is not running and hence the file watchers cannot detect.

use core::{fmt, panic};
use std::{
    collections::{HashMap, HashSet},
    ops::{Index, IndexMut},
};

use bitcode::{Decode, Encode};
use itertools::Itertools;
use tracing::warn;

use crate::{
    change::QBChange,
    common::{
        hash::QBHash,
        resource::{qbpaths, QBPath, QBResource},
    },
};

use super::{
    table::{QBFSChange, QBFSChangeKind},
    wrapper::QBFSWrapper,
};

/// a node stored in a [QBFileTree]
#[derive(Encode, Decode)]
pub enum QBFileTreeNode {
    /// a directory
    Dir(TreeDir),
    /// a file
    File(TreeFile),
    /// unoccupied
    None,
}

impl Default for QBFileTreeNode {
    fn default() -> Self {
        Self::None
    }
}

impl QBFileTreeNode {
    /// check whether this is a file
    #[inline]
    pub fn is_file(&self) -> bool {
        matches!(self, QBFileTreeNode::File(..))
    }

    /// check whether this is a dir
    #[inline]
    pub fn is_dir(&self) -> bool {
        matches!(self, QBFileTreeNode::Dir(..))
    }

    /// check whether this is none
    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(self, QBFileTreeNode::None)
    }

    /// unwrap mutable file
    #[inline]
    pub fn file_mut(&mut self) -> &mut TreeFile {
        if let QBFileTreeNode::File(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }

    /// unwrap file
    #[inline]
    pub fn file(&self) -> &TreeFile {
        if let QBFileTreeNode::File(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }

    /// unwrap mutable dir
    #[inline]
    pub fn dir_mut(&mut self) -> &mut TreeDir {
        if let QBFileTreeNode::Dir(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }

    /// unwrap dir
    #[inline]
    pub fn dir(&self) -> &TreeDir {
        if let QBFileTreeNode::Dir(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }
}

/// A struct stored in the tree, which represents a directory
/// on the file system.
#[derive(Encode, Decode, Default)]
pub struct TreeDir {
    /// the contents of the directory
    pub contents: HashMap<String, usize>,
}

impl TreeDir {
    /// Get an index entry of the directory
    ///
    /// Alias for self.get(key)
    #[inline]
    pub fn get(&self, key: impl AsRef<str>) -> Option<usize> {
        self.contents.get(key.as_ref()).copied()
    }
}

impl From<TreeDir> for QBFileTreeNode {
    fn from(val: TreeDir) -> Self {
        QBFileTreeNode::Dir(val)
    }
}

/// A struct stored in the tree, which represents a file
/// on the file system.
///    
/// File's contents will only be cached if file is
/// 1. utf8 encoded/non binary and therefore diffable
/// 2. smaller than a certain maximum
#[derive(Encode, Decode)]
pub struct TreeFile {
    /// the hash of this file
    pub hash: QBHash,
}

impl Default for TreeFile {
    fn default() -> Self {
        Self {
            hash: QBHash::compute(vec![]),
        }
    }
}

impl From<TreeFile> for QBFileTreeNode {
    fn from(val: TreeFile) -> Self {
        QBFileTreeNode::File(val)
    }
}

#[derive(Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Compare {
    resource: QBResource,
    hash: QBHash,
}

/// a tree that stores a snapshot of the filesystem
/// used for detecting offline changes, that is when the
/// file watchers failed to detect changes due to the application
/// not running
#[derive(Encode, Decode)]
pub struct QBFileTree {
    pub(crate) arena: Vec<QBFileTreeNode>,
}

impl Default for QBFileTree {
    fn default() -> Self {
        Self {
            arena: vec![QBFileTreeNode::Dir(Default::default())],
        }
    }
}

impl<T: AsRef<QBPath>> Index<T> for QBFileTree {
    type Output = QBFileTreeNode;

    #[inline]
    fn index(&self, index: T) -> &Self::Output {
        let idx = self.index(index).unwrap();
        &self.arena[idx]
    }
}

impl<T: AsRef<QBPath>> IndexMut<T> for QBFileTree {
    #[inline]
    fn index_mut(&mut self, index: T) -> &mut Self::Output {
        let idx = self.index(index).unwrap();
        &mut self.arena[idx]
    }
}

impl fmt::Display for QBFileTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut stack = vec![(0, "", 0)];

        while let Some((ident, name, curr)) = stack.pop() {
            let prefix = match self.arena[curr] {
                QBFileTreeNode::Dir(_) => "dir->",
                QBFileTreeNode::File(_) => "file->",
                QBFileTreeNode::None => "init->",
            };

            writeln!(f, "{}{}{}", "    ".repeat(ident), prefix, name)?;
            if let QBFileTreeNode::Dir(dir) = &self.arena[curr] {
                for (name, value) in dir.contents.iter() {
                    stack.push((ident + 1, name, *value));
                }
            }
        }

        Ok(())
    }
}

impl QBFileTree {
    /// Process changes that were applied to the underlying file system
    pub fn notify_change(&mut self, change: &QBFSChange) {
        let kind = &change.kind;
        let resource = &change.resource;
        match kind {
            QBFSChangeKind::Update { hash, .. } => {
                self.update(resource, hash.clone());
            }
            QBFSChangeKind::Delete => {
                self.delete(resource);
            }
            QBFSChangeKind::Create => {
                self.create(resource);
            }
        }
    }

    fn get_tree(&self, path: impl AsRef<QBPath>) -> HashSet<Compare> {
        let idx = match self.index(&path) {
            Some(idx) => idx,
            None => return HashSet::new(),
        };

        self.arena[idx]
            .dir()
            .contents
            .iter()
            .map(|(k, v)| match &self.arena[*v] {
                QBFileTreeNode::File(f) => Compare {
                    resource: path.as_ref().clone().substitue(k.clone()).unwrap().file(),
                    hash: f.hash.clone(),
                },
                QBFileTreeNode::Dir(_) => Compare {
                    resource: path.as_ref().clone().substitue(k.clone()).unwrap().dir(),
                    hash: Default::default(),
                },
                _ => panic!("uninitialized"),
            })
            .collect()
    }

    async fn get_fs(&self, fswrapper: &QBFSWrapper, dir: impl AsRef<QBPath>) -> Vec<Compare> {
        let mut entries = Vec::new();
        let resources = match fswrapper.read_dir(dir).await {
            Ok(resources) => resources,
            Err(err) => {
                warn!("{}", err);
                return Vec::new();
            }
        };

        for resource in resources {
            let mut hash = Default::default();
            if resource.kind.is_file() {
                let contents = fswrapper.read(&resource).await.unwrap();
                QBHash::compute_mut(&mut hash, contents);
            }

            entries.push(Compare { hash, resource });
        }

        entries
    }

    /// TODO: ignores
    /// TODO: implement
    pub async fn walk(&self, fswrapper: &QBFSWrapper) -> Vec<QBChange> {
        let mut stack: Vec<QBPath> = vec![qbpaths::ROOT.clone()];
        let mut changes = HashSet::new();

        while let Some(curr) = stack.pop() {
            if qbpaths::INTERNAL.is_parent(&curr) {
                continue;
            }

            let compare_fs = self.get_fs(fswrapper, &curr).await;
            let mut compare_tree = self.get_tree(&curr);

            stack.extend(
                compare_tree
                    .iter()
                    .filter(|e| e.resource.is_dir())
                    .map(|e| e.resource.path.clone()),
            );

            for entry in compare_fs {
                if !compare_tree.remove(&entry) {
                    if entry.resource.is_dir() {
                        stack.push(entry.resource.path.clone());
                    }
                    changes.insert((true, entry));
                }
            }

            for entry in compare_tree {
                changes.insert((false, entry));
            }

            // let diff =
            //     similar::capture_diff_slices(similar::Algorithm::Myers, &compare_tree, &compare_fs);
        }

        let _same_hash = changes
            .iter()
            .duplicates_by(|e| &e.1.hash)
            .collect::<Vec<_>>();

        let _same_resource = changes
            .iter()
            .duplicates_by(|e| &e.1.resource)
            .collect::<Vec<_>>();

        println!("DIFF: {:#?}", changes);

        Vec::new()
    }

    /// Get an entry of this tree
    #[inline]
    pub fn get(&self, path: impl AsRef<QBPath>) -> Option<&QBFileTreeNode> {
        let idx = self.index(path)?;
        Some(&self.arena[idx])
    }

    /// Get a mutable entry of this tree
    #[inline]
    pub fn get_mut(&mut self, path: impl AsRef<QBPath>) -> Option<&mut QBFileTreeNode> {
        let idx = self.index(path)?;
        Some(&mut self.arena[idx])
    }

    /// Get an entry of this tree
    #[inline]
    pub fn get_or_insert(
        &mut self,
        path: impl AsRef<QBPath>,
        default: QBFileTreeNode,
    ) -> Option<&QBFileTreeNode> {
        let idx = self.get_or_create_ptr(path)?;
        if self.arena[idx].is_none() {
            self.arena[idx] = default;
        }
        Some(&self.arena[idx])
    }

    /// Get an entry of this tree
    #[inline]
    pub fn get_or_insert_mut(
        &mut self,
        path: impl AsRef<QBPath>,
        default: QBFileTreeNode,
    ) -> Option<&mut QBFileTreeNode> {
        let idx = self.get_or_create_ptr(path)?;
        if self.arena[idx].is_none() {
            self.arena[idx] = default;
        }
        Some(&mut self.arena[idx])
    }

    /// Get the index for this resource
    fn index(&self, path: impl AsRef<QBPath>) -> Option<usize> {
        let mut pointer = 0;

        for seg in path.as_ref().segments() {
            match &self.arena[pointer] {
                QBFileTreeNode::Dir(children) => {
                    pointer = children.get(seg)?;
                }
                QBFileTreeNode::File(_) => return None,
                _ => panic!("uninitialized"),
            }
        }

        Some(pointer)
    }

    /// Allocate a spot in the area memory map
    ///
    /// TODO: use previously freed
    fn alloc(&mut self) -> usize {
        self.arena.push(Default::default());
        self.arena.len() - 1
    }

    /// Create path in the tree structure
    ///
    /// This might allocate multiple directories.
    /// This will return None if the path is taken.
    fn create_ptr(&mut self, path: impl AsRef<QBPath>) -> Option<usize> {
        let pointer = self.get_or_create_ptr(path)?;

        // check that pointer is not taken
        if !self.arena[pointer].is_none() {
            return None;
        }

        Some(pointer)
    }

    /// Create path in the tree structure
    ///
    /// This might allocate multiple directories.
    fn get_or_create_ptr(&mut self, path: impl AsRef<QBPath>) -> Option<usize> {
        let mut pointer = 0;

        for seg in path.as_ref().segments() {
            pointer = match &self.arena[pointer] {
                QBFileTreeNode::Dir(dir) => match dir.get(seg) {
                    None => {
                        let alloc = self.alloc();
                        self.arena[pointer]
                            .dir_mut()
                            .contents
                            .insert(seg.to_owned(), alloc);
                        alloc
                    }
                    Some(idx) => idx,
                },
                QBFileTreeNode::File(_) => return None,
                QBFileTreeNode::None => {
                    let alloc = self.alloc();
                    let mut contents = HashMap::new();
                    contents.insert(seg.to_owned(), alloc);
                    self.arena[pointer] = QBFileTreeNode::Dir(TreeDir { contents });
                    alloc
                }
            }
        }

        Some(pointer)
    }

    /// update this resource
    ///
    /// asserts that resource is a file
    pub fn update(&mut self, resource: &QBResource, hash: QBHash) {
        assert!(resource.is_file());
        self[resource].file_mut().hash = hash;
    }

    /// create this resource
    pub fn create(&mut self, resource: &QBResource) {
        match self.create_ptr(resource) {
            Some(ptr) => {
                self.arena[ptr] = match resource.is_dir() {
                    true => TreeDir::default().into(),
                    false => TreeFile::default().into(),
                };
            }
            None => warn!("filetree: create {} but path not available!", resource),
        }
    }

    /// delete this resource
    pub fn delete(&mut self, resource: &QBResource) {
        match self.index(resource) {
            Some(ptr) => {
                if self.arena[ptr].is_dir() != resource.is_dir() {
                    warn!(
                        "filetree: delete {} but entry kind does not match!",
                        resource
                    );
                    return;
                }

                std::mem::take(&mut self.arena[ptr]);
            }
            None => warn!("filetree: delete {} but not found!", resource),
        }
    }

    /// Insert a node into the tree structure
    pub fn insert(&mut self, path: impl AsRef<QBPath>, node: impl Into<QBFileTreeNode>) {
        let idx = self.create_ptr(path).expect("path goes over file");
        self.arena[idx] = node.into();
    }

    /// Remove and return an entry
    pub fn remove(&mut self, path: impl AsRef<QBPath>) -> Option<QBFileTreeNode> {
        let idx = self.index(path)?;
        Some(std::mem::take(&mut self.arena[idx]))
    }
}
