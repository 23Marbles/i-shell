use std::{
    error::Error,
    io::{Write, stdin, stdout},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use shell_core::{command::Command, parse::Parser, streams::StreamRx};
use shell_headless::controller::RemoteShellController;
use tokio::net::TcpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    print!("IP address reachable by the remote (LAN/VPN): ");
    stdout().flush()?;

    let advertise_ip: IpAddr = {
        let mut buf = String::new();
        stdin().read_line(&mut buf)?;
        buf.trim().parse()?
    };

    let tcp_socket = if advertise_ip.is_ipv4() {
        TcpSocket::new_v4()?
    } else {
        TcpSocket::new_v6()?
    };
    let bind_addr = match advertise_ip {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    };
    tcp_socket.bind(bind_addr)?;
    let port = tcp_socket.local_addr()?.port();
    let tcp_listener = tcp_socket.listen(128)?;

    println!("Controller listening; enter {advertise_ip}:{port} in remote.");
    print!("Remote peer address (shown by remote after it connects): ");
    stdout().flush()?;

    let remote_address: SocketAddr = {
        let mut buf = String::new();

        stdin().read_line(&mut buf)?;

        buf.trim().parse()
    }?;

    let tcp_stream = loop {
        let (stream, sock) = tcp_listener.accept().await?;

        println!("Connection request from {sock}");

        if sock == remote_address {
            break stream;
        }
    };

    println!("Accepted connection");

    let controller = RemoteShellController::new(tcp_stream, advertise_ip).await;

    let mut controller = controller.wait().await?;

    let mut buf = String::new();

    loop {
        let line = {
            buf.clear();
            print!("{}>> ", remote_address.ip());
            stdout().flush()?;
            stdin().read_line(&mut buf)?;
            buf.trim()
        };

        let mut parser = Parser::new(&line);

        let pipeline = match parser.parse() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };

        let cmd = pipeline.first_command;

        let cmd = Command::from_empheral(cmd);

        match controller
            .run_cmd(cmd, Some(StreamRx::from(tokio::io::empty())), None, None)
            .await?
        {
            Ok(()) => (),
            Err(e) => {
                eprintln!("{e}");
            }
        }
    }
}
