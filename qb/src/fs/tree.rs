// TODO: merge into fs

use core::{fmt, panic};
use std::{
    collections::{HashMap, HashSet},
    ops::{Index, IndexMut},
};

use bitcode::{Decode, Encode};
use itertools::Itertools;
use tracing::warn;

use crate::{
    common::{
        hash::QBHash,
        resource::{qbpaths, QBPath, QBResource},
    },
    sync::change::QBChange,
};

use super::wrapper::QBFSWrapper;

#[derive(Encode, Decode)]
pub enum QBFileTreeNode {
    Dir(TreeDir),
    File(TreeFile),
    Uninitialized,
}

impl Default for QBFileTreeNode {
    fn default() -> Self {
        Self::Uninitialized
    }
}

/// A struct stored in the tree, which represents a directory
/// on the file system.
#[derive(Encode, Decode, Default)]
pub struct TreeDir {
    pub contents: HashMap<String, usize>,
}

impl TreeDir {
    /// Get an index entry of the directory
    ///
    /// Alias for self.get(key)
    #[inline]
    pub fn get(&self, key: impl AsRef<str>) -> Option<usize> {
        self.contents.get(key.as_ref()).map(|v| *v)
    }
}

impl Into<QBFileTreeNode> for TreeDir {
    fn into(self) -> QBFileTreeNode {
        QBFileTreeNode::Dir(self)
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
    pub hash: QBHash,
}

impl Default for TreeFile {
    fn default() -> Self {
        Self {
            hash: QBHash::compute(vec![]),
        }
    }
}

impl Into<QBFileTreeNode> for TreeFile {
    fn into(self) -> QBFileTreeNode {
        QBFileTreeNode::File(self)
    }
}

impl QBFileTreeNode {
    pub fn file_mut(&mut self) -> &mut TreeFile {
        if let QBFileTreeNode::File(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }

    pub fn file(&self) -> &TreeFile {
        if let QBFileTreeNode::File(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }

    pub fn dir(&self) -> &TreeDir {
        if let QBFileTreeNode::Dir(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }

    pub fn dir_mut(&mut self) -> &mut TreeDir {
        if let QBFileTreeNode::Dir(val) = self {
            return val;
        }
        panic!("error while unpacking")
    }
}

#[derive(Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Compare {
    resource: QBResource,
    hash: QBHash,
}

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
                QBFileTreeNode::Uninitialized => "init->",
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
                    resource: path.as_ref().clone().sub(k.clone()).unwrap().file(),
                    hash: f.hash.clone(),
                },
                QBFileTreeNode::Dir(_) => Compare {
                    resource: path.as_ref().clone().sub(k.clone()).unwrap().dir(),
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
                    .filter_map(|e| e.resource.is_dir().then(|| e.resource.path.clone())),
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
    fn create(&mut self, path: impl AsRef<QBPath>) -> Option<usize> {
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
                QBFileTreeNode::Uninitialized => {
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

    /// Insert a node into the tree structure
    pub fn insert(&mut self, path: impl AsRef<QBPath>, node: impl Into<QBFileTreeNode>) {
        let idx = self.create(path).expect("path goes over file");
        self.arena[idx] = node.into();
    }

    /// Remove and return an entry
    pub fn remove(&mut self, path: impl AsRef<QBPath>) -> Option<QBFileTreeNode> {
        let idx = self.index(path)?;
        Some(std::mem::take(&mut self.arena[idx]))
    }
}
