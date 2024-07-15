use std::{
    path::Path,
    task::{Context, Poll, Waker},
    thread::JoinHandle,
};

pub use tokio::sync::mpsc;
use tracing::{info, span, Level};
pub use waker_fn::waker_fn;

pub mod change;
pub mod changelog;
pub mod diff;
pub mod filetree;
pub mod fswrapper;
pub mod hash;
pub mod ignore;
pub mod interface;
pub mod path;
pub mod protocol;
pub mod resource;
pub mod transaction;

// TODO: remove
pub use change::*;
pub use changelog::*;
pub use diff::*;
pub use filetree::*;
pub use fswrapper::*;
pub use hash::*;
pub use interface::*;
pub use path::*;
pub use protocol::*;
pub use resource::*;
pub use transaction::*;

struct QBIHandle {
    join_handle: JoinHandle<()>,
    tx: mpsc::Sender<QBMessage>,
    rx: mpsc::Receiver<QBIMessage>,
    common: QBHash,
    syncing: bool,
    id: String,
    // bridge: Box<dyn Fn(String) -> Box<dyn Any + Sync + Send + 'static>>,
}

pub struct QB {
    handles: Vec<QBIHandle>,
    noop: Waker,
    fswrapper: QBFSWrapper,
    pub changelog: QBChangelog,
}

impl QB {
    pub async fn init(root: impl AsRef<Path>) -> QB {
        let mut fswrapper = QBFSWrapper::new(root);
        fswrapper.init().await.unwrap();

        let changelog = fswrapper
            .load(qbpaths::INTERNAL_CHANGELOG.as_ref())
            .await
            .unwrap_or_else(|_| Default::default());

        QB {
            noop: waker_fn(|| {}),
            handles: Vec::new(),
            fswrapper,
            changelog,
        }
    }

    pub fn clean_handles(&mut self) {
        let pos = self
            .handles
            .iter()
            .position(|h| h.join_handle.is_finished());
        if let Some(pos) = pos {
            self.handles.swap_remove(pos);
        }
    }

    pub async fn process_handles(&mut self) {
        let mut broadcast = Vec::new();
        self.clean_handles();
        for handle in self.handles.iter_mut() {
            let span = span!(Level::INFO, "qbi-process", id = handle.id);
            let _guard = span.enter();

            // SYNCHRONIZE
            if !handle.syncing && handle.common != self.changelog.head() {
                info!("syncing");
                let entries = self.changelog.after_cloned(&handle.common).unwrap();
                handle.syncing = true;
                handle
                    .tx
                    .send(QBMessage::Sync {
                        common: handle.common.clone(),
                        entries,
                    })
                    .await
                    .unwrap();
            }

            let poll = handle.rx.poll_recv(&mut Context::from_waker(&self.noop));

            if let Poll::Ready(Some(msg)) = poll {
                info!("recv: {}", msg);

                match msg {
                    QBIMessage::Sync { common, entries } => {
                        assert!(handle.common == common);

                        let local_entries = self.changelog.after(&common).unwrap();

                        // Apply changes
                        //if !entries.is_empty() || !self.syncing {
                        let (mut entries, changes) =
                            QBChangelog::merge(local_entries.clone(), entries).unwrap();
                        self.changelog.append(&mut entries);
                        self.fswrapper.apply(&changes).await.unwrap();

                        self.fswrapper
                            .save(qbpaths::INTERNAL_CHANGELOG.as_ref(), &self.changelog)
                            .await
                            .unwrap();

                        handle.common = self.changelog.head();
                        self.fswrapper
                            .save(
                                qbpaths::INTERNAL_COMMON
                                    .clone()
                                    .sub(handle.id.as_str())
                                    .unwrap(),
                                &handle.common,
                            )
                            .await
                            .unwrap();
                        //}

                        // Send sync to remote
                        if !handle.syncing {
                            handle
                                .tx
                                .send(QBMessage::Sync {
                                    common,
                                    entries: local_entries,
                                })
                                .await
                                .unwrap();
                        }

                        handle.syncing = false;

                        // TODO: figure out what to do when sync errors arise
                    }
                    QBIMessage::Common { common } => {
                        self.fswrapper
                            .save(
                                qbpaths::INTERNAL_COMMON
                                    .clone()
                                    .sub(handle.id.as_str())
                                    .unwrap(),
                                &common,
                            )
                            .await
                            .unwrap();
                        handle.common = common;
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

    pub async fn attach_qbi<C, F, T, K: Into<String>>(
        &mut self,
        id: K,
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

        let common = self
            .fswrapper
            .load(qbpaths::INTERNAL_COMMON.clone().sub(id.as_str()).unwrap())
            .await
            .unwrap_or_else(|_| QB_ENTRY_BASE.hash().clone());

        self.handles.push(QBIHandle {
            syncing: false,
            id: id.clone(),
            join_handle: std::thread::spawn(move || {
                let span = span!(Level::INFO, "qbi", "id" = id);
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
            common,
            // bridge,
        });
    }

    pub async fn sync(&mut self) {
        for handle in self.handles.iter_mut() {
            let entries = self.changelog.after_cloned(&handle.common).unwrap();
            if entries.is_empty() {
                continue;
            }

            handle.syncing = true;
            handle
                .tx
                .send(QBMessage::Sync {
                    common: handle.common.clone(),
                    entries,
                })
                .await
                .unwrap();
        }
    }
}
