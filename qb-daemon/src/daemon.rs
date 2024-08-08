//! # daemon
//!
//! This module houses the daemon, that is, the unit
//! which handles controlling tasks and processes the control
//! requests sent by those. It manages the [master].

use qb_core::{common::qbpaths::INTERNAL_CONFIG, fs::wrapper::QBFSWrapper};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinSet};

use bitcode::{Decode, Encode};
use interprocess::local_socket::tokio::Stream;
use qb_ext::{
    control::{QBCId, QBCRequest, QBCResponse},
    interface::{QBIContext, QBIId, QBISetup},
};
use qb_proto::{QBPBlob, QBPDeserialize, QBP};
use thiserror::Error;
use tracing::{info_span, trace, warn, Instrument};

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
pub type QBIStartFn = Box<
    dyn for<'a> Fn(
        &'a mut QBMaster,
        QBIId,
        &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>,
>;
/// Function pointer to a function which sets up an interface.
pub type QBISetupFn = Box<dyn Fn(&mut JoinSet<Result<QBIDescriptior>>, String, QBPBlob)>;

/// A struct which can be stored persistently that describes how to
/// start a specific interface using its kind's name and a data payload.
#[derive(Encode, Decode)]
pub struct QBIDescriptior {
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
        match self.tx.send(msg.into()).await {
            Err(err) => warn!("could not send message to handle: {0}", err),
            Ok(_) => {}
        };
    }
}

struct HandleInit {
    id: QBCId,
    conn: Stream,
    tx: mpsc::Sender<(QBCId, QBCRequest)>,
    rx: mpsc::Receiver<QBCResponse>,
}

/// A struct which can be stored persistently to configure a daemon.
#[derive(Encode, Decode, Default)]
pub struct QBDaemonConfig {
    qbi_table: HashMap<QBIId, QBIDescriptior>,
    autostart: HashSet<QBIId>,
}

impl QBDaemonConfig {
    /// Try to get an interface from this table by its id.
    ///
    /// Returns Error::NotFound if the interface does not
    /// exist in this table.
    pub fn get(&self, id: &QBIId) -> Result<&QBIDescriptior> {
        self.qbi_table.get(id).ok_or(Error::NotFound)
    }

    /// Insert a new interface into this table.
    pub fn insert(&mut self, id: QBIId, descriptor: QBIDescriptior) {
        self.qbi_table.insert(id, descriptor);
    }
}

/// TODO: doc
#[derive(Default)]
pub struct Setup {
    join_set: JoinSet<Result<QBIDescriptior>>,
}

impl Setup {
    /// TODO: doc
    pub async fn join(&mut self) -> QBIDescriptior {
        loop {
            match self.join_set.join_next().await {
                Some(Ok(Ok(val))) => return val,
                Some(Ok(Err(err))) => warn!("setup error: {}", err),
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
    start_fns: HashMap<String, QBIStartFn>,
    setup_fns: HashMap<String, QBISetupFn>,
    config: QBDaemonConfig,
    wrapper: QBFSWrapper,

    /// TODO: doc
    pub setup: Setup,

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
        let ids = self.config.autostart.iter().cloned().collect::<Vec<_>>();
        for id in ids {
            self.start(id).await.unwrap();
        }
    }

    /// TODO: doc
    pub async fn process_setup(&mut self, descriptor: QBIDescriptior) -> Result<()> {
        let id = QBIId::generate();
        self.config.qbi_table.insert(id.clone(), descriptor);
        self.save().await;
        self.start(id).await?;
        Ok(())
    }

    /// Save daemon files
    pub async fn save(&self) {
        self.wrapper
            .save(INTERNAL_CONFIG.as_ref(), &self.config)
            .await
            .unwrap();
    }

    /// Start an interface by the given id.
    pub async fn start(&mut self, id: QBIId) -> Result<()> {
        self.config.autostart.insert(id.clone());
        let descriptor = self.config.get(&id)?;
        let name = &descriptor.name;
        let start = self.start_fns.get(name).ok_or(Error::NotSupported)?;
        start(&mut self.master, id, &descriptor.data).await?;
        Ok(())
    }

    /// Stop an interface by the given id.
    pub async fn stop(&mut self, id: QBIId) -> Result<()> {
        self.config.autostart.remove(&id);
        self.master.detach(&id).await?.await?;
        Ok(())
    }

    /// Add an interface.
    pub fn add(&mut self, name: String, blob: QBPBlob) -> Result<()> {
        let setup = self.setup_fns.get(&name).ok_or(Error::NotSupported)?;
        setup(&mut self.setup.join_set, name, blob);
        Ok(())
    }

    /// Remove an interface
    pub async fn remove(&mut self, id: &QBIId) -> Result<()> {
        self.config.autostart.remove(&id);
        if self.master.is_attached(&id) {
            self.master.detach(&id).await?.await?
        }
        self.config.qbi_table.remove(&id);
        self.save().await;
        Ok(())
    }

    /// List the QBIs.
    pub fn list(&self) -> Vec<(QBIId, String, bool)> {
        self.config
            .qbi_table
            .iter()
            .map(|(id, descriptor)| {
                (
                    id.clone(),
                    descriptor.name.clone(),
                    self.master.is_attached(id),
                )
            })
            .collect()
    }

    /// Register a QBI kind.
    pub fn register<S, I>(&mut self, name: impl Into<String>)
    where
        S: QBISetup<I> + QBPDeserialize,
        I: QBIContext + Encode + for<'a> Decode<'a> + 'static,
    {
        let name = name.into();
        self.start_fns.insert(
            name.clone(),
            Box::new(move |qb, id, data| {
                Box::pin(async move {
                    qb.attach(id, bitcode::decode::<I>(&data).unwrap()).await?;
                    Ok(())
                })
            }),
        );
        self.setup_fns.insert(
            name,
            Box::new(move |join_set, name, blob| {
                join_set.spawn(async move {
                    let span = info_span!("qbi-setup", name);
                    let setup = blob.deserialize::<S>()?;
                    let cx = setup.setup().instrument(span).await;
                    let data = bitcode::encode(&cx);
                    Ok(QBIDescriptior { name, data })
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
            QBCRequest::Add { name, blob } => self.add(name, blob)?,
            QBCRequest::Remove { id } => self.remove(&id).await?,
            QBCRequest::List => {
                let handle = self.handles.get(&caller).unwrap();
                handle.send(QBCResponse::List { list: self.list() }).await;
                return Ok(false);
            }
        };

        Ok(true)
    }

    /// Initialize a handle
    pub async fn init_handle(&mut self, conn: Stream) {
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

async fn handle_run(mut init: HandleInit) {
    let span = info_span!("handle", id = init.id.to_hex());

    match _handle_run(&mut init).instrument(span.clone()).await {
        Err(err) => span.in_scope(|| warn!("handle finished with error: {:?}", err)),
        Ok(_) => {}
    }
}

async fn _handle_run(init: &mut HandleInit) -> Result<()> {
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
