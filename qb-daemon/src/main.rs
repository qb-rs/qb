//! # qb-daemon
//!
//! This crate houses the daemon of the application,
//! that is, the application that runs in the background,
//! which handles interface tasks and their respective communication.
//!
//! We can communicate with the daemon using the [qb-control] messages.

#![warn(missing_docs)]

use std::{pin::Pin, str::FromStr, sync::Arc};

use clap::Parser;
use daemon::QBDaemon;
use interprocess::local_socket::{
    traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToNsName,
};
use master::QBMaster;
use qb_core::fs::wrapper::QBFSWrapper;
use qb_ext_local::QBILocal;
use qb_ext_tcp::{server::QBHTCPServerSetup, QBHTCPServer, QBITCPClient, QBITCPServer};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{info, level_filters::LevelFilter};
use tracing_panic::panic_hook;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

pub mod daemon;
pub mod master;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Bind to a socket for IPC [default]
    #[arg(long = "ípc")]
    _ipc_bind: bool,

    /// Do not bind to a socket for IPC
    #[clap(long = "no-ipc", overrides_with = "_ipc_bind")]
    no_ipc_bind: bool,

    /// Use STDIN/STDOUT for controlling (disables std logging)
    #[clap(long = "stdio", overrides_with = "_no_stdio_bind")]
    stdio_bind: bool,

    /// Do not use STDIN/STDOUT for controlling [default]
    #[clap(long = "no-stdio")]
    _no_stdio_bind: bool,

    /// The path, where the daemon stores its files
    #[clap(long, short, default_value = "./run/daemon1")]
    path: String,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    let args = Cli::parse();
    let ipc_bind = !args.no_ipc_bind;
    let stdio_bind = args.stdio_bind;

    // Setup formatting
    std::panic::set_hook(Box::new(panic_hook));

    // A layer that logs events to a file.
    let file = std::fs::File::create("/tmp/qb-daemon.log").unwrap();
    let debug_log = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(Arc::new(file));

    // disable stdout if std_bind
    if !stdio_bind {
        let stdout_log = tracing_subscriber::fmt::layer().pretty();
        let env_log_level = std::env::var("LOG_LEVEL").unwrap_or("info".to_string());
        tracing_subscriber::registry()
            .with(
                stdout_log
                    .with_filter(LevelFilter::from_str(env_log_level.as_str()).unwrap())
                    .and_then(debug_log),
            )
            .init();
    } else {
        tracing_subscriber::registry().with(debug_log).init();
    }

    let socket = if ipc_bind {
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
    let master = QBMaster::init(wrapper.clone()).await;

    // Initialize the daemon
    let mut daemon = QBDaemon::init(master, wrapper).await;
    daemon.register_qbi::<QBILocal, QBILocal>("local");
    daemon.register_qbi::<QBITCPClient, QBITCPClient>("tcp");
    daemon.register_qbh::<QBHTCPServerSetup, QBHTCPServer, QBITCPServer>("tcp-server");
    daemon.autostart().await;

    if stdio_bind {
        daemon.init_handle(StdStream::open()).await;
    }

    // Process
    loop {
        match &socket {
            Some(socket) => {
                tokio::select! {
                    // process interfaces
                    Some(v) = daemon.master.qbi_rx.recv() => daemon.master.iprocess(v).await,
                    // process hooks
                    Some(v) = daemon.master.qbh_rx.recv() => daemon.master.hprocess(v).await,
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
                    Some(v) = daemon.master.qbi_rx.recv() => daemon.master.iprocess(v).await,
                    // process hooks
                    Some(v) = daemon.master.qbh_rx.recv() => daemon.master.hprocess(v).await,
                    // process control messages
                    Some(v) = daemon.req_rx.recv() => daemon.process(v).await,
                    // process daemon setup queue
                    v = daemon.setup.join() => daemon.process_setup(v).await,
                }
            }
        }
    }
}

#[derive(Debug)]
struct StdStream {
    stdin: tokio::io::Stdin,
    stdout: tokio::io::Stdout,
}

impl StdStream {
    pub fn open() -> Self {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        StdStream { stdin, stdout }
    }
}

impl AsyncRead for StdStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_read(cx, buf)
    }
}

impl AsyncWrite for StdStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.stdout).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stdout).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stdout).poll_shutdown(cx)
    }
}
