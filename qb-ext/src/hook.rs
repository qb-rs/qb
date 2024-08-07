//! # quixbyte hook (QBH)
//!
//! This module contains stuff related to hooks, which can be attached to
//! the daemon. Hooks are tasks which listen for messages coming from the
//! master and control the master using hook messages.
//!
//! TODO: external hooks

use core::fmt;
use std::future::Future;

use bitcode::{Decode, Encode};
use hex::FromHexError;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::QBChannel;

/// Communicate from the interface to the master
pub type QBHChannel = QBChannel<QBHSlaveMessage, QBHHostMessage>;

/// An identifier for a hook.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Hash, Clone, Eq, PartialEq)]
pub struct QBHId {
    /// The nonce of this Id
    pub nonce: u64,
}

impl fmt::Display for QBHId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl QBHId {
    /// Generate a QBIId for a QBI which operates on the device with the given device_id.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            nonce: rng.gen::<u64>(),
        }
    }

    /// Get the string representation of this id in hex format
    pub fn to_hex(&self) -> String {
        hex::encode(self.nonce.to_be_bytes())
    }

    /// Decode a hexadecimal string to an id
    pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, FromHexError> {
        let mut bytes = [0u8; 8];
        hex::decode_to_slice(hex.as_ref(), &mut bytes)?;
        Ok(Self {
            nonce: u64::from_be_bytes(bytes),
        })
    }
}

pub enum QBHMessage {}

pub enum QBHHostMessage {
    Message(QBHMessage),
}

pub enum QBHSlaveMessage {
    Message(QBHMessage),
}

pub trait QBHContext {
    fn run(self, com: QBHChannel) -> impl Future<Output = ()> + Send + 'static;
}
