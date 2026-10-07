use tokio::task::{JoinError, JoinSet};

use crate::{
    command::{Command, CommandError, CommandResult, context::CommandContext},
    parse::token::{self, Pipe},
    shell::ShellHandle,
    streams::{StreamRx, StreamTx},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeKind {
    /// Pipe the current command's stderr into the next command's stdin.
    Err,

    /// Pipe the current command's stdout into the next command's stdin.
    Out,

    /// The current command is the last command in the chain.
    None,
}

impl PipeKind {
    pub fn from_pipe(pipe: Pipe) -> Self {
        let _ = pipe;
        Self::Out
    }
}

#[derive(Debug, Default)]
pub struct CommandChain {
    commands: Vec<(Command, PipeKind)>,
}

impl CommandChain {
    pub fn from_ephemeral(ephemeral: token::CommandChain<'_>) -> Self {
        let mut this = Self::default();

        let mut prev = this.commands.push_mut((
            Command::from_ephemeral(ephemeral.first_command),
            PipeKind::None,
        ));

        for (pipe, cmd) in ephemeral.opt_commands {
            prev.1 = PipeKind::from_pipe(pipe);
            prev = this
                .commands
                .push_mut((Command::from_ephemeral(cmd), PipeKind::None))
        }

        this
    }
}

impl IntoIterator for CommandChain {
    type Item = (Command, PipeKind);

    type IntoIter = <Vec<(Command, PipeKind)> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.commands.into_iter()
    }
}

pub struct RunningChain {
    command_set: JoinSet<Result<CommandResult, CommandError>>,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandSetError {
    #[error(transparent)]
    JoinErr(#[from] JoinError),
    #[error(transparent)]
    CommandErr(#[from] CommandError),
}

impl RunningChain {
    pub async fn result(mut self) -> Result<CommandResult, CommandSetError> {
        let mut result = None;

        while let Some(res) = self.command_set.join_next().await.map(|r| Ok(r??)) {
            if res.is_err() || matches!(res, Ok(CommandResult::Exit(_))) {
                result = Some(res);
                break;
            }
        }

        if result.is_some() {
            self.command_set.abort_all();

            while let Some(_) = self.command_set.join_next().await {}
        }

        result.unwrap_or(Ok(CommandResult::Continue))
    }
}

impl ShellHandle {
    pub async fn spawn_chain(
        &self,
        stdin: Option<StreamRx>,
        chain: CommandChain,
    ) -> Result<(RunningChain, Option<StreamRx>), CommandError> {
        let mut next_stdin: Option<StreamRx> = stdin;

        let mut set = JoinSet::new();

        for (cmd, pipe_kind) in chain.into_iter() {
            let mut ctx = CommandContext::new(self);

            if let Some(stdin) = next_stdin {
                ctx = ctx.with_stdin(stdin);
            }

            next_stdin = match pipe_kind {
                PipeKind::Err => {
                    let (writer, reader) = tokio::io::duplex(64 * 1024);

                    let tx = StreamTx::from(writer);
                    let rx = StreamRx::from(reader);

                    ctx = ctx.with_stderr(tx);

                    Some(rx)
                }
                PipeKind::Out => {
                    let (writer, reader) = tokio::io::duplex(64 * 1024);

                    let tx = StreamTx::from(writer);
                    let rx = StreamRx::from(reader);

                    ctx = ctx.with_stdout(tx);

                    Some(rx)
                }
                PipeKind::None => None,
            };

            let cmd = cmd.resolve(self).await?;

            set.spawn(cmd.run(ctx));
        }

        Ok((RunningChain { command_set: set }, next_stdin))
    }
}
