use std::fmt::Debug;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::codec::{self, Header, WireDecode, WireEncode, decode, encode};

pub mod adapter;

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error(transparent)]
    Send(#[from] StreamSendError),

    #[error(transparent)]
    Receive(#[from] StreamReceiveError),
}

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
        self.flush().await?;

        Ok(())
    }
}

pub struct StreamTx {
    inner: Box<dyn StreamSender + Send + Sync>,
}

impl Debug for StreamTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamTx").finish_non_exhaustive()
    }
}

impl StreamTx {
    pub async fn send<T: WireEncode>(&mut self, value: T) -> Result<(), StreamSendError> {
        self.inner.send(encode(value)).await
    }

    pub async fn send_raw(&mut self, value: bytes::Bytes) -> Result<(), StreamSendError> {
        self.inner.send(value).await
    }
}

pub struct StreamRx {
    inner: Box<dyn StreamReceiver + Send + Sync>,
}

impl Debug for StreamRx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamRx").finish_non_exhaustive()
    }
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

            if taken == 0 {
                return Err(StreamReceiveError::SenderClosed);
            }

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
    pub async fn recv_chunk(&mut self, buf: &mut [u8]) -> Result<usize, StreamReceiveError> {
        let len = self.inner.recv(buf).await?;

        if len == 0 {
            return Err(StreamReceiveError::SenderClosed);
        }

        Ok(len)
    }

    pub async fn recv<T: WireDecode>(&mut self) -> Result<T, StreamReceiveError> {
        let mut header_buf = [0u8; 8];
        self.inner.recv_exact(&mut header_buf).await?;
        let header = Header::decode(header_buf)?;

        let mut buf = vec![0u8; header.length];

        self.inner.recv_exact(&mut buf).await?;

        let val = decode(header, &mut buf.as_slice())?;

        Ok(val)
    }

    pub async fn recv_raw(&mut self) -> Result<(Header, bytes::Bytes), StreamReceiveError> {
        let mut header_buf = [0u8; 8];
        self.inner.recv_exact(&mut header_buf).await?;
        let header = Header::decode(header_buf)?;

        let mut buf = vec![0u8; header.length];

        self.inner.recv_exact(&mut buf).await?;

        Ok((header, buf.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamReceiveError, StreamReceiver, StreamRx};
    use tokio::io::{AsyncWriteExt, duplex};

    #[tokio::test]
    async fn recv_exact_reports_eof() {
        let (writer, mut reader) = duplex(8);
        drop(writer);

        let error = reader.recv_exact(&mut [0; 8]).await.unwrap_err();

        assert!(matches!(error, StreamReceiveError::SenderClosed));
    }

    #[tokio::test]
    async fn recv_chunk_preserves_unframed_bytes_and_reports_eof() {
        let (mut writer, reader) = duplex(8);
        writer.write_all(b"hello").await.unwrap();
        drop(writer);

        let mut receiver = StreamRx::from(reader);
        let mut buf = [0; 8];
        let len = receiver.recv_chunk(&mut buf).await.unwrap();

        assert_eq!(&buf[..len], b"hello");
        assert!(matches!(
            receiver.recv_chunk(&mut buf).await,
            Err(StreamReceiveError::SenderClosed)
        ));
    }
}
