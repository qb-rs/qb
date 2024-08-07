//! # quixbyte interface (QBI)
//!
//! This module contains stuff related to QBIs (quixbyte interfaces).
//! A QBI is a modular adaptor for communicating with different devices
//! and allowing to synchronize onto many different platforms.
//!
//! This module contains message structs. Please note that the messages
//! prefixed by QBI are local only and will not be shared with other devices.
//!
//! Although local-only these structs still derive Encode, Decode, Serialize
//! and Deserialize which implies that they would be sent over some protocol.
//! This however is only for (in the future) external QBI processes, which
//! also use the QBP to communicate.
//!
//!
//! TODO: external interfaces

use bitcode::{Decode, Encode};
use hex::FromHexError;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;

use qb_core::{
    change::QBChange,
    common::{device::QBDeviceId, hash::QBHash},
};

use crate::QBChannel;

/// Communicate from the interface to the master
pub type QBIChannel = QBChannel<QBISlaveMessage, QBIHostMessage>;

/// An identifier for an interface.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Hash, Clone, Eq, PartialEq)]
pub struct QBIId {
    /// The nonce of this Id
    pub nonce: u64,
}

impl fmt::Display for QBIId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl QBIId {
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

/// A message
/// this is the struct that is used internally
/// and externally for communicating with QBIs.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum QBIMessage {
    /// broadcast a message
    Broadcast {
        /// message to broadcast
        msg: String,
    },
    /// exchange the common change, sent when newest common
    /// change gets updated (synchronization)
    Common {
        /// hash that points to the common change
        common: QBHash,
    },
    /// synchronize
    Sync {
        /// the common hash that was used for creating the changes vector
        common: QBHash,
        /// a vector describing the changes
        changes: Vec<QBChange>,
    },
    /// An interface might not be properly initialized
    /// at attachment and we might not even know the Id
    /// of the device we are connecting through. (server
    /// doesn't know client device id before initialization).
    Device {
        /// The device id
        device_id: QBDeviceId,
    },
}

impl fmt::Display for QBIMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            QBIMessage::Sync {
                common,
                changes: entries,
            } => {
                writeln!(f, "QBI_MSG_SYNC common: {}", common)?;
                for entry in entries {
                    fmt::Display::fmt(entry, f)?;
                    writeln!(f)?;
                }
                Ok(())
            }
            QBIMessage::Common { common } => {
                write!(f, "QBI_MSG_COMMON {}", common)
            }
            QBIMessage::Broadcast { msg } => {
                write!(f, "QBI_MSG_BROADCAST {}", msg)
            }
            QBIMessage::Device { device_id } => {
                write!(f, "QBI_MSG_DEVICE {}", device_id)
            }
        }
    }
}

impl From<QBIMessage> for QBISlaveMessage {
    fn from(val: QBIMessage) -> Self {
        QBISlaveMessage::Message(val)
    }
}

impl From<QBIMessage> for QBIHostMessage {
    fn from(val: QBIMessage) -> Self {
        QBIHostMessage::Message(val)
    }
}

/// a message coming from the interface
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum QBISlaveMessage {
    /// message
    Message(QBIMessage),
}

/// a message coming from the master
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum QBIHostMessage {
    /// message
    Message(QBIMessage),
    /// stop the interface
    Stop,
}

/// The QBIContext is a struct which is responsible for running
/// the QBI. It is send between the master thread and the QBI thread
/// created by the master (might be the same thread as well, depends
/// on what tokio chooses to do). QBIs are asynchronous by default.
pub trait QBIContext: Send + Sync {
    /// The main function of the QBI which will be spawned into a seperate
    /// async task (might be a thread, depends on how tokio handles this).
    fn run(self, host_id: QBDeviceId, com: QBIChannel)
        -> impl Future<Output = ()> + Send + 'static;
}

/// TODO: doc
pub trait QBISetup<'a>: Encode + Decode<'a> + Serialize + Deserialize<'a> {
    /// Setup this kind of QBI.
    fn setup(self) -> impl Future<Output = ()> + Send;
}
