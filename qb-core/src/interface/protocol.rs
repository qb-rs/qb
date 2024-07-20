//! This module contains the protocol of the interfaces, that is,
//! the messages that are being sent between QBI and master.

use core::fmt;

use bitcode::{Decode, Encode};

use crate::{change::QBChange, common::hash::QBHash};

/// a message
#[derive(Debug, Clone, Encode, Decode)]
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
    /// allows the process connected to the
    /// master to communicate with the qbi
    Bridge {
        /// the message
        msg: Vec<u8>,
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
            Message::Bridge { msg } => {
                write!(
                    f,
                    "MSG_BRIDGE {}",
                    simdutf8::basic::from_utf8(msg).unwrap_or("binary data")
                )
            }
        }
    }
}

impl Into<QBIMessage> for Message {
    fn into(self) -> QBIMessage {
        QBIMessage(self)
    }
}

impl Into<QBMessage> for Message {
    fn into(self) -> QBMessage {
        QBMessage::Message(self)
    }
}

/// a message coming from the QBI
#[derive(Debug, Clone)]
pub struct QBIMessage(pub Message);

/// a message coming from the master
#[derive(Debug, Clone)]
pub enum QBMessage {
    /// message
    Message(Message),
    /// stop the QBI
    Stop,
}
