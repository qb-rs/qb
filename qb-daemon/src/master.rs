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
    hook::{QBHChannel, QBHContext, QBHHostMessage, QBHSlaveMessage},
    interface::{QBIChannel, QBIContext, QBIHostMessage, QBIMessage, QBISlaveMessage},
    QBExtId,
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
    qbi_handles: HashMap<QBExtId, QBIHandle>,
    /// Receiver for messages coming from interfaces.
    pub qbi_rx: mpsc::Receiver<(QBExtId, QBISlaveMessage)>,
    qbi_tx: mpsc::Sender<(QBExtId, QBISlaveMessage)>,

    qbh_handles: HashMap<QBExtId, QBHHandle>,
    /// Receiver for messages coming from hooks.
    pub qbh_rx: mpsc::Receiver<(QBExtId, QBHSlaveMessage)>,
    qbh_tx: mpsc::Sender<(QBExtId, QBHSlaveMessage)>,

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
            qbi_handles: HashMap::new(),
            qbi_rx: interface_rx,
            qbi_tx: interface_tx,
            qbh_handles: HashMap::new(),
            qbh_rx: hook_rx,
            qbh_tx: hook_tx,
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
    pub async fn hprocess(&mut self, (id, msg): (QBExtId, QBHSlaveMessage)) {
        let handle = self.qbh_handles.get(&id).unwrap();
        let handler_fn = handle.handler_fn.clone();
        handler_fn(self, msg).await;
    }

    /// Remove unused handles [from interfaces that have finished]
    fn iclean_handles(&mut self) {
        let to_remove = self
            .qbi_handles
            .iter()
            .filter(|(_, v)| v.join_handle.is_finished())
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for id in to_remove {
            self.qbi_handles.remove(&id);
        }
    }

    /// This will process a message from an interface.
    ///
    /// # Cancelation Safety
    /// This method is not cancelation safe.
    pub async fn iprocess(&mut self, (id, msg): (QBExtId, QBISlaveMessage)) {
        self.iclean_handles();

        let mut broadcast = Vec::new();

        // unwrap it
        let msg = match msg {
            QBISlaveMessage::Message(msg) => msg,
            _ => unimplemented!(),
        };

        let span = info_span!("qbi-process", id = id.to_hex());
        let _guard = span.enter();
        let handle = self.qbi_handles.get_mut(&id).unwrap();

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
                        self.devices.set_common(device_id, common);
                        handle.state = QBIState::Available {
                            device_id: device_id.clone(),
                            syncing: false,
                        };
                        self.sync().await;
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

        let handle_common = self.devices.get_common(device_id);

        match msg {
            QBIMessage::Sync {
                common,
                changes: remote,
            } => {
                assert!(handle_common == &common);

                // Find local changes
                let local = self.changemap.since(&common);

                // Apply changes to changelog
                let mut changemap = local.clone();
                _ = changemap.merge(remote).unwrap();
                self.changemap.append(changemap);

                // find the new common hash
                let new_common = self.changemap.head().clone();
                debug!("new common: {}", new_common);
                self.devices.set_common(device_id, new_common);

                // Send sync to remote
                if !*syncing {
                    let msg = QBIMessage::Sync {
                        common,
                        changes: local,
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
                self.devices.set_common(device_id, common);
            }
            QBIMessage::Broadcast { msg } => broadcast.push(msg),
            QBIMessage::Device { .. } => {
                warn!("received init message, even though already initialized")
            }
        }

        // send the broadcast messages
        for msg in broadcast {
            for handle in self.qbi_handles.values_mut() {
                let msg = QBIMessage::Broadcast { msg: msg.clone() }.into();
                handle.tx.send(msg).await.unwrap();
            }
        }
    }

    /// Try to hook a hook to the master. Returns error if already hooked.
    pub async fn hook<T: QBIContext + 'static>(
        &mut self,
        id: QBExtId,
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
                            master.attach(QBExtId::generate(), context).await.unwrap();
                        }
                        _ => unimplemented!(),
                    }
                })
            })),
            join_handle: tokio::spawn(
                cx.run(QBHChannel::new(id.clone(), self.qbh_tx.clone(), master_rx).into())
                    .instrument(span),
            ),
            tx: master_tx,
        };

        self.qbh_handles.insert(id.clone(), handle);

        Ok(())
    }

    /// Try to attach an interface to the master. Returns error if already attached.
    pub async fn attach(&mut self, id: QBExtId, cx: impl QBIContext) -> Result<()> {
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
                    QBIChannel::new(id.clone(), self.qbi_tx.clone(), master_rx),
                )
                .instrument(span),
            ),
            tx: master_tx,
            state: QBIState::Init,
        };

        self.qbi_handles.insert(id.clone(), handle);

        Ok(())
    }

    /// Returns whether an interface with the given id is attached to the master.
    #[inline(always)]
    pub fn is_attached(&self, id: &QBExtId) -> bool {
        self.qbi_handles.contains_key(id)
    }

    /// Returns whether an interface with the given id is attached to the master.
    #[inline(always)]
    pub fn is_hooked(&self, id: &QBExtId) -> bool {
        self.qbh_handles.contains_key(id)
    }

    /// Detach the given interface and return a join handle.
    pub async fn detach(&mut self, id: &QBExtId) -> Result<JoinHandle<()>> {
        let handle = self.qbi_handles.remove(id).ok_or(Error::NotFound)?;
        handle.tx.send(QBIHostMessage::Stop).await.unwrap();

        Ok(handle.join_handle)
    }

    /// Detach the given hook and return a join handle.
    pub async fn unhook(&mut self, id: &QBExtId) -> Result<JoinHandle<()>> {
        let handle = self.qbh_handles.remove(id).ok_or(Error::NotFound)?;
        handle.tx.send(QBHHostMessage::Stop).await.unwrap();

        Ok(handle.join_handle)
    }

    /// Stop an interface or hook with the given id.
    pub async fn stop(&mut self, id: &QBExtId) -> Result<JoinHandle<()>> {
        if self.is_attached(id) {
            return self.detach(id).await;
        }

        if self.is_hooked(id) {
            return self.unhook(id).await;
        }

        Err(Error::NotFound)
    }

    /// Returns whether an interface with the given id is detached from the master.
    #[inline(always)]
    pub fn is_detached(&self, id: &QBExtId) -> bool {
        self.qbi_handles.contains_key(id)
    }

    /// Synchronize changes across all interfaces.
    ///
    /// # Cancelation safety
    /// This method is not cancelation safe.
    pub async fn sync(&mut self) {
        for (id, handle) in self.qbi_handles.iter_mut() {
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

                let handle_common = self.devices.get_common(device_id);
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
    pub async fn send(&self, id: &QBExtId, msg: impl Into<QBIHostMessage>) {
        let handle = self.qbi_handles.get(id).unwrap();
        handle.tx.send(msg.into()).await.unwrap()
    }
}
