//! a rust library for the quixbyte service
//!
//! [Github](https://github.com/qb-rs/qb)
#![warn(missing_docs)]

use std::{
    collections::HashMap,
    path::Path,
    task::{Context, Poll, Waker},
    thread::JoinHandle,
    time::Duration,
};

use tokio::sync::mpsc;
use tracing::{info, span, warn, Level};
use waker_fn::waker_fn;

use change::log::QBChangelog;
use common::id::QBID;
use fs::QBFS;
use interface::{
    protocol::{Message, QBIMessage, QBMessage},
    QBICommunication,
};

pub mod change;
pub mod common;
pub mod fs;
pub mod interface;

struct QBIHandle {
    join_handle: JoinHandle<()>,
    tx: mpsc::Sender<QBMessage>,
    rx: mpsc::Receiver<QBIMessage>,
    pool: Vec<QBIMessage>,
    syncing: bool,
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
    noop: Waker,
    fs: QBFS,
}

impl QB {
    /// initialize the library
    ///
    /// root is the path where it will store its files
    pub async fn init(root: impl AsRef<Path>) -> QB {
        let fs = QBFS::init(root).await;

        QB {
            noop: waker_fn(|| {}),
            handles: HashMap::new(),
            fs,
        }
    }

    /// remove unused handles [from QBIs that have finished]
    pub fn clean_handles(&mut self) {
        let to_remove = self
            .handles
            .iter()
            .filter_map(|(k, v)| v.join_handle.is_finished().then(|| k.clone()))
            .collect::<Vec<_>>();
        for id in to_remove {
            self.handles.remove(&id);
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
        for (id, handle) in self.handles.iter_mut() {
            let span = span!(Level::INFO, "qbi-process", id = id.0);
            let _guard = span.enter();

            let handle_common = self.fs.devices.get_common(&id);

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

            let poll = handle.rx.poll_recv(&mut Context::from_waker(&self.noop));
            if let Poll::Ready(Some(msg)) = poll {
                handle.pool.push(msg);
            }

            while let Some(QBIMessage(msg)) = handle.pool.pop() {
                info!("recv: {}", msg);

                match msg {
                    Message::Sync { common, changes } => {
                        assert!(self.fs.devices.get_common(&id) == &common);

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
                    Message::Bridge { .. } => {
                        warn!("unhandled bridge response!");
                    }
                }
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

        self.handles.insert(
            id.clone(),
            QBIHandle {
                syncing: false,
                join_handle: std::thread::spawn(move || {
                    let span = span!(Level::INFO, "qbi", "id" = id.0);
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
                pool: Vec::new(),
                tx: main_tx,
                rx: main_rx,
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
            let handle_common = self.fs.devices.get_common(&id);

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

    /// perform a bridge request
    pub async fn bridge(&mut self, id: &QBID, msg: Vec<u8>) -> Option<Vec<u8>> {
        let handle = self.handles.get_mut(id)?;
        handle.send(Message::Bridge { msg }).await;

        loop {
            match tokio::time::timeout(Duration::from_secs(2), handle.rx.recv()).await {
                Ok(Some(QBIMessage(Message::Bridge { msg }))) => {
                    return Some(msg);
                }
                Ok(Some(msg)) => {
                    handle.pool.push(msg);
                }
                Ok(None) => {}
                Err(_) => return None,
            }
        }
    }
}
