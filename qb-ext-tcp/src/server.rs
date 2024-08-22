//! # server
//!
//! This module is for the stuff that runs on the server.

use std::{net::IpAddr, str::FromStr, sync::Arc};

use bitcode::{Decode, Encode};
use qb_core::device::QBDeviceId;
use qb_ext::{
    hook::{QBHContext, QBHHostMessage, QBHInit},
    interface::{QBIChannel, QBIContext},
    QBExtSetup,
};
use qb_proto::QBP;
use rcgen::SanType;
use rustls_cert_gen::CertificateBuilder;
use rustls_pemfile::private_key;
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::{TlsAcceptor, TlsStream};
use tracing::{debug, error, info};

use crate::Runner;

#[derive(Decode, Deserialize)]
pub struct QBHTCPServerSetup {
    #[serde(default = "port_default")]
    pub port: u16,
    #[serde(default = "host_default")]
    pub host: String,
    pub auth: Vec<u8>,
}

fn port_default() -> u16 {
    6969
}

fn host_default() -> String {
    "0.0.0.0".to_string()
}

impl QBExtSetup<QBHTCPServer> for QBHTCPServerSetup {
    async fn setup(self) -> QBHTCPServer {
        debug!("generating certificate...");
        let ca = CertificateBuilder::new()
            .certificate_authority()
            .country_name("Germany")
            .unwrap()
            .organization_name("QuixByte Local CA")
            .build()
            .unwrap();
        let chain_pem = ca.serialize_pem();
        let chain_bytes = chain_pem.cert_pem;
        let entity_pem = CertificateBuilder::new()
            .end_entity()
            .common_name("Tls End-Entity Certificate")
            .subject_alternative_names(vec![
                SanType::DnsName("quixbyte.local".try_into().unwrap()),
                SanType::IpAddress(IpAddr::from_str("0.0.0.0").unwrap()),
            ])
            .build(&ca)
            .unwrap()
            .serialize_pem();
        let entity_key_bytes = entity_pem.private_key_pem;
        let entity_cert_bytes = entity_pem.cert_pem;

        QBHTCPServer {
            chain_bytes,
            entity_key_bytes,
            entity_cert_bytes,
            host: self.host,
            port: self.port,
            auth: self.auth,
        }
    }
}

/// A hook which listens for incoming connections and yields
/// a [QBITCPServer].
#[derive(Encode, Decode)]
pub struct QBHTCPServer {
    entity_key_bytes: String,
    entity_cert_bytes: String,
    chain_bytes: String,

    host: String,
    port: u16,
    /// An authentication token sent on boot
    auth: Vec<u8>,
}

impl QBHContext<QBITCPServer> for QBHTCPServer {
    async fn run(self, mut init: QBHInit<QBITCPServer>) {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = match TcpListener::bind(addr.clone()).await {
            Ok(val) => {
                info!("successfully bind on {}", addr);
                val
            }
            Err(err) => {
                error!("unable to bind on {}: {}", addr, err);
                return;
            }
        };

        let mut ca_certs = rustls_pemfile::certs(&mut self.chain_bytes.as_bytes())
            .filter_map(|e| e.ok())
            .collect();
        let key = private_key(&mut self.entity_key_bytes.as_bytes())
            .unwrap()
            .unwrap();
        let mut certs: Vec<_> = rustls_pemfile::certs(&mut self.entity_cert_bytes.as_bytes())
            .filter_map(|e| e.ok())
            .collect();
        certs.append(&mut ca_certs);

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();

        loop {
            tokio::select! {
                msg = init.channel.recv() => {
                    if matches!(msg, QBHHostMessage::Stop) {
                        break;
                    }
                }
                Ok((stream, addr)) = listener.accept() => {
                    info!("connected: {}", addr);
                    // yield a [QBIServerSocket]
                    init.attach(QBITCPServer {
                        config: config.clone(),
                        stream,
                        auth: self.auth.clone(),
                    })
                    .await;
                }
            }
        }
    }
}

/// An interface that handles a socket, which has been accepted
/// from a listener using the accept method. This gets attached through
/// the [QBHTCPServer].
#[derive(Debug)]
pub struct QBITCPServer {
    pub stream: TcpStream,
    pub config: ServerConfig,
    /// An authentication token sent on boot
    pub auth: Vec<u8>,
}

impl QBIContext for QBITCPServer {
    async fn run(self, host_id: QBDeviceId, com: QBIChannel) {
        let stream = self.stream;

        let acceptor = TlsAcceptor::from(Arc::new(self.config));
        let mut stream = acceptor.accept(stream).await.unwrap();

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
            stream: TlsStream::Server(stream),
            protocol,
        };

        runner.run().await;
    }
}
