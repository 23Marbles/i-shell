mod impls;

use std::io::Read;

use bytes::BytesMut;
use thiserror::Error;

use crate::codec::impls::{EnumFromU32Err, TypeIds};

pub trait WireType {
    const TYPE_ID: impls::TypeIds;
}

pub trait WireRead {
    fn u32(&mut self) -> Result<u32, DecodeErr>;

    fn bytes(&mut self) -> Result<Vec<u8>, DecodeErr>;

    fn string(&mut self) -> Result<String, DecodeErr>;
}

impl WireRead for &'_ [u8] {
    fn u32(&mut self) -> Result<u32, DecodeErr> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, DecodeErr> {
        let length = self.u32()? as usize;

        let (bytes, left) = self
            .split_at_checked(length)
            .ok_or(DecodeErr::NotEnoughBytes {
                expected: length,
                got: self.len(),
            })?;

        *self = left;

        Ok(bytes.to_vec())
    }

    fn string(&mut self) -> Result<String, DecodeErr> {
        let bytes = <Self as WireRead>::bytes(self)?;

        String::from_utf8(bytes).map_err(|e| DecodeErr::Parse(e.to_string()))
    }
}

pub trait WireWrite {
    fn u32(&mut self, value: u32);

    fn bytes(&mut self, value: &[u8]);

    fn string(&mut self, value: &str);
}

impl WireWrite for Vec<u8> {
    fn u32(&mut self, value: u32) {
        let bytes = value.to_be_bytes();
        self.extend_from_slice(&bytes);
    }

    fn bytes(&mut self, value: &[u8]) {
        let len = value.len();

        self.reserve(len + 4);

        self.u32(len as u32);
        self.extend_from_slice(value);
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes())
    }
}

impl WireWrite for BytesMut {
    fn u32(&mut self, value: u32) {
        let bytes = value.to_be_bytes();
        self.extend_from_slice(&bytes);
    }

    fn bytes(&mut self, value: &[u8]) {
        let len = value.len();

        self.reserve(len + 4);

        self.u32(len as u32);
        self.extend_from_slice(value);
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes())
    }
}

pub trait WireDecode: WireType + Sized {
    fn decode_body(buf: &mut impl WireRead) -> Result<Self, DecodeErr>;
}

pub trait WireEncode: WireType {
    fn encode_body(&self, buf: &mut impl WireWrite);
}

#[derive(Debug, Error)]
pub enum DecodeErr {
    #[error("parse error: {0}")]
    Parse(String),

    #[error(transparent)]
    IoErr(#[from] std::io::Error),

    #[error("`{}` is not a recognized type", .0.got)]
    InvalidType(#[from] EnumFromU32Err),

    #[error("decode did not take all bytes from `body`, leaving `{0}` bytes behind")]
    TrailingBytes(usize),

    #[error("expected {expected} bytes, got {got}")]
    NotEnoughBytes { expected: usize, got: usize },

    #[error("expected type with id `{expected:?}`, got type id `{got:?}`")]
    UnexpectedType { expected: TypeIds, got: TypeIds },
}

pub struct Header {
    pub length: usize,

    type_id: TypeIds,
}

impl Header {
    pub fn new<T: WireType>(length: usize) -> Self {
        Self {
            type_id: T::TYPE_ID,
            length,
        }
    }

    pub fn decode(input: [u8; 8]) -> Result<Self, DecodeErr> {
        let mut type_id_arr = [0u8; 4];
        type_id_arr.copy_from_slice(&input[..4]);
        let type_id = TypeIds::try_from(u32::from_be_bytes(type_id_arr))?;

        let mut len_arr = [0u8; 4];
        len_arr.copy_from_slice(&input[4..]);
        let length = u32::from_be_bytes(len_arr) as usize;

        Ok(Self { type_id, length })
    }

    pub fn encode(self) -> [u8; 8] {
        let mut buf = [0u8; 8];

        buf[..4].copy_from_slice(&self.type_id.as_u32().to_be_bytes());

        buf[4..].copy_from_slice(&(self.length as u32).to_be_bytes());

        buf
    }
    pub fn encode_bytes(self) -> bytes::Bytes {
        let mut buf = Vec::new();

        buf.extend_from_slice(&self.type_id.as_u32().to_be_bytes());

        buf.extend_from_slice(&(self.length as u32).to_be_bytes());

        buf.into()
    }
}

pub fn encode<T: WireEncode>(value: T) -> (Header, bytes::Bytes) {
    let mut body = BytesMut::new();
    value.encode_body(&mut body);

    let header = Header::new::<T>(body.len());

    (header, body.freeze())
}

pub fn decode<T: WireDecode>(header: Header, buf: &mut &[u8]) -> Result<T, DecodeErr> {
    if header.type_id != T::TYPE_ID {
        return Err(DecodeErr::UnexpectedType {
            expected: T::TYPE_ID,
            got: header.type_id,
        });
    }

    let (body, rest) = buf
        .split_at_checked(header.length)
        .ok_or(DecodeErr::NotEnoughBytes {
            expected: header.length,
            got: buf.len(),
        })?;
    *buf = rest;

    let mut body = body;

    let value = T::decode_body(&mut body)?;

    if !body.is_empty() {
        return Err(DecodeErr::TrailingBytes(body.len()));
    }

    Ok(value)
}
