use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::codec::{self, Header, WireDecode, WireEncode, decode, encode};

#[derive(Debug, thiserror::Error)]
pub enum StreamSendError {
    #[error(transparent)]
    IoErr(#[from] std::io::Error),
}

#[async_trait::async_trait]
pub trait StreamSender: Send {
    async fn send(&mut self, value: bytes::Bytes) -> Result<(), StreamSendError>;
}

impl<T: StreamSender + Send + Sync + 'static> From<T> for StreamTx {
    fn from(value: T) -> Self {
        Self {
            inner: Box::new(value),
        }
    }
}

#[async_trait::async_trait]
impl<W: AsyncWriteExt + Send + Unpin> StreamSender for W {
    async fn send(&mut self, data: bytes::Bytes) -> Result<(), StreamSendError> {
        self.write_all(&data).await?;

        Ok(())
    }
}

pub struct StreamTx {
    inner: Box<dyn StreamSender + Send + Sync>,
}

impl StreamTx {
    pub async fn send<T: WireEncode>(&mut self, value: &T) -> Result<(), StreamSendError> {
        self.inner.send(encode(value)).await
    }
}

pub struct StreamRx {
    inner: Box<dyn StreamReceiver + Send + Sync>,
}

#[async_trait::async_trait]
impl<R: AsyncReadExt + Send + Unpin> StreamReceiver for R {
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, StreamReceiveError> {
        let len = self.read(buf).await?;

        Ok(len)
    }
}

impl<R: StreamReceiver + Send + Sync + 'static> From<R> for StreamRx {
    fn from(value: R) -> Self {
        Self {
            inner: Box::new(value),
        }
    }
}

#[async_trait::async_trait]
pub trait StreamReceiver: Send {
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, StreamReceiveError>;

    async fn recv_exact(&mut self, buf: &mut [u8]) -> Result<(), StreamReceiveError> {
        let mut left = buf;

        while !left.is_empty() {
            let taken = self.recv(left).await?;

            (_, left) = left.split_at_mut(taken);
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StreamReceiveError {
    #[error("the sender has been closed")]
    SenderClosed,

    #[error(transparent)]
    IoErr(#[from] std::io::Error),

    #[error(transparent)]
    Decode(#[from] codec::DecodeErr),
}

impl StreamRx {
    /// Will only return None if the Sender has been dropped
    pub async fn recv<T: WireDecode>(&mut self) -> Result<T, StreamReceiveError> {
        let mut header_buf = [0u8; 8];
        self.inner.recv_exact(&mut header_buf).await?;
        let header = Header::decode(header_buf)?;

        let mut buf = vec![0u8; header.length];

        self.inner.recv_exact(&mut buf).await?;

        let val = decode(header, &mut buf.as_slice())?;

        Ok(val)
    }
}
