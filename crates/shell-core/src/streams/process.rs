use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    codec::Header,
    streams::{StreamReceiveError, StreamReceiver, StreamSendError, StreamSender},
};

pub struct Stdin {
    inner: tokio::io::Stdin,
}

impl Stdin {
    pub fn new() -> Self {
        Self {
            inner: tokio::io::stdin(),
        }
    }
}

impl Default for Stdin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl StreamReceiver for Stdin {
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, StreamReceiveError> {
        Ok(self.inner.read(buf).await?)
    }
}
pub struct Stderr {
    inner: tokio::io::Stderr,
}

impl Stderr {
    pub fn new() -> Self {
        Self {
            inner: tokio::io::stderr(),
        }
    }
}

impl Default for Stderr {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl StreamSender for Stderr {
    async fn send(
        &mut self,
        _: Option<Header>,
        value: bytes::Bytes,
    ) -> Result<(), StreamSendError> {
        self.inner.write_all(&value).await?;
        self.inner.flush().await?;

        Ok(())
    }
}

pub struct Stdout {
    inner: tokio::io::Stdout,
}

impl Stdout {
    pub fn new() -> Self {
        Self {
            inner: tokio::io::stdout(),
        }
    }
}

impl Default for Stdout {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl StreamSender for Stdout {
    async fn send(
        &mut self,
        _: Option<Header>,
        value: bytes::Bytes,
    ) -> Result<(), StreamSendError> {
        self.inner.write_all(&value).await?;
        self.inner.flush().await?;

        Ok(())
    }
}
