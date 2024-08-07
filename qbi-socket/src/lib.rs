// TODO: rustls TLS impl for preventing MITM attacks

use std::net::SocketAddr;

use bitcode::{Decode, Encode};
use qb_core::common::QBDeviceId;
use qb_ext::{
    hook::{QBHContext, QBHInit},
    interface::{QBIChannel, QBIContext, QBIHostMessage, QBIMessage, QBISetup, QBISlaveMessage},
};
use qb_proto::QBP;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tracing::{error, info, warn};

/// A hook which listens for incoming connections and yields
/// a [QBIServerSocket].
pub struct QBHServerSocket {
    pub listener: TcpListener,
    /// An authentication token sent on boot
    pub auth: Vec<u8>,
}

impl QBHServerSocket {
    /// Listen locally. Tries to bind a socket to
    /// `0.0.0.0:{port}` and if it fails tries again with the
    /// next successive port number.
    pub async fn listen(mut port: u16, auth: impl Into<Vec<u8>>) -> QBHServerSocket {
        let auth = auth.into();
        let listener: TcpListener;
        loop {
            let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
            match TcpListener::bind(addr).await {
                Ok(val) => {
                    info!("successfully bind on {}", addr);
                    listener = val;
                    break;
                }
                Err(err) => {
                    warn!("unable to bind on {}: {}", addr, err);
                }
            };
            port += 1;
        }

        QBHServerSocket { auth, listener }
    }
}

impl QBHContext<QBIServerSocket> for QBHServerSocket {
    async fn run(self, init: QBHInit<QBIServerSocket>) {
        loop {
            // listen on incoming connections
            let (stream, addr) = self.listener.accept().await.unwrap();
            info!("connected: {}", addr);
            // yield a [QBIServerSocket]
            init.attach(QBIServerSocket {
                stream,
                auth: self.auth.clone(),
            })
            .await;
        }
    }
}

#[derive(Encode, Decode, Serialize, Deserialize, Debug)]
pub struct QBIClientSocket {
    pub addr: String,
    /// An authentication token sent on boot
    #[serde(with = "serde_bytes")]
    pub auth: Vec<u8>,
}

impl QBIContext for QBIClientSocket {
    async fn run(self, host_id: QBDeviceId, com: QBIChannel) {
        info!("initializing socket: {:#?}", self);

        let socket = TcpSocket::new_v4().unwrap();
        let addr = self.addr.parse().unwrap();
        let mut stream = socket.connect(addr).await.unwrap();
        let mut protocol = QBP::default();
        protocol.negotiate(&mut stream).await.unwrap();
        protocol
            .send_payload(&mut stream, &self.auth)
            .await
            .unwrap();

        info!("connected to socket: {:?}", stream);

        let runner = Runner {
            host_id,
            com,
            stream,
            protocol,
        };

        runner.run().await;
    }
}

impl<'a> QBISetup<'a> for QBIClientSocket {
    async fn setup(self) {
        // nothing to do here
    }
}

#[derive(Debug)]
pub struct QBIServerSocket {
    pub stream: TcpStream,
    /// An authentication token sent on boot
    pub auth: Vec<u8>,
}

impl QBIContext for QBIServerSocket {
    async fn run(self, host_id: QBDeviceId, com: QBIChannel) {
        let mut stream = self.stream;
        let mut protocol = QBP::default();
        protocol.negotiate(&mut stream).await.unwrap();
        let auth = protocol.recv_payload(&mut stream).await.unwrap();
        if self.auth != auth {
            error!("client sent incorrect auth token!");
            return;
        }

        let runner = Runner {
            host_id,
            com,
            stream,
            protocol,
        };

        runner.run().await;
    }
}

struct Runner {
    host_id: QBDeviceId,
    com: QBIChannel,
    stream: TcpStream,
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
                    info!("proxy to master: {}", msg);
                    self.com.send(QBISlaveMessage::Message(msg)).await;
                },
                msg = self.com.recv::<QBIHostMessage>() => {
                    match msg {
                        QBIHostMessage::Message(msg) => {
                            info!("proxy to remote: {}", msg);
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
