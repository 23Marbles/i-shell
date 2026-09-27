use crate::codec::{WireDecode, WireEncode, WireType};

#[derive(Debug, thiserror::Error)]
#[error("`{got}` is not a valid enum variant")]
pub struct EnumFromU32Err {
    pub got: u32,
}

macro_rules! impl_list {
    (
        $(
            $name:ident => $id:literal = $type:ty
            $(,
                fn decode($buf:ident) { $decode:expr }
            )?
            $(,
                fn encode($this:ident, $buf2:ident) { $encode:expr }
            )?
        );* $(;)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u32)]
        pub enum TypeIds {
            $(
                $name = $id,
            )*
        }

        impl TryFrom<u32> for TypeIds {
            type Error = EnumFromU32Err;

            fn try_from(value: u32) -> Result<Self, Self::Error> {
                match value {
                    $(
                        $id => Ok(Self::$name),
                    )*
                    _ => Err(EnumFromU32Err { got: value }),
                }
            }
        }

        $(
            impl WireType for $type {
                const TYPE_ID: TypeIds = TypeIds::$name;
            }

            $(
                impl WireDecode for $type {
                    fn decode_body(
                        $buf: &mut impl super::WireRead
                    ) -> Result<Self, super::DecodeErr> {
                        $decode
                    }
                }
            )?

            $(
                impl WireEncode for $type {
                    fn encode_body(
                        &$this,
                        $buf2: &mut impl super::WireWrite
                    ) {
                        $encode
                    }
                }
            )?
        )*
    };
}

impl TypeIds {
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl_list! {
    String => 1 = String,
        fn decode(buf) {
            buf.string()
        },
        fn encode(self, buf) {
            buf.string(self)
        };

    Str => 2 = &str,
        fn encode(self, buf) {
            buf.string(self)
        };

    U32 => 3 = u32,
        fn decode(buf) {
            buf.u32()
        },
        fn encode(self, buf) {
            buf.u32(*self)
        };

    Bytes => 4 = Vec<u8>,
        fn decode(buf) {
            buf.bytes()
        },
        fn encode(self, buf) {
            buf.bytes(self)
        };
}
