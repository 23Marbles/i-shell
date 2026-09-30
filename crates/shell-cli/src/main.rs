use std::process::ExitCode;

use rustyline::{Editor, error::ReadlineError};
use shell_core::{
    command::{CommandResult, Pipeline, context::CommandContext},
    shell::Shell,
};

use crate::helper::LineHelper;

mod helper;
#[tokio::main]
async fn main() -> Result<ExitCode, ReadlineError> {
    let shell_handle = Shell::default().into_handle();
    let mut rl = Editor::new()?;
    rl.set_helper(Some(LineHelper {}));

    loop {
        let line = match rl.readline(&format!(
            "{}>> ",
            shell_handle
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
                rl.add_history_entry(&s)?;
                s
            }
            Err(ReadlineError::Eof) => break,
            Err(ReadlineError::Interrupted) => {
                println!("\x1b[31m^C\x1b[0m");
                continue;
            }
            Err(e) => return Err(e),
        };

        let cmd = Pipeline::parse(&line).unwrap().cmd;

        let cmd = match cmd.resolve(&shell_handle).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };

        let ctx = CommandContext::new(&shell_handle);

        match cmd.run(ctx).await {
            Ok(CommandResult::Continue) => (),
            Ok(CommandResult::Exit(c)) => return Ok(ExitCode::from(c)),
            Err(e) => eprintln!("{e}"),
        }
    }

    Ok(ExitCode::SUCCESS)
}
