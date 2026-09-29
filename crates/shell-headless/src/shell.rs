use std::{
    net::SocketAddr,
    process::{ExitCode, ExitStatus},
};

use async_result_ext::AsyncResultExt;
use shell_connection::control::{
    CommandId, ControlConnection, ControlKind, ControlMessage, Negotiating, NegotiationError,
    Remote, RemoteMessage,
};
use shell_core::{
    command::{Command, CommandError, CommandResult, context::CommandContext},
    shell::{Shell, ShellHandle},
};
use tokio::net::{TcpSocket, TcpStream};

#[derive(Debug)]
pub enum StreamKind {
    Stdin,
    Stdout,
    Stderr,
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
    pub async fn run(mut self) -> Result<ExitStatus, Error> {
        let (exit_stat, cmd_id) = loop {
            let cmd = self.stream.recv_cmd().await?;

            match cmd {
                ControlMessage::RunCommand {
                    cmd,
                    id,
                    stdin,
                    stdout,
                    stderr,
                } => match &self.run_cmd(cmd, stdin, stdout, stderr).await {
                    Ok(CommandResult::Continue) => (),
                    Ok(CommandResult::Exit(c)) => break (*c, id),
                    Err(e) => {}
                },
            }
        };

        self.stream
            .send_msg(RemoteMessage::Exit {
                code: exit_stat.code().unwrap_or(-1),
                cmd_id,
            })
            .await?;

        Ok(exit_stat)
    }

    pub async fn run_cmd(
        &self,
        cmd: Command,
        stdin: SocketAddr,
        stdout: SocketAddr,
        stderr: SocketAddr,
    ) -> Result<CommandResult, Error> {
        let stdin = match if stdin.is_ipv4() {
            TcpSocket::new_v4()
        } else {
            TcpSocket::new_v6()
        }
        .async_and_then(async |sock| sock.connect(stdin).await)
        .await
        {
            Ok(stream) => stream,
            Err(err) => {
                return Err(Error::CreateStreamErr {
                    kind: StreamKind::Stdin,
                    err,
                });
            }
        };

        let stdout = match if stdout.is_ipv4() {
            TcpSocket::new_v4()
        } else {
            TcpSocket::new_v6()
        }
        .async_and_then(async |sock| sock.connect(stdout).await)
        .await
        {
            Ok(stream) => stream,
            Err(err) => {
                return Err(Error::CreateStreamErr {
                    kind: StreamKind::Stdout,
                    err,
                });
            }
        };

        let stderr = match if stderr.is_ipv4() {
            TcpSocket::new_v4()
        } else {
            TcpSocket::new_v6()
        }
        .async_and_then(async |sock| sock.connect(stderr).await)
        .await
        {
            Ok(stream) => stream,
            Err(err) => {
                return Err(Error::CreateStreamErr {
                    kind: StreamKind::Stderr,
                    err,
                });
            }
        };

        let cmd = Command::from(cmd).resolve(&self.shell_handle).await?;

        let ctx = CommandContext::new(&self.shell_handle)
            .with_stdin(stdin.into())
            .with_stdout(stdout.into())
            .with_stderr(stderr.into());

        Ok(cmd.run(ctx).await?)
    }
}
