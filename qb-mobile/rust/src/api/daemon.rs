use std::path::PathBuf;

use flutter_rust_bridge::frb;
use qb_core::fs::wrapper::QBFSWrapper;
use qb_daemon::{daemon::QBDaemon, master::QBMaster};
use qb_ext::QBExtId;
use qb_ext_tcp::client::QBITCPClientSetup;
use tokio::sync::{mpsc, Mutex};

use crate::android::QBIAndroid;

#[frb(opaque)]
pub struct DaemonWrapper {
    daemon: Mutex<QBDaemon>,
    cancel_rx: Option<mpsc::Receiver<()>>,
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
        daemon.register_qbi::<QBITCPClientSetup, _>("tcp");
        daemon.autostart().await;
        daemon
            .master
            .attach(QBExtId(0), QBIAndroid { path: files })
            .unwrap();

        let (cancel_tx, cancel_rx) = mpsc::channel(10);
        Self {
            daemon: Mutex::new(daemon),
            cancel_tx,
            cancel_rx: Some(cancel_rx),
        }
    }

    /// Cancel processing the daemon.
    pub async fn cancel(&self) {
        if self.cancel_rx.is_none() {
            self.cancel_tx.send(()).await.unwrap();
        }
    }

    /// Process the daemon. This can be canceled using the cancel method.
    /// If called twice, this will cancel the previous execution.
    pub async fn process(&mut self) {
        self.cancel().await;
        let daemon = &mut self.daemon.lock().await;
        let cancel_rx = self.cancel_rx.take().unwrap();
        self.cancel_rx = Some(Self::_process(cancel_rx, daemon).await);
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
