//! # qbi-tcp
//!
//! This crate is a collection of interfaces and hooks
//! that allow for two devices running quixbyte to communicate
//! over the TCP protocol (with TLS).

use qb_core::common::QBDeviceId;
use qb_ext::interface::{QBIChannel, QBIHostMessage, QBIMessage, QBISlaveMessage};
use qb_proto::QBP;
use tokio::net::TcpStream;
use tokio_rustls::TlsStream;
use tracing::{debug, info};

pub mod client;
pub mod server;

pub use client::QBITCPClient;
pub use server::QBHTCPServer;
pub use server::QBITCPServer;

/// A common runner which just proxies all incoming
/// and outgoing messages.
struct Runner {
    host_id: QBDeviceId,
    com: QBIChannel,
    stream: TlsStream<TcpStream>,
    protocol: QBP,
}

impl Runner {
    async fn run(mut self) {
        // initialize
        self.protocol
            .send(
                &mut self.stream,
                QBIMessage::Device {
                    device_id: self.host_id,
                },
            )
            .await
            .unwrap();

        // proxy messages
        loop {
            tokio::select! {
                Ok(msg) = self.protocol.recv::<QBIMessage>(&mut self.stream) => {
                    debug!("proxy to master: {}", msg);
                    self.com.send(QBISlaveMessage::Message(msg)).await;
                },
                msg = self.com.recv::<QBIHostMessage>() => {
                    match msg {
                        QBIHostMessage::Message(msg) => {
                            debug!("proxy to remote: {}", msg);
                            self.protocol.send(&mut self.stream, msg).await.unwrap();
                        }
                        QBIHostMessage::Stop => {
                            info!("stopping...");
                            break;
                        }
                    }
                }
            }
        }
    }
}
