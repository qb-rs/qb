//! # server
//!
//! This module is for the stuff that runs on the server.

use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use qb_core::device::QBDeviceId;
use qb_ext::{
    hook::{QBHContext, QBHInit},
    interface::{QBIChannel, QBIContext},
};
use qb_proto::QBP;
use rcgen::SanType;
use rustls::ServerConfig;
use rustls_cert_gen::CertificateBuilder;
use rustls_pemfile::private_key;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsStream};
use tracing::{debug, error, info, warn};

use crate::Runner;

/// A hook which listens for incoming connections and yields
/// a [QBITCPServer].
pub struct QBHTCPServer {
    pub listener: TcpListener,
    pub config: ServerConfig,
    /// An authentication token sent on boot
    pub auth: Vec<u8>,
}

impl QBHTCPServer {
    /// Listen locally. Tries to bind a socket to
    /// `0.0.0.0:{port}` and if it fails tries again with the
    /// next successive port number.
    pub async fn listen(mut port: u16, auth: impl Into<Vec<u8>>) -> QBHTCPServer {
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

        debug!("generating certificate...");
        let ca = CertificateBuilder::new()
            .certificate_authority()
            .country_name("Germany")
            .unwrap()
            .organization_name("QuixByte Local CA")
            .build()
            .unwrap();
        let chain_pem = ca.serialize_pem();
        let mut chain_bytes = chain_pem.cert_pem.as_bytes();
        let mut ca_certs = rustls_pemfile::certs(&mut chain_bytes)
            .filter_map(|e| e.ok())
            .collect();

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
        let mut entity_bytes = entity_pem.private_key_pem.as_bytes();
        let key = private_key(&mut entity_bytes).unwrap().unwrap();

        let mut entity_bytes = entity_pem.cert_pem.as_bytes();
        let mut certs: Vec<_> = rustls_pemfile::certs(&mut entity_bytes)
            .filter_map(|e| e.ok())
            .collect();
        certs.append(&mut ca_certs);

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();

        QBHTCPServer {
            auth,
            listener,
            config,
        }
    }
}

impl QBHContext<QBITCPServer> for QBHTCPServer {
    async fn run(self, init: QBHInit<QBITCPServer>) {
        loop {
            // listen on incoming connections
            let (stream, addr) = self.listener.accept().await.unwrap();
            info!("connected: {}", addr);
            // yield a [QBIServerSocket]
            init.attach(QBITCPServer {
                config: self.config.clone(),
                stream,
                auth: self.auth.clone(),
            })
            .await;
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
