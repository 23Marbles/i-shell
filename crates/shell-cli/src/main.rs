mod helper;

use std::process::ExitCode;

use rustyline::{Editor, error::ReadlineError};
use shell_core::{
    command::{Command, CommandResult, chain::CommandChain, context::CommandContext}, parse::Parser, shell::Shell,
};

use crate::helper::LineHelper;

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
                    return {
                        let path = shell.cwd.clone();
                        let string = path.to_string_lossy();

                        string.strip_prefix(r"\\?\").unwrap_or(&string).to_string()
                    };

                    #[cfg(not(target_os = "windows"))]
                    return shell
                        .cwd
                        .to_path_buf()
                        .file_name()
                        .map_or("/".to_string(), |n| n.to_string_lossy().into_owned());
                })
                .await
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

        let mut parser = Parser::new(&line);

        let pipeline = match parser.parse() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };

        let chain = pipeline.first_chain;

        let chain = CommandChain::from_empheral(match chain {
            shell_core::parse::token::Chain::LocatedChain(_located_chain) => todo!(),
            shell_core::parse::token::Chain::CommandChain(command_chain) => command_chain,
        });

        let spawned_chain = match shell_handle.spawn_chain(None, chain).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        }.0;

        match spawned_chain.result().await {
            Ok(CommandResult::Continue) => (),
            Ok(CommandResult::Exit(c)) => return Ok(ExitCode::from(c)),
            Err(e) => eprintln!("{e}"),
        }
    }

    Ok(ExitCode::SUCCESS)
}
