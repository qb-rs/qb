//! # qb-daemon
//!
//! This crate houses the daemon of the application,
//! that is, the application that runs in the background,
//! which handles interface tasks and their respective communication.
//!
//! We can communicate with the daemon using the [qb-control] messages.

#![warn(missing_docs)]

use std::sync::Arc;

use clap::Parser;
use daemon::QBDaemon;
use interprocess::local_socket::{
    traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToNsName,
};
use master::QBMaster;
use qb_ext::hook::QBHId;
use qbi_local::QBILocal;
use qbi_socket::{QBHServerSocket, QBIClientSocket};
use tracing::info;
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

pub mod daemon;
pub mod master;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Do not bind to a socket [default]
    #[arg(long = "bind")]
    _bind: bool,

    /// Don't bind to a socket
    #[clap(long = "no-bind", overrides_with = "_bind")]
    no_bind: bool,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let bind = !args.no_bind;

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

    let socket = if bind {
        let name = "qb-daemon.sock";
        info!("bind to socket {}", name);
        let name = name.to_ns_name::<GenericNamespaced>().unwrap();
        Some(
            ListenerOptions::new()
                .name(name)
                .nonblocking(ListenerNonblockingMode::Both)
                .create_tokio()
                .unwrap(),
        )
    } else {
        None
    };

    // Initialize the master
    let mut master = QBMaster::init();
    let hook = QBHServerSocket::listen(6969, b"").await;
    master.hook(QBHId::generate(), hook).await.unwrap();

    // Initialize the daemon
    let mut daemon = QBDaemon::init(master).await;
    daemon.register::<QBILocal>("local");
    daemon.register::<QBIClientSocket>("client-socket");

    // Process
    loop {
        match &socket {
            Some(socket) => {
                tokio::select! {
                    // process interfaces
                    Some(v) = daemon.master.interface_rx.recv() => daemon.master.iprocess(v).await,
                    // process hooks
                    Some(v) = daemon.master.hook_rx.recv() => daemon.master.hprocess(v).await,
                    // process control messages
                    Some(v) = daemon.req_rx.recv() => daemon.process(v).await,
                    // process daemon socket
                    Ok(conn) = socket.accept() => daemon.init_handle(conn).await,
                }
            }
            None => {
                tokio::select! {
                    // process interfaces
                    Some(v) = daemon.master.interface_rx.recv() => daemon.master.iprocess(v).await,
                    // process hooks
                    Some(v) = daemon.master.hook_rx.recv() => daemon.master.hprocess(v).await,
                }
            }
        }
    }
}
