// TODO: rustls TLS impl for preventing MITM attacks

use std::{net::SocketAddr, sync::Arc};

use bitcode::{Decode, Encode};
use qb_core::common::QBDeviceId;
use qb_ext::{
    hook::{QBHContext, QBHInit},
    interface::{QBIChannel, QBIContext, QBIHostMessage, QBIMessage, QBISetup, QBISlaveMessage},
};
use qb_proto::QBP;
use rustls::{
    client::danger::ServerCertVerifier,
    crypto::{
        verify_tls12_signature, verify_tls13_signature, CryptoProvider, WebPkiSupportedAlgorithms,
    },
    lock::Mutex,
    pki_types::{CertificateDer, ServerName},
    ClientConfig, RootCertStore, ServerConfig,
};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use tracing::{error, info, warn};

/// A hook which listens for incoming connections and yields
/// a [QBIServerSocket].
pub struct QBHServerSocket {
    pub listener: TcpListener,
    pub certified_key: rcgen::CertifiedKey,
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

        info!("generating certificate...");
        let subject_alt_names = vec!["quixbyte.application".to_string()];
        let certified_key = rcgen::generate_simple_self_signed(subject_alt_names).unwrap();
        println!("{:?}", certified_key.cert.pem());
        println!("{:?}", certified_key.key_pair);

        QBHServerSocket {
            auth,
            listener,
            certified_key,
        }
    }
}

impl QBHContext<QBIServerSocket> for QBHServerSocket {
    async fn run(self, init: QBHInit<QBIServerSocket>) {
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![self.certified_key.cert.der().clone()],
                self.certified_key
                    .key_pair
                    .serialize_der()
                    .try_into()
                    .unwrap(),
            )
            .unwrap();

        loop {
            // listen on incoming connections
            let (stream, addr) = self.listener.accept().await.unwrap();
            info!("connected: {}", addr);
            // yield a [QBIServerSocket]
            init.attach(QBIServerSocket {
                config: config.clone(),
                stream,
                auth: self.auth.clone(),
            })
            .await;
        }
    }
}

#[derive(Encode, Decode, Debug)]
pub struct QBIClientSocket {
    pub addr: String,
    /// An authentication token sent on boot
    pub auth: Vec<u8>,
    pub cert: Vec<u8>,
}

impl QBIContext for QBIClientSocket {
    async fn run(self, host_id: QBDeviceId, com: QBIChannel) {
        info!("initializing socket: {:#?}", self);

        let socket = TcpSocket::new_v4().unwrap();
        let addr = self.addr.parse().unwrap();
        let stream = socket.connect(addr).await.unwrap();

        let mut root_cert_store = RootCertStore::empty();
        root_cert_store.add(self.cert.into()).unwrap();
        // TODO: add root certificate
        let config = ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        let dnsname = ServerName::try_from("quixbyte.application").unwrap();
        let mut stream = connector.connect(dnsname, stream).await.unwrap();

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
            stream: TlsStream::Client(stream),
            protocol,
        };

        runner.run().await;
    }
}

#[derive(Encode, Decode, Serialize, Deserialize, Debug)]
pub struct QBIClientSocketSetup {
    pub addr: String,
    /// An authentication token sent on boot
    #[serde(with = "serde_bytes")]
    pub auth: Vec<u8>,
}

impl QBISetup<QBIClientSocket> for QBIClientSocketSetup {
    async fn setup(self) -> QBIClientSocket {
        let cert = Arc::new(Mutex::new(None));

        let socket = TcpSocket::new_v4().unwrap();
        let addr = self.addr.parse().unwrap();
        let stream = socket.connect(addr).await.unwrap();

        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SetupVerifier::new(cert.clone())))
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        let dnsname = ServerName::try_from("quixbyte.application").unwrap();
        let mut stream = connector.connect(dnsname, stream).await.unwrap();

        let mut protocol = QBP::default();
        protocol.negotiate(&mut stream).await.unwrap();
        protocol
            .send_payload(&mut stream, &self.auth)
            .await
            .unwrap();

        let cert = cert.lock().unwrap();
        let cert = cert.as_ref().unwrap();
        let cert: Vec<u8> = cert.as_ref().into();
        QBIClientSocket {
            addr: self.addr,
            auth: self.auth,
            cert,
        }
    }
}

// extract the server cert from the TLS connection
#[derive(Debug)]
pub struct SetupVerifier<'a> {
    cert: Arc<Mutex<Option<CertificateDer<'a>>>>,
    supported: WebPkiSupportedAlgorithms,
}

impl<'a> SetupVerifier<'a> {
    pub fn new(cert: Arc<Mutex<Option<CertificateDer<'a>>>>) -> Self {
        let provider = CryptoProvider::get_default().unwrap();
        let supported = provider.signature_verification_algorithms;
        Self { cert, supported }
    }
}

impl<'a> ServerCertVerifier for SetupVerifier<'a> {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        info!("extracting certificate");
        println!("KEKW");

        let mut cert = self.cert.lock().unwrap();
        *cert = Some(end_entity.clone().into_owned());

        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.supported.supported_schemes()
    }
}

#[derive(Debug)]
pub struct QBIServerSocket {
    pub stream: TcpStream,
    pub config: ServerConfig,
    /// An authentication token sent on boot
    pub auth: Vec<u8>,
}

impl QBIContext for QBIServerSocket {
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
