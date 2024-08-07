//! # qb-daemon
//!
//! This crate houses the daemon of the application,
//! that is, the application that runs in the background,
//! which handles interface tasks and their respective communication.
//!
//! We can communicate with the daemon using the [qb-control] messages.

#![warn(missing_docs)]

use std::{net::SocketAddr, sync::Arc};

use daemon::QBDaemon;
use interprocess::local_socket::{
    traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToNsName,
};
use master::QBMaster;
use qbi_local::QBILocal;
use qbi_socket::QBIClientSocket;
use tokio::net::TcpListener;
use tracing::{info, warn};
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

pub mod daemon;
pub mod master;

#[tokio::main]
async fn main() {
    // Setup formatting
    std::panic::set_hook(Box::new(panic_hook));

    let stdout_log = tracing_subscriber::fmt::layer().pretty();

    // A layer that logs events to a file.
    let file = std::fs::File::create("/tmp/qb-daemon.log").unwrap();
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

    // Initialize the master
    let master = QBMaster::init(Default::default());

    // Initialize the daemon
    let mut daemon = QBDaemon::init(master).await;
    daemon.register::<QBILocal>("local");
    daemon.register::<QBIClientSocket>("client-socket");

    let listener: TcpListener;
    let mut port = 6969;
    loop {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        match TcpListener::bind(addr).await {
            Ok(val) => {
                info!("successfully bind on {}", addr);
                listener = val;
                break;
            }
            Err(err) => {
                warn!("unable to bind on {}: {}", addr, err);
            }
        };
        port += 1;
    }

    // Process
    loop {
        tokio::select! {
            // process qbi
            v = daemon.master.read() => daemon.master.process(v).await,
            Some(v) = daemon.req_rx.recv() => daemon.process(v).await,
            Ok(conn) = socket.accept() => {
                daemon.init_handle(conn).await;
            }
            Ok(stream) = listener.accept() => {
                info!("received connection: {:?}", stream);
            }
        }
    }
}
