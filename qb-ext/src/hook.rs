//! # quixbyte hook (QBH)
//!
//! This module contains stuff related to hooks, which can be attached to
//! the daemon. Hooks are tasks which listen for messages coming from the
//! master and control the master using hook messages.
//!
//! TODO: switch to mutex instead of using messaging

use core::fmt;
use std::{any::Any, future::Future, marker::PhantomData};

use bitcode::{Decode, Encode};
use hex::FromHexError;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{interface::QBIContext, QBChannel};

/// Communicate from the interface to the master
pub type QBHChannel = QBChannel<QBHId, QBHSlaveMessage, QBHHostMessage>;

/// TODO: figure out what to call this
pub struct QBHInit<T: QBIContext + Any + Send> {
    channel: QBHChannel,
    _t: PhantomData<T>,
}

impl<T: QBIContext + Any + Send> QBHInit<T> {
    pub async fn attach(&self, context: T) {
        self.channel
            .send(QBHSlaveMessage::Attach {
                context: Box::new(context),
            })
            .await;
    }
}

impl<T: QBIContext + Any + Send> From<QBHChannel> for QBHInit<T> {
    fn from(value: QBHChannel) -> Self {
        Self {
            channel: value,
            _t: PhantomData::default(),
        }
    }
}

/// An identifier for a hook.
#[derive(Encode, Decode, Serialize, Deserialize, Hash, Clone, Eq, PartialEq)]
pub struct QBHId {
    /// The nonce of this Id
    pub nonce: u64,
}

impl fmt::Display for QBHId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for QBHId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QBHId({})", self.to_hex())
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

pub enum QBHHostMessage {
    Stop,
}

pub enum QBHSlaveMessage {
    Attach { context: Box<dyn Any + Send> },
}

/// A context which yields interfaces.
pub trait QBHContext<T: QBIContext + Any + Send> {
    fn run(self, init: QBHInit<T>) -> impl Future<Output = ()> + Send + 'static;
}
