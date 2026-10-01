use std::{error::Error, io::stdin, net::SocketAddr, process::ExitCode};

use shell_connection::tls::TlsSettings;
use tokio::net::TcpSocket;

use crate::shell::HeadlessShell;

pub mod shell;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let tls = TlsSettings::from_env()?;

    let socket_addr: SocketAddr = {
        let mut buf = String::new();

        stdin().read_line(&mut buf)?;

        buf.trim().parse()
    }?;

    let tcp_socket = TcpSocket::new_v4()?;
    let tcp_stream = tcp_socket.connect(socket_addr).await?;
    let tcp_stream = tls.connect(tcp_stream, socket_addr.ip()).await?;

    let shell = HeadlessShell::new(tcp_stream, tls).await;

    let shell = shell.wait().await?;

    let code = shell.run().await?;

    Ok(code)
}
