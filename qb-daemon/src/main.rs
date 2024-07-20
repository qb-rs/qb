use interprocess::local_socket::{
    traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToNsName,
};
use std::{collections::HashMap, fs::File, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, span, Level};
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

use qb_control::{
    qbi_local::{QBILocal, QBILocalInit},
    ProcessQBControlRequest, QBControlRequest, QBControlResponse,
};
use qb_core::{common::id::QBID, interface::QBI, QB};

struct Handle {
    tx: tokio::sync::mpsc::Sender<QBControlResponse>,
}

struct HandleInit {
    id: QBID,
    conn: interprocess::local_socket::tokio::Stream,
    tx: tokio::sync::mpsc::Sender<(QBID, QBControlRequest)>,
    rx: tokio::sync::mpsc::Receiver<QBControlResponse>,
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
    let mut qb = QB::init("./local").await;

    qb.attach(
        "local1",
        QBILocal::init,
        QBILocalInit {
            path: "./local1".into(),
        },
    )
    .await;

    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<(QBID, QBControlRequest)>(10);
    let mut handles: HashMap<QBID, Handle> = HashMap::new();

    // Process
    loop {
        tokio::select! {
            // process qbi
            _ = async {
                if let Some(response) = qb.poll_bridge_recv() {
                    handles.get(&response.caller)
                        .unwrap()
                        .tx
                        .send(QBControlResponse::Bridge {
                            msg: response.msg
                        })
                        .await
                        .unwrap();
                }
                qb.process_handles().await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            } => {},
            Some((caller, msg)) = req_rx.recv() => {
                qb.process(caller, msg).await;
            }
            // TODO: process socket
            Ok(conn) = socket.accept() => {
                let id = QBID::generate();
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

type LEN = u64;
const LEN_SIZE: usize = std::mem::size_of::<LEN>();
const READ_SIZE: usize = 64;

async fn handle_run(mut init: HandleInit) {
    let span = span!(Level::INFO, "handle", id = init.id.to_string());
    let _guard = span.enter();

    info!("create new handle with id={} conn={:?}", init.id, init.conn);

    let mut bytes = Vec::new();

    loop {
        if bytes.len() > 8 {
            // read a message from the recv buffer
            let mut buf: [u8; LEN_SIZE] = [0; LEN_SIZE];
            buf.copy_from_slice(&bytes[0..LEN_SIZE]);
            let packet_len = LEN_SIZE + LEN::from_be_bytes(buf) as usize;
            if packet_len > buf.len() {
                let packet = bytes.drain(0..packet_len).collect::<Vec<_>>();
                let request = bitcode::decode::<QBControlRequest>(&packet[LEN_SIZE..]).unwrap();
                info!("recv {}", request);
                init.tx.send((init.id.clone(), request)).await.unwrap();
            }
        }

        let mut read_bytes = [0; READ_SIZE];

        tokio::select! {
            Some(response) = init.rx.recv() => {
                // write a message to the socket
                info!("send {}", response);
                let contents = bitcode::encode(&response);
                init.conn.write(&contents.len().to_be_bytes()).await.unwrap();
                init.conn.write(&contents).await.unwrap();
            }
            Ok(len) = init.conn.read(&mut read_bytes) => {
                if len == 0 {
                    info!("connection closed!");
                    return;
                }

                bytes.extend_from_slice(&read_bytes[0..len]);
            }
        }
    }
}
