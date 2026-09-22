use std::io::Read;

use thiserror::Error;

use crate::codec::impls::{EnumFromU32Err, TypeIds};

mod impls;

pub trait WireType {
    const TYPE_ID: impls::TypeIds;
}

#[derive(Debug, Error)]
pub enum DecodeErr {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("expected {expected} bytes, got {got}")]
    NotEnoughBytes { expected: usize, got: usize },

    #[error("expected type with id `{expected:?}`, got type id `{got:?}`")]
    UnexpectedType { expected: TypeIds, got: TypeIds },

    #[error("`{}` is not a recognized type", .0.got)]
    InvalidType(#[from] EnumFromU32Err),

    #[error("decode did not take all bytes from `body`, leaving `{0}` bytes behind")]
    TrailingBytes(usize),

    #[error(transparent)]
    IoErr(#[from] std::io::Error),
}

pub trait WireWrite {
    fn u32(&mut self, value: u32);
    fn string(&mut self, value: &str);
    fn bytes(&mut self, value: &[u8]);
}

impl WireWrite for Vec<u8> {
    fn u32(&mut self, value: u32) {
        let bytes = value.to_be_bytes();
        self.extend_from_slice(&bytes);
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes())
    }

    fn bytes(&mut self, value: &[u8]) {
        let len = value.len();

        self.reserve(len + 4);

        self.u32(len as u32);
        self.extend_from_slice(value);
    }
}

pub trait WireRead {
    fn u32(&mut self) -> Result<u32, DecodeErr>;
    fn string(&mut self) -> Result<String, DecodeErr>;
    fn bytes(&mut self) -> Result<Vec<u8>, DecodeErr>;
}

impl WireRead for &'_ [u8] {
    fn u32(&mut self) -> Result<u32, DecodeErr> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    fn string(&mut self) -> Result<String, DecodeErr> {
        let bytes = <Self as WireRead>::bytes(self)?;

        String::from_utf8(bytes).map_err(|e| DecodeErr::Parse(e.to_string()))
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
}

pub trait WireDecode: WireType + Sized {
    fn decode_body(buf: &mut impl WireRead) -> Result<Self, DecodeErr>;
}

pub trait WireEncode: WireType {
    fn encode_body(&self, buf: &mut impl WireWrite);
}

pub fn encode<T: WireEncode>(value: &T) -> Vec<u8> {
    let mut body = Vec::new();
    value.encode_body(&mut body);

    let mut frame = Vec::with_capacity(8 + body.len());

    frame.extend_from_slice(&(T::TYPE_ID as u32).to_be_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&body);

    frame
}

pub fn decode<T: WireDecode>(buf: &mut &[u8]) -> Result<T, DecodeErr> {
    let type_id = {
        let mut type_id_buf = [0u8; 4];
        buf.read_exact(&mut type_id_buf)?;
        u32::from_be_bytes(type_id_buf).try_into()?
    };
    let length = {
        let mut len_buf = [0u8; 4];
        buf.read_exact(&mut len_buf)?;
        u32::from_be_bytes(len_buf) as usize
    };

    if type_id != T::TYPE_ID {
        return Err(DecodeErr::UnexpectedType {
            expected: T::TYPE_ID,
            got: type_id,
        });
    }

    let (body, rest) = buf
        .split_at_checked(length)
        .ok_or(DecodeErr::NotEnoughBytes {
            expected: length,
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
