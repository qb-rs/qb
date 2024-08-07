//! # quixbyte control (QBC)
//!
//! This module contains primitives for controllers of
//! a daemon. That is, messages for controlling a daemon,
//! as well as an identifier for controlling tasks.

#![warn(missing_docs)]

use std::fmt;

use crate::interface::QBIId;
use bitcode::{Decode, Encode};
use hex::FromHexError;

use qb_proto::QBPBlob;

use rand::Rng;
use serde::{Deserialize, Serialize};

/// An identifier to a daemon control handle.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct QBCId(pub(crate) u64);

impl fmt::Display for QBCId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for QBCId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QBCId({})", self.to_hex())
    }
}

impl QBCId {
    /// Generate a new ID
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        Self(rng.gen::<u64>())
    }

    /// Get the string representation of this id in hex format
    pub fn to_hex(&self) -> String {
        let id_bytes = self.0.to_be_bytes();
        hex::encode(id_bytes)
    }

    /// Decode a hexadecimal string to an id
    pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, FromHexError> {
        let mut id_bytes: [u8; 8] = [0; 8];
        hex::decode_to_slice(hex.as_ref(), &mut id_bytes)?;
        Ok(Self(u64::from_be_bytes(id_bytes)))
    }
}

/// A request comming from a controlling task.
#[derive(Encode, Decode, Serialize, Deserialize)]
pub enum QBCRequest {
    /// Add a new interface.
    Add {
        /// The name of the interface kind ("gdrive", "local", ...)
        name: String,
        /// The setup blob
        blob: QBPBlob,
    },
    /// Remove an interface
    Remove {
        /// the identifier
        id: QBIId,
    },
    /// Start an existing interface.
    Start {
        /// the identifier
        id: QBIId,
    },
    /// Stop an existing interface.
    Stop {
        /// the identifier
        id: QBIId,
    },
    /// List the available interfaces.
    List,
}

impl fmt::Display for QBCRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QBCRequest::Add { name, blob } => {
                write!(
                    f,
                    "QBC_MSG_REQ_ADD {} {} {}",
                    name,
                    blob.content_type,
                    simdutf8::basic::from_utf8(&blob.content).unwrap_or("binary data")
                )
            }
            QBCRequest::Remove { id } => {
                write!(f, "QBC_MSG_REQ_REMOVE {}", id)
            }
            QBCRequest::Start { id } => {
                write!(f, "QBC_MSG_REQ_START {}", id)
            }
            QBCRequest::Stop { id } => {
                write!(f, "QBC_MSG_REQ_STOP {}", id)
            }
            QBCRequest::List => {
                write!(f, "QBC_MSG_REQ_LIST")
            }
        }
    }
}

/// A response comming from the daemon.
#[derive(Encode, Decode, Serialize, Deserialize)]
pub enum QBCResponse {
    /// An error has occured.
    Error {
        /// The error message
        msg: String,
    },
    /// Response for the list request.
    List {
        /// the available interfaces
        list: Vec<(QBIId, String, bool)>,
    },
    /// Generic success request.
    Success,
}

impl fmt::Display for QBCResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QBCResponse::Error { msg } => {
                write!(f, "MSG_CONTROL_RESP_ERROR: {}", msg)
            }
            QBCResponse::Success => {
                write!(f, "MSG_CONTROL_RESP_SUCCESS")
            }
            QBCResponse::List { list } => {
                write!(f, "MSG_CONTROL_RESP_LIST:")?;
                for entry in list {
                    write!(f, "\n{} - {}", entry.0, entry.1)?;

                    if entry.2 {
                        write!(f, " - attached")?;
                    }
                }

                Ok(())
            }
        }
    }
}
