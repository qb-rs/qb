pub use bitcode::{Decode, Encode};
pub use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("bitcode error: {0}")]
    Bitcode(#[from] bitcode::Error),
    #[error("json error: {0}")]
    JSON(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait Message<'a>: Encode + Decode<'a> + Serialize + Deserialize<'a> {
    /// Parse a message from a json string.
    fn from_json(data: &'a str) -> Result<Self> {
        serde_json::from_str::<Self>(data).map_err(|e| e.into())
    }

    /// Dump a message into a json string.
    fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| e.into())
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

pub trait Protocol<'a, T>
where
    T: Message<'a>,
{
    type Error;
    fn from_bytes(bytes: &'a [u8]) -> Option<std::result::Result<(T, usize), Self::Error>>;
    fn to_bytes(msg: T) -> std::result::Result<Vec<u8>, Self::Error>;
}

#[derive(Error, Debug)]
pub enum ProtocolReaderError<ProtocolError> {
    #[error("protocol error: {0}")]
    Protocol(ProtocolError),
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

pub struct ProtocolReader {
    bytes: Vec<u8>,
}

impl ProtocolReader {
    async fn read_async<'a, R, T, P>(
        &'a mut self,
        read: &mut R,
    ) -> std::result::Result<T, ProtocolReaderError<P::Error>>
    where
        R: tokio::io::AsyncReadExt + Unpin,
        P: Protocol<'a, T>,
        T: Message<'a>,
    {
        loop {
            if let Some(res) = P::from_bytes(&self.bytes) {
                let (res, len) = res.map_err(|e| ProtocolReaderError::Protocol(e))?;

                return Ok(res);
            }

            let mut bytes: [u8; 1024] = [0; 1024];
            let len = read.read(&mut bytes).await?;
            if len == 0 {
                return Err(ProtocolReaderError::Other(
                    "received EOF while reading".into(),
                ));
            }
            self.bytes.extend_from_slice(&bytes[0..len]);
        }
    }
}

pub struct QBP {
    version: String,
    // encoding: Encoding, TODO: zip and stuff
}

#[derive(Error, Debug)]
pub enum QBPError {
    #[error("message error:")]
    Message(#[from] Error),
}

pub type QBPResult<T> = std::result::Result<T, QBPError>;

type Len = u64;
const LEN_SIZE: usize = std::mem::size_of::<Len>();

impl<'a, T> Protocol<'a, T> for QBP
where
    T: Message<'a>,
{
    type Error = QBPError;

    fn from_bytes(bytes: &'a [u8]) -> Option<QBPResult<(T, usize)>> {
        if bytes.len() < LEN_SIZE {
            return None;
        }

        let mut len_bytes: [u8; LEN_SIZE] = [0; LEN_SIZE];
        len_bytes.copy_from_slice(&bytes[0..LEN_SIZE]);
        let len = Len::from_be_bytes(len_bytes) as usize;
        let packet_len = LEN_SIZE + len;

        if bytes.len() < packet_len {
            return None;
        }

        Some(
            T::from_bitcode(&bytes[LEN_SIZE..packet_len])
                .map_err(|e| e.into())
                .map(|e| (e, packet_len)),
        )
    }

    fn to_bytes(msg: T) -> QBPResult<Vec<u8>> {
        let contents = msg.to_bitcode();
        let len_bytes = (contents.len() as Len).to_be_bytes();

        let mut res = len_bytes.to_vec();
        res.extend_from_slice(&contents);
        Ok(res)
    }
}

//  async fn read_async<R>(&mut self, read: &mut R) -> Result<T>
//  where
//      R: tokio::io::AsyncReadExt + Unpin,
//  {
//      loop {
//          let mut bytes: [u8; 1024] = [0; 1024];
//          let len = read.read(&mut bytes).await?;
//          if len == 0 {
//              return Err(Error::Other("received EOF while reading".into()));
//          }
//          self.bytes.extend_from_slice(&bytes[0..len]);
//      }
//  }
//
//  async fn write_async<W>(write: &mut W) -> Result<()>
//  where
//      W: tokio::io::AsyncWriteExt + Unpin,
//  {
//      todo!()
//  }
//
//  fn read(read: &mut impl std::io::Read) -> Result<T> {}
//
//  fn write(write: &mut impl std::io::Write) -> Result<()> {}
