use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use notify::{
    event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind, RenameMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use qb::{
    qbpaths, QBChange, QBChangeKind, QBChangelog, QBFSWrapper, QBFileDiff, QBHash,
    QBICommunication, QBIMessage, QBMessage, QBTransaction, TreeDir, TreeFile, QB_ENTRY_BASE,
};
use qb_derive::QBIAsync;
use tracing::{debug, info};

pub struct QBILocalInit {
    pub path: PathBuf,
}

#[derive(QBIAsync)]
#[context(QBILocalInit)]
pub struct QBILocal {
    com: QBICommunication,
    changelog: QBChangelog,
    transaction: QBTransaction,
    fswrapper: QBFSWrapper,
    common: QBHash,
    syncing: bool,
    watcher_skip: Vec<PathBuf>,
}

impl QBILocal {
    async fn init_async(cx: QBILocalInit, com: QBICommunication) -> Self {
        let mut fswrapper = QBFSWrapper::new(cx.path);
        fswrapper.init().await.unwrap();

        let changelog = fswrapper
            .load(qbpaths::INTERNAL_CHANGELOG.as_ref())
            .await
            .unwrap_or_else(|_| Default::default());

        let common = fswrapper
            .load(qbpaths::INTERNAL_COMMON.clone().sub("remote").unwrap())
            .await
            .unwrap_or_else(|_| QB_ENTRY_BASE.hash().clone());

        com.tx
            .send(QBIMessage::Common {
                common: common.clone(),
            })
            .await
            .unwrap();

        Self {
            syncing: false,
            watcher_skip: Vec::new(),
            transaction: Default::default(),
            fswrapper,
            changelog,
            common,
            com,
        }
    }

    async fn on_remote(&mut self, msg: QBMessage) {
        info!("recv {}", msg);

        match msg {
            QBMessage::Common { common } => {
                self.common = common;
            }
            QBMessage::Sync { common, entries } => {
                assert!(self.common == common);

                let local_entries = self.changelog.after(&common).unwrap();

                // Apply changes
                //if !entries.is_empty() || !self.syncing {
                let (entries, changes) =
                    QBChangelog::merge(local_entries.clone(), entries).unwrap();
                self.watcher_skip.append(
                    &mut changes
                        .iter()
                        .map(|e| self.fswrapper.fspath(&e.resource))
                        .collect(),
                );

                self.append(entries).await;
                self.fswrapper.apply(&changes).await.unwrap();
                // TODO: file tree

                self.common = self.changelog.head();
                self.save_common().await;

                // Send sync to remote
                if !self.syncing {
                    self.com
                        .tx
                        .send(QBIMessage::Sync {
                            common,
                            entries: local_entries,
                        })
                        .await
                        .unwrap();
                }

                self.syncing = false;

                // TODO: figure out what to do when sync errors arise
            }
            QBMessage::Broadcast { msg } => println!("BROADCAST: {}", msg),
        }
    }

    // TODO: filter events caused by apply
    async fn on_watcher(&mut self, event: Event) {
        debug!("event {:?}", event);
        for (i, fspath) in event.paths.iter().enumerate() {
            let path = self.fswrapper.parse(fspath.to_str().unwrap()).unwrap();

            // skip internal files
            if qbpaths::INTERNAL.is_parent(&path) {
                continue;
            }

            if self.watcher_skip.iter().find(|e| e == &fspath).is_some() {
                debug!("skip {:?}", path);
                continue;
            }

            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
                + i as u64;

            let entry = match event.kind {
                EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                    let kind = self.fswrapper.update(&path).await.unwrap();
                    match kind {
                        Some(QBFileDiff::Text(diff)) => {
                            QBChange::new(ts, QBChangeKind::Diff { diff }, path.file())
                        }
                        Some(QBFileDiff::Binary(contents)) => {
                            QBChange::new(ts, QBChangeKind::Change { contents }, path.file())
                        }
                        None => continue,
                    }
                }
                EventKind::Modify(ModifyKind::Name(RenameMode::From))
                | EventKind::Remove(RemoveKind::File) => {
                    self.fswrapper.filetree.remove(&path);
                    QBChange::new(ts, QBChangeKind::Delete, path.file())
                }
                EventKind::Remove(RemoveKind::Folder) => {
                    self.fswrapper.filetree.remove(&path);
                    QBChange::new(ts, QBChangeKind::Delete, path.dir())
                }
                EventKind::Create(CreateKind::File) => {
                    self.fswrapper.filetree.insert(&path, TreeFile::default());
                    QBChange::new(ts, QBChangeKind::Create, path.file())
                }
                EventKind::Create(CreateKind::Folder) => {
                    self.fswrapper.filetree.insert(&path, TreeDir::default());
                    QBChange::new(ts, QBChangeKind::Create, path.dir())
                }
                _ => continue,
            };

            self.transaction.push(entry);
        }
    }

    fn should_sync(&mut self) -> bool {
        !self.syncing && !self.transaction.complete().is_empty()
    }

    async fn append(&mut self, mut entries: Vec<QBChange>) {
        self.changelog.append(&mut entries);
        self.save_changelog().await;
    }

    /// Save common to fs
    async fn save_common(&self) {
        self.fswrapper
            .save(
                &qbpaths::INTERNAL_COMMON.clone().sub("remote").unwrap(),
                &self.common,
            )
            .await
            .unwrap();
    }

    /// Save the changelog to fs
    async fn save_changelog(&self) {
        self.fswrapper
            .save(qbpaths::INTERNAL_CHANGELOG.as_ref(), &self.changelog)
            .await
            .unwrap();
    }

    async fn sync(&mut self) {
        // TODO: minify entries vector
        info!("syncing");
        self.syncing = true;

        // Complete transaction
        let mut entries = std::mem::take(&mut self.transaction).complete_into();
        self.append(entries.clone()).await;

        // notify remote
        self.com
            .tx
            .send(QBIMessage::Sync {
                common: self.common.clone(),
                entries: std::mem::take(&mut entries),
            })
            .await
            .unwrap();
    }

    async fn run_async(mut self) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            tx.blocking_send(res).unwrap();
        })
        .unwrap();

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        watcher
            .watch(&self.fswrapper.root, RecursiveMode::Recursive)
            .unwrap();

        loop {
            //let msg = self.com.rx.recv().await.unwrap();
            tokio::select! {
                Some(msg) = self.com.rx.recv() => {
                    self.on_remote(msg).await;
                },
                Some(Ok(event)) = rx.recv() => {
                    self.on_watcher(event).await;
                },
                _ = tokio::time::sleep(Duration::from_secs(1)), if self.should_sync() => {
                    self.sync().await;
                },
                _ = tokio::time::sleep(Duration::from_secs(1)), if !self.watcher_skip.is_empty() => {
                    self.watcher_skip.clear();
                },
                // TODO: sync response timeout
            };
        }
    }
}
