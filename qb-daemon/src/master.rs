//! # master
//!
//! This module houses the master, that is, the unit
//! which handles interfaces and their communication.
//! It owns a device table and a changelog to allow syncing.

use std::{collections::HashMap, future::Future, pin::Pin, rc::Rc};

use qb_core::{
    change::QBChangeMap,
    device::{QBDeviceId, QBDeviceTable},
    fs::wrapper::QBFSWrapper,
    path::qbpaths::{INTERNAL_CHANGEMAP, INTERNAL_DEVICES},
};
use qb_ext::{
    hook::{QBHChannel, QBHContext, QBHHostMessage, QBHId, QBHSlaveMessage},
    interface::{QBIChannel, QBIContext, QBIHostMessage, QBIId, QBIMessage, QBISlaveMessage},
};
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, info, info_span, warn, Instrument};

/// An error that occured related to the master
#[derive(Error, Debug)]
pub enum Error {
    /// This error propagates when we try to detach an interface or unhook a hook
    /// with an id, of which none such interface exists in this master.
    #[error("no interface with the given id was found")]
    NotFound,
    /// This error propagates when we try to attach an interface
    /// with an id, of which another interface is already attached.
    #[error("an interface with the same id is already attached")]
    AlreadyAttached,
    /// This error propagates when we try to hook a hook
    /// with an id, of which another hook is already hooked.
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
    handler_fn: QBHHandlerFn,
    tx: mpsc::Sender<QBHHostMessage>,
}

/// A hook handler function. This is needed, because the type of the
/// interface context we send over the mpsc is Any and therefore the
/// context must be downcast individually.
pub type QBHHandlerFn = Rc<
    Box<
        dyn for<'a> Fn(&'a mut QBMaster, QBHSlaveMessage) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
    >,
>;

/// The master, that is, the struct that houses connection
/// to the individual interfaces and manages communication.
pub struct QBMaster {
    interfaces: HashMap<QBIId, QBIHandle>,
    /// Receiver for messages coming from interfaces.
    pub interface_rx: mpsc::Receiver<(QBIId, QBISlaveMessage)>,
    interface_tx: mpsc::Sender<(QBIId, QBISlaveMessage)>,
    hooks: HashMap<QBHId, QBHHandle>,
    /// Receiver for messages coming from hooks.
    pub hook_rx: mpsc::Receiver<(QBHId, QBHSlaveMessage)>,
    hook_tx: mpsc::Sender<(QBHId, QBHSlaveMessage)>,
    devices: QBDeviceTable,
    changemap: QBChangeMap,
    wrapper: QBFSWrapper,
}

impl QBMaster {
    /// Initialize the master with the given device id.
    ///
    /// This identifier should be negotiated and then stored
    /// somewhere and not randomly initialized every boot.
    pub async fn init(wrapper: QBFSWrapper) -> QBMaster {
        let (interface_tx, interface_rx) = mpsc::channel(10);
        let (hook_tx, hook_rx) = mpsc::channel(10);

        wrapper.init().await.unwrap();
        let devices = wrapper.dload(INTERNAL_DEVICES.as_ref()).await;
        let changemap = wrapper.dload(INTERNAL_CHANGEMAP.as_ref()).await;

        QBMaster {
            interfaces: HashMap::new(),
            interface_rx,
            interface_tx,
            hooks: HashMap::new(),
            hook_rx,
            hook_tx,
            devices,
            changemap,
            wrapper,
        }
    }

    /// TODO: doc
    pub async fn save(&self) {
        self.wrapper
            .save(INTERNAL_DEVICES.as_ref(), &self.devices)
            .await
            .unwrap();
        self.wrapper
            .save(INTERNAL_CHANGEMAP.as_ref(), &self.changemap)
            .await
            .unwrap();
    }

    /// This will process a message from a hook.
    ///
    /// # Cancelation Safety
    /// This method is not cancelation safe.
    pub async fn hprocess(&mut self, (id, msg): (QBHId, QBHSlaveMessage)) {
        let handle = self.hooks.get(&id).unwrap();
        let handler_fn = handle.handler_fn.clone();
        handler_fn(self, msg).await;
    }

    /// Remove unused handles [from interfaces that have finished]
    fn iclean_handles(&mut self) {
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

    /// This will process a message from an interface.
    ///
    /// # Cancelation Safety
    /// This method is not cancelation safe.
    pub async fn iprocess(&mut self, (id, msg): (QBIId, QBISlaveMessage)) {
        self.iclean_handles();

        let mut broadcast = Vec::new();

        // unwrap it
        let msg = match msg {
            QBISlaveMessage::Message(msg) => msg,
        };

        let span = info_span!("qbi-process", id = id.to_hex());
        let _guard = span.enter();
        let handle = self.interfaces.get_mut(&id).unwrap();

        debug!("recv: {}", msg);

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

        match msg {
            QBIMessage::Sync { common, .. } => {
                assert!(handle_common == &common);

                // Find local changes
                let local_entries = self.changemap.since(&common);

                // Apply changes to changelog
                // TODO: merging
                //let (mut entries, _) = QBChangelog::merge(local_entries.clone(), changes).unwrap();
                //self.changemap.append(&mut entries);

                // find the new common hash
                let new_common = self.changemap.head().clone();
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
                self.save().await;
                self.sync().await;
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
    pub async fn hook<T: QBIContext + 'static>(
        &mut self,
        id: QBHId,
        cx: impl QBHContext<T>,
    ) -> Result<()> {
        let span = info_span!("qb-hook", id = id.to_hex());

        // make sure we do not hook a hook twice
        if self.is_hooked(&id) {
            return Err(Error::AlreadyHooked);
        }

        let (master_tx, master_rx) = tokio::sync::mpsc::channel::<QBHHostMessage>(32);

        // create the handle
        let handle = QBHHandle {
            handler_fn: Rc::new(Box::new(move |master, msg| {
                Box::pin(async move {
                    match msg {
                        QBHSlaveMessage::Attach { context } => {
                            // downcast the context
                            let context = *context.downcast::<T>().unwrap();
                            master.attach(QBIId::generate(), context).await.unwrap();
                        }
                    }
                })
            })),
            join_handle: tokio::spawn(
                cx.run(QBHChannel::new(id.clone(), self.hook_tx.clone(), master_rx).into())
                    .instrument(span),
            ),
            tx: master_tx,
        };

        self.hooks.insert(id.clone(), handle);

        Ok(())
    }

    /// Try to attach an interface to the master. Returns error if already attached.
    pub async fn attach(&mut self, id: QBIId, cx: impl QBIContext) -> Result<()> {
        let span = info_span!("qb-interface", id = id.to_hex());

        // make sure we do not attach an interface twice
        if self.is_attached(&id) {
            return Err(Error::AlreadyAttached);
        }

        let (master_tx, master_rx) = tokio::sync::mpsc::channel::<QBIHostMessage>(32);

        // create the handle
        let handle = QBIHandle {
            join_handle: tokio::spawn(
                cx.run(
                    self.devices.host_id.clone(),
                    QBIChannel::new(id.clone(), self.interface_tx.clone(), master_rx),
                )
                .instrument(span),
            ),
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

    /// Detach the given hook and return a join handle.
    pub async fn unhook(&mut self, id: &QBHId) -> Result<JoinHandle<()>> {
        let handle = self.hooks.remove(id).ok_or(Error::NotFound)?;
        handle.tx.send(QBHHostMessage::Stop).await.unwrap();

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
        for (id, handle) in self.interfaces.iter_mut() {
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
                let changes = self.changemap.since_cloned(handle_common);

                // skip if no changes to sync
                if changes.is_empty() {
                    continue;
                }

                info!("syncing with {}", id);

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
