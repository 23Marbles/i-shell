use std::marker::PhantomData;

use tokio::{
    select,
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;

use crate::{
    codec::{WireDecode, WireEncode},
    streams::{StreamError, StreamReceiveError, StreamRx, StreamTx},
};

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("stream error: {0}")]
    Stream(#[from] StreamError),

    #[error("adapter task failed: {0}")]
    Join(#[from] JoinError),
}

#[async_trait::async_trait]
pub trait AdapterKind {
    async fn task(cancel: CancellationToken, rx: StreamRx, tx: StreamTx)
    -> Result<(), StreamError>;
}

/// Removes type information when sending (the header)
pub struct Blank;
#[async_trait::async_trait]
impl AdapterKind for Blank {
    async fn task(
        cancel: CancellationToken,
        mut rx: StreamRx,
        mut tx: StreamTx,
    ) -> Result<(), StreamError> {
        loop {
            let (_, bytes) = select! {
                _ = cancel.cancelled() => {
                    break;
                }

                res = rx.recv_raw() => {
                    match res {
                        Ok(b) => b,
                        Err(StreamReceiveError::SenderClosed) => break,
                        Err(e) => return Err(e.into()),
                    }
                }
            };

            select! {
                _ = cancel.cancelled() => {
                    break;
                }

                res = tx.send_raw(bytes) => {
                    match res {
                        Ok(()) => (),
                        Err(e) => return Err(e.into()),
                    }
                }
            };
        }

        Ok(())
    }
}

pub struct RawToWireBytes;
#[async_trait::async_trait]
impl AdapterKind for RawToWireBytes {
    async fn task(
        cancel: CancellationToken,
        mut rx: StreamRx,
        mut tx: StreamTx,
    ) -> Result<(), StreamError> {
        let mut buf = vec![0; 8192];

        loop {
            let len = select! {
                _ = cancel.cancelled() => break,
                res = rx.recv_chunk(&mut buf) => match res {
                    Ok(len) => len,
                    Err(StreamReceiveError::SenderClosed) => break,
                    Err(err) => return Err(err.into()),
                }
            };

            select! {
                _ = cancel.cancelled() => break,
                res = tx.send(buf[..len].to_vec()) => match res {
                    Ok(()) => (),
                    Err(err) => return Err(err.into()),
                }
            }
        }

        Ok(())
    }
}

pub struct WireBytesToRaw;
#[async_trait::async_trait]
impl AdapterKind for WireBytesToRaw {
    async fn task(
        cancel: CancellationToken,
        mut rx: StreamRx,
        mut tx: StreamTx,
    ) -> Result<(), StreamError> {
        loop {
            let bytes = select! {
                _ = cancel.cancelled() => break,
                res = rx.recv::<Vec<u8>>() => match res {
                    Ok(bytes) => bytes,
                    Err(StreamReceiveError::SenderClosed) => break,
                    Err(err) => return Err(err.into()),
                }
            };

            select! {
                _ = cancel.cancelled() => break,
                res = tx.send_raw(bytes.into()) => match res {
                    Ok(()) => (),
                    Err(err) => return Err(err.into()),
                }
            }
        }

        Ok(())
    }
}

/// Filters the types coming through to only allow `T` through
pub struct TypeFilter<T> {
    _phantom: PhantomData<T>,
}
#[async_trait::async_trait]
impl<T: WireDecode + WireEncode + Send> AdapterKind for TypeFilter<T> {
    async fn task(
        cancel: CancellationToken,
        mut rx: StreamRx,
        mut tx: StreamTx,
    ) -> Result<(), StreamError> {
        loop {
            let t = select! {
                _ = cancel.cancelled() => {
                    break;
                }

                res = rx.recv::<T>() => {
                    match res {
                        Ok(b) => b,
                        Err(StreamReceiveError::SenderClosed) => break,
                        Err(e) => return Err(e.into()),
                    }
                }
            };

            select! {
                _ = cancel.cancelled() => {
                    break;
                }

                res = tx.send(t) => {
                    match res {
                        Ok(()) => (),
                        Err(e) => return Err(e.into()),
                    }
                }
            };
        }

        Ok(())
    }
}

pub struct Relay;
#[async_trait::async_trait]
impl AdapterKind for Relay {
    async fn task(
        cancel: CancellationToken,
        mut rx: StreamRx,
        mut tx: StreamTx,
    ) -> Result<(), StreamError> {
        loop {
            let (header, bytes) = select! {
                _ = cancel.cancelled() => {
                    break;
                }

                res = rx.recv_raw() => {
                    match res {
                        Ok(frame) => frame,
                        Err(StreamReceiveError::SenderClosed) => break,
                        Err(err) => return Err(err.into()),
                    }
                }
            };

            select! {
                _ = cancel.cancelled() => {
                    break;
                }

                res = tx.send_raw(header.encode_bytes()) => {
                    match res {
                        Ok(()) => (),
                        Err(e) => return Err(e.into()),
                    }
                }
            };

            select! {
                _ = cancel.cancelled() => {
                    break;
                }

                res = tx.send_raw(bytes) => {
                    match res {
                        Ok(()) => (),
                        Err(e) => return Err(e.into()),
                    }
                }
            };
        }

        Ok(())
    }
}

pub struct StreamAdapter<A: AdapterKind> {
    cancel_token: CancellationToken,
    task_handle: JoinHandle<Result<(), StreamError>>,
    _kind: PhantomData<A>,
}

impl<A: AdapterKind> StreamAdapter<A> {
    pub fn into_join_handle(self) -> JoinHandle<Result<(), StreamError>> {
        self.task_handle
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    pub async fn result(&mut self) -> Result<(), AdapterError> {
        (&mut self.task_handle).await??;

        Ok(())
    }

    pub async fn close(mut self) -> Result<(), AdapterError> {
        self.cancel_token.cancel();

        self.result().await
    }

    pub fn spawn(reciever: StreamRx, sender: StreamTx) -> Self {
        let cancel = CancellationToken::new();
        let cancel_cloned = cancel.clone();

        let handle = tokio::task::spawn(A::task(cancel_cloned, reciever, sender));

        Self {
            cancel_token: cancel,
            task_handle: handle,
            _kind: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RawToWireBytes, StreamAdapter, WireBytesToRaw};
    use crate::{codec::encode, streams::StreamRx};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    #[tokio::test]
    async fn raw_to_wire_bytes_frames_chunks() {
        let (mut raw_writer, raw_reader) = duplex(64);
        let (wire_writer, wire_reader) = duplex(64);
        let mut adapter =
            StreamAdapter::<RawToWireBytes>::spawn(raw_reader.into(), wire_writer.into());

        raw_writer.write_all(b"hello").await.unwrap();
        drop(raw_writer);
        adapter.result().await.unwrap();

        let mut receiver = StreamRx::from(wire_reader);
        assert_eq!(receiver.recv::<Vec<u8>>().await.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn wire_bytes_to_raw_removes_framing() {
        let (mut wire_writer, wire_reader) = duplex(64);
        let (raw_writer, mut raw_reader) = duplex(64);
        let mut adapter =
            StreamAdapter::<WireBytesToRaw>::spawn(wire_reader.into(), raw_writer.into());

        wire_writer
            .write_all(&encode(b"hello".to_vec()))
            .await
            .unwrap();
        drop(wire_writer);
        adapter.result().await.unwrap();

        let mut output = Vec::new();
        raw_reader.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, b"hello");
    }
}
