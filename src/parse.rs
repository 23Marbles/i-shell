use crate::command::Args;

#[derive(Debug, thiserror::Error)]
pub enum PipelineParseErr {}

pub struct Pipeline {
    pub cmd: CommandSpec,
}

impl TryFrom<String> for Pipeline {
    type Error = PipelineParseErr;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let iter = value.split_whitespace().collect::<Vec<_>>();

        let cmd_name = iter[0];

        let args = &iter[1..];

        Ok(Self {
            cmd: CommandSpec {
                name: cmd_name.to_string(),
                args: Args {
                    values: args.iter().map(|s| s.to_string()).collect(),
                },
            },
        })
    }
}

pub struct CommandSpec {
    pub name: String,
    pub args: Args,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {}
