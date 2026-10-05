use tokio::task::JoinSet;

use crate::{
    command::{Command, CommandError, CommandResult, ResolvedCommand, context::CommandContext},
    shell::ShellHandle,
    streams::{StreamRx, StreamTx},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeKind {
    Err,

    Out,

    None,
}

#[derive(Debug, Clone)]
enum NextCommand {
    Finish,

    Command {
        cmd: Box<ChainedCommand>,

        pipe: PipeKind,
    },
}

#[derive(Debug, Default)]
pub struct CommandList {
    inner: Vec<(CommandContext, ResolvedCommand)>,
}

impl CommandList {
    pub fn spawn_all(self) -> JoinSet<Result<CommandResult, CommandError>> {
        let mut set = JoinSet::new();

        for (ctx, cmd) in self.inner.into_iter() {
            let _ = set.spawn(cmd.run(ctx));
        }

        set
    }
}

#[derive(Debug, Clone)]
pub struct ChainedCommand {
    cmd: Command,

    next: Option<NextCommand>,
}

impl IntoIterator for ChainedCommand {
    type Item = (PipeKind, Command);

    type IntoIter = ChainedCommandIter;

    fn into_iter(self) -> Self::IntoIter {
        ChainedCommandIter {
            current: Some(self),
        }
    }
}

pub struct ChainedCommandIter {
    current: Option<ChainedCommand>,
}

impl Iterator for ChainedCommandIter {
    type Item = (PipeKind, Command);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.take()?;
        let ChainedCommand { cmd, next } = current;
        let output_pipe = match next {
            Some(NextCommand::Command { pipe, cmd }) => {
                self.current = Some(*cmd);
                pipe
            }
            Some(NextCommand::Finish) | None => PipeKind::None,
        };

        Some((output_pipe, cmd))
    }
}

impl ShellHandle {
    pub async fn spawn_chain(
        &self,
        stdin: Option<StreamRx>,
        chained_command: ChainedCommand,
    ) -> Result<JoinSet<Result<CommandResult, CommandError>>, CommandError> {
        let mut command_list = CommandList::default();

        let mut next_stdin: Option<StreamRx> = stdin;

        for (pipe_kind, cmd) in chained_command.into_iter() {
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

            command_list.inner.push((ctx, cmd.resolve(self).await?));
        }

        let set = command_list.spawn_all();

        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::{ChainedCommand, NextCommand, PipeKind};
    use crate::command::Args;

    fn command(name: &str) -> ChainedCommand {
        ChainedCommand {
            cmd: crate::command::Command {
                name: name.to_owned(),
                args: Args { values: Vec::new() },
            },
            next: None,
        }
    }

    #[test]
    fn iterator_associates_pipe_with_preceding_command_output() {
        let mut first = command("first");
        let mut second = command("second");
        let mut third = command("third");

        third.next = Some(NextCommand::Finish);
        second.next = Some(NextCommand::Command {
            pipe: PipeKind::Err,
            cmd: Box::new(third),
        });
        first.next = Some(NextCommand::Command {
            pipe: PipeKind::Out,
            cmd: Box::new(second),
        });

        let items: Vec<_> = first
            .into_iter()
            .map(|(pipe, command)| (pipe, command.name))
            .collect();

        assert_eq!(
            items,
            [
                (PipeKind::Out, "first".to_owned()),
                (PipeKind::Err, "second".to_owned()),
                (PipeKind::None, "third".to_owned()),
            ]
        );
    }
}
