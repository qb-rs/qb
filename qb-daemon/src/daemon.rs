//! # daemon
//!
//! This module houses the daemon, that is, the unit
//! which handles controlling tasks and processes the control
//! requests sent by those. It manages the [master].

use core::fmt;
use qb_core::{fs::wrapper::QBFSWrapper, path::qbpaths::INTERNAL_CONFIG};
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinSet};

use bitcode::{Decode, Encode};
use qb_ext::{
    control::{QBCId, QBCRequest, QBCResponse},
    hook::QBHContext,
    interface::QBIContext,
    QBExtId, QBExtSetup,
};
use qb_proto::{QBPBlob, QBPDeserialize, QBP};
use thiserror::Error;
use tracing::{info, info_span, trace, warn, Instrument};

use crate::master::QBMaster;

/// Error struct for daemons.
///
/// TODO: doc
#[derive(Error, Debug)]
pub enum Error {
    /// Protocol error
    #[error("protocol error: {0}")]
    Protocol(#[from] qb_proto::Error),
    /// Join error
    #[error("error while joining to QBI task")]
    JoinError(#[from] tokio::task::JoinError),
    /// NotFound error
    #[error("a QBI with the given id could not be found")]
    NotFound,
    /// NotSupported error
    #[error("this type of QBI is not supported")]
    NotSupported,
    /// Malformed error
    #[error("the given content is malformed")]
    Malformed,
    /// Master error
    #[error("master error: {0}")]
    MasterError(#[from] crate::master::Error),
}

/// Result type alias for making our life easier.
pub type Result<T> = std::result::Result<T, Error>;

/// Function pointer to a function which starts an interface.
pub type QBExtStartFn = Box<
    dyn for<'a> Fn(
            &'a mut QBMaster,
            QBExtId,
            &'a [u8],
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'a>>
        + Send
        + Sync,
>;
/// Function pointer to a function which sets up an interface.
pub type QBExtSetupFn = Box<dyn Fn(&mut SetupQueue, QBCId, String, QBPBlob) + Send + Sync>;

/// A struct which can be stored persistently that describes how to
/// start a specific extension using its kind's name and a data payload.
#[derive(Encode, Decode)]
pub struct QBExtDescriptor {
    name: String,
    data: Vec<u8>,
}

/// A handle to a task processing a QBP stream for controlling the daemon.
pub struct QBCHandle {
    tx: mpsc::Sender<QBCResponse>,
}

impl QBCHandle {
    /// Send a message to this handle
    pub async fn send(&self, msg: impl Into<QBCResponse>) {
        if let Err(err) = self.tx.send(msg.into()).await {
            warn!("could not send message to handle: {0}", err);
        }
    }
}

struct HandleInit<T>
where
    T: qb_proto::ReadWrite + fmt::Debug + Send + 'static,
{
    id: QBCId,
    conn: T,
    tx: mpsc::Sender<(QBCId, QBCRequest)>,
    rx: mpsc::Receiver<QBCResponse>,
}

/// A struct which can be stored persistently to configure a daemon.
#[derive(Encode, Decode, Default)]
pub struct QBDaemonConfig {
    ext_table: HashMap<QBExtId, QBExtDescriptor>,
    ext_autostart: HashSet<QBExtId>,
}

impl QBDaemonConfig {
    /// Try to get an interface from this table by its id.
    ///
    /// Returns Error::NotFound if the interface does not
    /// exist in this table.
    pub fn get(&self, id: &QBExtId) -> Result<&QBExtDescriptor> {
        self.ext_table.get(id).ok_or(Error::NotFound)
    }
}

/// TODO: doc
#[derive(Default)]
pub struct SetupQueue {
    join_set: JoinSet<(QBCId, Result<QBExtDescriptor>)>,
}

impl SetupQueue {
    /// TODO: doc
    pub async fn join(&mut self) -> (QBCId, Result<QBExtDescriptor>) {
        loop {
            match self.join_set.join_next().await {
                Some(Ok(val)) => return val,
                None => tokio::time::sleep(Duration::from_secs(1)).await,
                Some(Err(_)) => {}
            }
        }
    }
}

/// This struct represents a daemon, which handles connection to
/// control tasks and their communication.
pub struct QBDaemon {
    /// The master
    pub master: QBMaster,
    // "available QBIs" -> could be "attached" or "detached"
    // => every QBI that is attached to the master must be in this map
    start_fns: HashMap<String, QBExtStartFn>,
    setup_fns: HashMap<String, QBExtSetupFn>,
    config: QBDaemonConfig,
    wrapper: QBFSWrapper,

    /// TODO: doc
    pub setup: SetupQueue,

    // control stuff
    req_tx: mpsc::Sender<(QBCId, QBCRequest)>,
    /// A channel for receiving messages from controlling tasks
    pub req_rx: mpsc::Receiver<(QBCId, QBCRequest)>,
    handles: HashMap<QBCId, QBCHandle>,
}

impl QBDaemon {
    /// Build the daemon
    pub async fn init(master: QBMaster, wrapper: QBFSWrapper) -> Self {
        let (req_tx, req_rx) = mpsc::channel(10);
        let config = wrapper.dload(INTERNAL_CONFIG.as_ref()).await;
        Self {
            start_fns: Default::default(),
            setup_fns: Default::default(),
            handles: Default::default(),
            setup: Default::default(),
            master,
            wrapper,
            config,
            req_tx,
            req_rx,
        }
    }

    /// Start all available interfaces
    pub async fn autostart(&mut self) {
        // autostart
        let ids = self
            .config
            .ext_autostart
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.start(id).await.unwrap();
        }
    }

    /// Process the result of the setup queue.
    pub async fn process_setup(&mut self, (id, maybe_setup): (QBCId, Result<QBExtDescriptor>)) {
        match maybe_setup {
            Ok(val) => {
                // success: add the descriptor to this daemon
                self.add_already_setup(val).await.unwrap();

                if id.is_root() {
                    return;
                }

                let handle = self.handles.get(&id).unwrap();
                handle.send(QBCResponse::Success).await;
            }
            Err(err) => {
                warn!("error while setting up extension: {err}");

                if id.is_root() {
                    return;
                }

                // error: forward error to the QBCHandle which issued setup
                let handle = self.handles.get(&id).unwrap();
                handle
                    .send(QBCResponse::Error {
                        msg: err.to_string(),
                    })
                    .await;
            }
        }
    }

    /// Save daemon files
    pub async fn save(&self) {
        self.wrapper
            .save(INTERNAL_CONFIG.as_ref(), &self.config)
            .await
            .unwrap();
    }

    /// Start an interface by the given id.
    pub async fn start(&mut self, id: QBExtId) -> Result<()> {
        self.config.ext_autostart.insert(id.clone());
        self.save().await;
        let descriptor = self.config.get(&id)?;
        let name = &descriptor.name;
        let start = self.start_fns.get(name).ok_or(Error::NotSupported)?;
        start(&mut self.master, id, &descriptor.data).await?;
        Ok(())
    }

    /// Stop an interface by the given id.
    pub async fn stop(&mut self, id: QBExtId) -> Result<()> {
        self.config.ext_autostart.remove(&id);
        self.save().await;
        self.master.stop(&id).await?.await?;
        Ok(())
    }

    /// Add an interface.
    pub fn add(&mut self, caller: QBCId, name: String, blob: QBPBlob) -> Result<()> {
        let setup = self.setup_fns.get(&name).ok_or(Error::NotSupported)?;
        setup(&mut self.setup, caller, name, blob);
        Ok(())
    }

    /// Add an interface that has already been setup.
    pub async fn add_already_setup(&mut self, descriptor: QBExtDescriptor) -> Result<()> {
        let id = QBExtId::generate();
        self.config.ext_table.insert(id.clone(), descriptor);
        self.save().await;
        self.start(id).await?;
        Ok(())
    }

    /// Remove an interface
    pub async fn remove(&mut self, id: QBExtId) -> Result<()> {
        self.config.ext_autostart.remove(&id);
        if self.master.is_attached(&id) {
            self.master.detach(&id).await?.await?
        }
        self.config.ext_table.remove(&id);
        self.save().await;
        Ok(())
    }

    /// List the QBIs.
    pub fn list(&self) -> Vec<(QBExtId, String, String)> {
        self.config
            .ext_table
            .iter()
            .map(|(id, descriptor)| {
                let mut desc = match () {
                    _ if self.master.is_attached(id) => "attached",
                    _ if self.master.is_hooked(id) => "hooked",
                    _ => "not active",
                }
                .into();

                if self.config.ext_autostart.contains(id) {
                    desc += " - autostart";
                }

                (id.clone(), descriptor.name.clone(), desc)
            })
            .collect()
    }

    /// Register an interface kind.
    pub fn register_qbi<S, I>(&mut self, name: impl Into<String>)
    where
        S: QBExtSetup<I> + QBPDeserialize,
        I: QBIContext + Encode + for<'a> Decode<'a> + 'static,
    {
        let name = name.into();
        self.start_fns.insert(
            name.clone(),
            Box::new(move |qb, id, data| {
                Box::pin(async move {
                    qb.attach(id, bitcode::decode::<I>(data).unwrap())?;
                    Ok(())
                })
            }),
        );
        self.setup_fns.insert(
            name,
            Box::new(move |setup, caller, name, blob| {
                setup.join_set.spawn(async move {
                    let maybe_setup: Result<QBExtDescriptor> = async move {
                        let span = info_span!("qbi-setup", name);
                        let setup = blob.deserialize::<S>()?;
                        let cx = setup.setup().instrument(span).await;
                        let data = bitcode::encode(&cx);
                        Ok(QBExtDescriptor { name, data })
                    }
                    .await;
                    (caller, maybe_setup)
                });
            }),
        );
    }

    /// Register an interface kind.
    pub fn register_qbh<S, H, I>(&mut self, name: impl Into<String>)
    where
        S: QBExtSetup<H> + QBPDeserialize,
        H: QBHContext<I> + Encode + for<'a> Decode<'a> + Send + Sync + 'static,
        I: QBIContext + Any + Send,
    {
        let name = name.into();
        self.start_fns.insert(
            name.clone(),
            Box::new(move |qb, id, data| {
                Box::pin(async move {
                    qb.hook(id, bitcode::decode::<H>(data).unwrap()).await?;
                    Ok(())
                })
            }),
        );
        self.setup_fns.insert(
            name,
            Box::new(move |setup, caller, name, blob| {
                setup.join_set.spawn(async move {
                    let maybe_setup: Result<QBExtDescriptor> = async move {
                        let span = info_span!("qbi-setup", name);
                        let setup = blob.deserialize::<S>()?;
                        let cx = setup.setup().instrument(span).await;
                        let data = bitcode::encode(&cx);
                        Ok(QBExtDescriptor { name, data })
                    }
                    .await;
                    (caller, maybe_setup)
                });
            }),
        );
    }

    /// TODO: doc
    pub async fn process(&mut self, (caller, msg): (QBCId, QBCRequest)) {
        let resp = self._process(caller.clone(), msg).await;
        let handle = self.handles.get(&caller).unwrap();
        match resp {
            Ok(true) => handle.send(QBCResponse::Success).await,
            Ok(false) => {}
            Err(err) => {
                handle
                    .send(QBCResponse::Error {
                        msg: format!("{:?}", err),
                    })
                    .await
            }
        };
    }

    // internal, used for handling errors
    async fn _process(&mut self, caller: QBCId, msg: QBCRequest) -> Result<bool> {
        match msg {
            QBCRequest::Start { id } => self.start(id).await?,
            QBCRequest::Stop { id } => self.stop(id).await?,
            QBCRequest::Add { name, blob } => {
                self.add(caller, name, blob)?;
                return Ok(false);
            }
            QBCRequest::Remove { id } => self.remove(id).await?,
            QBCRequest::List => {
                let handle = self.handles.get(&caller).unwrap();
                handle.send(QBCResponse::List { list: self.list() }).await;
                return Ok(false);
            }
            _ => unimplemented!(),
        };

        Ok(true)
    }

    /// Initialize a handle
    pub async fn init_handle<T>(&mut self, conn: T)
    where
        T: qb_proto::ReadWrite + fmt::Debug + Send + 'static,
    {
        let id = QBCId::generate();
        let (resp_tx, resp_rx) = mpsc::channel::<QBCResponse>(10);
        self.handles.insert(id.clone(), QBCHandle { tx: resp_tx });

        let init = HandleInit {
            tx: self.req_tx.clone(),
            rx: resp_rx,
            conn,
            id,
        };

        tokio::spawn(handle_run(init));
    }
}

async fn handle_run<T>(mut init: HandleInit<T>)
where
    T: qb_proto::ReadWrite + fmt::Debug + Send + 'static,
{
    let span = info_span!("handle", id = init.id.to_hex());

    if let Err(err) = _handle_run(&mut init).instrument(span.clone()).await {
        span.in_scope(|| info!("handle finished with: {:?}", err));
    }
}

async fn _handle_run<T>(init: &mut HandleInit<T>) -> Result<()>
where
    T: qb_proto::ReadWrite + fmt::Debug + Send + 'static,
{
    trace!("create new handle with id={} conn={:?}", init.id, init.conn);

    let mut protocol = QBP::default();

    loop {
        tokio::select! {
            Some(response) = init.rx.recv() => {
                // write a message to the socket
                trace!("send {}", response);
                protocol.send(&mut init.conn, response).await?;
            }
            res = protocol.update::<QBCRequest>(&mut init.conn) => {
                match res {
                    Ok(msg) => {
                        init.tx.send((init.id.clone(), msg)).await.unwrap();
                    }
                    Err(err) => return Err(err.into()),
                }

            }
        }
    }
}
