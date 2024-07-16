//! a rust library for the quixbyte service
//!
//! [Github](https://github.com/qb-rs/qb)
#![warn(missing_docs)]

use std::{
    path::Path,
    task::{Context, Poll, Waker},
    thread::JoinHandle,
};

use tokio::sync::mpsc;
use tracing::{info, span, Level};
use waker_fn::waker_fn;

use change::log::QBChangelog;
use fs::QBFS;
use interface::{
    communication::QBICommunication,
    protocol::{QBIMessage, QBMessage},
    QBID,
};

pub mod change;
pub mod common;
pub mod fs;
pub mod interface;

struct QBIHandle {
    id: QBID,
    join_handle: JoinHandle<()>,
    tx: mpsc::Sender<QBMessage>,
    rx: mpsc::Receiver<QBIMessage>,
    syncing: bool,
    // bridge: Box<dyn Fn(String) -> Box<dyn Any + Sync + Send + 'static>>,
}

/// the library core that controls the messaging between
/// the master thread and the QBIs
pub struct QB {
    handles: Vec<QBIHandle>,
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
            handles: Vec::new(),
            fs,
        }
    }

    /// remove unused handles [from QBIs that have finished]
    pub fn clean_handles(&mut self) {
        let pos = self
            .handles
            .iter()
            .position(|h| h.join_handle.is_finished());
        if let Some(pos) = pos {
            self.handles.swap_remove(pos);
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
        for handle in self.handles.iter_mut() {
            let span = span!(Level::INFO, "qbi-process", id = handle.id.0);
            let _guard = span.enter();

            let handle_common = self.fs.devices.get_common(&handle.id);

            // SYNCHRONIZE
            if !handle.syncing && handle_common != &self.fs.changelog.head() {
                info!("syncing");
                let changes = self.fs.changelog.after_cloned(handle_common).unwrap();
                handle.syncing = true;
                handle
                    .tx
                    .send(QBMessage::Sync {
                        common: handle_common.clone(),
                        changes,
                    })
                    .await
                    .unwrap();
            }

            let poll = handle.rx.poll_recv(&mut Context::from_waker(&self.noop));

            if let Poll::Ready(Some(msg)) = poll {
                info!("recv: {}", msg);

                match msg {
                    QBIMessage::Sync { common, changes } => {
                        assert!(handle_common == &common);

                        let local_entries = self.fs.changelog.after(&common).unwrap();

                        // Apply changes
                        let (mut entries, fschanges) =
                            QBChangelog::merge(local_entries.clone(), changes).unwrap();
                        self.fs.changelog.append(&mut entries);

                        let fschanges = self.fs.table.to_fschanges(fschanges);
                        self.fs.apply_changes(fschanges).await.unwrap();

                        self.fs.save_changelog().await.unwrap();

                        let new_common = self.fs.changelog.head();
                        self.fs.devices.set_common(&handle.id, new_common);
                        self.fs.save_devices().await.unwrap();

                        // Send sync to remote
                        if !handle.syncing {
                            handle
                                .tx
                                .send(QBMessage::Sync {
                                    common,
                                    changes: local_entries,
                                })
                                .await
                                .unwrap();
                        }

                        handle.syncing = false;
                    }
                    QBIMessage::Common { common } => {
                        self.fs.devices.set_common(&handle.id, common);
                        self.fs.save_devices().await.unwrap();
                    }
                    QBIMessage::Broadcast { msg } => broadcast.push(msg),
                }
            }
        }

        for msg in broadcast {
            for handle in self.handles.iter_mut() {
                handle
                    .tx
                    .send(QBMessage::Broadcast { msg: msg.clone() })
                    .await
                    .unwrap();
            }
        }
    }

    /// attach a QBI to the master
    pub async fn attach_qbi<C, F, T>(
        &mut self,
        id: impl Into<QBID>,
        init: F,
        // bridge: Box<dyn Fn(String) -> Box<dyn Any + Sync + Send + 'static>>,
        cx: C,
    ) where
        C: Sync + Send + 'static,
        T: interface::QBI<C>,
        F: FnOnce(C, QBICommunication) -> T + std::marker::Send + 'static,
    {
        let id = id.into();
        let (main_tx, qbi_rx) = tokio::sync::mpsc::channel::<QBMessage>(32);
        let (qbi_tx, main_rx) = tokio::sync::mpsc::channel::<QBIMessage>(32);

        // let some = tokio::sync::oneshot::channel::<Message>(); // could be used for file transfer

        self.handles.push(QBIHandle {
            syncing: false,
            id: id.clone(),
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
            tx: main_tx,
            rx: main_rx,
            // bridge,
        });
    }

    /// synchronize changes across all QBIs
    pub async fn sync(&mut self) {
        for handle in self.handles.iter_mut() {
            let handle_common = self.fs.devices.get_common(&handle.id);

            let changes = self.fs.changelog.after_cloned(handle_common).unwrap();
            if changes.is_empty() {
                continue;
            }

            handle.syncing = true;
            handle
                .tx
                .send(QBMessage::Sync {
                    common: handle_common.clone(),
                    changes,
                })
                .await
                .unwrap();
        }
    }
}
