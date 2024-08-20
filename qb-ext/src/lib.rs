//! qb-ext
//!
//! This crate exposes primitives about extending quixbyte's capabilities
//! like adding interfaces and hooks or controlling the master via control
//! messages.

use tokio::sync::mpsc;

pub mod control;
pub mod hook;
pub mod interface;

use core::fmt;
use std::future::Future;

use bitcode::{Decode, Encode};
use hex::FromHexError;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// An identifier for an interface.
#[derive(Encode, Decode, Serialize, Deserialize, Hash, Clone, Eq, PartialEq)]
pub struct QBExtId(pub u64);

impl fmt::Display for QBExtId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for QBExtId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QBIId({})", self.to_hex())
    }
}

impl QBExtId {
    /// Generate a QBIId for a QBI which operates on the device with the given device_id.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        Self(rng.gen::<u64>())
    }

    /// Get the string representation of this id in hex format
    pub fn to_hex(&self) -> String {
        hex::encode(self.0.to_be_bytes())
    }

    /// Decode a hexadecimal string to an id
    pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, FromHexError> {
        let mut bytes = [0u8; 8];
        hex::decode_to_slice(hex.as_ref(), &mut bytes)?;
        Ok(Self(u64::from_be_bytes(bytes)))
    }
}

/// TODO: doc
pub trait QBExtSetup<T> {
    /// Setup this extension.
    fn setup(self) -> impl Future<Output = T> + Send + 'static;
}

/// A channel used for communication from a slave
pub struct QBExtChannel<I: Clone, S, R> {
    id: I,
    tx: mpsc::Sender<(I, S)>,
    rx: mpsc::Receiver<R>,
}

impl<I: Clone, S, R> QBExtChannel<I, S, R> {
    /// Construct a new channel
    pub fn new(id: I, tx: mpsc::Sender<(I, S)>, rx: mpsc::Receiver<R>) -> Self {
        QBExtChannel { id, tx, rx }
    }

    /// Send a message to this channel
    pub async fn send(&self, msg: impl Into<S>) {
        self.tx.send((self.id.clone(), msg.into())).await.unwrap()
    }

    /// Receive a message from this channel
    pub async fn recv<T>(&mut self) -> T
    where
        T: From<R>,
    {
        self.rx.recv().await.expect("channel closed").into()
    }
}
