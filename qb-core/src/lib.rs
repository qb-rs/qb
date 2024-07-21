//! a rust library for the quixbyte service
//!
//! [Github](https://github.com/qb-rs/qb)
#![warn(missing_docs)]

use std::{collections::HashMap, path::Path, thread::JoinHandle, time::Duration};

use tokio::{
    sync::mpsc,
    task::{AbortHandle, JoinSet},
};
use tracing::{info, span, trace, warn, Level};

use change::log::QBChangelog;
use common::id::QBID;
use fs::QBFS;
use interface::{
    protocol::{BridgeMessage, Message, QBIMessage, QBMessage},
    QBICommunication,
};

pub mod change;
pub mod common;
pub mod fs;
pub mod interface;

struct QBIHandle {
    join_handle: JoinHandle<()>,
    abort_handle: AbortHandle,
    tx: mpsc::Sender<QBMessage>,
    syncing: bool,
}

struct Recv {
    id: QBID,
    rx: mpsc::Receiver<QBIMessage>,
    msg: Option<QBIMessage>,
}

impl QBIHandle {
    /// TODO: doc
    pub async fn send(&self, msg: impl Into<QBMessage>) {
        self.tx.send(msg.into()).await.unwrap()
    }
}

/// the library core that controls the messaging between
/// the master thread and the QBIs
pub struct QB {
    handles: HashMap<QBID, QBIHandle>,
    fs: QBFS,
    recv_pool: JoinSet<Recv>,
    bridge_recv_pool: Vec<BridgeMessage>,
}

impl QB {
    /// initialize the library
    ///
    /// root is the path where it will store its files
    pub async fn init(root: impl AsRef<Path>) -> QB {
        let fs = QBFS::init(root).await;

        QB {
            handles: HashMap::new(),
            recv_pool: JoinSet::new(),
            bridge_recv_pool: Vec::new(),
            fs,
        }
    }

    /// poll a bridge message
    pub fn poll_bridge_recv(&mut self) -> Option<BridgeMessage> {
        if self.bridge_recv_pool.is_empty() {
            return None;
        }

        Some(self.bridge_recv_pool.swap_remove(0))
    }

    /// remove unused handles [from QBIs that have finished]
    pub fn clean_handles(&mut self) {
        let to_remove = self
            .handles
            .iter()
            .filter(|(_, v)| v.join_handle.is_finished())
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for id in to_remove {
            self.handles.remove(&id);
        }
    }

    /// Receive a message
    async fn recv(&mut self) -> Option<(QBID, QBIMessage)> {
        loop {
            match self.recv_pool.join_next().await {
                Some(Ok(Recv {
                    id,
                    mut rx,
                    msg: Some(msg),
                })) => {
                    let handle = self.handles.get_mut(&id).unwrap();
                    handle.abort_handle = self.recv_pool.spawn({
                        let id = id.clone();
                        async move {
                            let msg = rx.recv().await;
                            Recv { rx, msg, id }
                        }
                    });

                    return Some((id, msg));
                }
                Some(Err(err)) if err.is_panic() => {
                    std::panic::resume_unwind(err.into_panic());
                }
                None => {
                    // no entry in join pool, delay to avoid high cpu usage
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    return None;
                }
                _ => {
                    // canceled, retry recv
                }
            }
        }
    }

    /// process the handles
    ///
    /// this will look for new messages from the QBIs and
    /// handle those respectively. Additionally this will
    /// synchronize when new changes arise.
    pub async fn process_handles(&mut self) {
        let mut broadcast = Vec::new();
        self.clean_handles();

        // check for new changes. TODO: asyncify this
        for (id, handle) in self.handles.iter_mut() {
            let span = span!(Level::INFO, "qbi-process", id = id.to_hex());
            let _guard = span.enter();

            let handle_common = self.fs.devices.get_common(id);

            // SYNCHRONIZE
            if !handle.syncing && handle_common != &self.fs.changelog.head() {
                info!("syncing");
                let changes = self.fs.changelog.after_cloned(handle_common).unwrap();
                handle.syncing = true;
                handle
                    .send(Message::Sync {
                        common: handle_common.clone(),
                        changes,
                    })
                    .await;
            }
        }

        // process messages
        let (id, msg) = match self.recv().await {
            Some((id, QBIMessage(msg))) => (id, msg),
            None => return,
        };
        let span = span!(Level::INFO, "qbi-process", id = id.to_hex());
        let _guard = span.enter();
        let handle = self.handles.get_mut(&id).unwrap();
        let handle_common = self.fs.devices.get_common(&id);

        trace!("recv: {}", msg);

        match msg {
            Message::Sync { common, changes } => {
                assert!(handle_common == &common);

                let local_entries = self.fs.changelog.after(&common).unwrap();

                // Apply changes
                let (mut entries, fschanges) =
                    QBChangelog::merge(local_entries.clone(), changes).unwrap();
                self.fs.changelog.append(&mut entries);

                let fschanges = self.fs.table.to_fschanges(fschanges);
                self.fs.apply_changes(fschanges).await.unwrap();

                let new_common = self.fs.changelog.head();
                self.fs.devices.set_common(&id, new_common);

                // Send sync to remote
                if !handle.syncing {
                    handle
                        .send(Message::Sync {
                            common,
                            changes: local_entries,
                        })
                        .await;
                }

                handle.syncing = false;

                self.fs.save().await.unwrap();
            }
            Message::Common { common } => {
                self.fs.devices.set_common(&id, common);
                self.fs.save_devices().await.unwrap();
            }
            Message::Broadcast { msg } => broadcast.push(msg),
            Message::Bridge(bridge) => {
                self.bridge_recv_pool.push(bridge);
            }
        }

        for msg in broadcast {
            for handle in self.handles.values_mut() {
                handle.send(Message::Broadcast { msg: msg.clone() }).await;
            }
        }
    }

    /// attach a QBI to the master
    pub async fn attach<C, F, T>(&mut self, id: impl Into<QBID>, init: F, cx: C)
    where
        C: Sync + Send + 'static,
        T: interface::QBI<C>,
        F: FnOnce(C, QBICommunication) -> T + std::marker::Send + 'static,
    {
        let id = id.into();
        let (main_tx, qbi_rx) = tokio::sync::mpsc::channel::<QBMessage>(32);
        let (qbi_tx, main_rx) = tokio::sync::mpsc::channel::<QBIMessage>(32);

        // spawn receive task
        let abort_handle = self.recv_pool.spawn({
            let id = id.clone();
            let mut rx = main_rx;
            async move {
                let msg = rx.recv().await;
                Recv { rx, msg, id }
            }
        });

        self.handles.insert(
            id.clone(),
            QBIHandle {
                syncing: false,
                join_handle: std::thread::spawn(move || {
                    let span = span!(Level::INFO, "qbi", "id" = id.to_hex());
                    let _guard = span.enter();
                    init(
                        cx,
                        QBICommunication {
                            rx: qbi_rx,
                            tx: qbi_tx,
                        },
                    )
                    .run()
                }),
                abort_handle,
                tx: main_tx,
            },
        );
    }

    /// detach the given interface and return a join handle
    pub async fn detach(&mut self, id: &QBID) -> Option<std::thread::JoinHandle<()>> {
        let handle = self.handles.remove(id)?;
        handle.send(QBMessage::Stop).await;

        Some(handle.join_handle)
    }

    /// synchronize changes across all QBIs
    pub async fn sync(&mut self) {
        for (id, handle) in self.handles.iter_mut() {
            let handle_common = self.fs.devices.get_common(id);

            let changes = self.fs.changelog.after_cloned(handle_common).unwrap();
            if changes.is_empty() {
                continue;
            }

            handle.syncing = true;
            handle
                .send(Message::Sync {
                    common: handle_common.clone(),
                    changes,
                })
                .await;
        }
    }

    /// send a message to a QBI with the given id
    pub async fn send(&self, id: &QBID, msg: impl Into<QBMessage>) {
        // TODO: error handling
        self.handles.get(id).unwrap().send(msg).await
    }
}
