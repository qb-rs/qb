use core::panic;
use std::{collections::HashMap, fs::File, sync::Arc};

use bitcode::DecodeOwned;
use interprocess::local_socket::{
    tokio::Stream, traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode,
    ListenerOptions, ToNsName,
};
use qb_control::{
    qbi_local::QBILocal, ProcessQBControlRequest, QBControlRequest, QBControlResponse,
};
use qb_core::{
    common::id::QBId,
    interface::{QBIContext, QBIId, QBISetup},
    QB,
};
use qb_proto::{QBPBlob, QBP};
use tokio::sync::mpsc;
use tracing::{span, trace, Level};
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

pub type StartFn = Box<dyn Fn(&mut QB, QBIId, &[u8])>;
pub type SetupFn = Box<dyn Fn(&mut QBDaemon, QBPBlob)>;

pub struct QBIDescriptior {
    name: String,
    data: Vec<u8>,
}

pub struct QBDaemon {
    qb: QB,
    qbis: HashMap<QBIId, QBIDescriptior>,
    start_fns: HashMap<String, StartFn>,
    setup_fns: HashMap<String, SetupFn>,
}

impl QBDaemon {
    /// Build the daemon
    pub fn init(qb: QB) -> Self {
        Self {
            qb,
            qbis: Default::default(),
            start_fns: Default::default(),
            setup_fns: Default::default(),
        }
    }

    /// Start a QBI by the given id.
    pub fn start(&mut self, id: QBIId) {
        let descriptor = self.qbis.get(&id).unwrap();
        let start = self.start_fns.get(&descriptor.name).unwrap();
        start(&mut self.qb, id, &descriptor.data);
    }

    /// Register a QBI kind.
    pub fn register<T>(&mut self, name: impl Into<String>)
    where
        for<'a> T: QBIContext + QBISetup<'a> + DecodeOwned,
    {
        let name = name.into();
        self.start_fns.insert(
            name.clone(),
            Box::new(|qb, id, data| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                runtime.block_on(qb.attach(id, bitcode::decode::<T>(data).unwrap()));
            }),
        );
        let name_clone = name.clone();
        self.setup_fns.insert(
            name,
            Box::new(move |daemon, blob| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                let cx = blob.deserialize::<T>().unwrap();
                let data = bitcode::encode(&cx);
                let id = runtime.block_on(cx.setup());
                daemon.qbis.insert(
                    id,
                    QBIDescriptior {
                        name: name_clone.clone(),
                        data,
                    },
                );
            }),
        );
    }

    /// Register the default QBI kinds.
    pub fn register_default(&mut self) {
        self.register::<QBILocal>("local");
        // self.register::<QBIGDrive>("gdrive");
    }
}

struct Handle {
    tx: tokio::sync::mpsc::Sender<QBControlResponse>,
}

struct HandleInit {
    id: QBId,
    conn: Stream,
    tx: mpsc::Sender<(QBId, QBControlRequest)>,
    rx: mpsc::Receiver<QBControlResponse>,
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
    let mut daemon = QBDaemon::init(QB::init("./local").await);

    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<(QBId, QBControlRequest)>(10);
    let mut handles: HashMap<QBId, Handle> = HashMap::new();

    // Process
    loop {
        tokio::select! {
            // process qbi
            _ =  daemon.qb.process_handles() => {
                if let Some(response) = daemon.qb.poll_bridge_recv() {
                    handles.get(&response.caller)
                        .unwrap()
                        .tx
                        .send(QBControlResponse::Bridge {
                            msg: response.msg
                        })
                        .await
                        .unwrap();
                }
            },
            Some((caller, msg)) = req_rx.recv() => {
                daemon.qb.process(caller, msg).await; // TODO: embed this
            }
            Ok(conn) = socket.accept() => {
                let id = QBId::generate();
                let (resp_tx, resp_rx) = tokio::sync::mpsc::channel::<QBControlResponse>(10);
                handles.insert(id.clone(), Handle {
                    tx: resp_tx,
                });

                let init = HandleInit {
                    tx: req_tx.clone(),
                    rx: resp_rx,
                    conn,
                    id,
                };

                tokio::spawn(handle_run(init));
            }
        }
    }
}

async fn handle_run(mut init: HandleInit) -> qb_proto::Result<()> {
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
                protocol.send(&mut init.conn, response).await?;
            }
            res = protocol.update::<QBControlRequest>(&mut init.conn) => {
                match res {
                    Ok(Some(msg)) => {
                        init.tx.send((init.id, msg)).await.unwrap();
                        todo!();
                    }
                    Ok(None) => {},
                    Err(err) => return Err(err),
                }

            }
        }
    }
}
