//! # qb-daemon
//!
//! This crate houses the daemon of the application,
//! that is, the application that runs in the background,
//! which handles interface tasks and their respective communication.
//!
//! We can communicate with the daemon using the [qb-control] messages.

#![warn(missing_docs)]

pub mod daemon;
pub mod master;

#[tokio::main]
pub async fn main() {
    todo!()
}
