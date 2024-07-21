// TODO: convert to struct

use interprocess::local_socket::{
    tokio::Stream, traits::tokio::Listener, GenericNamespaced, ListenerNonblockingMode,
    ListenerOptions, ToNsName,
};
use std::{collections::HashMap, fs::File, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};
use tracing::{span, trace, Level};
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
    conn: Stream,
    tx: mpsc::Sender<(QBID, QBControlRequest)>,
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
            _ =  qb.process_handles() => {
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
            },
            Some((caller, msg)) = req_rx.recv() => {
                qb.process(caller, msg).await;
            }
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

type Len = u64;
const LEN_SIZE: usize = std::mem::size_of::<Len>();
const READ_SIZE: usize = 64;

async fn handle_run(mut init: HandleInit) {
    let span = span!(Level::TRACE, "handle", id = init.id.to_hex());

    span.in_scope(|| {
        trace!("create new handle with id={} conn={:?}", init.id, init.conn);
    });

    let mut bytes = Vec::new();

    loop {
        if bytes.len() > LEN_SIZE {
            // read a message from the recv buffer
            let mut buf: [u8; LEN_SIZE] = [0; LEN_SIZE];
            buf.copy_from_slice(&bytes[0..LEN_SIZE]);
            let packet_len = LEN_SIZE + Len::from_be_bytes(buf) as usize;
            if packet_len > buf.len() {
                let packet = bytes.drain(0..packet_len).collect::<Vec<_>>();
                let request = bitcode::decode::<QBControlRequest>(&packet[LEN_SIZE..]).unwrap();
                span.in_scope(|| {
                    trace!("recv {}", request);
                });
                init.tx.send((init.id.clone(), request)).await.unwrap();
            }
        }

        let mut read_bytes = [0; READ_SIZE];

        tokio::select! {
            Some(response) = init.rx.recv() => {
                // write a message to the socket
                span.in_scope(|| {
                    trace!("send {}", response);
                });
                let contents = bitcode::encode(&response);
                let contents_len = contents.len() as Len;
                write_buf(&mut init.conn, &contents_len.to_be_bytes()).await;
                write_buf(&mut init.conn, &contents).await;
            }
            Ok(len) = init.conn.read(&mut read_bytes) => {
                if len == 0 {
                    span.in_scope(|| {
                        trace!("connection closed!");
                    });
                    return;
                }

                bytes.extend_from_slice(&read_bytes[0..len]);
            }
        }
    }
}

async fn write_buf(conn: &mut Stream, buf: &[u8]) {
    let mut written = 0;
    while written < buf.len() {
        written += conn.write(&buf[written..]).await.unwrap();
    }
}
