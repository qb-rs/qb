use std::path::PathBuf;

use flutter_rust_bridge::frb;
use qb_core::fs::wrapper::QBFSWrapper;
use qb_daemon::{daemon::QBDaemon, master::QBMaster};
use qb_ext::{control::QBCId, QBExtId};
use qb_ext_tcp::client::QBITCPClientSetup;
use qb_proto::QBPBlob;
use tokio::sync::{mpsc, Mutex};

use crate::android::QBIAndroid;

pub use qb_ext::control::{QBCRequest, QBCResponse};

#[frb(opaque)]
pub struct DaemonWrapper {
    daemon: Mutex<QBDaemon>,
    cancel_rx: Mutex<Option<mpsc::Receiver<()>>>,
    cancel_tx: mpsc::Sender<()>,
}

impl DaemonWrapper {
    /// Initialize a new daemon process.
    pub async fn init(path: String) -> Self {
        let files = path.clone() + "/files";
        let path: PathBuf = path.into();
        let wrapper = QBFSWrapper::new(path);

        let master = QBMaster::init(wrapper.clone()).await;
        let mut daemon = QBDaemon::init(master, wrapper).await;
        daemon.register_qbi::<QBITCPClientSetup, _>("tcp-client");
        daemon.autostart().await;
        daemon
            .master
            .attach(QBExtId(0), QBIAndroid { path: files })
            .unwrap();

        let (cancel_tx, cancel_rx) = mpsc::channel(10);
        Self {
            daemon: Mutex::new(daemon),
            cancel_tx,
            cancel_rx: Mutex::new(Some(cancel_rx)),
        }
    }

    /// Add an extension to this daemon
    ///
    /// This will cancel cancelable tasks, which block execution,
    /// as they require mutable access to the daemon.
    pub async fn add(&self, name: String, content_type: String, content: Vec<u8>) {
        self.cancel().await;
        let daemon = &mut self.daemon.lock().await;
        let blob = QBPBlob {
            content_type,
            content,
        };
        daemon.add(QBCId::root(), name, blob).unwrap();
    }

    /// Remove an extension to this daemon.
    ///
    /// This will cancel cancelable tasks, which block execution,
    /// as they require mutable access to the daemon.
    pub async fn remove(&self, id: u64) {
        self.cancel().await;
        let daemon = &mut self.daemon.lock().await;
        daemon.remove(QBExtId(id)).await.unwrap();
    }

    /// Start an extension for this daemon.
    ///
    /// This will cancel cancelable tasks, which block execution,
    /// as they require mutable access to the daemon.
    pub async fn start(&self, id: u64) {
        self.cancel().await;
        let daemon = &mut self.daemon.lock().await;
        daemon.start(QBExtId(id)).await.unwrap();
    }

    /// Stop an extension for this daemon.
    ///
    /// This will cancel cancelable tasks, which block execution,
    /// as they require mutable access to the daemon.
    pub async fn stop(&self, id: u64) {
        self.cancel().await;
        let daemon = &mut self.daemon.lock().await;
        daemon.stop(QBExtId(id)).await.unwrap();
    }

    /// Cancel cancelable tasks.
    pub async fn cancel(&self) {
        if self.cancel_rx.lock().await.is_none() {
            self.cancel_tx.send(()).await.unwrap();
        }
    }

    /// Process the daemon.
    ///
    /// This will cancel cancelable tasks, which block execution,
    /// as they require mutable access to the daemon.
    ///
    /// This task is cancelable using the cancel method.
    pub async fn process(&self) {
        self.cancel().await;
        let daemon = &mut self.daemon.lock().await;
        let cancel_rx = self.cancel_rx.lock().await.take().unwrap();
        *self.cancel_rx.lock().await = Some(Self::_process(cancel_rx, daemon).await);
    }

    async fn _process(
        mut cancel_rx: mpsc::Receiver<()>,
        daemon: &mut QBDaemon,
    ) -> mpsc::Receiver<()> {
        tokio::select! {
            // process interfaces
            Some(v) = daemon.master.qbi_rx.recv() => daemon.master.iprocess(v).await,
            // process hooks
            Some(v) = daemon.master.qbh_rx.recv() => daemon.master.hprocess(v),
            // process control messages
            Some(v) = daemon.req_rx.recv() => daemon.process(v).await,
            // process daemon setup queue
            v = daemon.setup.join() => daemon.process_setup(v).await,
            _ = cancel_rx.recv() => {}
        }
        cancel_rx
    }
}
