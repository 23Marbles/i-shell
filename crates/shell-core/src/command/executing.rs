use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::command::{
    Args, CommandError, CommandErrorKind, CommandResult, IntoCommandError, ResolvedCommand,
    context::CommandContext, program::Program,
};

pub struct RunningCommand {
    name: String,
    args: Args,
    cancel: CancellationToken,
    handle: JoinHandle<Result<CommandResult, CommandError>>,
}

impl RunningCommand {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub async fn kill(self) -> Result<CommandResult, CommandError> {
        self.cancel();

        match self.result().await {
            Ok(r) => Ok(r),
            Err(CommandError {
                kind: CommandErrorKind::Cancelled,
                ..
            }) => Ok(CommandResult::Continue),
            Err(e) => Err(e),
        }
    }

    pub async fn result(self) -> Result<CommandResult, CommandError> {
        match self.handle.await {
            Ok(res) => res,
            Err(e) => Err(e.into_command_error(self.name, self.args)),
        }
    }

    #[inline(always)]
    pub fn spawn(cmd: ResolvedCommand, ctx: CommandContext) -> Self {
        Self::spawn_with_cancel(cmd, ctx, CancellationToken::new())
    }

    pub fn spawn_with_cancel(
        cmd: ResolvedCommand,
        ctx: CommandContext,
        cancel: CancellationToken,
    ) -> Self {
        let (name, args) = match &cmd {
            ResolvedCommand::Builtin { name, args, .. } => (name.clone(), args.clone()),
            ResolvedCommand::Program { path, args, .. } => {
                (path.to_string_lossy().into_owned(), args.clone())
            }
        };
        let cancle_cloned = cancel.clone();

        let handle = tokio::spawn((async move || {
            let cancel = cancle_cloned;
            match cmd {
                ResolvedCommand::Builtin { name, args, func } => tokio::select! {
                    _ = cancel.cancelled() => {
                        Err(CommandError::cancelled(name, args))
                    }

                    res = func(ctx, args.clone()) => {
                        Ok(res?)
                    }
                },
                ResolvedCommand::Program { path, args } => {
                    let mut program = Program::spawn(path, args, ctx).await?;

                    program
                        .result(cancel)
                        .await
                        .map(|_| CommandResult::Continue)
                }
            }
        })());

        Self {
            cancel,
            handle,
            name,
            args,
        }
    }
}
