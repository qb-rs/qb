//! # qb-daemon
//!
//! This crate houses the daemon of the application,
//! that is, the application that runs in the background,
//! which handles interface tasks and their respective communication.
//!
//! We can communicate with the daemon using the [qb-control] messages.

#![warn(missing_docs)]

use std::{str::FromStr, sync::Arc};

use clap::Parser;
use daemon::QBDaemon;
use interprocess::local_socket::{
    traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToNsName,
};
use master::QBMaster;
use qb_core::fs::wrapper::QBFSWrapper;
use qb_ext::hook::QBHId;
use qbi_local::QBILocal;
use qbi_tcp::{QBHTCPServer, QBITCPClient};
use tracing::{info, level_filters::LevelFilter};
use tracing_panic::panic_hook;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

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

    #[clap(long, short, default_value = "./")]
    path: String,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
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

    let env_log_level = std::env::var("LOG_LEVEL").unwrap_or("info".to_string());
    tracing_subscriber::registry()
        .with(
            stdout_log
                .with_filter(LevelFilter::from_str(env_log_level.as_str()).unwrap())
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

    let wrapper = QBFSWrapper::new(args.path);
    // Initialize the master
    let mut master = QBMaster::init(wrapper.clone()).await;

    // TODO: persistent hook
    let hook = QBHTCPServer::listen(6969, b"").await;
    master.hook(QBHId::generate(), hook).await.unwrap();

    // Initialize the daemon
    let mut daemon = QBDaemon::init(master, wrapper).await;
    daemon.register::<QBILocal, QBILocal>("local");
    daemon.register::<QBITCPClient, QBITCPClient>("tcp");
    daemon.autostart().await;

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
                    // process daemon setup queue
                    v = daemon.setup.join() => daemon.process_setup(v).await,
                }
            }
            None => {
                tokio::select! {
                    // process interfaces
                    Some(v) = daemon.master.interface_rx.recv() => daemon.master.iprocess(v).await,
                    // process hooks
                    Some(v) = daemon.master.hook_rx.recv() => daemon.master.hprocess(v).await,
                    // process daemon setup queue
                    v = daemon.setup.join() => daemon.process_setup(v).await,
                }
            }
        }
    }
}
