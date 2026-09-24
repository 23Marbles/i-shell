use std::marker::PhantomData;

use tokio::task::{JoinError, JoinHandle};

use crate::{
    codec::{WireDecode, WireEncode, encode},
    streams::{StreamError, StreamReceiveError, StreamRx, StreamTx},
};

pub trait AdapterKind {}
/// Removes type information when sending (the header)
pub struct Blank;
impl AdapterKind for Blank {}

/// Filters the types coming through to only allow `T` through
pub struct TypeFilter<T> {
    _phantom: PhantomData<T>,
}
impl<T> AdapterKind for TypeFilter<T> {}

pub struct StreamAdapter<A: AdapterKind> {
    task_handle: JoinHandle<Result<(), StreamError>>,
    _kind: PhantomData<A>,
}

impl StreamAdapter<Blank> {
    /// Will funnel data straight from the reciever into the sender removing type and length data
    pub fn spawn(reciever: StreamRx, sender: StreamTx) -> Self {
        let handle = tokio::task::spawn(async move {
            let mut reciever = reciever;
            let mut sender = sender;

            loop {
                let bytes = match reciever.recv_raw().await {
                    Ok(b) => b,
                    Err(StreamReceiveError::SenderClosed) => break,
                    Err(e) => return Err(e.into()),
                };

                sender.send_raw(bytes).await?;
            }

            Ok(())
        });

        Self {
            task_handle: handle,
            _kind: PhantomData,
        }
    }
}

impl<T: AdapterKind> StreamAdapter<T> {
    pub async fn result(self) -> Result<Result<(), StreamError>, JoinError> {
        self.task_handle.await
    }
}

impl<T: WireDecode + WireEncode + Send> StreamAdapter<TypeFilter<T>> {
    /// Will funnel data straight from the reciever into the sender removing type and length data
    pub fn spawn(reciever: StreamRx, sender: StreamTx) -> Self {
        let handle = tokio::task::spawn(async move {
            let mut reciever = reciever;
            let mut sender = sender;

            loop {
                let t = match reciever.recv::<T>().await {
                    Ok(t) => t,
                    Err(StreamReceiveError::SenderClosed) => break,
                    Err(e) => return Err(e.into()),
                };

                sender.send_raw(encode(&t)).await?;
            }

            Ok(())
        });

        Self {
            task_handle: handle,
            _kind: PhantomData,
        }
    }
}
