use crate::{
    command::{Args, CommandError, IntoCommandError},
    streams::{StreamRx, StreamTx},
};
use std::{path::PathBuf, process::Stdio};

pub struct Program {
    path: PathBuf,
    args: Args,
    child: tokio::process::Child,
}

pub struct ProgramBridge {
    pub stderr_reader: StreamRx,
    pub stdout_reader: StreamRx,
    pub stdin_writer: StreamTx,
}

impl Program {
    pub fn run(path: PathBuf, args: Args) -> Result<(Self, ProgramBridge), CommandError> {
        let mut child = match tokio::process::Command::new(&path)
            .args(&args.values)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn()
        {
            Ok(t) => t,
            Err(e) => return Err(CommandError::io(path.to_string_lossy(), args, e)),
        };

        let stdout = child.stdout.take().expect("Just set the stdout as piped");
        let stderr = child.stderr.take().expect("Just set the stderr as piped");
        let stdin = child.stdin.take().expect("Just set the stdin as piped");

        Ok((
            Self { path, args, child },
            ProgramBridge {
                stderr_reader: StreamRx::from(stderr),
                stdout_reader: StreamRx::from(stdout),
                stdin_writer: StreamTx::from(stdin),
            },
        ))
    }

    pub async fn result(self) -> Result<(), CommandError> {
        let Self {
            path,
            args,
            mut child,
        } = self;

        match child.wait().await {
            Ok(err) => {
                if err.success() {
                    Ok(())
                } else {
                    Err(CommandError::child_exit_err(
                        path.to_string_lossy(),
                        args,
                        err,
                    ))
                }
            }
            Err(e) => Err(e.into_command_error(path.to_string_lossy(), args)),
        }
    }
}
