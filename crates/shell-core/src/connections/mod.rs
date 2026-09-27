use std::marker::PhantomData;

use tokio::net::TcpStream;

pub trait ConnKind {}
pub struct Controller;
impl ConnKind for Controller {}
pub struct Controlled;
impl ConnKind for Controlled {}

pub struct ControlConnection<K: ConnKind> {
    connection: TcpStream,
    _phantom_kind: PhantomData<K>,
}
