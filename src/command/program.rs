use std::{
    error::Error,
    io::Write,
    path::PathBuf,
    process::{ChildStdin, Stdio},
};

use crate::{
    command::{Args, CommandContext},
    streams::{StreamSendError, StreamSender},
};

pub struct Program {
    ctx: CommandContext,
    path: PathBuf,
    args: Args,
}

impl Program {
    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut child = std::process::Command::new(&self.path)
            .args(&self.args.values)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdin = child.stdin.take().unwrap();

        Ok(())
    }
}

#[async_trait::async_trait]
impl StreamSender for ChildStdin {
    async fn send(&self, data: &[u8]) -> Result<(), StreamSendError> {
        self.write_all(data)?;
        Ok(())
    }
}
