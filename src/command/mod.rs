use crate::{
    shell::Shell,
    streams::{StreamRx, StreamTx},
};

pub mod builtin;

pub struct CommandContext<'a, T> {
    pub shell: &'a mut Shell,
    pub stdin: StreamRx<T>,
    pub stdout: StreamTx<T>,
    pub stderr: StreamTx<T>,
}

impl<T> CommandContext<'_, T> {
    pub fn send_stdout(&self, v: T) -> Result<(), T> {
        self.stdout.send(v)
    }

    pub fn send_stderr(&self, v: T) -> Result<(), T> {
        self.stderr.send(v)
    }

    pub fn recv_stdin(&self) -> Option<T> {
        self.stdin.recv()
    }
}

pub enum CommandResult {
    Continue,
    Exit(i32),
}

pub enum CommandError {
    Usage(String),
    Io(std::io::Error),
    NotFound(String),
    PermissionDenied(String),
    Other(String),
}
