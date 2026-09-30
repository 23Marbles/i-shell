use std::{
    error::Error,
    io::{Write, stdin, stdout},
    net::SocketAddr,
};

use shell_core::command::Pipeline;
use shell_headless::controller::RemoteShellController;
use tokio::net::TcpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let tcp_socket = TcpSocket::new_v4()?;

    tcp_socket.bind("127.0.0.1:0".parse()?)?;

    println!("{}", tcp_socket.local_addr()?);

    let socket_addr: SocketAddr = {
        let mut buf = String::new();

        stdin().read_line(&mut buf)?;

        buf.trim().parse()
    }?;

    let tcp_listener = tcp_socket.listen(128)?;

    let tcp_stream = loop {
        let (stream, sock) = tcp_listener.accept().await?;

        println!("Connection request from {sock}");

        if sock == socket_addr {
            break stream;
        }
    };

    println!("Accepted connection");

    let controller = RemoteShellController::new(tcp_stream).await;

    let mut controller = controller.wait().await?;

    let mut buf = String::new();

    loop {
        let line = {
            buf.clear();
            print!("{}>> ", socket_addr.ip());
            stdout().flush()?;
            stdin().read_line(&mut buf)?;
            buf.trim()
        };

        let cmd = Pipeline::parse(line).unwrap().cmd;

        match controller.run_cmd(cmd, None, None, None).await? {
            Ok(()) => (),
            Err(e) => {
                eprintln!("{e}");
            }
        }
    }
}
