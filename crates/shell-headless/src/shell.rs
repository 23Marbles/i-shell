use std::{net::SocketAddr, process::ExitCode};

use shell_connection::control::{
    CommandId, ControlConnection, ControlKind, ControlMessage, Negotiating, NegotiationError,
    Remote, RemoteCommandError, RemoteMessage, StreamKind,
};
use shell_connection::tls::TlsSettings;
use shell_core::{
    command::{Command, CommandError, CommandResult, context::CommandContext},
    shell::{Shell, ShellHandle},
};
use tokio::net::{TcpSocket, TcpStream};
use tokio_rustls::TlsStream;

async fn connect_tls_stream(
    tls: &TlsSettings,
    address: SocketAddr,
    kind: StreamKind,
) -> Result<(TlsStream<TcpStream>, SocketAddr), Error> {
    let socket = match if address.is_ipv4() {
        TcpSocket::new_v4()
    } else {
        TcpSocket::new_v6()
    } {
        Ok(socket) => socket,
        Err(err) => return Err(Error::CreateStreamErr { kind, err }),
    };

    let stream = match socket.connect(address).await {
        Ok(stream) => stream,
        Err(err) => return Err(Error::CreateStreamErr { kind, err }),
    };
    let local_addr = stream.local_addr()?;
    let stream = match tls.connect(stream, address.ip()).await {
        Ok(stream) => stream,
        Err(err) => return Err(Error::CreateStreamErr { kind, err }),
    };

    Ok((stream, local_addr))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IoErr(#[from] std::io::Error),
    #[error(transparent)]
    CommandError(#[from] CommandError),

    #[error("")]
    CreateStreamErr {
        kind: StreamKind,
        err: std::io::Error,
    },
}

impl From<Error> for RemoteCommandError {
    fn from(value: Error) -> Self {
        match value {
            Error::IoErr(error) => Self::IoErr(error.to_string()),
            Error::CommandError(command_error) => Self::CommandError(command_error.into()),
            Error::CreateStreamErr { kind, err } => Self::CreateStreamErr {
                kind,
                err: err.to_string(),
            },
        }
    }
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
    tls: TlsSettings,
}

impl HeadlessShell<Waiting> {
    pub async fn new(stream: TlsStream<TcpStream>, tls: TlsSettings) -> Self {
        Self {
            stream: ControlConnection::from(stream),
            shell_handle: Shell::default().into_handle(),
            tls,
        }
    }

    pub async fn wait(mut self) -> Result<HeadlessShell<Connected>, NegotiationError> {
        self.stream.attempt_conn().await.map(|_| HeadlessShell {
            stream: ControlConnection::from(self.stream.stream),
            shell_handle: self.shell_handle,
            tls: self.tls,
        })
    }
}

impl HeadlessShell<Connected> {
    pub async fn run(mut self) -> Result<ExitCode, Error> {
        let (exit_code, cmd_id) = loop {
            let cmd = self.stream.recv_cmd().await?;

            match cmd {
                ControlMessage::RunCommand {
                    cmd,
                    id,
                    stdin,
                    stdout,
                    stderr,
                } => match self.run_cmd(cmd, stdin, stdout, stderr, id).await {
                    Ok(CommandResult::Continue) => {
                        self.stream
                            .send_msg(RemoteMessage::CommandFinished { cmd_id: id })
                            .await?;
                    }
                    Ok(CommandResult::Exit(c)) => break (c, id),
                    Err(e) => {
                        self.stream
                            .send_msg(RemoteMessage::CommandError {
                                err: e.into(),
                                cmd_id: id,
                            })
                            .await?;
                    }
                },
            }
        };

        self.stream
            .send_msg(RemoteMessage::Exit {
                code: exit_code as u32,
                cmd_id,
            })
            .await?;

        Ok(ExitCode::from(exit_code))
    }

    pub async fn run_cmd(
        &mut self,
        cmd: Command,
        stdin: SocketAddr,
        stdout: SocketAddr,
        stderr: SocketAddr,
        cmd_id: CommandId,
    ) -> Result<CommandResult, Error> {
        let (stdin, stdin_local_addr) =
            connect_tls_stream(&self.tls, stdin, StreamKind::Stdin).await?;
        let (stdout, stdout_local_addr) =
            connect_tls_stream(&self.tls, stdout, StreamKind::Stdout).await?;
        let (stderr, stderr_local_addr) =
            connect_tls_stream(&self.tls, stderr, StreamKind::Stderr).await?;

        self.stream
            .send_msg(RemoteMessage::SetupStreams {
                stdin: stdin_local_addr,
                stdout: stdout_local_addr,
                stderr: stderr_local_addr,
                cmd_id,
            })
            .await?;

        let cmd = cmd.resolve(&self.shell_handle).await?;

        let ctx = CommandContext::new(&self.shell_handle)
            .with_stdin(stdin.into())
            .with_stdout(stdout.into())
            .with_stderr(stderr.into());

        Ok(cmd.run(ctx).await?)
    }
}
