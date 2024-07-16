use core::fmt;

use crate::{common::hash::QBHash, sync::change::QBChange};

// TODO: figure out what to call this
#[derive(Debug, Clone)]
pub enum QBIMessage {
    // TODO: figure out which structs to send over
    // RTCConnectionOffer {},
    // RTCConnectionAnswer {},
    Broadcast {
        msg: String,
    },
    Common {
        common: QBHash,
    }, // When newest common entry gets updated
    Sync {
        common: QBHash,
        changes: Vec<QBChange>,
    },
    // SyncComplete is the same as Sync with empty entries
    //SyncComplete {
    //    common: QBHash,
    //},
    //BridgeRequest {
    //    key: String,
    //},
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

#[derive(Debug, Clone)]
pub enum QBMessage {
    // TODO: figure out which structs to send over
    // RTCConnectionOffer {},
    // RTCConnectionAnswer {},
    Broadcast {
        msg: String,
    },
    // Check if we even need this
    Common {
        common: QBHash,
    }, // When newest common entry gets updated
    Sync {
        common: QBHash,
        changes: Vec<QBChange>,
    },
    // SyncComplete is the same as Sync with empty entries
    //SyncComplete {
    //    common: QBHash,
    //},
    //BridgeRequest {
    //    val: Box<dyn Any + Send + Sync + 'static>,
    //},
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
