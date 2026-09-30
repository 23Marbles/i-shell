use std::collections::VecDeque;

use crate::command::{Args, Command, Pipeline};

#[derive(Debug, thiserror::Error)]
pub enum PipelineParseErr {}

#[derive(Default)]
pub struct ParseState {
    pub single: bool,
    pub double: bool,
}

impl Pipeline {
    pub fn parse(s: &str) -> Option<Result<Self, PipelineParseErr>> {
        let mut state = ParseState::default();

        let mut args = VecDeque::new();

        let mut current_buf = String::new();

        for ch in s.chars() {
            match ch {
                '"' if state.double || !state.single => state.double ^= true,
                '\'' if state.single || !state.double => state.single ^= true,
                ' ' if !state.double && !state.single => {
                    args.push_back(current_buf);
                    current_buf = String::new();
                }
                ch => current_buf.push(ch),
            }
        }

        args.push_back(current_buf);

        Some(Ok(Self {
            cmd: Command {
                name: args.pop_front()?,
                args: Args {
                    values: args.into_iter().collect(),
                },
            },
        }))
    }
}
