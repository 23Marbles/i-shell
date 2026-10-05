use std::net::SocketAddr;

pub enum DaemonRequest {
    RequestConnection { target: ConnectionTarget },
}

pub enum ConnectionTarget {
    Ip(SocketAddr),

    Name(String),
}
