//! # quixbyte hook (QBH)
//!
//! This module contains stuff related to hooks, which can be attached to
//! the daemon. Hooks are tasks which listen for messages coming from the
//! master and control the master using hook messages.
//!
//! TODO: switch to mutex instead of using messaging

use std::any::Any;
use std::{future::Future, marker::PhantomData};

use crate::interface::QBIContext;
use crate::QBExtId;

use crate::QBExtChannel;

/// Communicate from the interface to the master
pub type QBHChannel = QBExtChannel<QBExtId, QBHSlaveMessage, QBHHostMessage>;

/// TODO: figure out what to call this
pub struct QBHInit<T: QBIContext + Send + 'static> {
    pub channel: QBHChannel,
    _t: PhantomData<T>,
}

impl<T: QBIContext + Any + Send + 'static> QBHInit<T> {
    pub async fn attach(&self, context: T) {
        self.channel
            .send(QBHSlaveMessage::Attach {
                context: Box::new(context),
            })
            .await;
    }
}

impl<T: QBIContext + Send> From<QBHChannel> for QBHInit<T> {
    fn from(value: QBHChannel) -> Self {
        Self {
            channel: value,
            _t: PhantomData,
        }
    }
}

#[non_exhaustive]
pub enum QBHHostMessage {
    Stop,
}

#[non_exhaustive]
pub enum QBHSlaveMessage {
    Attach {
        context: Box<dyn Any + Send + 'static>,
    },
}

/// A context which yields interfaces.
pub trait QBHContext<I: QBIContext + Any + Send + 'static> {
    fn run(self, init: QBHInit<I>) -> impl Future<Output = ()> + Send + 'static;
}
