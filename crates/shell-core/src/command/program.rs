use tokio::{
    select,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

use crate::{
    command::{Args, CommandError, IntoCommandError, context::CommandContext},
    streams::adapter::AdapterError,
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
    stdout_adapter: Option<JoinHandle<Result<(), AdapterError>>>,
    stderr_adapter: Option<JoinHandle<Result<(), AdapterError>>>,
    stdin_adapter: Option<JoinHandle<Result<(), AdapterError>>>,
    stdin_cancel: Option<CancellationToken>,
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

        let mut stdin_cancel = None;
        let stdin_adapter = has_stdin_adapter.then(|| {
            let stdin = child.stdin.take().expect("Just set the stdin as piped");
            let cancel = CancellationToken::new();
            let task_cancel = cancel.clone();
            let mut stream = ctx.stdin.unwrap();
            stdin_cancel = Some(cancel);

            tokio::spawn(async move {
                stream
                    .copy_to(stdin, task_cancel)
                    .await
                    .map_err(AdapterError::from)
            })
        });
        let stdout_adapter = has_stdout_adapter.then(|| {
            let stdout = child.stdout.take().expect("Just set the stdout as piped");
            let mut stream = ctx.stdout.unwrap();
            tokio::spawn(async move {
                stream
                    .send_reader(stdout, CancellationToken::new())
                    .await
                    .map_err(AdapterError::from)
            })
        });
        let stderr_adapter = has_stderr_adapter.then(|| {
            let stderr = child.stderr.take().expect("Just set the stderr as piped");
            let mut stream = ctx.stderr.unwrap();
            tokio::spawn(async move {
                stream
                    .send_reader(stderr, CancellationToken::new())
                    .await
                    .map_err(AdapterError::from)
            })
        });

        Ok(Self {
            path,
            args,
            child,
            adapters: Adapters {
                stdout_adapter,
                stderr_adapter,
                stdin_adapter,
                stdin_cancel,
            },
        })
    }

    pub async fn monitor(mut self) -> Result<(), CommandError> {
        let mut adapter_tasks: JoinSet<Result<(), AdapterError>> = tokio::task::JoinSet::new();

        let Adapters {
            stdout_adapter,
            stderr_adapter,
            stdin_adapter,
            stdin_cancel,
        } = self.adapters;

        let adapter_handles = [stdout_adapter, stderr_adapter, stdin_adapter];

        for handle in adapter_handles.into_iter().flatten() {
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
                            if let Some(cancel) = &stdin_cancel {
                                cancel.cancel();
                            }
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
