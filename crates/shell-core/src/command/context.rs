use std::{fmt::Display, path::PathBuf};

use crate::{
    shell::{Shell, ShellHandle},
    streams::{StreamReceiveError, StreamRx, StreamSendError, StreamTx},
};

#[derive(Debug)]
pub struct CommandContext {
    pub stdin: Option<StreamRx>,

    pub stdout: Option<StreamTx>,

    pub stderr: Option<StreamTx>,

    pub opt_path: Option<PathBuf>,

    shell_handle: ShellHandle,
}

impl CommandContext {
    pub fn new(shell_handle: &ShellHandle) -> Self {
        Self {
            shell_handle: shell_handle.clone(),
            stdin: None,
            stdout: None,
            stderr: None,
            opt_path: None,
        }
    }

    #[must_use]
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.opt_path = Some(path);
        self
    }
    #[must_use]
    pub fn with_stdin(mut self, stdin: StreamRx) -> Self {
        self.stdin = Some(stdin);
        self
    }
    #[must_use]
    pub fn with_stdout(mut self, stdout: StreamTx) -> Self {
        self.stdout = Some(stdout);
        self
    }
    #[must_use]
    pub fn with_stderr(mut self, stderr: StreamTx) -> Self {
        self.stderr = Some(stderr);
        self
    }

    pub async fn print<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut out) = self.stdout {
            out.send(v.to_string().into_bytes()).await
        } else {
            let mut shell = self.shell_handle.handle.write().await;

            shell.print(v.to_string()).await
        }
    }

    pub async fn eprint<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut err) = self.stderr {
            err.send(v.to_string().into_bytes()).await
        } else {
            let mut shell = self.shell_handle.handle.write().await;

            shell.eprint(v.to_string()).await
        }
    }

    #[must_use]
    #[inline(always)]
    pub async fn shell<R>(&self, func: impl FnOnce(&Shell) -> R) -> R {
        self.shell_handle.shell(func).await
    }
    #[must_use]
    #[inline(always)]
    pub async fn shell_mut<R>(&self, func: impl FnOnce(&mut Shell) -> R) -> R {
        self.shell_handle.shell_mut(func).await
    }

    pub async fn println<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut out) = self.stdout {
            out.send(format!("{v}\n").into_bytes()).await
        } else {
            let mut shell = self.shell_handle.handle.write().await;

            shell.println(v.to_string()).await
        }
    }

    pub async fn eprintln<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut err) = self.stderr {
            err.send(format!("{v}\n").into_bytes()).await
        } else {
            let mut shell = self.shell_handle.handle.write().await;

            shell.eprintln(v.to_string()).await
        }
    }

    pub async fn recv_stdin(&mut self) -> Option<Result<Vec<u8>, StreamReceiveError>> {
        Some(self.stdin.as_mut()?.recv().await)
    }

    pub async fn current_dir(&self) -> PathBuf {
        self.opt_path
            .as_ref()
            .unwrap_or(&self.shell_handle.handle.read().await.cwd)
            .clone()
    }

    pub async fn send_stdout(&mut self, v: Vec<u8>) -> Option<Result<(), StreamSendError>> {
        Some(self.stdout.as_mut()?.send(v).await)
    }
    pub async fn send_stderr(&mut self, v: Vec<u8>) -> Option<Result<(), StreamSendError>> {
        Some(self.stderr.as_mut()?.send(v).await)
    }

    pub async fn set_current_dir(&mut self, new_path: PathBuf) {
        match &mut self.opt_path {
            Some(path) => *path = new_path,
            None => self.shell_mut(|shell| shell.cwd = new_path).await,
        }
    }
}
