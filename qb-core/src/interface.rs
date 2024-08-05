//! This module contains stuff related to QBIs (quixbyte interfaces).
//! A QBI is a modular adaptor for communicating with different services
//! and allowing to synchronize onto many different platforms.

pub mod protocol;

use std::collections::HashMap;

use bitcode::{Decode, Encode};

use crate::{
    change::QB_CHANGELOG_BASE,
    common::{device::QBDeviceId, hash::QBHash},
};
use protocol::{QBIMessage, QBMessage};

/// trait which all quixbyte interfaces need to implement
///
/// if you need async support take a look at QBIAsync from
/// the qb-derive crate as well.
pub trait QBI<T> {
    /// Initialize this QBI.
    fn init(cx: T, com: QBICommunication) -> Self;
    /// main loop
    fn run(self);
}

/// struct that stores common changes and names for all connections
#[derive(Encode, Decode, Debug, Clone, Default)]
pub struct QBDevices {
    commons: HashMap<QBDeviceId, QBHash>,
    names: HashMap<QBDeviceId, String>,
}

impl QBDevices {
    /// Get the common hash of the connection with the id.
    pub fn get_common(&self, id: &QBDeviceId) -> &QBHash {
        self.commons.get(id).unwrap_or(QB_CHANGELOG_BASE.hash())
    }

    /// Set the common hash of the connection with the id.
    pub fn set_common(&mut self, id: &QBDeviceId, hash: QBHash) {
        self.commons.insert(id.clone(), hash);
    }

    /// Get the name of the connection with the id.
    pub fn get_name(&self, id: &QBDeviceId) -> &str {
        self.names.get(id).map(|a| a.as_str()).unwrap_or("untitled")
    }

    /// Set the name of the connection with the id.
    pub fn set_name(&mut self, id: &QBDeviceId, name: String) {
        self.names.insert(id.clone(), name);
    }
}

use tokio::sync::mpsc;

/// struct describing the communication interface between QBI and master
pub struct QBICommunication {
    /// the transmission channel
    pub tx: mpsc::Sender<QBIMessage>,
    /// the receive channel
    pub rx: mpsc::Receiver<QBMessage>,
}

impl QBICommunication {
    /// TODO: doc
    pub async fn send(&self, msg: impl Into<QBIMessage>) {
        self.tx.send(msg.into()).await.unwrap()
    }

    /// TODO: doc
    pub fn blocking_send(&self, msg: impl Into<QBIMessage>) {
        self.tx.blocking_send(msg.into()).unwrap()
    }
}
