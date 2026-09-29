use shell_connection::control::{
    ControlConnection, ControlKind, Controller, Negotiating, NegotiationError,
};
use tokio::net::TcpStream;

pub trait ControllerState {
    type ConnKind: ControlKind;
}
pub struct Connected;
impl ControllerState for Connected {
    type ConnKind = Controller;
}

pub struct Waiting;
impl ControllerState for Waiting {
    type ConnKind = Negotiating;
}

pub struct RemoteShellController<S: ControllerState> {
    stream: ControlConnection<S::ConnKind>,
}

impl RemoteShellController<Waiting> {
    pub async fn new(stream: TcpStream) -> Self {
        Self {
            stream: ControlConnection::from(stream),
        }
    }

    pub async fn wait(mut self) -> Result<RemoteShellController<Connected>, NegotiationError> {
        self.stream
            .attempt_conn()
            .await
            .map(|_| RemoteShellController {
                stream: ControlConnection::from(self.stream.stream),
            })
    }
}

impl RemoteShellController<Connected> {}
