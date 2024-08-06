use std::fmt;

use bitcode::{Decode, Encode};
use qb_core::interface::QBIId;

// re-export qbis
pub use qbi_local;

use serde::{Deserialize, Serialize};

#[derive(Encode, Decode, Serialize, Deserialize)]
pub enum QBControlRequest {
    /// This message packet must be followed by
    /// a binary packet containing the setup contents.
    Setup {
        name: String,
        content_type: String,
    },
    Start {
        id: QBIId,
    },
    Stop {
        id: QBIId,
    },
    /// Talk to the QBI
    Bridge {
        id: QBIId,
        msg: Vec<u8>,
    },
}

impl fmt::Display for QBControlRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QBControlRequest::Setup { name, content_type } => {
                write!(f, "MSG_CONTROL_REQ_SETUP {} {}", name, content_type)
            }
            QBControlRequest::Start { id } => {
                write!(f, "MSG_CONTROL_REQ_START {}", id)
            }
            QBControlRequest::Stop { id } => {
                write!(f, "MSG_CONTROL_REQ_STOP {}", id)
            }
            QBControlRequest::Bridge { id, msg } => {
                write!(
                    f,
                    "MSG_CONTROL_REQ_BRIDGE {}: {}",
                    id,
                    simdutf8::basic::from_utf8(msg).unwrap_or("binary data")
                )
            }
        }
    }
}

#[derive(Encode, Decode, Serialize, Deserialize)]
pub enum QBControlResponse {
    // TODO: attach/detach success/error
    Bridge { msg: Vec<u8> },
}

impl fmt::Display for QBControlResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QBControlResponse::Bridge { msg } => {
                write!(
                    f,
                    "MSG_CONTROL_RESP_BRIDGE: {}",
                    simdutf8::basic::from_utf8(msg).unwrap_or("binary data")
                )
            }
        }
    }
}
