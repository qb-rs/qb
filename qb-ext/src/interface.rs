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
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;

use crate::QBExtId;
use qb_core::{change::QBChangeMap, device::QBDeviceId, time::QBTimeStampUnique};

use crate::QBExtChannel;

/// Communicate from the interface to the master
pub type QBIChannel = QBExtChannel<QBExtId, QBISlaveMessage, QBIHostMessage>;

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
        common: QBTimeStampUnique,
    },
    /// synchronize
    Sync {
        /// the common hash that was used for creating the changes vector
        common: QBTimeStampUnique,
        /// a vector describing the changes
        changes: QBChangeMap,
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
            QBIMessage::Sync { common, changes } => {
                writeln!(f, "QBI_MSG_SYNC common: {}", common)?;
                for (resource, entry) in changes.iter() {
                    fmt::Display::fmt(entry, f)?;
                    write!(f, " ")?;
                    fmt::Display::fmt(resource, f)?;
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
/// this has to be done in a task, otherwise it won't work
pub trait QBISetup<T: QBIContext> {
    /// Setup this kind of QBI.
    fn setup(self) -> impl Future<Output = T> + Send + 'static;
}
