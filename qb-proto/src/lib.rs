use std::collections::HashMap;

use phf::phf_ordered_map;
use thiserror::Error;
use url_search_params::{build_url_search_params, parse_url_search_params};

/// A packet which has been read from the QBPReader.
pub struct QBPPacket {
    pub content: Vec<u8>,
}

/// An error which occured when trying to convert a packet into a QBPHeaderPacket
#[derive(Error, Debug)]
pub enum QBPHeaderError<'a> {
    #[error("invalid packet size: {0}, minimum required: {0}")]
    InvalidPacketSize(usize, String),
    #[error("invalid magic bytes: {0:?}, expected: {0:?}")]
    InvalidMagicBytes(&'a [u8], &'a [u8]),
    #[error("header str contains non ascii characters!")]
    NonAscii,
}

/// The header packet which is used for content and version negotiation.
pub struct QBPHeaderPacket {
    pub major_byte: u8,
    pub minor_byte: u8,
    pub headers: HashMap<String, String>,
}

/// The bytes that come first at every header packet's payload to ensure
/// that the connected device actually communicates over QBP.
pub const MAGIC_BYTES: [u8; 3] = *b"QBP";

impl From<QBPHeaderPacket> for QBPPacket {
    fn from(value: QBPHeaderPacket) -> Self {
        value.serialize()
    }
}

impl QBPHeaderPacket {
    /// Convert from a standard QBPPacket.
    pub fn deserialize<'a>(packet: &'a QBPPacket) -> Result<Self, QBPHeaderError> {
        // check whether packet length is valid
        if packet.content.len() < 5 {
            return Err(QBPHeaderError::InvalidPacketSize(
                packet.content.len(),
                ">= 5".into(),
            ));
        }

        // check whether magic bytes are valid
        let magic_bytes = &packet.content[0..3];
        if magic_bytes != &MAGIC_BYTES {
            return Err(QBPHeaderError::InvalidMagicBytes(magic_bytes, &MAGIC_BYTES));
        }

        // unwrap version
        let major_byte = packet.content[3];
        let minor_byte = packet.content[4];

        // read headers
        let head_bytes = &packet.content[5..];
        if !head_bytes.is_ascii() {
            return Err(QBPHeaderError::NonAscii);
        }
        let head = unsafe { std::str::from_utf8_unchecked(head_bytes) };
        let headers = parse_url_search_params(head);

        Ok(Self {
            major_byte,
            minor_byte,
            headers,
        })
    }

    /// Convert into a standard QBPPacket.
    pub fn serialize(self) -> QBPPacket {
        let head = build_url_search_params(self.headers);
        let head_bytes = head.as_bytes();

        let mut content = Vec::with_capacity(head_bytes.len() + 5);
        content.extend_from_slice(&MAGIC_BYTES);
        content.push(self.major_byte);
        content.push(self.minor_byte);
        content.extend_from_slice(&head_bytes);

        QBPPacket { content }
    }
}

pub struct QBPReader {}

pub const SUPPORTED_CONTENT_TYPES: phf::OrderedMap<&'static str, ContentType> = phf_ordered_map! {
    "application/bitcode" => ContentType::Bitcode,
    "application/json" => ContentType::Json,
};

/// Negotiate the content-type.
pub fn negotiate(headers: &HashMap<String, String>) -> Option<&ContentType> {
    let accept = headers.get("accept").unwrap();
    let accept = accept
        .split(',')
        .enumerate()
        .map(|(i, e)| (e.trim(), i))
        .collect::<HashMap<&str, usize>>();

    let mut possible_canidates: Vec<(&str, usize)> = Vec::new();

    for (i, name) in SUPPORTED_CONTENT_TYPES.keys().enumerate() {
        if let Some(other) = accept.get(name) {
            possible_canidates.push((name, i + other))
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
    })
}

pub enum ContentType {
    Json,
    Bitcode,
}
