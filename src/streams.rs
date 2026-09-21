#[async_trait::async_trait]
pub trait StreamSender<T>: Send {
    async fn send(&self, value: T) -> Result<(), T>;
}

pub struct StreamRecvError {}

#[async_trait::async_trait]
pub trait StreamReceiver: Send {
    async fn recv(&mut self, buf: &mut [u8]) -> Result<(), StreamRecvError>;
}

pub struct StreamRx<T> {
    inner: Box<dyn StreamReceiver<T>>,
}

impl<T> StreamRx<T> {
    /// Will only return None if the Sender has been dropped
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }
}

pub struct StreamTx<T> {
    inner: Box<dyn StreamSender<T>>,
}

impl<T> StreamTx<T> {
    pub async fn send(&self, v: T) -> Result<(), T> {
        self.inner.send(v).await
    }
}
