use std::{
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use flutter_rust_bridge::frb;
use qb_core::fs::wrapper::QBFSWrapper;
use qb_daemon::{daemon::QBDaemon, master::QBMaster};

// TODO: figure out how to make QBDaemon send.
static DAEMON_SINGLETON: LazyLock<Mutex<Option<QBDaemon>>> = LazyLock::new(|| Mutex::new(None));

#[frb(opaque)]
pub struct DaemonWrapper {}

impl DaemonWrapper {
    pub async fn init(path: String) {
        let path: PathBuf = path.into();
        let wrapper = QBFSWrapper::new(path);

        let master = QBMaster::init(wrapper.clone()).await;
        let mut daemon = QBDaemon::init(master, wrapper).await;
        daemon.autostart().await;

        *DAEMON_SINGLETON.lock().unwrap() = Some(daemon);
    }
}
