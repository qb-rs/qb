// TODO: rustls TLS impl for preventing MITM attacks

use bitcode::{Decode, Encode};
use qb_core::common::QBDeviceId;
use qb_ext::interface::{
    QBIChannel, QBIContext, QBIHostMessage, QBIMessage, QBISetup, QBISlaveMessage,
};
use qb_proto::QBP;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpSocket, TcpStream};
use tracing::{error, info};

#[derive(Encode, Decode, Serialize, Deserialize, Debug)]
pub struct QBIClientSocket {
    /// A IPv4 addr
    /// TODO: think about IPv6
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
        // TODO: add initialization message
        //todo!()
    }
}

#[derive(Debug)]
pub struct QBISocket {
    pub stream: TcpStream,

    /// An authentication token sent on boot
    pub auth: Vec<u8>,
}

impl QBIContext for QBISocket {
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
                    self.com.send(QBISlaveMessage::Message(msg)).await;
                },
                msg = self.com.recv::<QBIHostMessage>() => {
                    self.protocol.send(&mut self.stream, msg).await.unwrap();
                }
            }
        }
    }
}
