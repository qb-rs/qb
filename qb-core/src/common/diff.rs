//! A diff describes a transformation that can be applied to a specific
//! input to get a specific output. It is used for compressing changes on
//! text files.

use bitcode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use super::hash::QBHash;

/// struct which stores operations for a transformation on a string
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub struct QBDiff {
    /// Describes the hash of the content before the transformation.
    pub old_hash: QBHash,
    /// the transformations themselves
    pub ops: Vec<QBDiffOp>,
}

/// struct which stores a single operation for a transformation on a string
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum QBDiffOp {
    /// range is equal
    Equal {
        /// length of range
        len: usize,
    },
    /// range should not exist in transformed string
    Delete {
        /// length of range
        len: usize,
    },
    /// content should get inserted in transformed string
    Insert {
        /// text to be inserted
        content: String,
    },
    /// content should replace the range in transformed string
    Replace {
        /// length of range
        len: usize,
        /// text to be inserted
        content: String,
    },
}

struct Index {
    old_start: usize,
    old_end: usize,
    #[allow(dead_code)]
    new_start: usize,
    new_end: usize,
}

impl QBDiff {
    /// Compute a diff
    pub fn compute(old: String, new: String) -> QBDiff {
        let changes = similar::TextDiff::configure().diff_lines(&old, &new);
        let old_hash = QBHash::compute(&old);
        let new = changes.new_slices();
        let ops = changes
            .ops()
            .iter()
            .map(|e| match e {
                similar::DiffOp::Equal { len, .. } => QBDiffOp::Equal { len: *len },
                similar::DiffOp::Delete { old_len, .. } => QBDiffOp::Delete { len: *old_len },
                similar::DiffOp::Insert {
                    new_index, new_len, ..
                } => QBDiffOp::Insert {
                    content: new[*new_index..(new_index + new_len)].join(""),
                },
                similar::DiffOp::Replace {
                    new_index,
                    old_len,
                    new_len,
                    ..
                } => QBDiffOp::Replace {
                    len: *old_len,
                    content: new[*new_index..(new_index + new_len)].join(""),
                },
            })
            .collect();

        QBDiff { old_hash, ops }
    }

    /// Apply this diff to a string
    pub fn apply(&self, old: String) -> String {
        let old_hash = QBHash::compute(&old);
        assert!(self.old_hash == old_hash);

        let old = old.split_inclusive('\n').collect::<Vec<_>>();

        let mut old_index = 0;
        let mut new = String::new();
        for op in self.ops.iter() {
            match op {
                QBDiffOp::Equal { len } => {
                    new += &old[old_index..(old_index + len)].join("");
                    old_index += len;
                }
                QBDiffOp::Insert { content } => new += content,
                QBDiffOp::Delete { len } => old_index += len,
                QBDiffOp::Replace { content, len } => {
                    new += content;
                    old_index += len;
                }
            }
        }

        new
    }

    /// Get the indicies for each operation
    fn get_indicies(&self) -> Vec<Index> {
        let mut old_index = 0;
        let mut new_index = 0;
        let mut indicies = Vec::new();
        for op in self.ops.iter() {
            let old_start = old_index;
            let new_start = new_index;
            match op {
                QBDiffOp::Delete { len } => {
                    old_index += len;
                }
                QBDiffOp::Equal { len } => {
                    old_index += len;
                    new_index += len;
                }
                QBDiffOp::Replace { len, content } => {
                    old_index += len;
                    new_index += content.len();
                }
                QBDiffOp::Insert { content } => {
                    new_index += content.len();
                }
            }
            let old_end = old_index;
            let new_end = new_index;

            indicies.push(Index {
                old_start,
                old_end,
                new_start,
                new_end,
            });
        }

        indicies
    }

    /// Merge two diffs into one. This may return errors
    /// TODO: I don't think this is optimal
    pub fn merge(mut a: QBDiff, mut b: QBDiff) -> Option<QBDiff> {
        assert!(a.old_hash == b.old_hash);

        let mut a_indicies = a.get_indicies();
        let mut b_indicies = b.get_indicies();

        let mut a_index = 0;
        let mut b_index = 0;

        // Splitting entities
        while a_indicies.len() > a_index && b_indicies.len() > b_index {
            let (stay, split, stay_index, split_index) = match a_indicies[a_index]
                .old_end
                .cmp(&b_indicies[b_index].old_end)
            {
                std::cmp::Ordering::Greater => (
                    (&mut b_indicies, &mut b.ops),
                    (&mut a_indicies, &mut a.ops),
                    b_index,
                    a_index,
                ),
                std::cmp::Ordering::Less => (
                    (&mut a_indicies, &mut a.ops),
                    (&mut b_indicies, &mut b.ops),
                    a_index,
                    b_index,
                ),
                std::cmp::Ordering::Equal => {
                    a_index += 1;
                    b_index += 1;
                    continue;
                }
            };

            // TODO: if assertion fails, merge conflict => return error
            assert!(matches!(split.1[split_index], QBDiffOp::Equal { .. }));

            let (a, b) = (&stay.0[stay_index], &split.0[split_index]);

            // Sorry for the confusing naming
            // len_a is the length of the first part of the split
            // len_b is the length of the second part of the split
            let len_a = a.old_end - b.old_start;
            let len_b = b.old_end - a.old_end;

            let old_split = a.old_end;
            let new_split = b.new_end - len_b;
            let old_end = b.old_end;
            let new_end = b.new_end;

            split.0.insert(
                split_index,
                Index {
                    old_start: old_split,
                    old_end,
                    new_start: new_split,
                    new_end,
                },
            );
            split.1[split_index] = QBDiffOp::Equal { len: len_a };
            split.1.insert(split_index, QBDiffOp::Equal { len: len_b });
            split.0[split_index].old_end = old_split;
            split.0[split_index].new_end = new_split;

            a_index += 1;
            b_index += 1;
        }

        let mut ops = Vec::new();

        let mut a_index = 0;
        let mut b_index = 0;

        // Splitting entities
        while a_indicies.len() > a_index && b_indicies.len() > b_index {
            match a_indicies[a_index]
                .old_end
                .cmp(&b_indicies[b_index].old_end)
            {
                std::cmp::Ordering::Less => {
                    ops.push(a.ops[a_index].clone());
                    a_index += 1;
                    continue;
                }
                std::cmp::Ordering::Greater => {
                    ops.push(b.ops[a_index].clone());
                    b_index += 1;
                    continue;
                }
                _ => {}
            };

            match (&a.ops[a_index], &b.ops[b_index]) {
                (QBDiffOp::Equal { .. }, _) => {
                    ops.push(b.ops[b_index].clone());
                    a_index += 1;
                    b_index += 1;
                }
                (_, QBDiffOp::Equal { .. }) => {
                    ops.push(a.ops[a_index].clone());
                    a_index += 1;
                    b_index += 1;
                }
                _ => unimplemented!(),
            };
        }

        Some(QBDiff {
            old_hash: a.old_hash,
            ops,
        })
    }
}
