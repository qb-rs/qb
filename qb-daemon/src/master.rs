//! # master
//!
//! This module houses the master, that is, the unit
//! which handles interfaces and their communication.
//! It owns a device table and a changelog to allow syncing.

use std::{collections::HashMap, time::Duration};

use qb_core::{
    change::log::QBChangelog,
    common::device::{QBDeviceId, QBDeviceTable},
    interface::{Message, QBICommunication, QBIContext, QBIHostMessage, QBIId, QBISlaveMessage},
};
use thiserror::Error;
use tokio::{
    sync::mpsc,
    task::{AbortHandle, JoinHandle, JoinSet},
};
use tracing::{info, info_span};

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
}

/// Result type alias for making our life easier.
pub type Result<T> = std::result::Result<T, Error>;

/// A handle to an interface.
pub struct QBIHandle {
    join_handle: JoinHandle<()>,
    abort_handle: AbortHandle,
    tx: mpsc::Sender<QBIHostMessage>,
    syncing: bool,
    init: bool,
}

// struct used for pool receiving
struct Recv {
    id: QBIId,
    rx: mpsc::Receiver<QBISlaveMessage>,
    msg: Option<QBISlaveMessage>,
}

impl QBIHandle {
    /// Send a message to this interface.
    pub async fn send(&self, msg: impl Into<QBIHostMessage>) {
        self.tx.send(msg.into()).await.unwrap()
    }
}

/// The master, that is, the struct that houses connection
/// to the individual interfaces and manages communication.
pub struct QBMaster {
    handles: HashMap<QBIId, QBIHandle>,
    devices: QBDeviceTable,
    changelog: QBChangelog,
    device_id: QBDeviceId,
    recv_pool: JoinSet<Recv>,
}

impl QBMaster {
    /// Initialize the master with the given device id.
    ///
    /// This identifier should be negotiated and then stored
    /// somewhere and not randomly initialized every boot.
    pub fn init(device_id: QBDeviceId) -> QBMaster {
        QBMaster {
            handles: HashMap::new(),
            recv_pool: JoinSet::new(),
            device_id,
            // TODO: pass through constructor, these should be persistent
            devices: Default::default(),
            changelog: Default::default(),
        }
    }

    /// Remove unused handles [from interfaces that have finished]
    pub fn clean_handles(&mut self) {
        let to_remove = self
            .handles
            .iter()
            .filter(|(_, v)| v.join_handle.is_finished())
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for id in to_remove {
            self.handles.remove(&id);
        }
    }

    /// Receive a message.
    ///
    /// # Cancelation safety
    /// This method is cancelation safe
    pub async fn read(&mut self) -> (QBIId, QBISlaveMessage) {
        // read loop
        loop {
            match self.recv_pool.join_next().await {
                // handle message
                Some(Ok(Recv {
                    id,
                    mut rx,
                    msg: Some(msg),
                })) => {
                    // respawn receive task
                    let handle = self.handles.get_mut(&id).unwrap();
                    handle.abort_handle = self.recv_pool.spawn({
                        let id = id.clone();
                        async move {
                            let msg = rx.recv().await;
                            Recv { rx, msg, id }
                        }
                    });

                    return (id, msg);
                }
                // propagate the error
                Some(Err(err)) if err.is_panic() => {
                    std::panic::resume_unwind(err.into_panic());
                }
                // no entry in join pool, delay to avoid high cpu usage
                None => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                // the mpsc was closed, we do not respawn the receive task
                _ => {}
            }
        }
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
        let handle = self.handles.get_mut(&id).unwrap();
        let handle_common = self.devices.get_common(&id.device_id);

        info!("recv: {}", msg);

        match msg {
            Message::Sync { common, changes } => {
                assert!(handle_common == &common);

                // Find local changes
                let local_entries = self.changelog.after(&common).unwrap();

                // Apply changes to changelog
                let (mut entries, _) = QBChangelog::merge(local_entries.clone(), changes).unwrap();
                self.changelog.append(&mut entries);

                // Negotiate a new common hash
                let new_common = self.changelog.head();
                self.devices.set_common(&id.device_id, new_common);

                // Send sync to remote
                if !handle.syncing {
                    handle
                        .send(Message::Sync {
                            common,
                            changes: local_entries,
                        })
                        .await;
                }

                handle.syncing = false;
            }
            Message::Common { common } => {
                handle.init = true;
                self.devices.set_common(&id.device_id, common);
            }
            Message::Broadcast { msg } => broadcast.push(msg),
        }

        // send the broadcast messages
        for msg in broadcast {
            for handle in self.handles.values_mut() {
                handle.send(Message::Broadcast { msg: msg.clone() }).await;
            }
        }
    }

    /// Try to attach a QBI to the master. Returns none if already attached.
    pub async fn attach(&mut self, id: QBIId, cx: impl QBIContext) -> Result<()> {
        // make sure we do not attach an interface twice
        if self.is_attached(&id) {
            return Err(Error::AlreadyAttached);
        }

        let (main_tx, qbi_rx) = tokio::sync::mpsc::channel::<QBIHostMessage>(32);
        let (qbi_tx, main_rx) = tokio::sync::mpsc::channel::<QBISlaveMessage>(32);

        // spawn receive task
        let abort_handle = self.recv_pool.spawn({
            let id = id.clone();
            let mut rx = main_rx;
            async move {
                let msg = rx.recv().await;
                Recv { rx, msg, id }
            }
        });

        // create the handle
        let handle = QBIHandle {
            join_handle: tokio::spawn(cx.run(
                self.device_id.clone(),
                QBICommunication {
                    rx: qbi_rx,
                    tx: qbi_tx,
                },
            )),
            abort_handle,
            tx: main_tx,
            syncing: false,
            init: false,
        };

        self.handles.insert(id.clone(), handle);

        Ok(())
    }

    /// Returns whether an interface with the given id is attached to the master.
    #[inline(always)]
    pub fn is_attached(&self, id: &QBIId) -> bool {
        self.handles.contains_key(id)
    }

    /// Detach the given interface and return a join handle.
    pub async fn detach(&mut self, id: &QBIId) -> Result<JoinHandle<()>> {
        let handle = self.handles.remove(id).ok_or(Error::NotFound)?;
        handle.send(QBIHostMessage::Stop).await;

        Ok(handle.join_handle)
    }

    /// Returns whether an interface with the given id is detached from the master.
    #[inline(always)]
    pub fn is_detached(&self, id: &QBIId) -> bool {
        self.handles.contains_key(id)
    }

    /// Synchronize changes across all interfaces.
    ///
    /// # Cancelation safety
    /// This method is not cancelation safe.
    pub async fn sync(&mut self) {
        for (id, handle) in self.handles.iter_mut() {
            // skip uninitialized
            if !handle.init {
                continue;
            }

            // skip already synchronizing interfaces
            if handle.syncing {
                continue;
            }

            let handle_common = self.devices.get_common(&id.device_id);
            let changes = self.changelog.after_cloned(handle_common).unwrap();

            // skip if no changes to sync
            if changes.is_empty() {
                continue;
            }

            // synchronize
            handle.syncing = true;
            handle
                .send(Message::Sync {
                    common: handle_common.clone(),
                    changes,
                })
                .await;
        }
    }

    /// Send a message to an interface with the given id.
    ///
    /// This is expected to never fail.
    pub async fn send(&self, id: &QBIId, msg: impl Into<QBIHostMessage>) {
        self.handles.get(id).unwrap().send(msg).await
    }
}
