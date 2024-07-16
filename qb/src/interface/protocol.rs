//! This module contains the protocol of the interfaces, that is,
//! the messages that are being sent between QBI and master.

use core::fmt;

use crate::{change::QBChange, common::hash::QBHash};

/// a message coming from the QBI
#[derive(Debug, Clone)]
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
}

impl fmt::Display for QBIMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            QBIMessage::Sync {
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
            QBIMessage::Common { common } => {
                write!(f, "MSG_COMMON {}", common)
            }
            QBIMessage::Broadcast { msg } => {
                write!(f, "MSG_BROADCAST {}", msg)
            }
        }
    }
}

/// a message coming from the master
#[derive(Debug, Clone)]
pub enum QBMessage {
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

impl fmt::Display for QBMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            QBMessage::Sync {
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
            QBMessage::Common { common } => {
                write!(f, "MSG_COMMON {}", common)
            }
            QBMessage::Broadcast { msg } => {
                write!(f, "MSG_BROADCAST {}", msg)
            }
        }
    }
}
