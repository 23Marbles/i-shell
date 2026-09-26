use std::{fmt::Display, process::ExitStatus};

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

pub struct CommandContextBuilder {
    shell_handle: ShellHandle,
    stdin: Option<StreamRx>,
    stdout: Option<StreamTx>,
    stderr: Option<StreamTx>,
}

impl CommandContextBuilder {
    pub fn new(shell_handle: ShellHandle) -> CommandContextBuilder {
        Self {
            shell_handle,
            stdin: None,
            stdout: None,
            stderr: None,
        }
    }

    pub fn with_stdin(mut self, stdin: StreamRx) -> Self {
        self.stdin = Some(stdin);
        self
    }

    pub fn with_stdout(mut self, stdout: StreamTx) -> Self {
        self.stdout = Some(stdout);
        self
    }

    pub fn with_stderr(mut self, stderr: StreamTx) -> Self {
        self.stderr = Some(stderr);
        self
    }

    pub fn build(self) -> CommandContext {
        let Self {
            shell_handle,
            stdin,
            stdout,
            stderr,
        } = self;

        CommandContext {
            shell_handle,
            stdin,
            stdout,
            stderr,
        }
    }
}

pub struct CommandContext {
    shell_handle: ShellHandle,
    pub stdin: Option<StreamRx>,
    pub stdout: Option<StreamTx>,
    pub stderr: Option<StreamTx>,
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

    pub async fn print<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut out) = self.stdout {
            out.send(v.to_string()).await
        } else {
            print!("{v}");

            Ok(())
        }
    }

    pub async fn println<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut out) = self.stdout {
            out.send(format!("{v}\n")).await
        } else {
            println!("{v}");

            Ok(())
        }
    }

    pub async fn eprint<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut err) = self.stderr {
            err.send(v.to_string()).await
        } else {
            eprint!("{v}");

            Ok(())
        }
    }

    pub async fn eprintln<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut err) = self.stderr {
            err.send(format!("{v}\n")).await
        } else {
            eprintln!("{v}");

            Ok(())
        }
    }

    pub async fn send_stdout<T: WireEncode>(
        &mut self,
        v: T,
    ) -> Option<Result<(), StreamSendError>> {
        Some(self.stdout.as_mut()?.send(v).await)
    }

    pub async fn send_stderr<T: WireEncode>(
        &mut self,
        v: T,
    ) -> Option<Result<(), StreamSendError>> {
        Some(self.stderr.as_mut()?.send(v).await)
    }

    pub async fn recv_stdin<T: WireDecode>(&mut self) -> Option<Result<T, StreamReceiveError>> {
        Some(self.stdin.as_mut()?.recv().await)
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
