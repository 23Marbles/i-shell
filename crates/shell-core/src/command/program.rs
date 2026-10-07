use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio},
};

use command_group::{AsyncCommandGroup, AsyncGroupChild};
use tokio::{select, task::JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{
    command::{Args, CommandError, IntoCommandError, context::CommandContext},
    streams::adapter::AdapterError,
};

pub struct Program {
    path: PathBuf,

    args: Args,

    child: AsyncGroupChild,

    finished: bool,

    adapters: Adapters,
}

impl Program {
    pub async fn spawn(
        path: PathBuf,
        args: Args,
        ctx: CommandContext,
    ) -> Result<Self, CommandError> {
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

        let mut child = match command.group_spawn() {
            Ok(t) => t,
            Err(e) => return Err(CommandError::io(path.to_string_lossy(), args, e)),
        };

        let mut adapter_set = JoinSet::new();

        let mut stdin_cancel = None;
        has_stdin_adapter.then(|| {
            let stdin = child
                .inner()
                .stdin
                .take()
                .expect("Just set the stdin as piped");
            let cancel = CancellationToken::new();
            let task_cancel = cancel.clone();
            let mut stream = ctx.stdin.unwrap();
            stdin_cancel = Some(cancel);

            adapter_set.spawn(async move {
                stream
                    .copy_to(stdin, task_cancel)
                    .await
                    .map_err(AdapterError::from)
            })
        });
        has_stdout_adapter.then(|| {
            let stdout = child
                .inner()
                .stdout
                .take()
                .expect("Just set the stdout as piped");
            let mut stream = ctx.stdout.unwrap();

            adapter_set.spawn(async move {
                stream
                    .send_reader(stdout, CancellationToken::new())
                    .await
                    .map_err(AdapterError::from)
            })
        });
        has_stderr_adapter.then(|| {
            let stderr = child
                .inner()
                .stderr
                .take()
                .expect("Just set the stderr as piped");
            let mut stream = ctx.stderr.unwrap();

            adapter_set.spawn(async move {
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
                stdin_cancel,
                adapter_set,
            },
            finished: false,
        })
    }

    pub fn cancel(&mut self) -> std::io::Result<()> {
        if let Some(ref cancel) = self.adapters.stdin_cancel
            && !cancel.is_cancelled()
        {
            cancel.cancel();
        };
        self.child.start_kill()
    }

    pub async fn result(&mut self, cancel: CancellationToken) -> Result<(), CommandError> {
        if self.finished {
            return Err(CommandError::double_return(
                self.path.to_string_lossy(),
                self.args.clone(),
            ));
        }

        let mut child_status: Option<ExitStatus> = None;

        while child_status.is_none() || !self.adapters.adapter_set.is_empty() {
            select! {
                _ = cancel.cancelled() => {
                    if self.finished {
                        return Err(CommandError::double_return(
                            self.path.to_string_lossy(),
                            self.args.clone(),
                        ));
                    }

                    let _ = self.child.start_kill();
                    self.adapters.adapter_set.abort_all();

                    let _ = self.child.wait().await;
                    while self.adapters.adapter_set.join_next().await.is_some() {}

                    self.finished = true;

                    return Err(CommandError::cancelled(
                        self.path.to_string_lossy(),
                        self.args.clone(),
                    ));
                }

                child_res = self.child.inner().wait(),
                    if child_status.is_none() =>
                {
                    match child_res {
                        Ok(status) => {
                            child_status = Some(status);
                            if let Some(cancel) = &self.adapters.stdin_cancel {
                                cancel.cancel();
                            }
                        }
                        Err(e) => {
                            self.adapters.adapter_set.abort_all();
                            let _ = self.child.start_kill();
                            let _ = self.child.wait().await;

                            while self.adapters.adapter_set.join_next().await.is_some() {}

                            if self.finished {
                                return Err(CommandError::double_return(
                                    self.path.to_string_lossy(),
                                    self.args.clone(),
                                ));
                            }

                            self.finished = true;

                            return Err(e.into_command_error(
                                self.path.to_string_lossy(),
                                self.args.clone(),
                            ));
                        }
                    }
                }

                Some(result) = self.adapters.adapter_set.join_next(),
                    if !self.adapters.adapter_set.is_empty() =>
                {
                    match result {
                        Ok(Ok(())) => {}

                        Ok(Err(e)) => {
                            self.adapters.adapter_set.abort_all();
                            let _ = self.child.start_kill();
                            let _ = self.child.wait().await;

                            while self.adapters.adapter_set.join_next().await.is_some() {}

                            if self.finished {
                                return Err(CommandError::double_return(
                                    self.path.to_string_lossy(),
                                    self.args.clone(),
                                ));
                            }

                            self.finished = true;

                            return Err(e.into_command_error(
                                self.path.to_string_lossy(),
                                self.args.clone(),
                            ));
                        }

                        Err(e) => {
                            self.adapters.adapter_set.abort_all();
                            let _ = self.child.start_kill();
                            let _ = self.child.wait().await;

                            while self.adapters.adapter_set.join_next().await.is_some() {}

                            if self.finished {
                                return Err(CommandError::double_return(
                                    self.path.to_string_lossy(),
                                    self.args.clone(),
                                ));
                            }

                            self.finished = true;

                            return Err(CommandError::adapter_err(
                                self.path.to_string_lossy(),
                                self.args.clone(),
                                e.into(),
                            ));
                        }
                    }
                }
            }
        }

        let status =
            self.child.wait().await.map_err(|e| {
                e.into_command_error(self.path.to_string_lossy(), self.args.clone())
            })?;

        if self.finished {
            return Err(CommandError::double_return(
                self.path.to_string_lossy(),
                self.args.clone(),
            ));
        }

        self.finished = true;

        if status.success() {
            Ok(())
        } else {
            Err(CommandError::child_exit_err(
                self.path.to_string_lossy(),
                self.args.clone(),
                status,
            ))
        }
    }
}
pub struct Adapters {
    stdin_cancel: Option<CancellationToken>,
    adapter_set: JoinSet<Result<(), AdapterError>>,
}
