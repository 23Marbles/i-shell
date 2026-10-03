use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::Arc,
};

use pathsearch::find_executable_in_path;
use tokio::sync::RwLock;

use crate::{
    command::builtin::{BUILTINS, Builtin, BuiltinFunc, BuiltinRegisterError, Builtins},
    streams::{StreamRx, StreamSendError, StreamTx},
};

#[derive(Debug, Clone)]
pub struct ShellHandle {
    pub handle: Arc<tokio::sync::RwLock<Shell>>,
}

impl ShellHandle {
    #[must_use]
    pub async fn shell<R>(&self, func: impl FnOnce(&Shell) -> R) -> R {
        (func)(self.handle.read().await.deref())
    }

    #[must_use]
    pub async fn shell_mut<R>(&self, func: impl FnOnce(&mut Shell) -> R) -> R {
        (func)(self.handle.write().await.deref_mut())
    }
}

impl From<Shell> for ShellHandle {
    fn from(value: Shell) -> Self {
        Self {
            handle: Arc::new(RwLock::new(value)),
        }
    }
}

#[derive(Debug)]
pub struct Shell {
    pub cwd: PathBuf,
    builtins: Builtins,
    stderr: StreamTx,
    stdout: StreamTx,
    stdin: StreamRx,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or(if let Some(path) = std::env::home_dir() {
                std::env::set_current_dir(&path).unwrap();
                path
            } else {
                panic!()
            }),
            builtins: Builtins::from_builtins(BUILTINS).unwrap(),
            stderr: StreamTx::from(crate::streams::process::Stderr::new()),
            stdout: StreamTx::from(crate::streams::process::Stdout::new()),
            stdin: StreamRx::from(crate::streams::process::Stdin::new()),
        }
    }
}

impl Shell {
    pub async fn print(&mut self, s: String) -> Result<(), StreamSendError> {
        self.stdout.send_raw(s.into()).await
    }

    pub async fn println(&mut self, s: String) -> Result<(), StreamSendError> {
        self.stdout.send_raw(format!("{s}\n").into()).await
    }

    pub async fn eprint(&mut self, s: String) -> Result<(), StreamSendError> {
        self.stderr.send_raw(s.into()).await
    }

    pub async fn eprintln(&mut self, s: String) -> Result<(), StreamSendError> {
        self.stderr.send_raw(format!("{s}\n").into()).await
    }

    #[inline]
    pub fn add_builtin(&mut self, builtin: Builtin) -> Result<(), BuiltinRegisterError> {
        self.builtins.register(builtin)
    }

    #[inline]
    pub fn resolve_builtin(&self, builtin_name: &str) -> Option<BuiltinFunc> {
        self.builtins.get(builtin_name)
    }

    #[inline]
    pub fn has_builtin(&self, builtin_name: &str) -> bool {
        self.resolve_builtin(builtin_name).is_some()
    }

    #[inline]
    pub fn resolve_program(&self, program_name: &str) -> Option<PathBuf> {
        find_executable_in_path(program_name)
    }

    #[inline]
    pub fn has_program(&self, program_name: &str) -> bool {
        self.resolve_program(program_name).is_some()
    }

    pub fn into_handle(self) -> ShellHandle {
        ShellHandle::from(self)
    }
}
