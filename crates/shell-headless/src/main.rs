pub mod shell;

use std::{error::Error, io::stdin, net::SocketAddr, process::ExitCode};

use tokio::net::TcpSocket;

use crate::shell::HeadlessShell;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let socket_addr: SocketAddr = {
        let mut buf = String::new();

        stdin().read_line(&mut buf)?;

        buf.trim().parse()
    }?;

    let tcp_socket = TcpSocket::new_v4()?;
    let tcp_strean = tcp_socket.connect(socket_addr).await?;

    let shell = HeadlessShell::new(tcp_strean).await;

    let shell = shell.wait().await?;

    let code = shell.run().await?;

    Ok(code)
}
