use tokio::{select, task::JoinSet};

use crate::{
    command::{Args, CommandContext, CommandError, IntoCommandError},
    streams::adapter::{AdapterError, Blank, StreamAdapter},
};
use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio},
};

pub struct Program {
    path: PathBuf,
    args: Args,
    child: tokio::process::Child,
    adapters: Adapters,
}

pub struct Adapters {
    stdout_adapter: Option<StreamAdapter<Blank>>,
    stderr_adapter: Option<StreamAdapter<Blank>>,
    stdin_adapter: Option<StreamAdapter<Blank>>,
}

impl Program {
    pub async fn run(path: PathBuf, args: Args, ctx: CommandContext) -> Result<Self, CommandError> {
        let mut command = tokio::process::Command::new(&path);

        command.current_dir(ctx.current_dir().await);
        command.args(&args.values);

        let has_stderr_adapter = if ctx.stderr.is_some() {
            command.stderr(Stdio::piped());
            true
        } else {
            command.stderr(Stdio::inherit());
            false
        };

        let has_stdout_adapter = if ctx.stdout.is_some() {
            command.stdout(Stdio::piped());
            true
        } else {
            command.stdout(Stdio::inherit());
            false
        };

        let has_stdin_adapter = if ctx.stdin.is_some() {
            command.stdin(Stdio::piped());
            true
        } else {
            command.stdin(Stdio::inherit());
            false
        };

        let mut child = match command.spawn() {
            Ok(t) => t,
            Err(e) => return Err(CommandError::io(path.to_string_lossy(), args, e)),
        };

        let stdin_adapter = has_stdin_adapter.then(|| {
            let stdin = child.stdin.take().expect("Just set the stdin as piped");
            StreamAdapter::<Blank>::spawn(ctx.stdin.unwrap(), stdin.into())
        });
        let stdout_adapter = has_stdout_adapter.then(|| {
            let stdout = child.stdout.take().expect("Just set the stdout as piped");
            StreamAdapter::<Blank>::spawn(stdout.into(), ctx.stdout.unwrap())
        });
        let stderr_adapter = has_stderr_adapter.then(|| {
            let stderr = child.stderr.take().expect("Just set the stderr as piped");
            StreamAdapter::<Blank>::spawn(stderr.into(), ctx.stderr.unwrap())
        });

        Ok(Self {
            path,
            args,
            child,
            adapters: Adapters {
                stdout_adapter,
                stderr_adapter,
                stdin_adapter,
            },
        })
    }

    pub async fn monitor(mut self) -> Result<(), CommandError> {
        let mut adapter_tasks: JoinSet<Result<(), AdapterError>> = tokio::task::JoinSet::new();

        let Adapters {
            stdout_adapter,
            stderr_adapter,
            stdin_adapter,
        } = self.adapters;

        for adapter in [stdout_adapter, stderr_adapter, stdin_adapter]
            .into_iter()
            .flatten()
        {
            let handle = adapter.into_join_handle();

            adapter_tasks.spawn(async move {
                handle.await??;
                Ok(())
            });
        }

        let mut child_status: Option<ExitStatus> = None;

        while child_status.is_none() || !adapter_tasks.is_empty() {
            select! {
                child_res = self.child.wait(),
                    if child_status.is_none() =>
                {
                    match child_res {
                        Ok(status) => {
                            child_status = Some(status);
                        }
                        Err(e) => {
                            adapter_tasks.abort_all();

                            while adapter_tasks.join_next().await.is_some() {}

                            return Err(e.into_command_error(
                                self.path.to_string_lossy(),
                                self.args,
                            ));
                        }
                    }
                }

                Some(result) = adapter_tasks.join_next(),
                    if !adapter_tasks.is_empty() =>
                {
                    match result {
                        Ok(Ok(())) => {
                            // This adapter completed.
                            // Continue monitoring the remaining tasks.
                        }

                        Ok(Err(e)) => {
                            adapter_tasks.abort_all();

                            let _ = self.child.start_kill();
                            let _ = self.child.wait().await;

                            while adapter_tasks.join_next().await.is_some() {}

                            return Err(e.into_command_error(
                                self.path.to_string_lossy(),
                                self.args,
                            ));
                        }

                        Err(e) => {
                            adapter_tasks.abort_all();

                            let _ = self.child.start_kill();
                            let _ = self.child.wait().await;

                            while adapter_tasks.join_next().await.is_some() {}

                            return Err(CommandError::adapter_err(
                                self.path.to_string_lossy(),
                                self.args,
                                e.into(),
                            ));
                        }
                    }
                }
            }
        }

        // While branch would not have completed if it was None
        let status = child_status.unwrap();

        if status.success() {
            Ok(())
        } else {
            Err(CommandError::child_exit_err(
                self.path.to_string_lossy(),
                self.args,
                status,
            ))
        }
    }
}
