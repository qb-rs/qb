//! # daemon
//!
//! This module houses the daemon, that is, the unit
//! which handles controlling tasks and processes the control
//! requests sent by those. It manages the [master].

use std::{collections::HashMap, future::Future, pin::Pin};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};

use bitcode::{Decode, DecodeOwned, Encode};
use interprocess::local_socket::tokio::Stream;
use qb_ext::{
    control::{QBCId, QBCRequest, QBCResponse},
    interface::{QBIContext, QBIId, QBISetup},
};
use qb_proto::{QBPBlob, QBP};
use thiserror::Error;
use tracing::{trace, trace_span, warn, Instrument};

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
pub type QBISetupFn =
    Box<dyn Fn(QBPBlob) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + 'static>>>;

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

/// A struct which can be stored persistently that describes how to
/// start a specific interface using its kind's name and a data payload.
#[derive(Encode, Decode)]
pub struct QBIDescriptior {
    name: String,
    data: Vec<u8>,
}

/// A struct which can be stored persistently to configure a daemon.
#[derive(Encode, Decode, Default)]
pub struct QBDaemonConfig {
    qbi_table: HashMap<QBIId, QBIDescriptior>,
    autostart: Vec<QBIId>,
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

    req_tx: mpsc::Sender<(QBCId, QBCRequest)>,
    /// A channel for receiving messages from controlling tasks
    pub req_rx: mpsc::Receiver<(QBCId, QBCRequest)>,
    handles: HashMap<QBCId, QBCHandle>,
}

impl QBDaemon {
    /// The path to the config
    pub const CONFIG_PATH: &'static str = "./qb-daemon.bin";

    /// Build the daemon
    pub async fn init(master: QBMaster) -> Self {
        let (req_tx, req_rx) = mpsc::channel(10);
        let config = Self::load_conf().await;
        Self {
            master,
            start_fns: Default::default(),
            setup_fns: Default::default(),
            handles: Default::default(),
            config,
            req_tx,
            req_rx,
        }
    }

    /// Start an interface by the given id.
    pub async fn start(&mut self, id: QBIId) -> Result<()> {
        let descriptor = self.config.get(&id)?;
        let name = &descriptor.name;
        let start = self.start_fns.get(name).ok_or(Error::NotSupported)?;
        start(&mut self.master, id, &descriptor.data).await?;
        Ok(())
    }

    /// Stop an interface by the given id.
    pub async fn stop(&mut self, id: QBIId) -> Result<()> {
        self.master.detach(&id).await?.await?;
        Ok(())
    }

    /// Save a configuration to the default path.
    pub async fn save_conf(&mut self) {
        let content = bitcode::encode(&self.config);
        let mut conf_file = File::create(Self::CONFIG_PATH).await.unwrap();
        conf_file.write_all(&content).await.unwrap();
    }

    /// Load a configuration from the default path.
    pub async fn load_conf() -> QBDaemonConfig {
        let exists = match tokio::fs::metadata(Self::CONFIG_PATH).await {
            Ok(meta) => meta.is_file(),
            Err(_) => false,
        };

        if exists {
            let mut conf_file = File::open(Self::CONFIG_PATH).await.unwrap();
            let mut contents = Vec::new();
            conf_file.read_to_end(&mut contents).await.unwrap();
            bitcode::decode(&contents).unwrap()
        } else {
            Default::default()
        }
    }

    /// Setup an interface.
    pub async fn setup(&mut self, name: String, blob: QBPBlob) -> Result<()> {
        let setup = self.setup_fns.get(&name).ok_or(Error::NotSupported)?;
        let data = setup(blob).await?;
        let id = QBIId::generate();
        self.config
            .qbi_table
            .insert(id.clone(), QBIDescriptior { name, data });
        self.save_conf().await;
        self.start(id).await.unwrap();
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
    pub fn register<T>(&mut self, name: impl Into<String>)
    where
        for<'a> T: QBIContext + QBISetup<'a> + DecodeOwned,
    {
        let name = name.into();
        self.start_fns.insert(
            name.clone(),
            Box::new(move |qb, id, data| {
                Box::pin(async move {
                    qb.attach(id, bitcode::decode::<T>(&data).unwrap()).await?;
                    Ok(())
                })
            }),
        );
        self.setup_fns.insert(
            name,
            Box::new(move |blob| {
                Box::pin(async move {
                    let cx = blob.deserialize::<T>()?;
                    let data = bitcode::encode(&cx);
                    cx.setup().await;
                    Ok(data)
                })
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
            QBCRequest::Setup { name, blob } => {
                self.setup(name, blob).await?;
            }
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
    let span = trace_span!("handle", id = init.id.to_hex());

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
