//! qb-ext
//!
//! This crate exposes primitives about extending quixbyte's capabilities
//! like adding interfaces and hooks or controlling the master via control
//! messages.

use tokio::sync::mpsc;

pub mod control;
pub mod hook;
pub mod interface;

/// A channel used for communication from a slave
pub struct QBChannel<I: Clone, S, R> {
    id: I,
    tx: mpsc::Sender<(I, S)>,
    rx: mpsc::Receiver<R>,
}

impl<I: Clone, S, R> QBChannel<I, S, R> {
    /// Construct a new channel
    pub fn new(id: I, tx: mpsc::Sender<(I, S)>, rx: mpsc::Receiver<R>) -> Self {
        QBChannel { id, tx, rx }
    }

    /// Send a message to this channel
    pub async fn send(&self, msg: impl Into<S>) {
        self.tx.send((self.id.clone(), msg.into())).await.unwrap()
    }

    /// Receive a message from this channel
    pub async fn recv<T>(&mut self) -> T
    where
        T: From<R>,
    {
        self.rx.recv().await.expect("channel closed").into()
    }
}
