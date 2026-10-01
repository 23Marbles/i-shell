use crate::shell::{CliError, CliShell};
use std::process::ExitCode;

pub mod helper;
pub mod shell;
#[tokio::main]
async fn main() -> Result<ExitCode, CliError> {
    let shell = CliShell::new()?;

    shell.run().await
}
