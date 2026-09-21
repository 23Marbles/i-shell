use crate::codec::{WireDecode, WireEncode, WireType};

macro_rules! impl_list {
    ( $( $type:ty, fn decode() { $decode:expr }, fn decode() { $decode:expr } ),* ) => {};
}

#[derive(Debug)]
#[repr(u32)]
enum TypeIds {
    String = 1,
}

#[derive(Debug, thiserror::Error)]
#[error("`{got}` is not a valid enum variant")]
struct EnumFromU32Err {
    got: u32,
}

impl TryFrom<u32> for TypeIds {
    type Error = EnumFromU32Err;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::String),
            _ => Err(EnumFromU32Err { got: value }),
        }
    }
}

impl WireType for String {
    const TYPE_ID: u32 = TypeIds::String as u32;
}

impl WireDecode for String {
    fn decode_body(buf: &mut impl super::WireRead) -> Result<Self, super::DecodeErr> {
        buf.string()
    }
}

impl WireEncode for String {
    fn encode_body(&self, buf: &mut impl super::WireWrite) {
        buf.string(self);
    }
}
