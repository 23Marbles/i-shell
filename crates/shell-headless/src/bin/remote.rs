use std::{error::Error, io::stdin, net::SocketAddr, process::ExitCode};

use shell_headless::shell::HeadlessShell;
use tokio::net::TcpSocket;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let tcp_socket = TcpSocket::new_v4()?;

    tcp_socket.bind("127.0.0.1:0".parse()?)?;

    println!("{}", tcp_socket.local_addr()?);

    let socket_addr: SocketAddr = {
        let mut buf = String::new();

        stdin().read_line(&mut buf)?;

        buf.trim().parse()
    }?;

    let tcp_stream = tcp_socket.connect(socket_addr).await?;

    let shell = HeadlessShell::new(tcp_stream).await;

    let shell = shell.wait().await?;

    let code = shell.run().await?;

    Ok(code)
}
