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

pub trait AdapterKind {}
/// Removes type information when sending (the header)
pub struct Blank;
impl AdapterKind for Blank {}

/// Filters the types coming through to only allow `T` through
pub struct TypeFilter<T> {
    _phantom: PhantomData<T>,
}
impl<T> AdapterKind for TypeFilter<T> {}

pub struct Relay;
impl AdapterKind for Relay {}

pub struct StreamAdapter<A: AdapterKind> {
    cancel_token: CancellationToken,
    task_handle: JoinHandle<Result<(), StreamError>>,
    _kind: PhantomData<A>,
}

impl<A: AdapterKind> StreamAdapter<A> {
    pub fn into_join_handle(self) -> JoinHandle<Result<(), StreamError>> {
        self.task_handle
    }
}

impl StreamAdapter<Blank> {
    /// Will funnel data straight from the reciever into the sender removing type and length data
    pub fn spawn(reciever: StreamRx, sender: StreamTx) -> Self {
        let cancel = CancellationToken::new();
        let cancel_cloned = cancel.clone();

        let handle = tokio::task::spawn(async move {
            let cancel = cancel_cloned;

            let mut reciever = reciever;
            let mut sender = sender;

            loop {
                let (_, bytes) = select! {
                    _ = cancel.cancelled() => {
                        break;
                    }

                    res = reciever.recv_raw() => {
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

                    res = sender.send_raw(bytes) => {
                        match res {
                            Ok(()) => (),
                            Err(e) => return Err(e.into()),
                        }
                    }
                };
            }

            Ok(())
        });

        Self {
            task_handle: handle,
            _kind: PhantomData,
            cancel_token: cancel,
        }
    }
}

impl<T: AdapterKind> StreamAdapter<T> {
    pub async fn result(&mut self) -> Result<(), AdapterError> {
        (&mut self.task_handle).await??;

        Ok(())
    }

    pub async fn close(mut self) -> Result<(), AdapterError> {
        self.cancel_token.cancel();

        self.result().await
    }
}

impl<T: WireDecode + WireEncode + Send> StreamAdapter<TypeFilter<T>> {
    pub fn spawn(reciever: StreamRx, sender: StreamTx) -> Self {
        let cancel = CancellationToken::new();
        let cancel_cloned = cancel.clone();

        let handle = tokio::task::spawn(async move {
            let cancel = cancel_cloned;

            let mut reciever = reciever;
            let mut sender = sender;

            loop {
                let t = select! {
                    _ = cancel.cancelled() => {
                        break;
                    }

                    res = reciever.recv::<T>() => {
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

                    res = sender.send(t) => {
                        match res {
                            Ok(()) => (),
                            Err(e) => return Err(e.into()),
                        }
                    }
                };
            }

            Ok(())
        });

        Self {
            cancel_token: cancel,
            task_handle: handle,
            _kind: PhantomData,
        }
    }
}

impl StreamAdapter<Relay> {
    pub fn spawn(reciever: StreamRx, sender: StreamTx) -> Self {
        let cancel = CancellationToken::new();
        let cancel_cloned = cancel.clone();

        let handle = tokio::task::spawn(async move {
            let cancel = cancel_cloned;

            let mut reciever = reciever;
            let mut sender = sender;

            loop {
                let (header, bytes) = select! {
                    _ = cancel.cancelled() => {
                        break;
                    }

                    res = reciever.recv_raw() => {
                        res
                    }
                }?;

                select! {
                    _ = cancel.cancelled() => {
                        break;
                    }

                    res = sender.send_raw(header.encode_bytes()) => {
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

                    res = sender.send_raw(bytes) => {
                        match res {
                            Ok(()) => (),
                            Err(e) => return Err(e.into()),
                        }
                    }
                };
            }

            Ok(())
        });

        Self {
            cancel_token: cancel,
            task_handle: handle,
            _kind: PhantomData,
        }
    }
}
