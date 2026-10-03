use std::fmt::Debug;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::codec::{self, Header, WireDecode, WireEncode, decode, encode};

pub mod adapter;
pub mod process;

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
    async fn send(
        &mut self,
        header: Option<Header>,
        value: bytes::Bytes,
    ) -> Result<(), StreamSendError>;
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
    async fn send(
        &mut self,
        header: Option<Header>,
        data: bytes::Bytes,
    ) -> Result<(), StreamSendError> {
        if let Some(header) = header {
            self.write_all(&header.encode_bytes()).await?;
        }
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
        let (header, b) = encode(value);

        self.inner.send(Some(header), b).await
    }

    pub async fn send_raw(&mut self, value: bytes::Bytes) -> Result<(), StreamSendError> {
        self.inner.send(None, value).await
    }

    pub async fn send_reader<R: tokio::io::AsyncRead + Unpin>(
        &mut self,
        mut reader: R,
        cancel: CancellationToken,
    ) -> Result<(), StreamError> {
        let mut buf = [0; 8192];

        loop {
            let len = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = reader.read(&mut buf) => {
                    result.map_err(StreamReceiveError::from)?
                },
            };

            if len == 0 {
                return Ok(());
            }

            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = self.send(buf[..len].to_vec()) => result?,
            }
        }
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
    pub async fn copy_to<W: tokio::io::AsyncWrite + Unpin>(
        &mut self,
        mut writer: W,
        cancel: CancellationToken,
    ) -> Result<(), StreamError> {
        loop {
            let bytes = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = self.recv::<Vec<u8>>() => match result {
                    Ok(bytes) => bytes,
                    Err(StreamReceiveError::SenderClosed) => return Ok(()),
                    Err(error) => return Err(error.into()),
                },
            };

            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = async {
                    writer.write_all(&bytes).await?;
                    writer.flush().await
                } => {
                    result.map_err(StreamSendError::from)?
                },
            }
        }
    }

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
    use super::{StreamReceiveError, StreamReceiver, StreamRx, StreamTx};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};
    use tokio_util::sync::CancellationToken;

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

    #[tokio::test]
    async fn send_reader_frames_raw_bytes() {
        let (mut raw_writer, raw_reader) = duplex(64);
        let (wire_writer, wire_reader) = duplex(64);
        raw_writer.write_all(b"hello").await.unwrap();
        drop(raw_writer);

        let mut sender = StreamTx::from(wire_writer);
        sender
            .send_reader(raw_reader, CancellationToken::new())
            .await
            .unwrap();
        drop(sender);

        let mut receiver = StreamRx::from(wire_reader);
        assert_eq!(receiver.recv::<Vec<u8>>().await.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn copy_to_writes_framed_bytes_to_raw_writer() {
        let (wire_writer, wire_reader) = duplex(64);
        let (mut raw_reader, raw_writer) = duplex(64);

        let mut sender = StreamTx::from(wire_writer);
        sender.send(b"hello".to_vec()).await.unwrap();
        drop(sender);

        let mut receiver = StreamRx::from(wire_reader);
        receiver
            .copy_to(raw_writer, CancellationToken::new())
            .await
            .unwrap();

        let mut bytes = Vec::new();
        raw_reader.read_to_end(&mut bytes).await.unwrap();
        assert_eq!(bytes, b"hello");
    }
}
