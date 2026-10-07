use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{
    command::{
        Command, CommandError, CommandResult, ResolvedCommand, context::CommandContext,
        executing::RunningCommand,
    },
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
    cancellation_token: CancellationToken,
    command_set: JoinSet<Result<CommandResult, CommandError>>,
}

impl RunningChain {
    pub fn new() -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            command_set: JoinSet::new(),
        }
    }

    pub fn spawn(&mut self, cmd: ResolvedCommand, ctx: CommandContext) {
        let running =
            RunningCommand::spawn_with_cancel(cmd, ctx, self.cancellation_token.child_token());
        self.command_set.spawn(running.result());
    }

    pub async fn join_next(
        &mut self,
    ) -> Option<Result<Result<CommandResult, CommandError>, JoinError>> {
        self.command_set.join_next().await
    }

    async fn cancel_remaining(&mut self) {
        self.cancellation_token.cancel();
        while self.command_set.join_next().await.is_some() {}
    }
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
        while let Some(res) = self.command_set.join_next().await {
            match res {
                Ok(Ok(CommandResult::Continue)) => {}
                Ok(Ok(exit @ CommandResult::Exit(_))) => {
                    self.cancel_remaining().await;
                    return Ok(exit);
                }
                Ok(Err(err)) => {
                    self.cancel_remaining().await;
                    return Err(CommandSetError::CommandErr(err));
                }
                Err(err) => {
                    self.cancel_remaining().await;
                    return Err(CommandSetError::JoinErr(err));
                }
            }
        }

        Ok(CommandResult::Continue)
    }
}

impl Drop for RunningChain {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

impl ShellHandle {
    pub async fn spawn_chain(
        &self,
        stdin: Option<StreamRx>,
        chain: CommandChain,
    ) -> Result<(RunningChain, Option<StreamRx>), CommandError> {
        let mut running_chain = RunningChain::new();
        let mut next_stdin: Option<StreamRx> = stdin;
        let mut resolved = Vec::new();

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

            resolved.push((cmd, ctx));
        }

        for (cmd, ctx) in resolved {
            running_chain.spawn(cmd, ctx);
        }

        Ok((running_chain, next_stdin))
    }
}

#[cfg(test)]
mod tests {
    use std::{future, time::Duration};

    use super::*;
    use crate::command::builtin::BuiltinFuture;
    use crate::shell::Shell;

    fn args() -> crate::command::Args {
        crate::command::Args { values: Vec::new() }
    }

    fn context() -> CommandContext {
        let shell = Shell::default().into_handle();
        CommandContext::new(&shell)
    }

    fn resolved(func: crate::command::builtin::BuiltinFunc) -> ResolvedCommand {
        ResolvedCommand::Builtin {
            name: "test".to_string(),
            args: args(),
            func,
        }
    }

    fn continue_command(_: CommandContext, _: crate::command::Args) -> BuiltinFuture {
        Box::pin(async { Ok(CommandResult::Continue) })
    }

    fn exit_command(_: CommandContext, _: crate::command::Args) -> BuiltinFuture {
        Box::pin(async { Ok(CommandResult::Exit(7)) })
    }

    fn error_command(_: CommandContext, args: crate::command::Args) -> BuiltinFuture {
        Box::pin(async move {
            Err(CommandError::other(
                "test",
                args,
                "expected failure".to_string(),
            ))
        })
    }

    fn pending_command(_: CommandContext, _: crate::command::Args) -> BuiltinFuture {
        Box::pin(async { future::pending::<Result<CommandResult, CommandError>>().await })
    }

    fn panicking_command(_: CommandContext, _: crate::command::Args) -> BuiltinFuture {
        Box::pin(async { panic!("expected task panic") })
    }

    #[tokio::test]
    async fn empty_chain_returns_continue() {
        assert!(matches!(
            RunningChain::new().result().await,
            Ok(CommandResult::Continue)
        ));
    }

    #[tokio::test]
    async fn result_waits_for_all_continue_commands() {
        let mut chain = RunningChain::new();
        chain.spawn(resolved(continue_command), context());
        chain.spawn(resolved(continue_command), context());

        assert!(matches!(chain.result().await, Ok(CommandResult::Continue)));
    }

    #[tokio::test]
    async fn exit_cancels_other_commands_and_is_returned() {
        let mut chain = RunningChain::new();
        chain.spawn(resolved(pending_command), context());
        chain.spawn(resolved(exit_command), context());

        let result = tokio::time::timeout(Duration::from_secs(1), chain.result())
            .await
            .expect("the pending command should be cancelled");

        assert!(matches!(result, Ok(CommandResult::Exit(7))));
    }

    #[tokio::test]
    async fn command_error_cancels_other_commands_and_is_returned() {
        let mut chain = RunningChain::new();
        chain.spawn(resolved(pending_command), context());
        chain.spawn(resolved(error_command), context());

        let result = tokio::time::timeout(Duration::from_secs(1), chain.result())
            .await
            .expect("the pending command should be cancelled");

        assert!(matches!(result, Err(CommandSetError::CommandErr(_))));
    }

    #[tokio::test]
    async fn task_panic_cancels_other_commands_and_is_returned() {
        let mut chain = RunningChain::new();
        chain.spawn(resolved(pending_command), context());
        chain.spawn(resolved(panicking_command), context());

        let result = tokio::time::timeout(Duration::from_secs(1), chain.result())
            .await
            .expect("the pending command should be cancelled");

        assert!(matches!(result, Err(CommandSetError::CommandErr(_))));
    }

    #[tokio::test]
    async fn join_next_returns_completed_command_result() {
        let mut chain = RunningChain::new();
        chain.spawn(resolved(exit_command), context());

        let joined = chain.join_next().await.expect("one command was spawned");
        assert!(matches!(joined, Ok(Ok(CommandResult::Exit(7)))));
        assert!(chain.join_next().await.is_none());
    }

    #[tokio::test]
    async fn dropping_chain_signals_cancellation() {
        let mut chain = RunningChain::new();
        chain.spawn(resolved(pending_command), context());
        let token = chain.cancellation_token.clone();

        drop(chain);

        assert!(token.is_cancelled());
    }
}
