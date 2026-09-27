use std::{fmt::Display, path::PathBuf};

use crate::{
    codec::{WireDecode, WireEncode},
    shell::{Shell, ShellHandle},
    streams::{StreamReceiveError, StreamRx, StreamSendError, StreamTx},
};

pub struct CommandContext {
    shell_handle: ShellHandle,
    pub stdin: Option<StreamRx>,
    pub stdout: Option<StreamTx>,
    pub stderr: Option<StreamTx>,
}

impl CommandContext {
    pub fn new(shell_handle: &ShellHandle) -> Self {
        Self {
            shell_handle: shell_handle.clone(),
            stdin: None,
            stdout: None,
            stderr: None,
        }
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

    pub async fn print<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut out) = self.stdout {
            out.send(v.to_string()).await
        } else {
            print!("{v}");

            Ok(())
        }
    }

    pub async fn println<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut out) = self.stdout {
            out.send(format!("{v}\n")).await
        } else {
            println!("{v}");

            Ok(())
        }
    }

    pub async fn eprint<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut err) = self.stderr {
            err.send(v.to_string()).await
        } else {
            eprint!("{v}");

            Ok(())
        }
    }

    pub async fn eprintln<T: Display>(&mut self, v: T) -> Result<(), StreamSendError> {
        if let Some(ref mut err) = self.stderr {
            err.send(format!("{v}\n")).await
        } else {
            eprintln!("{v}");

            Ok(())
        }
    }

    #[inline(always)]
    pub async fn current_dir(&self) -> PathBuf {
        self.shell_handle.handle.read().await.cwd.clone()
    }

    pub async fn send_stdout<T: WireEncode>(
        &mut self,
        v: T,
    ) -> Option<Result<(), StreamSendError>> {
        Some(self.stdout.as_mut()?.send(v).await)
    }

    pub async fn send_stderr<T: WireEncode>(
        &mut self,
        v: T,
    ) -> Option<Result<(), StreamSendError>> {
        Some(self.stderr.as_mut()?.send(v).await)
    }

    pub async fn recv_stdin<T: WireDecode>(&mut self) -> Option<Result<T, StreamReceiveError>> {
        Some(self.stdin.as_mut()?.recv().await)
    }
}
