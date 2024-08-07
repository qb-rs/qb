//! qb-ext
//!
//! TODO: doc

use tokio::sync::mpsc;

pub mod control;
pub mod hook;
pub mod interface;

/// A channel used for communication
pub struct QBChannel<S, R> {
    tx: mpsc::Sender<S>,
    rx: mpsc::Receiver<R>,
}

impl<S, R> QBChannel<S, R> {
    /// Construct a new channel
    pub fn new(tx: mpsc::Sender<S>, rx: mpsc::Receiver<R>) -> Self {
        QBChannel { tx, rx }
    }

    /// Send a message to this channel
    pub async fn send(&self, msg: impl Into<S>) {
        self.tx.send(msg.into()).await.unwrap()
    }

    /// Receive a message from this channel
    pub async fn recv<T>(&mut self) -> T
    where
        T: From<R>,
    {
        self.rx.recv().await.expect("channel closed").into()
    }
}
