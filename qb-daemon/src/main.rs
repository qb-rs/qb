use core::panic;
use std::{collections::HashMap, fs::File, future::Future, pin::Pin, sync::Arc};

use bitcode::{Decode, DecodeOwned, Encode};
use interprocess::local_socket::{
    tokio::Stream, traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode,
    ListenerOptions, ToNsName,
};
use qb_control::{qbi_local::QBILocal, QBControlRequest, QBControlResponse};
use qb_core::{
    common::id::QBId,
    interface::{QBIBridgeMessage, QBIContext, QBIId, QBISetup},
    QB,
};
use qb_proto::{QBPBlob, QBP};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{span, trace, warn, Instrument, Level};
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Error, Debug)]
pub enum Error {
    #[error("protocol error: {0}")]
    Protocol(#[from] qb_proto::Error),
    #[error("error while joining to QBI task")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("a QBI with the given id could not be found")]
    NotFound,
    #[error("this type of QBI is not supported")]
    NotSupported,
    #[error("the given content is malformed")]
    Malformed,
}

pub type Result<T> = std::result::Result<T, Error>;

pub type StartFn =
    Box<dyn for<'a> Fn(&'a mut QB, QBIId, &'a [u8]) -> Pin<Box<dyn Future<Output = ()> + 'a>>>;
pub type SetupFn =
    Box<dyn Fn(QBPBlob) -> Pin<Box<dyn Future<Output = Result<(QBIId, Vec<u8>)>> + 'static>>>;

pub struct Handle {
    tx: mpsc::Sender<QBControlResponse>,
}

impl Handle {
    /// Send a message to this handle
    pub async fn send(&self, msg: impl Into<QBControlResponse>) {
        match self.tx.send(msg.into()).await {
            Err(err) => warn!("could not send message to handle: {0}", err),
            Ok(_) => {}
        };
    }
}

pub struct HandleInit {
    id: QBId,
    conn: Stream,
    tx: mpsc::Sender<(QBId, QBControlRequest, Option<QBPBlob>)>,
    rx: mpsc::Receiver<QBControlResponse>,
}

#[derive(Encode, Decode)]
pub struct QBIDescriptior {
    name: String,
    data: Vec<u8>,
}

#[derive(Encode, Decode, Default)]
pub struct QBITable(HashMap<QBIId, QBIDescriptior>);

impl QBITable {
    /// Try to get an interface from this table by its id.
    ///
    /// Returns Error::NotFound if the interface does not
    /// exist in this table.
    pub fn get(&self, id: &QBIId) -> Result<&QBIDescriptior> {
        self.0.get(id).ok_or(Error::NotFound)
    }

    /// Insert a new interface into this table.
    pub fn insert(&mut self, id: QBIId, descriptor: QBIDescriptior) {
        self.0.insert(id, descriptor);
    }
}

pub struct QBDaemon {
    qb: QB,
    // "available QBIs" -> could be "attached" or "detached"
    // => every QBI that is attached to the master must be in this map
    qbi_table: QBITable,
    start_fns: HashMap<String, StartFn>,
    setup_fns: HashMap<String, SetupFn>,

    req_tx: mpsc::Sender<(QBId, QBControlRequest, Option<QBPBlob>)>,
    req_rx: mpsc::Receiver<(QBId, QBControlRequest, Option<QBPBlob>)>,
    handles: HashMap<QBId, Handle>,
}

impl QBDaemon {
    /// Build the daemon
    pub fn init(qb: QB) -> Self {
        let (req_tx, req_rx) = mpsc::channel(10);
        Self {
            qb,
            qbi_table: Default::default(),
            start_fns: Default::default(),
            setup_fns: Default::default(),
            handles: Default::default(),
            req_tx,
            req_rx,
        }
    }

    /// Start a QBI by the given id.
    pub async fn start(&mut self, id: QBIId) -> Result<()> {
        let descriptor = self.qbi_table.get(&id)?;
        let name = &descriptor.name;
        let start = self.start_fns.get(name).ok_or(Error::NotSupported)?;
        start(&mut self.qb, id, &descriptor.data).await;
        Ok(())
    }

    /// Stop a QBI by the given id.
    pub async fn stop(&mut self, id: QBIId) -> Result<()> {
        self.qb.detach(&id).await.ok_or(Error::NotFound)?.await?;
        Ok(())
    }

    pub async fn setup(&mut self, name: String, blob: QBPBlob) -> Result<()> {
        let setup = self.setup_fns.get(&name).ok_or(Error::NotSupported)?;
        let (id, data) = setup(blob).await?;
        self.qbi_table
            .insert(id.clone(), QBIDescriptior { name, data });
        self.start(id).await.unwrap();
        Ok(())
    }

    /// List the QBIs.
    pub fn list(&self) -> Vec<(QBIId, String, bool)> {
        self.qbi_table
            .0
            .iter()
            .map(|(id, descriptor)| (id.clone(), descriptor.name.clone(), self.qb.is_attached(id)))
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
                    qb.attach(id, bitcode::decode::<T>(&data).unwrap()).await;
                })
            }),
        );
        self.setup_fns.insert(
            name,
            Box::new(move |blob| {
                Box::pin(async move {
                    let cx = blob.deserialize::<T>()?;
                    let data = bitcode::encode(&cx);
                    let id = cx.setup().await;
                    Ok((id, data))
                })
            }),
        );
    }

    pub async fn process(&mut self, caller: QBId, msg: QBControlRequest, blob: Option<QBPBlob>) {
        let resp = self._process(caller.clone(), msg, blob).await;
        let handle = self.handles.get(&caller).unwrap();
        match resp {
            Ok(true) => handle.send(QBControlResponse::Success).await,
            Ok(false) => {}
            Err(err) => {
                handle
                    .send(QBControlResponse::Error {
                        msg: format!("{:?}", err),
                    })
                    .await
            }
        };
    }

    async fn _process(
        &mut self,
        caller: QBId,
        msg: QBControlRequest,
        blob: Option<QBPBlob>,
    ) -> Result<bool> {
        match msg {
            QBControlRequest::Start { id } => self.start(id).await?,
            QBControlRequest::Stop { id } => self.stop(id).await?,
            QBControlRequest::Setup { name, .. } => {
                let blob = blob.unwrap();
                self.setup(name, blob).await?;
            }
            QBControlRequest::Bridge { id, msg } => {
                self.qb.send(&id, QBIBridgeMessage { caller, msg }).await;
                return Ok(false);
            }
            QBControlRequest::List => {
                let handle = self.handles.get(&caller).unwrap();
                handle
                    .send(QBControlResponse::List { list: self.list() })
                    .await;
                return Ok(false);
            }
        };

        Ok(true)
    }

    pub async fn init_handle(&mut self, conn: Stream) {
        let id = QBId::generate();
        let (resp_tx, resp_rx) = mpsc::channel::<QBControlResponse>(10);
        self.handles.insert(id.clone(), Handle { tx: resp_tx });

        let init = HandleInit {
            tx: self.req_tx.clone(),
            rx: resp_rx,
            conn,
            id,
        };

        tokio::spawn(handle_run(init));
    }
}

#[tokio::main]
async fn main() {
    // Setup formatting
    std::panic::set_hook(Box::new(panic_hook));

    let stdout_log = tracing_subscriber::fmt::layer().pretty();

    // A layer that logs events to a file.
    let file = File::create("debug.log").unwrap();
    let debug_log = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(Arc::new(file));

    tracing_subscriber::registry()
        .with(
            stdout_log
                .with_filter(filter::LevelFilter::INFO)
                .and_then(debug_log),
        )
        .init();

    let name = "qb-daemon.sock";
    let name = name.to_ns_name::<GenericNamespaced>().unwrap();
    let socket = ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Both)
        .create_tokio()
        .unwrap();

    // Initialize the core library
    let qb = QB::init("./local").await;

    // Setup the daemon
    let mut daemon = QBDaemon::init(qb);
    daemon.register::<QBILocal>("local");

    // Process
    loop {
        tokio::select! {
            // process qbi
            _ =  daemon.qb.process_handles() => {
                if let Some(response) = daemon.qb.poll_bridge_recv() {
                    daemon.handles.get(&response.caller)
                        .unwrap()
                        .send(QBControlResponse::Bridge {
                            msg: response.msg
                        })
                        .await;
                }
            },
            Some((caller, msg, blob)) = daemon.req_rx.recv() => {
                daemon.process(caller, msg, blob).await;
            }
            Ok(conn) = socket.accept() => {
                daemon.init_handle(conn).await;
            }
        }
    }
}

async fn handle_run(mut init: HandleInit) {
    let span = span!(Level::TRACE, "handle", id = init.id.to_hex());

    match _handle_run(&mut init).await {
        Err(err) => span.in_scope(|| warn!("handle finished with error: {:?}", err)),
        Ok(_) => {}
    }
}

async fn _handle_run(init: &mut HandleInit) -> Result<()> {
    let span = span!(Level::TRACE, "handle", id = init.id.to_hex());

    span.in_scope(|| {
        trace!("create new handle with id={} conn={:?}", init.id, init.conn);
    });

    let mut protocol = QBP::default();

    loop {
        tokio::select! {
            Some(response) = init.rx.recv() => {
                // write a message to the socket
                span.in_scope(|| {
                    trace!("send {}", response);
                });
                protocol.send(&mut init.conn, response).instrument(span.clone()).await?;
            }
            res = protocol.update::<QBControlRequest>(&mut init.conn).instrument(span.clone()) => {
                match res {
                    Ok(msg) => {
                        let blob = match msg {
                            QBControlRequest::Setup { ref content_type, .. } => {
                                let content = protocol.read_payload(&mut init.conn).instrument(span.clone()).await?;
                                Some(QBPBlob { content_type: content_type.clone(), content })
                            }
                            _ => None
                        };
                        init.tx.send((init.id.clone(), msg, blob)).instrument(span.clone()).await.unwrap();
                    }
                    Err(err) => return Err(err.into()),
                }

            }
        }
    }
}
