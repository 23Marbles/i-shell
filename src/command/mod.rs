use std::process::ExitStatus;

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

use crate::{
    codec::{WireDecode, WireEncode},
    shell::{Shell, ShellHandle},
    streams::{StreamReceiveError, StreamRx, StreamSendError, StreamTx},
};

#[derive(Debug, Clone)]
pub struct Args {
    pub values: Vec<String>,
}

pub mod builtin;
pub mod program;

pub struct CommandContext {
    shell_handle: ShellHandle,
    pub stdin: StreamRx,
    pub stdout: StreamTx,
    pub stderr: StreamTx,
}

impl CommandContext {
    #[must_use]
    pub async fn shell(&self) -> OwnedRwLockReadGuard<Shell> {
        self.shell_handle.handle.clone().read_owned().await
    }

    #[must_use]
    pub async fn shell_mut(&self) -> OwnedRwLockWriteGuard<Shell> {
        self.shell_handle.handle.clone().write_owned().await
    }

    pub async fn send_stdout<T: WireEncode>(&mut self, v: T) -> Result<(), StreamSendError> {
        self.stdout.send(&v).await
    }

    pub async fn send_stderr<T: WireEncode>(&mut self, v: T) -> Result<(), StreamSendError> {
        self.stderr.send(&v).await
    }

    pub async fn recv_stdin<T: WireDecode>(&mut self) -> Result<T, StreamReceiveError> {
        self.stdin.recv().await
    }
}

pub enum CommandResult {
    Continue,
    Exit(i32),
}

#[derive(Debug, thiserror::Error)]
#[error(
    "{cmd_name}: {}: {kind}",
    .cmd_args.values.iter().enumerate().map( |(i, s)|
        if &Some(i) == .offending_arg {
            format!("\x1b[1m{s}\x1b[22m ")
        } else {
            format!("{s} ")
        })
        .collect::<String>()
        .trim_end()
    )
]
pub struct CommandError {
    cmd_name: String,
    cmd_args: Args,
    offending_arg: Option<usize>,
    kind: CommandErrorKind,
}

macro_rules! command_error_kinds_impl {
    ($($func:ident -> $kind:ident($input_type:ty)),*) => {
        impl CommandError {
            $(
                pub fn $func(cmd_name: impl Into<String>, cmd_args: Args, inner: $input_type) -> Self {
                    Self {
                        cmd_name: cmd_name.into(),
                        cmd_args,
                        offending_arg: None,
                        kind: CommandErrorKind::$kind(inner),
                    }
                }
            )*
        }
    };
}

command_error_kinds_impl!(
    args -> Args(String),
    io -> Io(std::io::Error),
    not_found -> NotFound(String),
    send_error -> SendError(StreamSendError),
    recv_error -> RecvError(StreamReceiveError),
    other -> Other(String),
    child_exit_err -> ChildExitErr(ExitStatus)
);

#[derive(Debug, thiserror::Error)]
pub enum CommandErrorKind {
    #[error("args parse error: {0}")]
    Args(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("command `{0}` not found")]
    NotFound(String),
    #[error(transparent)]
    SendError(#[from] StreamSendError),
    #[error(transparent)]
    RecvError(#[from] StreamReceiveError),
    #[error("{0}")]
    Other(String),

    #[error("{0}")]
    ChildExitErr(ExitStatus),
}

pub trait IntoCommandError {
    fn into_command_error(self, cmd_name: impl Into<String>, args: Args) -> CommandError;
}

impl<T: Into<CommandErrorKind>> IntoCommandError for T {
    fn into_command_error(self, cmd_name: impl Into<String>, cmd_args: Args) -> CommandError {
        CommandError {
            cmd_name: cmd_name.into(),
            cmd_args,
            offending_arg: None,
            kind: self.into(),
        }
    }
}
