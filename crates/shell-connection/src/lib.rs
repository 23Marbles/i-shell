use std::io;

use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub mod control;
pub mod daemon;
pub mod tls;

async fn read_msg<T: DeserializeOwned>(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<T> {
    let len = stream.read_u32().await? as usize;

    if len > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Message too large",
        ));
    }

    let mut buf = vec![0; len];
    stream.read_exact(&mut buf).await?;

    rmp_serde::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

async fn write_msg<T: Serialize>(
    stream: &mut (impl AsyncWrite + Unpin),
    value: &T,
) -> io::Result<()> {
    let buf = rmp_serde::to_vec(value).map_err(io::Error::other)?;

    stream.write_u32(buf.len() as u32).await?;
    stream.write_all(&buf).await?;

    Ok(())
}
