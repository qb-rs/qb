//! # qb-proto
//!
//! TODO: doc

#![warn(missing_docs)]

use std::collections::HashMap;

use bitcode::{Decode, Encode};
use itertools::Itertools;
use phf::phf_ordered_map;
use serde::{Deserialize, Serialize};
use simdutf8::basic::Utf8Error;
use thiserror::Error;
use tracing::trace;
use url_search_params::{build_url_search_params, parse_url_search_params};

/// This struct contains errors which may yield when working with QBP.
#[derive(Error, Debug)]
pub enum Error {
    /// An error occured while working with bitcode.
    /// This could indicate, for example, that the
    /// received payload was malformed, or encoded
    /// in another content-type or content-encoding
    /// than the one that was negotiated.
    #[error("bitcode: {0}")]
    BitcodeError(#[from] bitcode::Error),
    /// An error occured while working with json.
    /// This could indicate, for example, that the
    /// received payload was malformed, or encoded
    /// in another content-type or content-encoding
    /// than the one that was negotiated.
    #[error("json: {0}")]
    JsonError(#[from] serde_json::Error),
    /// An error occured while working with utf8.
    /// This could indicate, for example, that the
    /// received payload was malformed, or encoded
    /// in another content-type or content-encoding
    /// than the one that was negotiated.
    #[error("utf8: {0}")]
    Utf8Error(#[from] Utf8Error),
    /// An I/O error occured.
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
    /// Packet is of invalid size.
    #[error("invalid packet size: {0}, required: {0}")]
    InvalidPacketSize(usize, String),
    /// Header packet contains invalid magic bytes.
    #[error("header contains invalid magic bytes: {0:?}, expected: {0:?}")]
    InvalidMagicBytes(Vec<u8>, Vec<u8>),
    /// Header string contains non ascii characters.
    #[error("header str contains non ascii characters!")]
    NonAscii,
    /// Content type and/or content encoding could not
    /// be negotiated. This may be because the two peers
    /// do not state support of a common content type and/or
    /// encoding in the header packet.
    /// Otherwise this is a bug.
    #[error("could not negotiate {0}!")]
    NegotiationFailed(String),
    /// Connection has not been negotiated yet.
    #[error("connection not ready yet!")]
    NotReady,
    /// Connection has been closed while negotiating.
    #[error("received EOF while reading")]
    Closed,
}

/// A result type alias for convenience.
pub type Result<T> = std::result::Result<T, Error>;

/// A blob which can be sent over the protocol to allow different
/// messages in a different content-type than negotiated.
#[derive(Encode, Decode, Serialize, Deserialize)]
pub struct QBPBlob {
    /// The content type name. This should be a mime type string like
    /// `application/json` or `application/bitcode`.
    pub content_type: String,
    /// The actual content of this blob, which is in the format specified above.
    #[serde(with = "serde_bytes")]
    pub content: Vec<u8>,
}

impl QBPBlob {
    /// Deserialize this blob.
    ///
    /// This might throw an error if the content is malformed
    /// or the content type is not supported.
    pub fn deserialize<T: QBPDeserialize>(&self) -> Result<T> {
        match SUPPORTED_CONTENT_TYPES.get(&self.content_type) {
            Some(content_type) => content_type.from_bytes(&self.content),
            None => Err(Error::NegotiationFailed(format!(
                "{} not supported!",
                self.content_type
            ))),
        }
    }
}

/// The header packet whichOk(ServerCertVerified::assertion()) is used for content and version negotiation.
#[derive(Debug)]
pub struct QBPHeaderPacket {
    /// The major version of the QBP used to construct this packet.
    pub major_version: u8,
    /// The minor version of the QBP used to construct this packet.
    pub minor_version: u8,
    /// The headers of the QBP used to construct this packet.
    pub headers: HashMap<String, String>,
}

/// The bytes that come first at every header packet's payload to ensure
/// that the connected device actually communicates over QBP.
pub const MAGIC_BYTES: [u8; 3] = *b"QBP";

/// The major version of this QBP.
pub const MAJOR_VERSION: u8 = 0;
/// The minor version of this QBP.
pub const MINOR_VERSION: u8 = 0;

/// The content types which this QBP supports.
pub const SUPPORTED_CONTENT_TYPES: phf::OrderedMap<&'static str, QBPContentType> = phf_ordered_map! {
    "application/bitcode" => QBPContentType::Bitcode,
    "application/json" => QBPContentType::Json,
};

/// The content encodings which this QBP supports.
pub const SUPPORTED_CONTENT_ENCODINGS: phf::OrderedMap<&'static str, QBPContentEncoding> = phf_ordered_map! {
    "zlib" => QBPContentEncoding::Zlib,
    "gzip" => QBPContentEncoding::Gzip,
    "plain" => QBPContentEncoding::Plain,
};

impl QBPHeaderPacket {
    /// Convert from a standard QBPPacket.
    pub fn deserialize(packet: &[u8]) -> Result<Self> {
        // check whether packet length is valid
        if packet.len() < 5 {
            return Err(Error::InvalidPacketSize(packet.len(), ">= 5".into()));
        }

        // check whether magic bytes are valid
        let magic_bytes = &packet[0..3];
        if magic_bytes != MAGIC_BYTES {
            return Err(Error::InvalidMagicBytes(
                magic_bytes.into(),
                MAGIC_BYTES.into(),
            ));
        }

        // unwrap version
        let major_version = packet[3];
        let minor_version = packet[4];

        // read headers
        let head_bytes = &packet[5..];
        if !head_bytes.is_ascii() {
            return Err(Error::NonAscii);
        }
        let head = unsafe { std::str::from_utf8_unchecked(head_bytes) };
        let headers = parse_url_search_params(head);

        Ok(Self {
            major_version,
            minor_version,
            headers,
        })
    }

    /// Convert into a standard QBPPacket.
    pub fn serialize(self) -> Vec<u8> {
        let head = build_url_search_params(self.headers);
        let head_bytes = head.as_bytes();

        let mut content = Vec::with_capacity(head_bytes.len() + 5);
        content.extend_from_slice(&MAGIC_BYTES);
        content.push(self.major_version);
        content.push(self.minor_version);
        content.extend_from_slice(head_bytes);

        content
    }

    /// Get the header packet for this device.
    pub fn host() -> QBPHeaderPacket {
        let mut headers = HashMap::new();
        let accept = SUPPORTED_CONTENT_TYPES.keys().join(",");
        headers.insert("accept".to_owned(), accept);
        let accept_encoding = SUPPORTED_CONTENT_ENCODINGS.keys().join(",");
        headers.insert("accept-encoding".to_owned(), accept_encoding);
        QBPHeaderPacket {
            major_version: MAJOR_VERSION,
            minor_version: MINOR_VERSION,
            headers,
        }
    }
}

/// Negotiate the content-type.
pub fn negotiate_content_type(headers: &HashMap<String, String>) -> Option<QBPContentType> {
    let accept = headers.get("accept").unwrap();
    let accept = accept
        .split(',')
        .enumerate()
        .map(|(i, e)| (e.trim(), i))
        .collect::<HashMap<&str, usize>>();

    let mut possible_canidates: Vec<(&str, usize)> = Vec::new();

    for (i, name) in SUPPORTED_CONTENT_TYPES.keys().enumerate() {
        if let Some(other_i) = accept.get(name) {
            possible_canidates.push((name, i + other_i))
        }
    }

    // This one sorts the possible canidates by the sum
    // of the indicies (lower is better). If two entries
    // have the same sum, we sort by name instead ('a...'
    // is better than 'z...'). The best entry will be at index 0.
    possible_canidates.sort_unstable_by(|a, b| match a.1.cmp(&b.1) {
        std::cmp::Ordering::Equal => b.0.cmp(a.0),
        v => v,
    });

    Some(unsafe {
        SUPPORTED_CONTENT_TYPES
            .get(possible_canidates.first()?.0)
            .unwrap_unchecked()
            .clone()
    })
}

/// Negotiate the content-encoding.
pub fn negotiate_content_encoding(headers: &HashMap<String, String>) -> Option<QBPContentEncoding> {
    let accept_encoding = headers.get("accept-encoding").unwrap();
    let accept = accept_encoding
        .split(',')
        .enumerate()
        .map(|(i, e)| (e.trim(), i))
        .collect::<HashMap<&str, usize>>();

    let mut possible_canidates: Vec<(&str, usize)> = Vec::new();

    for (i, name) in SUPPORTED_CONTENT_ENCODINGS.keys().enumerate() {
        if let Some(other_i) = accept.get(name) {
            possible_canidates.push((name, i + other_i))
        }
    }

    // This one sorts the possible canidates by the sum
    // of the indicies (lower is better). If two entries
    // have the same sum, we sort by name instead ('a...'
    // is better than 'z...'). The best entry will be at index 0.
    possible_canidates.sort_unstable_by(|a, b| match a.1.cmp(&b.1) {
        std::cmp::Ordering::Equal => b.0.cmp(a.0),
        v => v,
    });

    Some(unsafe {
        SUPPORTED_CONTENT_ENCODINGS
            .get(possible_canidates.first()?.0)
            .unwrap_unchecked()
            .clone()
    })
}

/// This struct describes a content encoding that can be negotiated
/// in a QBP connection.
#[derive(Debug, Clone)]
pub enum QBPContentEncoding {
    /// Use zlib to (de)compress payloads.
    Zlib,
    /// Use gzip to (de)compress payloads.
    Gzip,
    /// Do not (de)compress payloads.
    Plain,
}

// This is in a seperate module, as it uses the
// synchronous Write trait from std::io, which conflicts
// the asynchronous write traits from tokio.
mod encodeimpl {
    use super::QBPContentEncoding;
    use flate2::{
        write::{GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder},
        Compression,
    };
    use std::io::Write;
    use tracing::trace;

    impl QBPContentEncoding {
        /// Encode data with this encoding.
        pub fn encode(&self, data: &[u8]) -> Vec<u8> {
            match self {
                QBPContentEncoding::Zlib => {
                    trace!("encode: encoding data with zlib: {}", data.len());

                    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
                    encoder.write_all(data).unwrap();
                    let res = encoder.finish().unwrap();

                    trace!("encode: result: {}", res.len());

                    res
                }
                QBPContentEncoding::Gzip => {
                    trace!("encode: encoding data with gzip: {}", data.len());

                    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
                    encoder.write_all(data).unwrap();
                    let res = encoder.finish().unwrap();

                    trace!("encode: result: {}", res.len());

                    res
                }
                QBPContentEncoding::Plain => {
                    trace!("encode: skip compression");

                    data.into()
                }
            }
        }

        /// Decode encoded data.
        pub fn decode(&self, data: &[u8]) -> Vec<u8> {
            match self {
                QBPContentEncoding::Zlib => {
                    let mut decoder = ZlibDecoder::new(Vec::new());
                    decoder.write_all(data).unwrap();
                    decoder.finish().unwrap()
                }
                QBPContentEncoding::Gzip => {
                    let mut decoder = GzDecoder::new(Vec::new());
                    decoder.write_all(data).unwrap();
                    decoder.finish().unwrap()
                }
                QBPContentEncoding::Plain => {
                    trace!("encode: skip decompression");

                    data.into()
                }
            }
        }
    }
}

/// This struct describes a content type that can be negotiated
/// in a QBP connection.
#[derive(Debug, Clone)]
pub enum QBPContentType {
    /// application/json
    ///
    /// Supported by most backends. Slower compared to
    /// Bitcode and also includes schema, so it produces
    /// larger messages as well.
    Json,
    /// application/bitcode
    ///
    /// Supported only by rust backends, no support with
    /// other programming languages. This normally is fast and
    /// tiny compared to Json, which is why it is prefered.
    Bitcode,
}

impl QBPContentType {
    /// Convert bytes of this content type to a message.
    pub fn from_bytes<T: QBPDeserialize>(&self, data: &[u8]) -> Result<T> {
        Ok(match self {
            QBPContentType::Json => T::from_json(data)?,
            QBPContentType::Bitcode => T::from_bitcode(data)?,
        })
    }

    /// Convert a message to bytes of this content type.
    pub fn to_bytes(&self, msg: impl QBPSerialize) -> Result<Vec<u8>> {
        Ok(match self {
            QBPContentType::Json => msg.to_json()?,
            QBPContentType::Bitcode => msg.to_bitcode(),
        })
    }
}

/// The message utility trait
pub trait QBPSerialize: Encode + Serialize {
    /// Dump a message into an encoded json string.
    fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_string(self)?.into_bytes())
    }

    /// Dump a message into a bitcode binary.
    fn to_bitcode(&self) -> Vec<u8> {
        bitcode::encode(self)
    }
}
impl<T> QBPSerialize for T where T: Encode + Serialize {}

/// The message utility trait
pub trait QBPDeserialize: for<'a> Decode<'a> + for<'a> Deserialize<'a> {
    /// Parse a message from an encoded json string.
    fn from_json(data: &[u8]) -> Result<Self> {
        serde_json::from_str::<Self>(simdutf8::basic::from_utf8(data)?).map_err(|e| e.into())
    }

    /// Parse a message from a bitcode binary.
    fn from_bitcode(data: &[u8]) -> Result<Self> {
        bitcode::decode(data).map_err(|e| e.into())
    }
}
impl<T> QBPDeserialize for T where T: for<'a> Decode<'a> + for<'a> Deserialize<'a> {}

/// The message utility trait
pub trait QBPMessage: QBPSerialize + QBPDeserialize {}
impl<T> QBPMessage for T where T: QBPSerialize + QBPDeserialize {}

/// This enum represents the state a QBP connection is in.
#[derive(Debug)]
pub enum QBPState {
    /// Initial state. We need to send the header
    /// for negotiation purposes.
    Initial,
    /// Negotiation state. We need to negotiate
    /// the content type and the content encoding
    /// in order to send messages.
    Negotiate,
    /// Receive messages, after the content
    /// type and encoding has been negotiated.
    Messages {
        /// the negotiated content_type
        content_type: QBPContentType,
        /// the negotiated content_encoding
        content_encoding: QBPContentEncoding,
    },
}

impl Default for QBPState {
    fn default() -> Self {
        Self::Initial
    }
}

/// This struct represents a QBP connection.
#[derive(Debug, Default)]
pub struct QBP {
    state: QBPState,
    reader: QBPReader,
    writer: QBPWriter,
}

/// Utility trait for impl usage.
pub trait Read: tokio::io::AsyncReadExt + Unpin {}
impl<T> Read for T where T: tokio::io::AsyncReadExt + Unpin {}

/// Utility trait for impl usage.
pub trait Write: tokio::io::AsyncWriteExt + Unpin {}
impl<T> Write for T where T: tokio::io::AsyncWriteExt + Unpin {}

/// Utility trait for impl usage.
pub trait ReadWrite: tokio::io::AsyncReadExt + tokio::io::AsyncWriteExt + Unpin {}
impl<T> ReadWrite for T where T: tokio::io::AsyncReadExt + tokio::io::AsyncWriteExt + Unpin {}

impl QBP {
    /// Returns whether this connection is unitialized,
    /// which means that no negotiation request has been sent yet.
    pub fn is_uninitialized(&self) -> bool {
        matches!(self.state, QBPState::Initial)
    }

    /// Returns whether this connection is negotiating a
    /// common content type and encoding.
    pub fn is_negotiating(&self) -> bool {
        matches!(self.state, QBPState::Negotiate)
    }

    /// Returns whether this connection is ready,
    /// which means that the content type and encoding
    /// has been negotiated.
    pub fn is_ready(&self) -> bool {
        matches!(self.state, QBPState::Messages { .. })
    }

    /// Send a packet through this protocol.
    ///
    /// You probably don't want to use this method, as-is,
    /// as content-type and content-encoding play no role here.
    ///
    /// To send a binary payload after content-encoding has been
    /// negotiated see [send_payload].
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn send_packet(&mut self, write: &mut impl Write, packet: &[u8]) -> Result<()> {
        self.writer.write(write, packet).await
    }

    /// Receive a message from this protocol.
    ///
    /// You probably don't want to use this method, as-is,
    /// as content-type and content-encoding play no role here.
    ///
    /// To read a binary payload after content-encoding has been
    /// negotiated see [read_payload].
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn recv_packet(&mut self, read: &mut impl Read) -> Result<Vec<u8>> {
        self.reader.read(read).await
    }

    /// Send a binary payload through this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn send_payload(&mut self, write: &mut impl Write, payload: &[u8]) -> Result<()> {
        let (_, content_encoding) = self.get_content()?;
        let packet = content_encoding.encode(payload);
        self.send_packet(write, &packet).await
    }

    /// Receive a binary payload through this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn recv_payload(&mut self, read: &mut impl Read) -> Result<Vec<u8>> {
        let packet = self.recv_packet(read).await?;
        let (_, content_encoding) = self.get_content()?;
        let payload = content_encoding.decode(&packet);
        Ok(payload)
    }

    /// Send a message through this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn send(&mut self, write: &mut impl Write, msg: impl QBPSerialize) -> Result<()> {
        let (content_type, content_encoding) = self.get_content()?;
        let payload = content_type.to_bytes(msg)?;
        let packet = content_encoding.encode(&payload);
        self.send_packet(write, &packet).await
    }

    /// Read a message from this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn recv<T: QBPDeserialize>(&mut self, read: &mut impl Read) -> Result<T> {
        let packet = self.recv_packet(read).await?;
        let (content_type, content_encoding) = self.get_content()?;
        let payload = content_encoding.decode(&packet);
        let message = content_type.from_bytes::<T>(&payload)?;
        Ok(message)
    }

    /// Try to get content-type and content-encoding of this
    /// protocol. Returns an error if not negotiated yet.
    fn get_content(&self) -> Result<(&QBPContentType, &QBPContentEncoding)> {
        match &self.state {
            QBPState::Messages {
                content_type,
                content_encoding,
            } => Ok((content_type, content_encoding)),
            _ => Err(Error::NotReady),
        }
    }

    /// Update the connection. This will instantiate negotiation if
    /// uninitialized and wait for a negotiated connection. It then
    /// returns the decoded messages. This method is useful for working
    /// with tokio::select!.
    ///
    /// If you only want to negotiate a connection and send/receive
    /// data in a synchronous way, see [negotiate], [read] and [send].
    /// (note that your code will not be cancelation safe, if it
    /// involves multiple cancelation safe methods).
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn update<T: QBPDeserialize>(&mut self, conn: &mut impl ReadWrite) -> Result<T> {
        // send header packet
        if let QBPState::Initial = self.state {
            self.state = QBPState::Negotiate;
            let header = QBPHeaderPacket::host();
            self.send_packet(conn, &header.serialize()).await?;
        }

        // flush the writer
        self.writer.flush(conn).await?;

        loop {
            let packet = self.recv_packet(conn).await?;

            match &self.state {
                QBPState::Negotiate => {
                    let header = QBPHeaderPacket::deserialize(&packet)?;
                    trace!("recv header: {:?}", header);
                    let content_type = negotiate_content_type(&header.headers)
                        .ok_or(Error::NegotiationFailed("content-type".into()))?;
                    let content_encoding = negotiate_content_encoding(&header.headers)
                        .ok_or(Error::NegotiationFailed("content-encoding".into()))?;
                    self.state = QBPState::Messages {
                        content_type,
                        content_encoding,
                    };
                }
                QBPState::Messages {
                    content_type,
                    content_encoding,
                } => {
                    let payload = content_encoding.decode(&packet);
                    let message = content_type.from_bytes::<T>(&payload)?;
                    return Ok(message);
                }
                _ => panic!("unexpected behavior"),
            }
        }
    }

    /// Negotiate a connection. This only works on uninitialized connections
    /// (see [is_uninitialized]). This will send a header packet and then wait
    /// for a response, which is also a header packet. Those packets are then
    /// used to negotiate a common content-type and content-encoding.
    ///
    /// # Cancelation Safety
    /// This method is partially cancelation safe, meaning, if you use it
    /// in tokio::select! and another branch completes first, you may
    /// not use this method again, as the QBP is now partially initialized,
    /// and the writer may not be flushed.
    ///
    /// Please take a look at [update] instead.
    pub async fn negotiate(&mut self, conn: &mut impl ReadWrite) -> Result<()> {
        assert!(self.is_uninitialized());

        let header = QBPHeaderPacket::host();
        self.send_packet(conn, &header.serialize()).await?;
        self.state = QBPState::Negotiate;

        let packet = self.recv_packet(conn).await?;
        let header = QBPHeaderPacket::deserialize(&packet)?;
        trace!("recv header: {:?}", header);
        let content_type = negotiate_content_type(&header.headers)
            .ok_or(Error::NegotiationFailed("content-type".into()))?;
        let content_encoding = negotiate_content_encoding(&header.headers)
            .ok_or(Error::NegotiationFailed("content-encoding".into()))?;
        self.state = QBPState::Messages {
            content_type,
            content_encoding,
        };

        Ok(())
    }
}

#[derive(Debug, Default)]
struct QBPWriter {
    bytes: Vec<u8>,
    written: usize,
}

impl QBPWriter {
    /// Write a packet.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn write(&mut self, write: &mut impl Write, packet: &[u8]) -> Result<()> {
        trace!("write: len {}:", packet.len());
        let len_bytes = (packet.len() as u64).to_be_bytes();
        self.bytes.extend_from_slice(&len_bytes);
        trace!("write: data");
        self.bytes.extend_from_slice(packet);
        self.flush(write).await
    }

    /// Flush this writer.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn flush(&mut self, write: &mut impl Write) -> Result<()> {
        trace!("write: bytes to flush: {}", self.bytes.len());
        while self.bytes.len() > self.written {
            let len = write.write(&self.bytes[self.written..]).await?;
            trace!("write: wrote bytes: {}", len);
            self.written += len;
        }
        write.flush().await?;
        self.bytes.clear();
        self.written = 0;
        trace!("write: flush complete");
        Ok(())
    }
}

#[derive(Debug, Default)]
struct QBPReader {
    packet_len: Option<usize>,
    bytes: Vec<u8>,
}

impl QBPReader {
    /// Read a packet.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn read(&mut self, read: &mut impl Read) -> Result<Vec<u8>> {
        trace!("read: read packet");
        loop {
            // process loop
            loop {
                trace!("read: bytes in buffer {}", self.bytes.len());
                match self.packet_len {
                    Some(len) => {
                        // read payload
                        if self.bytes.len() >= len {
                            trace!("read: complete");
                            let packet = self.bytes.drain(0..len).collect::<Vec<_>>();
                            self.packet_len = None;
                            return Ok(packet);
                        } else {
                            break;
                        }
                    }
                    None => {
                        // read length
                        if self.bytes.len() >= 8 {
                            let mut len_bytes = [0u8; 8];
                            len_bytes.copy_from_slice(&self.bytes[0..8]);
                            // remove len bytes from buffer
                            self.bytes.drain(0..8);
                            let len = u64::from_be_bytes(len_bytes) as usize;
                            trace!("read: len: {}", len);
                            self.packet_len = Some(len);
                        } else {
                            break;
                        }
                    }
                }
            }

            // read data
            let mut bytes: [u8; 1024] = [0; 1024];
            let len = read.read(&mut bytes).await?;
            trace!("read: read bytes from source: {}", len);
            if len == 0 {
                return Err(Error::Closed);
            }
            self.bytes.extend_from_slice(&bytes[0..len]);
        }
    }
}
