use tokio::task::JoinSet;

use crate::{
    command::{Command, CommandError, CommandResult, context::CommandContext},
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

pub struct CommandChain {
    commands: Vec<(Command, PipeKind)>,
}

impl IntoIterator for CommandChain {
    type Item = (Command, PipeKind);

    type IntoIter = <Vec<(Command, PipeKind)> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.commands.into_iter()
    }
}

pub struct RunningChain {
    ouput: Option<StreamRx>,

    command_set: JoinSet<Result<CommandResult, CommandError>>,
}

impl ShellHandle {
    pub async fn spawn_chain(
        &self,
        stdin: Option<StreamRx>,
        chain: CommandChain,
    ) -> Result<RunningChain, CommandError> {
        let mut next_stdin: Option<StreamRx> = stdin;

        let mut set = JoinSet::new();

        for (cmd, pipe_kind) in chain.into_iter() {
            let mut ctx = CommandContext::new(self);

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

        Ok(RunningChain {
            command_set: set,
            ouput: next_stdin,
        })
    }
}
