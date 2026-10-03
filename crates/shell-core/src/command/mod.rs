use std::{path::PathBuf, process::ExitStatus};

use crate::{
    command::{builtin::BuiltinFunc, context::CommandContext, program::Program},
    shell::ShellHandle,
    streams::{StreamReceiveError, StreamSendError, adapter::AdapterError},
};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Args {
    pub values: Vec<String>,
}

pub mod builtin;
pub mod chain;
pub mod context;
pub mod program;

pub struct Pipeline {
    pub cmd: Command,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub args: Args,
}

impl Command {
    pub async fn resolve(self, shell: &ShellHandle) -> Result<ResolvedCommand, CommandError> {
        let Self { name, args } = self;

        if let Some(func) = shell.shell(|shell| shell.resolve_builtin(&name)).await {
            Ok(ResolvedCommand::Builtin { name, args, func })
        } else if let Some(path) = shell.shell(|shell| shell.resolve_program(&name)).await {
            Ok(ResolvedCommand::Program { path, args })
        } else {
            Err(CommandError::not_found(name.clone(), args, name))
        }
    }
}

#[derive(Debug)]
pub enum ResolvedCommand {
    Builtin {
        name: String,
        args: Args,
        func: BuiltinFunc,
    },
    Program {
        path: PathBuf,
        args: Args,
    },
}

impl ResolvedCommand {
    pub async fn run(self, ctx: CommandContext) -> Result<CommandResult, CommandError> {
        match self {
            ResolvedCommand::Builtin {
                name: _,
                args,
                func,
            } => (func)(ctx, args).await,
            ResolvedCommand::Program { path, args } => Program::run(path, args, ctx)
                .await?
                .monitor()
                .await
                .map(|_| CommandResult::Continue),
        }
    }
}

pub enum CommandResult {
    Continue,
    Exit(u8),
}

#[derive(Debug, thiserror::Error)]
#[error(
    "{cmd_name}: {}{kind}",
    {
        let args = .cmd_args.values.iter().enumerate().map( |(i, s)|
            if &Some(i) == .offending_arg {
                format!("\x1b[1m{s}\x1b[22m ")
            } else {
                format!("{s} ")
            })
            .collect::<String>();

        let args = args.trim_end();

        if args.is_empty() {
            String::new()
        } else {
            format!("{args}: ")
        }
    }
)]
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
    child_exit_err -> ChildExitErr(ExitStatus),
    adapter_err -> AdapterErr(AdapterError)
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
    #[error("adapter failed: {0}")]
    AdapterErr(#[from] AdapterError),
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

#[cfg(feature = "serde")]
pub mod wire {
    use std::fmt::Display;

    use super::*;

    #[derive(Debug, thiserror::Error, serde::Serialize, serde::Deserialize)]
    #[error(
    "{cmd_name}: {}{kind}",
        {
            let args = .cmd_args.values.iter().enumerate().map( |(i, s)|
                if &Some(i) == .offending_arg {
                    format!("\x1b[1m{s}\x1b[22m ")
                } else {
                    format!("{s} ")
                })
                .collect::<String>();

            let args = args.trim_end();

            if args.is_empty() {
                String::new()
            } else {
                format!("{args}: ")
            }
        }
    )]
    pub struct WireCommandError {
        cmd_name: String,
        cmd_args: Args,
        offending_arg: Option<usize>,
        kind: WireCommandErrorKind,
    }

    impl From<CommandError> for WireCommandError {
        fn from(value: CommandError) -> Self {
            let CommandError {
                cmd_name,
                cmd_args,
                offending_arg,
                kind,
            } = value;

            Self {
                cmd_name,
                cmd_args,
                offending_arg,
                kind: kind.into(),
            }
        }
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum WireCommandErrorKind {
        Args(String),
        Io(String),
        NotFound(String),
        SendError(String),
        RecvError(String),
        Other(String),
        ChildExitErr(i32),
        AdapterErr(String),
    }

    impl Display for WireCommandErrorKind {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                WireCommandErrorKind::Args(s)
                | WireCommandErrorKind::Io(s)
                | WireCommandErrorKind::NotFound(s)
                | WireCommandErrorKind::SendError(s)
                | WireCommandErrorKind::RecvError(s)
                | WireCommandErrorKind::Other(s)
                | WireCommandErrorKind::AdapterErr(s) => write!(f, "{s}"),
                WireCommandErrorKind::ChildExitErr(c) => write!(f, "{c}"),
            }
        }
    }

    impl From<CommandErrorKind> for WireCommandErrorKind {
        fn from(err: CommandErrorKind) -> Self {
            match err {
                CommandErrorKind::Args(e) => Self::Args(e),
                CommandErrorKind::Io(e) => Self::Io(e.to_string()),
                CommandErrorKind::NotFound(e) => Self::NotFound(e),
                CommandErrorKind::SendError(e) => Self::SendError(e.to_string()),
                CommandErrorKind::RecvError(e) => Self::RecvError(e.to_string()),
                CommandErrorKind::Other(e) => Self::Other(e),
                CommandErrorKind::ChildExitErr(e) => Self::ChildExitErr(e.code().unwrap_or(-1)),
                CommandErrorKind::AdapterErr(e) => Self::AdapterErr(e.to_string()),
            }
        }
    }
}
