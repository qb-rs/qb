// TODO: rustls TLS impl for preventing MITM attacks

use bitcode::{Decode, Encode};
use qb_core::{
    common::QBDeviceId,
    interface::{Message, QBICommunication, QBIContext, QBIId, QBISetup, QBISlaveMessage},
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
    pub auth: Vec<u8>,
}

impl QBIContext for QBIClientSocket {
    async fn run(self, host_id: QBDeviceId, com: QBICommunication) {
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
    async fn setup(self) -> QBIId {
        // TODO: add initialization message
        todo!()
    }
}

#[derive(Debug)]
pub struct QBISocket {
    pub stream: TcpStream,

    /// An authentication token sent on boot
    pub auth: Vec<u8>,
}

impl QBIContext for QBISocket {
    async fn run(self, host_id: QBDeviceId, com: QBICommunication) {
        let mut stream = self.stream;
        let mut protocol = QBP::default();
        protocol.negotiate(&mut stream).await.unwrap();
        let auth = protocol.read_payload(&mut stream).await.unwrap();
        if self.auth != auth {
            error!("client sent incorrect auth token!");
            return;
        }

        let runner = Runner {
            _host_id: host_id,
            com,
            stream,
            protocol,
        };

        runner.run().await;
    }
}

struct Runner {
    _host_id: QBDeviceId,
    com: QBICommunication,
    stream: TcpStream,
    protocol: QBP,
}

impl Runner {
    async fn run(mut self) {
        loop {
            let msg = self
                .protocol
                .read::<Message>(&mut self.stream)
                .await
                .unwrap();
            self.com.send(QBISlaveMessage::Message(msg)).await;
        }
    }
}
