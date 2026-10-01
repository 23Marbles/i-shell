use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use shell_connection::control::{
    CommandId, ControlConnection, ControlKind, ControlMessage, Controller, Negotiating,
    NegotiationError, RemoteCommandError, RemoteMessage, StreamKind,
};
use shell_connection::tls::TlsSettings;
use shell_core::{
    command::Command,
    streams::{
        StreamRx, StreamTx,
        adapter::{AdapterError, RawToWireBytes, Relay, StreamAdapter, WireBytesToRaw},
    },
};
use tokio::{
    net::{TcpListener, TcpSocket, TcpStream},
    task::JoinSet,
};
use tokio_rustls::TlsStream;

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
    advertise_ip: IpAddr,
    tls: TlsSettings,
}

impl RemoteShellController<Waiting> {
    pub async fn new(stream: TlsStream<TcpStream>, advertise_ip: IpAddr, tls: TlsSettings) -> Self {
        Self {
            stream: ControlConnection::from(stream),
            advertise_ip,
            tls,
        }
    }

    pub async fn wait(mut self) -> Result<RemoteShellController<Connected>, NegotiationError> {
        self.stream
            .attempt_conn()
            .await
            .map(|_| RemoteShellController {
                stream: ControlConnection::from(self.stream.stream),
                advertise_ip: self.advertise_ip,
                tls: self.tls,
            })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ControlCommandError {
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
    #[error("stream adapter failed: {0}")]
    StreamAdapterErr(String),
    #[error("unexpected response while setting up or finishing command")]
    UnexpectedRemoteMessage,
    #[error("child exited with error code {0}")]
    RemoteExited(u32),
}

fn create_stream_listener(
    advertise_ip: IpAddr,
    kind: StreamKind,
) -> Result<TcpListener, ControlCommandError> {
    let bind_addr = match advertise_ip {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    };

    let listener = (|| {
        let socket = if advertise_ip.is_ipv4() {
            TcpSocket::new_v4()?
        } else {
            TcpSocket::new_v6()?
        };
        socket.bind(bind_addr)?;
        socket.listen(128)
    })()
    .map_err(|err| ControlCommandError::CreateStreamErr { err, kind })?;

    Ok(listener)
}

impl RemoteShellController<Connected> {
    pub async fn run_cmd(
        &mut self,
        cmd: Command,
        stdin: Option<StreamRx>,
        stdout: Option<StreamTx>,
        stderr: Option<StreamTx>,
    ) -> Result<Result<(), ControlCommandError>, std::io::Error> {
        let stdin_listener = match create_stream_listener(self.advertise_ip, StreamKind::Stdin) {
            Ok(l) => l,
            Err(e) => return Ok(Err(e)),
        };
        let stdout_listener = match create_stream_listener(self.advertise_ip, StreamKind::Stdout) {
            Ok(l) => l,
            Err(e) => return Ok(Err(e)),
        };
        let stderr_listener = match create_stream_listener(self.advertise_ip, StreamKind::Stderr) {
            Ok(l) => l,
            Err(e) => return Ok(Err(e)),
        };

        let id = CommandId::new_random();

        self.stream
            .send_cmd(ControlMessage::RunCommand {
                cmd,
                stdin: match stdin_listener.local_addr() {
                    Ok(s) => SocketAddr::new(self.advertise_ip, s.port()),
                    Err(err) => {
                        return Ok(Err(ControlCommandError::CreateStreamErr {
                            err,
                            kind: StreamKind::Stdin,
                        }));
                    }
                },
                stdout: match stdout_listener.local_addr() {
                    Ok(s) => SocketAddr::new(self.advertise_ip, s.port()),
                    Err(err) => {
                        return Ok(Err(ControlCommandError::CreateStreamErr {
                            err,
                            kind: StreamKind::Stdout,
                        }));
                    }
                },
                stderr: match stderr_listener.local_addr() {
                    Ok(s) => SocketAddr::new(self.advertise_ip, s.port()),
                    Err(err) => {
                        return Ok(Err(ControlCommandError::CreateStreamErr {
                            err,
                            kind: StreamKind::Stderr,
                        }));
                    }
                },
                id,
            })
            .await?;

        let (remote_stdin_sock, remote_stdout_sock, remote_stderr_sock) =
            match self.stream.recv_response().await? {
                RemoteMessage::CommandError { err, cmd_id } => {
                    return Ok(Err(if cmd_id != id {
                        ControlCommandError::MismatchedCommandIds {
                            this_id: id,
                            remote_id: cmd_id,
                        }
                    } else {
                        ControlCommandError::RemoteCommandErr(err)
                    }));
                }
                RemoteMessage::Exit { code, cmd_id } => {
                    return Ok(Err(if cmd_id != id {
                        ControlCommandError::MismatchedCommandIds {
                            this_id: id,
                            remote_id: cmd_id,
                        }
                    } else {
                        ControlCommandError::RemoteExited(code)
                    }));
                }
                RemoteMessage::CommandFinished { cmd_id } => {
                    return Ok(Err(if cmd_id != id {
                        ControlCommandError::MismatchedCommandIds {
                            this_id: id,
                            remote_id: cmd_id,
                        }
                    } else {
                        ControlCommandError::UnexpectedRemoteMessage
                    }));
                }
                RemoteMessage::SetupStreams {
                    stdin,
                    stdout,
                    stderr,
                    cmd_id,
                } => {
                    if cmd_id != id {
                        return Ok(Err(ControlCommandError::MismatchedCommandIds {
                            this_id: id,
                            remote_id: cmd_id,
                        }));
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
                    let accepted = match accepted {
                        Ok(a) => a,
                        Err(err) => {
                            return Ok(Err(ControlCommandError::CreateStreamErr {
                                err,
                                kind: StreamKind::Stdin,
                            }));
                        }
                    };

                    if accepted.1 != remote_stdin_sock {
                        continue;
                    }

                    let stream = match self.tls.accept(accepted.0).await {
                        Ok(stream) => stream,
                        Err(err) => {
                            return Ok(Err(ControlCommandError::CreateStreamErr {
                                err,
                                kind: StreamKind::Stdin,
                            }));
                        }
                    };

                    stdin_sender = Some(stream);
                    stdin_listener = None;
                }
                accepted = async { stdout_listener.as_ref().unwrap().accept().await }, if stdout_receiver.is_none() => {
                    let accepted = match accepted {
                        Ok(a) => a,
                        Err(err) => {
                            return Ok(Err(ControlCommandError::CreateStreamErr {
                                err,
                                kind: StreamKind::Stdout,
                            }));
                        }
                    };

                    if accepted.1 != remote_stdout_sock {
                        continue;
                    }

                    let stream = match self.tls.accept(accepted.0).await {
                        Ok(stream) => stream,
                        Err(err) => {
                            return Ok(Err(ControlCommandError::CreateStreamErr {
                                err,
                                kind: StreamKind::Stdout,
                            }));
                        }
                    };

                    stdout_receiver = Some(stream);
                    stdout_listener = None;
                }
                accepted = async { stderr_listener.as_ref().unwrap().accept().await }, if stderr_receiver.is_none() => {
                    let accepted = match accepted {
                        Ok(a) => a,
                        Err(err) => {
                            return Ok(Err(ControlCommandError::CreateStreamErr {
                                err,
                                kind: StreamKind::Stderr,
                            }));
                        }
                    };

                    if accepted.1 != remote_stderr_sock {
                        continue;
                    }

                    let stream = match self.tls.accept(accepted.0).await {
                        Ok(stream) => stream,
                        Err(err) => {
                            return Ok(Err(ControlCommandError::CreateStreamErr {
                                err,
                                kind: StreamKind::Stderr,
                            }));
                        }
                    };

                    stderr_receiver = Some(stream);
                    stderr_listener = None;
                }
            }
        }

        let stdin_sender = stdin_sender.unwrap();
        let stdout_receiver = stdout_receiver.unwrap();
        let stderr_receiver = stderr_receiver.unwrap();

        let mut set: JoinSet<Result<(), AdapterError>> = JoinSet::new();

        let stdin_cancel = if let Some(stdin) = stdin {
            let mut adapter = StreamAdapter::<Relay>::spawn(stdin, stdin_sender.into());
            let cancel = adapter.cancellation_token();
            set.spawn(async move { adapter.result().await });
            cancel
        } else {
            let mut adapter = StreamAdapter::<RawToWireBytes>::spawn(
                StreamRx::from(tokio::io::stdin()),
                stdin_sender.into(),
            );
            let cancel = adapter.cancellation_token();
            set.spawn(async move { adapter.result().await });
            cancel
        };

        if let Some(stdout) = stdout {
            let mut adapter = StreamAdapter::<Relay>::spawn(stdout_receiver.into(), stdout);
            set.spawn(async move { adapter.result().await });
        } else {
            let mut adapter = StreamAdapter::<WireBytesToRaw>::spawn(
                stdout_receiver.into(),
                StreamTx::from(tokio::io::stdout()),
            );
            set.spawn(async move { adapter.result().await });
        }

        if let Some(stderr) = stderr {
            let mut adapter = StreamAdapter::<Relay>::spawn(stderr_receiver.into(), stderr);
            set.spawn(async move { adapter.result().await });
        } else {
            let mut adapter = StreamAdapter::<WireBytesToRaw>::spawn(
                stderr_receiver.into(),
                StreamTx::from(tokio::io::stderr()),
            );
            set.spawn(async move { adapter.result().await });
        }

        let response = self.stream.recv_response().await;
        stdin_cancel.cancel();

        let mut adapter_error = None;
        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok(())) => (),
                Ok(Err(err)) => {
                    adapter_error.get_or_insert_with(|| err.to_string());
                }
                Err(err) => {
                    adapter_error.get_or_insert_with(|| err.to_string());
                }
            };
        }

        let response = response?;

        match response {
            RemoteMessage::CommandFinished { cmd_id } => {
                if cmd_id != id {
                    return Ok(Err(ControlCommandError::MismatchedCommandIds {
                        this_id: id,
                        remote_id: cmd_id,
                    }));
                }

                if let Some(err) = adapter_error {
                    Ok(Err(ControlCommandError::StreamAdapterErr(err)))
                } else {
                    Ok(Ok(()))
                }
            }
            RemoteMessage::CommandError { err, cmd_id } => Ok(Err(if cmd_id != id {
                ControlCommandError::MismatchedCommandIds {
                    this_id: id,
                    remote_id: cmd_id,
                }
            } else {
                ControlCommandError::RemoteCommandErr(err)
            })),
            RemoteMessage::Exit { code, cmd_id } => Ok(Err(if cmd_id != id {
                ControlCommandError::MismatchedCommandIds {
                    this_id: id,
                    remote_id: cmd_id,
                }
            } else {
                ControlCommandError::RemoteExited(code)
            })),
            RemoteMessage::SetupStreams { .. } => {
                Ok(Err(ControlCommandError::UnexpectedRemoteMessage))
            }
        }
    }
}
