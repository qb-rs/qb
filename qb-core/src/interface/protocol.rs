//! This module contains the protocol of the interfaces, that is,
//! the messages that are being sent between QBI and master.

use core::fmt;

use bitcode::{Decode, Encode};

use crate::{
    change::QBChange,
    common::{hash::QBHash, id::QBId},
};

/// a bridge message
#[derive(Debug, Clone, Encode, Decode)]
pub struct BridgeMessage {
    /// the id of the caller
    pub caller: QBId,
    /// the message
    pub msg: Vec<u8>,
}

/// A message
/// this is the struct that is used internally
/// and externally for communicating with QBIs.
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

impl From<Message> for QBIMessage {
    fn from(val: Message) -> Self {
        QBIMessage::Message(val)
    }
}

impl From<Message> for QBMessage {
    fn from(val: Message) -> Self {
        QBMessage::Message(val)
    }
}

/// a message coming from the QBI
#[derive(Debug, Clone)]
pub enum QBIMessage {
    /// message
    Message(Message),
    /// allows the QBI to communicate with the application
    Bridge(BridgeMessage),
}

/// a message coming from the master
#[derive(Debug, Clone)]
pub enum QBMessage {
    /// message
    Message(Message),
    /// allows the QBI to communicate with the application
    Bridge(BridgeMessage),
    /// stop the QBI
    Stop,
}
