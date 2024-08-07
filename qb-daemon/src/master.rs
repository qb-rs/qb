//! # master
//!
//! This module houses the master, that is, the unit
//! which handles interfaces and their communication.
//! It owns a device table and a changelog to allow syncing.

use std::collections::HashMap;

use qb_core::{
    change::log::QBChangelog,
    common::device::{QBDeviceId, QBDeviceTable},
};
use qb_ext::{
    hook::{QBHChannel, QBHContext, QBHHostMessage, QBHId, QBHSlaveMessage},
    interface::{QBIChannel, QBIContext, QBIHostMessage, QBIId, QBIMessage, QBISlaveMessage},
};
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{info, info_span, warn};

/// An error that occured related to the master
#[derive(Error, Debug)]
pub enum Error {
    /// This error propagates when we try to detach an interface
    /// with an id, of which none such interface exists in this master.
    #[error("no interface with the given id was found")]
    NotFound,
    /// This error propagates when we try to attach an interface
    /// with an id, of which another interface is already attached.
    #[error("an interface with the same id is already attached")]
    AlreadyAttached,
    #[error("a hook with the same id is already hooked")]
    AlreadyHooked,
}

/// Result type alias for making our life easier.
pub type Result<T> = std::result::Result<T, Error>;

/// The state which an interface can be in.
pub enum QBIState {
    /// no param known
    Init,
    /// device_id known, missing common hash
    Device {
        /// the device id
        device_id: QBDeviceId,
    },
    /// device_id known, common hash known
    Available {
        /// the device id
        device_id: QBDeviceId,
        /// is the device currently synchronizing
        syncing: bool,
    },
}

/// A handle to an interface.
pub struct QBIHandle {
    join_handle: JoinHandle<()>,
    state: QBIState,
    tx: mpsc::Sender<QBIHostMessage>,
}

/// A handle to a hook.
pub struct QBHHandle {
    join_handle: JoinHandle<()>,
    tx: mpsc::Sender<QBHHostMessage>,
}

/// The master, that is, the struct that houses connection
/// to the individual interfaces and manages communication.
pub struct QBMaster {
    interfaces: HashMap<QBIId, QBIHandle>,
    interface_rx: mpsc::Receiver<(QBIId, QBISlaveMessage)>,
    interface_tx: mpsc::Sender<(QBIId, QBISlaveMessage)>,
    hooks: HashMap<QBHId, QBHHandle>,
    hook_rx: mpsc::Receiver<(QBHId, QBHSlaveMessage)>,
    hook_tx: mpsc::Sender<(QBHId, QBHSlaveMessage)>,
    devices: QBDeviceTable,
    changelog: QBChangelog,
}

impl QBMaster {
    /// Initialize the master with the given device id.
    ///
    /// This identifier should be negotiated and then stored
    /// somewhere and not randomly initialized every boot.
    pub fn init() -> QBMaster {
        let (interface_tx, interface_rx) = mpsc::channel(10);
        let (hook_tx, hook_rx) = mpsc::channel(10);

        QBMaster {
            interfaces: HashMap::new(),
            interface_rx,
            interface_tx,
            hooks: HashMap::new(),
            hook_rx,
            hook_tx,
            // TODO: pass through constructor, these should be persistent
            devices: Default::default(),
            changelog: Default::default(),
        }
    }

    /// Remove unused handles [from interfaces that have finished]
    pub fn clean_handles(&mut self) {
        let to_remove = self
            .interfaces
            .iter()
            .filter(|(_, v)| v.join_handle.is_finished())
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for id in to_remove {
            self.interfaces.remove(&id);
        }
    }

    /// Receive a message.
    ///
    /// # Cancelation safety
    /// This method is cancelation safe
    pub async fn recv(&mut self) -> (QBIId, QBISlaveMessage) {
        // read loop
        self.interface_rx.recv().await.expect("channel closed")
    }

    /// This will look for new messages from the interfaces and
    /// handle those respectively. Additionally this will
    /// synchronize when new changes arise.
    ///
    /// # Cancelation Safety
    /// This method is not cancelation safe.
    pub async fn process(&mut self, (id, msg): (QBIId, QBISlaveMessage)) {
        self.clean_handles();

        let mut broadcast = Vec::new();

        // unwrap it
        let msg = match msg {
            QBISlaveMessage::Message(msg) => msg,
        };

        let span = info_span!("qbi-process", id = id.to_hex());
        let _guard = span.enter();
        let handle = self.interfaces.get_mut(&id).unwrap();

        // handle uninitialized handles
        let (device_id, syncing) = match handle.state {
            QBIState::Available {
                ref device_id,
                ref mut syncing,
            } => (device_id, syncing),
            QBIState::Device { ref device_id } => {
                match msg {
                    QBIMessage::Common { common } => {
                        // TODO: negotiate this instead
                        self.devices.set_common(&device_id, common);
                        handle.state = QBIState::Available {
                            device_id: device_id.clone(),
                            syncing: false,
                        };
                    }
                    // The interface should not send any messages before the
                    // init message has been sent. This is likely an error.
                    val => warn!("unexpected message: {}", val),
                }
                return;
            }
            QBIState::Init => {
                match msg {
                    QBIMessage::Device { device_id } => {
                        let common = self.devices.get_common(&device_id).clone();
                        handle.state = QBIState::Device { device_id };
                        let msg = QBIMessage::Common { common }.into();
                        handle.tx.send(msg).await.unwrap();
                    }
                    // The interface should not send any messages before the
                    // init message has been sent. This is likely an error.
                    val => warn!("unexpected message: {}", val),
                }
                return;
            }
        };

        let handle_common = self.devices.get_common(&device_id);

        info!("recv: {}", msg);

        match msg {
            QBIMessage::Sync { common, changes } => {
                assert!(handle_common == &common);

                // Find local changes
                let local_entries = self.changelog.after(&common).unwrap();

                // Apply changes to changelog
                let (mut entries, _) = QBChangelog::merge(local_entries.clone(), changes).unwrap();
                self.changelog.append(&mut entries);

                // Negotiate a new common hash
                let new_common = self.changelog.head();
                self.devices.set_common(&device_id, new_common);

                // Send sync to remote
                if !*syncing {
                    let msg = QBIMessage::Sync {
                        common,
                        changes: local_entries,
                    }
                    .into();
                    handle.tx.send(msg).await.unwrap();
                }

                *syncing = false;
            }
            // TODO: negotiate this instead
            QBIMessage::Common { common } => {
                self.devices.set_common(&device_id, common);
            }
            QBIMessage::Broadcast { msg } => broadcast.push(msg),
            QBIMessage::Device { .. } => {
                warn!("received init message, even though already initialized")
            }
        }

        // send the broadcast messages
        for msg in broadcast {
            for handle in self.interfaces.values_mut() {
                let msg = QBIMessage::Broadcast { msg: msg.clone() }.into();
                handle.tx.send(msg).await.unwrap();
            }
        }
    }

    /// Try to hook a hook to the master. Returns error if already hooked.
    pub async fn hook(&mut self, id: QBHId, cx: impl QBHContext) -> Result<()> {
        // make sure we do not hook a hook twice
        if self.is_hooked(&id) {
            return Err(Error::AlreadyHooked);
        }

        let (master_tx, master_rx) = tokio::sync::mpsc::channel::<QBHHostMessage>(32);

        // create the handle
        let handle = QBHHandle {
            join_handle: tokio::spawn(cx.run(QBHChannel::new(
                id.clone(),
                self.hook_tx.clone(),
                master_rx,
            ))),
            tx: master_tx,
        };

        self.hooks.insert(id.clone(), handle);

        Ok(())
    }

    /// Try to attach an interface to the master. Returns error if already attached.
    pub async fn attach(&mut self, id: QBIId, cx: impl QBIContext) -> Result<()> {
        // make sure we do not attach an interface twice
        if self.is_attached(&id) {
            return Err(Error::AlreadyAttached);
        }

        let (master_tx, master_rx) = tokio::sync::mpsc::channel::<QBIHostMessage>(32);

        // create the handle
        let handle = QBIHandle {
            join_handle: tokio::spawn(cx.run(
                self.devices.host_id.clone(),
                QBIChannel::new(id.clone(), self.interface_tx.clone(), master_rx),
            )),
            tx: master_tx,
            state: QBIState::Init,
        };

        self.interfaces.insert(id.clone(), handle);

        Ok(())
    }

    /// Returns whether an interface with the given id is attached to the master.
    #[inline(always)]
    pub fn is_attached(&self, id: &QBIId) -> bool {
        self.interfaces.contains_key(id)
    }

    /// Returns whether an interface with the given id is attached to the master.
    #[inline(always)]
    pub fn is_hooked(&self, id: &QBHId) -> bool {
        self.hooks.contains_key(id)
    }

    /// Detach the given interface and return a join handle.
    pub async fn detach(&mut self, id: &QBIId) -> Result<JoinHandle<()>> {
        let handle = self.interfaces.remove(id).ok_or(Error::NotFound)?;
        handle.tx.send(QBIHostMessage::Stop).await.unwrap();

        Ok(handle.join_handle)
    }

    /// Returns whether an interface with the given id is detached from the master.
    #[inline(always)]
    pub fn is_detached(&self, id: &QBIId) -> bool {
        self.interfaces.contains_key(id)
    }

    /// Synchronize changes across all interfaces.
    ///
    /// # Cancelation safety
    /// This method is not cancelation safe.
    pub async fn sync(&mut self) {
        for (_, handle) in self.interfaces.iter_mut() {
            // skip uninitialized
            if let QBIState::Available {
                ref device_id,
                ref mut syncing,
            } = handle.state
            {
                // skip syncing
                if *syncing {
                    continue;
                }

                let handle_common = self.devices.get_common(&device_id);
                let changes = self.changelog.after_cloned(handle_common).unwrap();

                // skip if no changes to sync
                if changes.is_empty() {
                    continue;
                }

                // synchronize
                *syncing = true;
                let msg = QBIMessage::Sync {
                    common: handle_common.clone(),
                    changes,
                }
                .into();
                handle.tx.send(msg).await.unwrap();
            }
        }
    }

    /// Send a message to an interface with the given id.
    ///
    /// This is expected to never fail.
    pub async fn send(&self, id: &QBIId, msg: impl Into<QBIHostMessage>) {
        let handle = self.interfaces.get(id).unwrap();
        handle.tx.send(msg.into()).await.unwrap()
    }
}
