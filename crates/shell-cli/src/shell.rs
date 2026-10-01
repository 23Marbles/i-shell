use std::process::ExitCode;

use rustyline::{Editor, error::ReadlineError, history::DefaultHistory};
use shell_core::{
    command::{CommandResult, Pipeline, context::CommandContext},
    shell::{Shell, ShellHandle},
};
use shell_headless::controller::{Connected, RemoteShellController};
use thiserror::Error;

use crate::helper::LineHelper;

pub struct CliShell {
    shell_handle: ShellHandle,
    remotes: Vec<RemoteShellController<Connected>>,
    rl: Editor<LineHelper, DefaultHistory>,
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Rustlyline(#[from] rustyline::error::ReadlineError),
}

impl CliShell {
    pub fn new() -> Result<Self, CliError> {
        let shell_handle = Shell::default().into_handle();
        let mut rl = Editor::new()?;
        rl.set_helper(Some(LineHelper {}));

        Ok(Self {
            shell_handle,
            remotes: Vec::new(),
            rl,
        })
    }

    pub async fn run(mut self) -> Result<ExitCode, CliError> {
        loop {
            let line = match self.rl.readline(&format!(
                "{}>> ",
                self.shell_handle
                    .shell(|shell| {
                        #[cfg(target_os = "windows")]
                        return shell.cwd.clone().into_os_string();

                        #[cfg(not(target_os = "windows"))]
                        return shell
                            .cwd
                            .to_path_buf()
                            .file_name()
                            .map_or(std::ffi::OsString::new(), |n| n.to_os_string());
                    })
                    .await
                    .display()
            )) {
                Ok(s) => {
                    self.rl.add_history_entry(&s)?;
                    s
                }
                Err(ReadlineError::Eof) => break,
                Err(ReadlineError::Interrupted) => {
                    println!("\x1b[31m^C\x1b[0m");
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            let cmd = match Pipeline::parse(&line) {
                Some(res) => res.unwrap().cmd,
                None => continue,
            };

            let cmd = match cmd.resolve(&self.shell_handle).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{e}");
                    continue;
                }
            };

            let ctx = CommandContext::new(&self.shell_handle);

            match cmd.run(ctx).await {
                Ok(CommandResult::Continue) => (),
                Ok(CommandResult::Exit(c)) => return Ok(ExitCode::from(c)),
                Err(e) => eprintln!("{e}"),
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
