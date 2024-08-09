//! # qb-core
//!
//! This crate is the core library of quixbyte
//! and houses many of the commonly used structures
//! like device tables or hashes or event the QBIContext
//! and QBISetup traits which are required to create
//! an interface.
//!
//! By design this crate should not depend on any
//! of the other crates, to avoid creating a dependency
//! chain, except, when the other crate is a general purpose
//! crate, that is, when it is not project specific.

#![warn(missing_docs)]

pub mod change;
pub mod device;
pub mod diff;
pub mod fs;
pub mod hash;
pub mod ignore;
pub mod path;
pub mod time;
