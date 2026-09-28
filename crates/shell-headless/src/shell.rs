use std::process::ExitCode;

use shell_connection::control::{
    ControlConnection, ControlKind, ControlMessage, Negotiating, NegotiationError, Remote,
    ResponseMessage,
};
use shell_core::{
    command::{Command, CommandResult, context::CommandContext},
    shell::{Shell, ShellHandle},
};
use tokio::net::TcpStream;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IoErr(#[from] std::io::Error),
}

pub trait HeadlessState {
    type ConnKind: ControlKind;
}
pub struct Connected;
impl HeadlessState for Connected {
    type ConnKind = Remote;
}

pub struct Waiting;
impl HeadlessState for Waiting {
    type ConnKind = Negotiating;
}

pub struct HeadlessShell<S: HeadlessState> {
    stream: ControlConnection<S::ConnKind>,
    shell_handle: ShellHandle,
}

impl HeadlessShell<Waiting> {
    pub async fn new(stream: TcpStream) -> Self {
        Self {
            stream: ControlConnection::from(stream),
            shell_handle: Shell::default().into_handle(),
        }
    }

    pub async fn wait(mut self) -> Result<HeadlessShell<Connected>, NegotiationError> {
        self.stream.attempt_conn().await.map(|_| HeadlessShell {
            stream: ControlConnection::from(self.stream.stream),
            shell_handle: self.shell_handle,
        })
    }
}

impl HeadlessShell<Connected> {
    pub async fn run(self) -> Result<ExitCode, Error> {
        let Self {
            mut stream,
            shell_handle,
        } = self;

        loop {
            let cmd = stream.recv_cmd().await?;

            match cmd {
                ControlMessage::RunCommand { cmd, id } => {
                    let cmd = match Command::from(cmd).resolve(&shell_handle).await {
                        Ok(c) => c,
                        Err(e) => {
                            stream
                                .send_response(ResponseMessage::CommandError {
                                    err: e.into(),
                                    cmd_id: id,
                                })
                                .await?;

                            continue;
                        }
                    };

                    let ctx = CommandContext::new(&shell_handle);

                    match cmd.run(ctx).await {
                        Ok(CommandResult::Continue) => (),
                        Ok(CommandResult::Exit(c)) => return Ok(c),
                        Err(e) => {
                            stream
                                .send_response(ResponseMessage::CommandError {
                                    err: e.into(),
                                    cmd_id: id,
                                })
                                .await?
                        }
                    };
                }
            }
        }
    }
}
