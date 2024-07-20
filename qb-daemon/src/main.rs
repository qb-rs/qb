use interprocess::local_socket::{
    GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToNsName,
};
use qb_control::qbi_local::{QBILocal, QBILocalInit};
use std::{fs::File, sync::Arc, time::Duration};
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

use qb_core::{interface::QBI, QB};

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
    let _socket = ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Both)
        .create_tokio()
        .unwrap();

    // TODO: implement
    // socket.accept().await.unwrap();
    // println!("RECV: connection");

    // Initialize the core library
    let mut qb = QB::init("./local").await;

    qb.attach(
        "local1",
        QBILocal::init,
        QBILocalInit {
            path: "./local1".into(),
        },
    )
    .await;

    // Process handles
    loop {
        qb.process_handles().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
