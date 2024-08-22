use std::path::PathBuf;

use flutter_rust_bridge::frb;
use qb_core::fs::wrapper::QBFSWrapper;
use qb_daemon::{daemon::QBDaemon, master::QBMaster};

#[frb(opaque)]
pub struct DaemonWrapper {
    daemon: QBDaemon,
}

impl DaemonWrapper {
    pub async fn init(path: String) -> Self {
        let path: PathBuf = path.into();
        let wrapper = QBFSWrapper::new(path);

        let master = QBMaster::init(wrapper.clone()).await;
        let mut daemon = QBDaemon::init(master, wrapper).await;
        daemon.autostart().await;

        Self { daemon }
    }
}
