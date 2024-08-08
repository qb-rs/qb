//! # client
//!
//! This module is for the stuff that runs on the client.

use std::sync::Arc;

use bitcode::{Decode, Encode};
use qb_core::common::QBDeviceId;
use qb_ext::interface::{QBIChannel, QBIContext, QBISetup};
use qb_proto::QBP;
use rustls::{
    client::{danger::ServerCertVerifier, WebPkiServerVerifier},
    lock::Mutex,
    pki_types::{CertificateDer, ServerName},
    RootCertStore,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpSocket;
use tokio_rustls::{TlsConnector, TlsStream};
use tracing::{debug, info};

use crate::Runner;

#[derive(Encode, Decode, Serialize, Deserialize, Debug)]
pub struct QBITCPClient {
    pub addr: String,
    /// An authentication token sent on boot
    pub auth: Vec<u8>,

    #[serde(skip)]
    pub cert: Vec<u8>,
}

impl QBIContext for QBITCPClient {
    async fn run(self, host_id: QBDeviceId, com: QBIChannel) {
        debug!("initializing socket: {}", self.addr);

        let socket = TcpSocket::new_v4().unwrap();
        let addr = self.addr.parse().unwrap();
        let stream = socket.connect(addr).await.unwrap();

        let cert = Arc::new(Mutex::new(None));
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SetupVerifier::new(cert.clone()))
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        let dnsname = ServerName::try_from("quixbyte.local").unwrap();
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

// Somehow TLS does not work in this method. Find out why and fix it
impl QBISetup<QBITCPClient> for QBITCPClient {
    async fn setup(mut self) -> Self {
        debug!("initializing socket: {}", self.addr);

        let socket = TcpSocket::new_v4().unwrap();
        let addr = self.addr.parse().unwrap();
        let stream = socket.connect(addr).await.unwrap();

        let cert = Arc::new(Mutex::new(None));
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SetupVerifier::new(cert.clone()))
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        let dnsname = ServerName::try_from("quixbyte.local").unwrap();
        debug!("do TLS handshake");
        let mut stream = connector.connect(dnsname, stream).await.unwrap();
        self.cert = cert.lock().unwrap().as_ref().unwrap().clone();
        debug!("successfully extracted certificate");

        debug!("do quixbyte protocol handshake");
        let mut protocol = QBP::default();
        protocol.negotiate(&mut stream).await.unwrap();
        debug!("do quixbyte protocol auth");
        protocol
            .send_payload(&mut stream, &self.auth)
            .await
            .unwrap();
        info!("client-socket successfully setup");

        self
    }
}

// used for extracting the certificate from the TLS stream.
#[derive(Debug)]
struct SetupVerifier {
    // TODO: don't use webpki
    webpki: Arc<WebPkiServerVerifier>,
    cert: Arc<Mutex<Option<Vec<u8>>>>,
}

impl SetupVerifier {
    pub fn new(cert: Arc<Mutex<Option<Vec<u8>>>>) -> Arc<Self> {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let webpki = WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .unwrap();
        Arc::new(Self { cert, webpki })
    }
}

impl<'a> ServerCertVerifier for SetupVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let end_entity = end_entity.as_ref();
        let mut cert = self.cert.lock().unwrap();

        if cert.as_ref().is_some_and(|e| e != end_entity) {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ));
        }

        *cert = Some(end_entity.into());

        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.webpki.verify_tls13_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.webpki.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.webpki.supported_verify_schemes()
    }
}
