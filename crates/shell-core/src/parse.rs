use crate::command::{Args, Command, Pipeline};

#[derive(Debug, thiserror::Error)]
pub enum PipelineParseErr {}

impl Pipeline {
    pub fn parse(s: &str) -> Result<Option<Self>, PipelineParseErr> {
        let iter = s.split_whitespace().collect::<Vec<_>>();

        let cmd_name = iter[0];

        let args = &iter[1..];

        Ok(Some(Self {
            cmd: Command {
                name: cmd_name.to_string(),
                args: Args {
                    values: args.iter().map(|s| s.to_string()).collect(),
                },
            },
        }))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {}
