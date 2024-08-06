use std::collections::HashMap;

use bitcode::{Decode, Encode};
use compression::prelude::*;
use itertools::Itertools;
use phf::phf_ordered_map;
use serde::{Deserialize, Serialize};
use simdutf8::basic::Utf8Error;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url_search_params::{build_url_search_params, parse_url_search_params};

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
    /// An error occured while
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("invalid packet size: {0}, required: {0}")]
    InvalidPacketSize(usize, String),
    #[error("header contains invalid magic bytes: {0:?}, expected: {0:?}")]
    InvalidMagicBytes(Vec<u8>, Vec<u8>),
    #[error("header str contains non ascii characters!")]
    NonAscii,
    #[error("could not negotiate {0}!")]
    NegotiationFailed(String),
    #[error("connection not ready yet!")]
    NotReady,
    #[error("received EOF while reading")]
    Closed,
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct QBPBlob {
    pub content_type: String,
    pub content: Vec<u8>,
}

impl QBPBlob {
    /// Deserialize this blob.
    ///
    /// This might throw an error if the content is malformed
    /// or the content type is not supported.
    pub fn deserialize<T>(&self) -> Result<T>
    where
        for<'a> T: QBPMessage<'a>,
    {
        match SUPPORTED_CONTENT_TYPES.get(&self.content_type) {
            Some(content_type) => content_type.from_bytes(&self.content),
            None => Err(Error::NegotiationFailed(format!(
                "{} not supported!",
                self.content_type
            ))),
        }
    }
}

/// The header packet which is used for content and version negotiation.
pub struct QBPHeaderPacket {
    pub major_version: u8,
    pub minor_version: u8,
    pub headers: HashMap<String, String>,
}

/// The bytes that come first at every header packet's payload to ensure
/// that the connected device actually communicates over QBP.
pub const MAGIC_BYTES: [u8; 3] = *b"QBP";

pub const MAJOR_VERSION: u8 = 0;
pub const MINOR_VERSION: u8 = 0;

/// The content types which this QBP supports.
pub const SUPPORTED_CONTENT_TYPES: phf::OrderedMap<&'static str, QBPContentType> = phf_ordered_map! {
    "application/bitcode" => QBPContentType::Bitcode,
    "application/json" => QBPContentType::Json,
};

pub const SUPPORTED_CONTENT_ENCODINGS: phf::OrderedMap<&'static str, QBPContentEncoding> = phf_ordered_map! {
    "bzip2" => QBPContentEncoding::BZip2,
    "gzip" => QBPContentEncoding::GZip,
    "zlib" => QBPContentEncoding::Zlib,
};

impl QBPHeaderPacket {
    /// Convert from a standard QBPPacket.
    pub fn deserialize<'a>(packet: &'a [u8]) -> Result<Self> {
        // check whether packet length is valid
        if packet.len() < 5 {
            return Err(Error::InvalidPacketSize(packet.len(), ">= 5".into()));
        }

        // check whether magic bytes are valid
        let magic_bytes = &packet[0..3];
        if magic_bytes != &MAGIC_BYTES {
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
        content.extend_from_slice(&head_bytes);

        content
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

#[derive(Debug, Clone)]
pub enum QBPContentEncoding {
    BZip2,
    GZip,
    Zlib,
}

impl QBPContentEncoding {
    /// Encode a blob of data using this encoding.
    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        match self {
            QBPContentEncoding::BZip2 => Self::_encode(data, BZip2Encoder::new(9)),
            QBPContentEncoding::GZip => Self::_encode(data, GZipEncoder::new()),
            QBPContentEncoding::Zlib => Self::_encode(data, ZlibEncoder::new()),
        }
    }

    #[inline(always)]
    fn _encode<E: Encoder<In = u8, Out = u8>>(data: &[u8], mut encoder: E) -> Vec<u8>
    where
        CompressionError: From<E::Error>,
        E::Error: std::fmt::Debug,
    {
        data.into_iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }

    /// Decode a blob of data using this encoding.
    pub fn decode(&self, data: &[u8]) -> Vec<u8> {
        match self {
            QBPContentEncoding::BZip2 => Self::_decode(data, BZip2Decoder::new()),
            QBPContentEncoding::GZip => Self::_decode(data, GZipDecoder::new()),
            QBPContentEncoding::Zlib => Self::_decode(data, ZlibDecoder::new()),
        }
    }

    #[inline(always)]
    fn _decode<D: Decoder<Input = u8, Output = u8>>(data: &[u8], mut decoder: D) -> Vec<u8>
    where
        CompressionError: From<D::Error>,
        D::Error: std::fmt::Debug,
    {
        data.into_iter()
            .cloned()
            .decode(&mut decoder)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }
}

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
    pub fn from_bytes<T>(&self, data: &[u8]) -> Result<T>
    where
        for<'a> T: QBPMessage<'a>,
    {
        Ok(match self {
            QBPContentType::Json => T::from_json(data)?,
            QBPContentType::Bitcode => T::from_bitcode(data)?,
        })
    }

    /// Convert a message to bytes of this content type.
    pub fn to_bytes<T>(&self, msg: T) -> Result<Vec<u8>>
    where
        for<'a> T: QBPMessage<'a>,
    {
        Ok(match self {
            QBPContentType::Json => msg.to_json()?,
            QBPContentType::Bitcode => msg.to_bitcode(),
        })
    }
}

pub trait QBPMessage<'a>: Encode + Decode<'a> + Serialize + Deserialize<'a> {
    /// Parse a message from an encoded json string.
    fn from_json(data: &'a [u8]) -> Result<Self> {
        serde_json::from_str::<Self>(simdutf8::basic::from_utf8(data)?).map_err(|e| e.into())
    }

    /// Dump a message into an encoded json string.
    fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_string(self)?.into_bytes())
    }

    /// Parse a message from a bitcode binary.
    fn from_bitcode(data: &'a [u8]) -> Result<Self> {
        bitcode::decode(data).map_err(|e| e.into())
    }

    /// Dump a message into a bitcode binary.
    fn to_bitcode(&self) -> Vec<u8> {
        bitcode::encode(self)
    }
}

/// auto implement the message trait
impl<'a, T> QBPMessage<'a> for T where T: Encode + Decode<'a> + Serialize + Deserialize<'a> {}

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
        content_type: QBPContentType,
        content_encoding: QBPContentEncoding,
    },
}

impl Default for QBPState {
    fn default() -> Self {
        Self::Initial
    }
}

#[derive(Debug, Default)]
pub struct QBP {
    state: QBPState,
    reader: QBPReader,
    writer: QBPWriter,
}

pub trait Read: AsyncReadExt + Unpin {}
impl<T> Read for T where T: AsyncReadExt + Unpin {}

pub trait Write: AsyncWriteExt + Unpin {}
impl<T> Write for T where T: AsyncWriteExt + Unpin {}

pub trait ReadWrite: AsyncReadExt + AsyncWriteExt + Unpin {}
impl<T> ReadWrite for T where T: AsyncReadExt + AsyncWriteExt + Unpin {}

impl QBP {
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

    /// Read a message from this protocol.
    ///
    /// You probably don't want to use this method, as-is,
    /// as content-type and content-encoding play no role here.
    ///
    /// To read a binary payload after content-encoding has been
    /// negotiated see [read_payload].
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn read_packet(&mut self, read: &mut impl Read) -> Result<Vec<u8>> {
        self.reader.read(read).await
    }

    /// Send a binary payload through this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn send_payload(&mut self, write: &mut impl Write, payload: &[u8]) -> Result<()> {
        let (_, content_encoding) = self.get_content()?;
        let packet = content_encoding.encode(&payload);
        self.send_packet(write, &packet).await
    }

    /// Read a binary payload through this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn read_payload(&mut self, read: &mut impl Read) -> Result<Vec<u8>> {
        let packet = self.read_packet(read).await?;
        let (_, content_encoding) = self.get_content()?;
        let payload = content_encoding.decode(&packet);
        Ok(payload)
    }

    /// Send a message through this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn send<T>(&mut self, write: &mut impl Write, msg: T) -> Result<()>
    where
        for<'a> T: QBPMessage<'a>,
    {
        let (content_type, content_encoding) = self.get_content()?;
        let payload = content_type.to_bytes(msg)?;
        let packet = content_encoding.encode(&payload);
        self.send_packet(write, &packet).await
    }

    /// Read a message from this protocol.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn read<T>(&mut self, read: &mut impl Read) -> Result<T>
    where
        for<'a> T: QBPMessage<'a>,
    {
        let packet = self.read_packet(read).await?;
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

    /// Update the connection. Returns a decoded
    /// message, if any.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn update<T>(&mut self, conn: &mut impl ReadWrite) -> Result<Option<T>>
    where
        for<'a> T: QBPMessage<'a>,
    {
        // flush the writer
        self.writer.flush(conn).await?;

        // send header packet
        if let QBPState::Initial = self.state {
            let mut headers = HashMap::new();
            let accept = SUPPORTED_CONTENT_TYPES.keys().join(",");
            headers.insert("accept".to_owned(), accept);
            let header = QBPHeaderPacket {
                major_version: MAJOR_VERSION,
                minor_version: MINOR_VERSION,
                headers,
            };

            conn.write_all(&header.serialize()).await?;
            self.state = QBPState::Negotiate;
            return Ok(None);
        }

        let packet = self.read_packet(conn).await?;

        match &self.state {
            QBPState::Negotiate => {
                let header = QBPHeaderPacket::deserialize(&packet)?;
                let content_type = negotiate_content_type(&header.headers)
                    .ok_or(Error::NegotiationFailed("content-type".into()))?;
                let content_encoding = negotiate_content_encoding(&header.headers)
                    .ok_or(Error::NegotiationFailed("content-encoding".into()))?;
                self.state = QBPState::Messages {
                    content_type,
                    content_encoding,
                };
                Ok(None)
            }
            QBPState::Messages {
                content_type,
                content_encoding,
            } => {
                let payload = content_encoding.decode(&packet);
                let message = content_type.from_bytes::<T>(&payload)?;
                Ok(Some(message))
            }
            _ => panic!("unexpected behavior"),
        }
    }
}

#[derive(Debug, Default)]
pub struct QBPWriter {
    bytes: Vec<u8>,
    written: usize,
}

impl QBPWriter {
    /// Write a packet.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn write(&mut self, write: &mut impl Write, packet: &[u8]) -> Result<()> {
        let len_bytes = (packet.len() as u64).to_be_bytes();
        self.bytes.extend_from_slice(&len_bytes);
        self.bytes.extend_from_slice(&packet);
        self.flush(write).await
    }

    /// Flush this writer.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn flush(&mut self, write: &mut impl Write) -> Result<()> {
        while self.bytes.len() - self.written > 0 {
            self.written += write.write(&self.bytes[self.written..]).await?;
        }
        self.bytes.clear();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct QBPReader {
    len: Option<usize>,
    bytes: Vec<u8>,
}

impl QBPReader {
    /// Read a packet.
    ///
    /// # Cancelation Safety
    /// This method is cancelation safe.
    pub async fn read(&mut self, read: &mut impl Read) -> Result<Vec<u8>> {
        loop {
            match self.len {
                Some(len) => {
                    // read payload
                    if self.bytes.len() >= len {
                        return Ok(self.bytes.drain(0..len).collect::<Vec<_>>());
                    }
                }
                None => {
                    // read length
                    if self.bytes.len() >= 8 {
                        let mut len_bytes = [0u8; 8];
                        len_bytes.copy_from_slice(&self.bytes);
                        self.len = Some(u64::from_be_bytes(len_bytes) as usize);
                    }
                }
            }

            let mut bytes: [u8; 1024] = [0; 1024];
            let len = read.read(&mut bytes).await?;
            if len == 0 {
                return Err(Error::Closed);
            }
            self.bytes.extend_from_slice(&bytes[0..len]);
        }
    }
}
