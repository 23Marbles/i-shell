use std::net::SocketAddr;

pub enum DaemonRequest {
    RequestConnection { target: ConnectionTarget },
}

pub enum ConnectionTarget {
    Name(String),
    Ip(SocketAddr),
}
