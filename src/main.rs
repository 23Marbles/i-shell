use rustyline::error::ReadlineError;

use crate::shell::Shell;

pub mod codec;
pub mod command;
pub mod parse;
pub mod read;
pub mod shell;
pub mod streams;

#[tokio::main]
async fn main() -> Result<(), ReadlineError> {
    let shell = Shell::new();

    shell.run().await
}
