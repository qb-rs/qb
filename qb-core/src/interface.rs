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
    common::{device::QBDeviceId, hash::QBHash},
};

/// An identifier for a QBI.
///
/// This also includes the identifier for the device,
/// as each QBI is attached to exactly one device.
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
    /// An interface might not be properly initialized
    /// at attachment and we might not even know the Id
    /// of the device we are connecting through. (server
    /// doesn't know client device id before initialization).
    Device {
        /// The device id
        device_id: QBDeviceId,
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
            Message::Device { device_id } => {
                write!(f, "MSG_DEVICE {}", device_id)
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
}

/// a message coming from the master
#[derive(Encode, Decode, Serialize, Deserialize, Debug, Clone)]
pub enum QBIHostMessage {
    /// message
    Message(Message),
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
    fn run(
        self,
        host_id: QBDeviceId,
        com: QBICommunication,
    ) -> impl Future<Output = ()> + Send + 'static;
}

/// TODO: doc
pub trait QBISetup<'a>: Encode + Decode<'a> + Serialize + Deserialize<'a> {
    /// Setup this kind of QBI.
    fn setup(self) -> impl Future<Output = ()> + Send;
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
    pub async fn read(&mut self) -> QBIHostMessage {
        self.rx.recv().await.expect("channel closed")
    }
}
