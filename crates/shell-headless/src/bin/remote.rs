use std::{
    error::Error,
    io,
    io::{Write, stdin, stdout},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    process::ExitCode,
    time::Duration,
};

use shell_headless::shell::HeadlessShell;
use tokio::net::{TcpSocket, TcpStream};

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    print!("Controller address: ");
    stdout().flush()?;

    let socket_addr: SocketAddr = {
        let mut buf = String::new();

        stdin().read_line(&mut buf)?;

        buf.trim().parse()
    }?;

    let tcp_socket = if socket_addr.is_ipv4() {
        TcpSocket::new_v4()?
    } else {
        TcpSocket::new_v6()?
    };
    let bind_addr = match socket_addr.ip() {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    };
    tcp_socket.bind(bind_addr)?;

    let tcp_stream = connect_when_available(tcp_socket, socket_addr).await?;
    println!(
        "Connected from {}; enter this address in controller.",
        tcp_stream.local_addr()?
    );

    let shell = HeadlessShell::new(tcp_stream).await;

    let shell = shell.wait().await?;

    let code = shell.run().await?;

    Ok(code)
}

async fn connect_when_available(
    socket: TcpSocket,
    address: SocketAddr,
) -> io::Result<TcpStream> {
    let local_address = socket.local_addr()?;
    let mut socket = Some(socket);
    let mut reported_wait = false;

    loop {
        match socket.take().unwrap().connect(address).await {
            Ok(stream) => return Ok(stream),
            Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => {
                if !reported_wait {
                    eprintln!("Controller is not listening at {address}; retrying...");
                    reported_wait = true;
                }

                tokio::time::sleep(Duration::from_millis(250)).await;

                let retry_socket = if local_address.is_ipv4() {
                    TcpSocket::new_v4()?
                } else {
                    TcpSocket::new_v6()?
                };
                retry_socket.bind(local_address)?;
                socket = Some(retry_socket);
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::connect_when_available;
    use tokio::net::{TcpListener, TcpSocket};

    #[tokio::test]
    async fn retries_until_controller_starts_listening() {
        let address = "127.0.0.1:0".parse().unwrap();
        let address_reserver = TcpSocket::new_v4().unwrap();
        address_reserver.bind(address).unwrap();
        let address = address_reserver.local_addr().unwrap();
        drop(address_reserver);

        let client_socket = TcpSocket::new_v4().unwrap();
        client_socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let client_address = client_socket.local_addr().unwrap();

        let listener_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            TcpListener::bind(address).await.unwrap()
        });

        let client_task = tokio::spawn(async move {
            connect_when_available(client_socket, address).await.unwrap()
        });

        let listener = listener_task.await.unwrap();
        let (server_stream, _) = listener.accept().await.unwrap();
        let client_stream = client_task.await.unwrap();

        assert_eq!(client_stream.local_addr().unwrap(), client_address);
        assert_eq!(server_stream.peer_addr().unwrap(), client_address);
    }
}
