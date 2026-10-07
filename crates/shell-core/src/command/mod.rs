pub mod builtin;
pub mod chain;
pub mod context;
pub mod executing;
pub mod program;

use std::{path::PathBuf, process::ExitStatus};

use tokio::task::JoinError;
use tokio_util::sync::CancellationToken;

use crate::{
    command::{builtin::BuiltinFunc, context::CommandContext, program::Program},
    shell::ShellHandle,
    streams::{StreamReceiveError, StreamSendError, adapter::AdapterError},
};

pub enum CommandResult {
    Exit(u8),

    Continue,
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
            ResolvedCommand::Program { path, args } => Program::spawn(path, args, ctx)
                .await?
                .result(CancellationToken::new())
                .await
                .map(|_| CommandResult::Continue),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Args {
    pub values: Vec<String>,
}

impl Args {
    pub fn from_ephemeral(args: Vec<crate::parse::token::Arg<'_>>) -> Self {
        Self {
            values: args.into_iter().map(|arg| arg.raw_to_string()).collect(),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,

    pub args: Args,
}

impl Command {
    pub fn from_ephemeral(cmd: crate::parse::token::Command<'_>) -> Self {
        Self {
            name: cmd.name.lexme.to_string(),
            args: Args::from_ephemeral(cmd.args),
        }
    }

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
    kind: CommandErrorKind,

    cmd_name: String,
    cmd_args: Args,

    offending_arg: Option<usize>,
}

macro_rules! command_error_kinds_impl {
    ($($func:ident -> $kind:ident$(($($type_name:ident: $input_type:ty$(,)?)+))?$(,)?)*) => {
        impl CommandError {
            $(
                pub fn $func(cmd_name: impl Into<String>, cmd_args: Args, $($($type_name: $input_type),+)?) -> Self {
                    Self {
                        cmd_name: cmd_name.into(),
                        cmd_args,
                        offending_arg: None,
                        kind: CommandErrorKind::$kind$(($($type_name),+))?,
                    }
                }
            )*
        }
    };
}

command_error_kinds_impl!(
    args -> Args(err: String),
    io -> Io(io_err: std::io::Error),
    not_found -> NotFound(name: String),
    send_error -> SendError(err: StreamSendError),
    recv_error -> RecvError(err: StreamReceiveError),
    other -> Other(v: String),
    child_exit_err -> ChildExitErr(status: ExitStatus),
    adapter_err -> AdapterErr(err: AdapterError),
    cancelled -> Cancelled,
    join_error -> JoinErr(join_err: JoinError),
    double_return -> DoubleReturn,
);

#[cfg(feature = "serde")]
pub mod wire {
    use super::*;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, thiserror::Error)]
    pub enum WireCommandErrorKind {
        #[error("{0}")]
        Io(String),

        #[error("args parse error: {0}")]
        Args(String),

        #[error("{0}")]
        Other(String),

        #[error("command `{0}` not found")]
        NotFound(String),

        #[error("{0}")]
        SendError(String),

        #[error("{0}")]
        RecvError(String),

        #[error("adapter failed: {0}")]
        AdapterErr(String),

        #[error("{0}")]
        ChildExitErr(i32),

        #[error("this task was cancelled")]
        Cancelled,

        #[error("the task this command is attached to failed: {0}")]
        JoinErr(String),

        #[error("tried to finish a task for a second time")]
        DoubleReturn,
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
                CommandErrorKind::Cancelled => Self::Cancelled,
                CommandErrorKind::JoinErr(join_error) => Self::JoinErr(join_error.to_string()),
                CommandErrorKind::DoubleReturn => Self::DoubleReturn,
            }
        }
    }

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
        kind: WireCommandErrorKind,

        cmd_name: String,
        cmd_args: Args,

        offending_arg: Option<usize>,
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

#[derive(Debug, thiserror::Error)]
pub enum CommandErrorKind {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("args parse error: {0}")]
    Args(String),

    #[error("{0}")]
    Other(String),

    #[error("command `{0}` not found")]
    NotFound(String),

    #[error(transparent)]
    SendError(#[from] StreamSendError),

    #[error(transparent)]
    RecvError(#[from] StreamReceiveError),

    #[error("adapter failed: {0}")]
    AdapterErr(#[from] AdapterError),

    #[error("{0}")]
    ChildExitErr(ExitStatus),

    #[error("this command was cancelled")]
    Cancelled,

    #[error("the task this command is attached to failed: {0}")]
    JoinErr(#[from] JoinError),

    #[error("tried to finish a task for a second time")]
    DoubleReturn,
}
