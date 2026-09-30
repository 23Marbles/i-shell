use shell_connection::control::{
    CommandId, ControlConnection, ControlKind, ControlMessage, Controller, Negotiating,
    NegotiationError, RemoteCommandError, RemoteMessage, StreamKind,
};
use shell_core::{
    command::{Command, context::CommandContext},
    streams::{
        StreamRx, StreamTx,
        adapter::{Relay, StreamAdapter},
    },
};
use tokio::net::{TcpSocket, TcpStream};

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

#[derive(Debug, thiserror::Error)]
pub enum ComtrolCommandError {
    #[error("error creating {kind} stream: {err}")]
    CreateStreamErr {
        err: std::io::Error,
        kind: StreamKind,
    },
    #[error("incorrect command id provided")]
    MismatchedCommandIds {
        this_id: CommandId,
        remote_id: CommandId,
    },
    #[error(transparent)]
    RemoteCommandErr(RemoteCommandError),
    #[error("child exited with error code {0}")]
    RemoteExited(u32),
    #[error(transparent)]
    IoErr(#[from] std::io::Error),
}

impl RemoteShellController<Connected> {
    pub async fn run_cmd(
        &mut self,
        cmd: Command,
        ctx: CommandContext,
    ) -> Result<(), ComtrolCommandError> {
        let stdin_listener = TcpSocket::new_v4()
            .and_then(|sock| sock.listen(32))
            .map_err(|e| ComtrolCommandError::CreateStreamErr {
                kind: StreamKind::Stdin,
                err: e,
            })?;
        let stdout_listener = TcpSocket::new_v4()
            .and_then(|sock| sock.listen(32))
            .map_err(|e| ComtrolCommandError::CreateStreamErr {
                kind: StreamKind::Stdout,
                err: e,
            })?;
        let stderr_listener = TcpSocket::new_v4()
            .and_then(|sock| sock.listen(32))
            .map_err(|e| ComtrolCommandError::CreateStreamErr {
                kind: StreamKind::Stderr,
                err: e,
            })?;

        let id = CommandId::new_random();

        self.stream
            .send_cmd(ControlMessage::RunCommand {
                cmd,
                stdin: stdin_listener.local_addr()?,
                stdout: stdout_listener.local_addr()?,
                stderr: stderr_listener.local_addr()?,
                id,
            })
            .await?;

        let (remote_stdin_sock, remote_stdout_sock, remote_stderr_sock) =
            match self.stream.recv_response().await? {
                RemoteMessage::CommandError { err, cmd_id } => {
                    return Err(if cmd_id != id {
                        ComtrolCommandError::MismatchedCommandIds {
                            this_id: id,
                            remote_id: cmd_id,
                        }
                    } else {
                        ComtrolCommandError::RemoteCommandErr(err)
                    });
                }
                RemoteMessage::Exit { code, cmd_id } => {
                    return Err(if cmd_id != id {
                        ComtrolCommandError::MismatchedCommandIds {
                            this_id: id,
                            remote_id: cmd_id,
                        }
                    } else {
                        ComtrolCommandError::RemoteExited(code)
                    });
                }
                RemoteMessage::SetupStreams {
                    stdin,
                    stdout,
                    stderr,
                    cmd_id,
                } => {
                    if cmd_id != id {
                        return Err(ComtrolCommandError::MismatchedCommandIds {
                            this_id: id,
                            remote_id: cmd_id,
                        });
                    } else {
                        (stdin, stdout, stderr)
                    }
                }
            };

        let mut stdin_listener = Some(stdin_listener);
        let mut stdout_listener = Some(stdout_listener);
        let mut stderr_listener = Some(stderr_listener);

        let mut stdin_sender = None;
        let mut stdout_receiver = None;
        let mut stderr_receiver = None;

        while stderr_receiver.is_none() || stdin_sender.is_none() || stdout_receiver.is_none() {
            tokio::select! {
                accepted = async { stdin_listener.as_ref().unwrap().accept().await }, if stdin_sender.is_none() => {
                    let accepted = accepted?;

                    if accepted.1 != remote_stdin_sock {
                        continue;
                    }

                    stdin_sender = Some(accepted.0);
                    stdin_listener = None;
                }
                accepted = async { stdout_listener.as_ref().unwrap().accept().await }, if stdout_receiver.is_none() => {
                    let accepted = accepted?;

                    if accepted.1 != remote_stdout_sock {
                        continue;
                    }

                    stdout_receiver = Some(accepted.0);
                    stdout_listener = None;
                }
                accepted = async { stderr_listener.as_ref().unwrap().accept().await }, if stderr_receiver.is_none() => {
                    let accepted = accepted?;

                    if accepted.1 != remote_stderr_sock {
                        continue;
                    }

                    stderr_receiver = Some(accepted.0);
                    stderr_listener = None;
                }
            }
        }

        let stdin_sender = stdin_sender.unwrap();
        let stdout_receiver = stdout_receiver.unwrap();
        let stderr_receiver = stderr_receiver.unwrap();

        let stdin_adapter = StreamAdapter::<Relay>::spawn(
            ctx.stdin.unwrap_or(StreamRx::from(tokio::io::stdin())),
            stdin_sender.into(),
        );
        let stdout_adapter = StreamAdapter::<Relay>::spawn(
            stdout_receiver.into(),
            ctx.stdout.unwrap_or(StreamTx::from(tokio::io::stdout())),
        );
        let stderr_adapter = StreamAdapter::<Relay>::spawn(
            stderr_receiver.into(),
            ctx.stderr.unwrap_or(StreamTx::from(tokio::io::stderr())),
        );

        todo!("Monitoring and graceful shutdown");

        Ok(())
    }
}
