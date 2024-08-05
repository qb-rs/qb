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

use bitcode::{Decode, Encode};
use hex::FromHexError;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use tokio::sync::mpsc;

use crate::{
    change::QBChange,
    common::{device::QBDeviceId, hash::QBHash, id::QBId},
};

/// An identifier for a QBI.
///
/// This also includes the identifier for the device,
/// as each QBI is attached to exactly one device.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Hash, Clone, Eq, PartialEq)]
pub struct QBIId {
    /// The id of the device this QBI is attached to
    pub device_id: QBDeviceId,
    /// A nonce to allow multiple QBIs on the same device
    /// TODO: figure out if this is needed
    /// TODO: figure out whether size is appropriate
    pub nonce: u64,
}

impl QBIId {
    /// Generate a QBIId for a QBI which operates on the device with the given device_id.
    pub fn generate(device_id: QBDeviceId) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            nonce: rng.gen::<u64>(),
            device_id,
        }
    }

    /// Get the string representation of this id in hex format
    pub fn to_hex(&self) -> String {
        self.device_id.to_hex() + &hex::encode(self.nonce.to_be_bytes())
    }

    /// Decode a hexadecimal string to an id
    pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, FromHexError> {
        let hex = hex.as_ref();
        let device_id = QBDeviceId::from_hex(&hex[0..16])?;
        let mut nonce_bytes = [0u8; 8];
        hex::decode_to_slice(&hex[16..], &mut nonce_bytes)?;
        Ok(Self {
            device_id,
            nonce: u64::from_be_bytes(nonce_bytes),
        })
    }
}

/// a bridge message
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub struct QBIBridgeMessage {
    /// the id of the caller
    pub caller: QBId,
    /// the message
    pub msg: Vec<u8>,
}

/// A message
/// this is the struct that is used internally
/// and externally for communicating with QBIs.
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum Message {
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
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Message::Sync {
                common,
                changes: entries,
            } => {
                writeln!(f, "MSG_SYNC common: {}", common)?;
                for entry in entries {
                    fmt::Display::fmt(entry, f)?;
                    writeln!(f)?;
                }
                Ok(())
            }
            Message::Common { common } => {
                write!(f, "MSG_COMMON {}", common)
            }
            Message::Broadcast { msg } => {
                write!(f, "MSG_BROADCAST {}", msg)
            }
        }
    }
}

impl From<Message> for QBISlaveMessage {
    fn from(val: Message) -> Self {
        QBISlaveMessage::Message(val)
    }
}

impl From<Message> for QBIHostMessage {
    fn from(val: Message) -> Self {
        QBIHostMessage::Message(val)
    }
}

/// a message coming from the QBI
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum QBISlaveMessage {
    /// message
    Message(Message),
    /// allows the QBI to communicate with the application
    Bridge(QBIBridgeMessage),
}

/// a message coming from the master
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum QBIHostMessage {
    /// message
    Message(Message),
    /// allows the QBI to communicate with the application
    Bridge(QBIBridgeMessage),
    /// stop the QBI slave
    Stop,
}

/// The QBIContext is a struct which is responsible for running
/// the QBI. It is send between the master thread and the QBI thread
/// created by the master (might be the same thread as well, depends
/// on what tokio chooses to do). QBIs are asynchronous by default.
pub trait QBIContext: Send + Sync {
    /// The main function of the QBI which will be spawned into a seperate
    /// async task (might be a thread, depends on how tokio handles this).
    fn run(self, com: QBICommunication) -> impl Future<Output = ()> + Send + 'static;
}

/// struct describing the communication interface between QBI and master
pub struct QBICommunication {
    /// the transmission channel
    pub tx: mpsc::Sender<QBISlaveMessage>,
    /// the receive channel
    pub rx: mpsc::Receiver<QBIHostMessage>,
}

impl QBICommunication {
    /// TODO: doc
    pub async fn send(&self, msg: impl Into<QBISlaveMessage>) {
        self.tx.send(msg.into()).await.unwrap()
    }

    /// TODO: doc
    pub fn blocking_send(&self, msg: impl Into<QBISlaveMessage>) {
        self.tx.blocking_send(msg.into()).unwrap()
    }
}
