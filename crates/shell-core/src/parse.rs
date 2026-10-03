use std::{
    collections::VecDeque,
    fmt::{Display, Write},
};

use crate::command::{Args, Command, chain::ChainedCommand};

pub mod token;

#[derive(Debug, thiserror::Error)]
pub enum ParseErr {}

//pub struct Parser<'a> {
//    remaining: &'a str,
//    tokens: Vec<Token<'a>>,
//}

#[derive(Default)]
pub struct ParseState {
    pub q_single: bool,
    pub q_double: bool,
}

/*impl Parser<'_> {
    pub fn next_token(&self) -> Result<Option<Token>, ParseErr> {
        if self.remaining.is_empty() {
            return Ok(None);
        };

        let mut buf = String::new();

        Ok(())
    }
    /* pub fn parse(s: &str) -> Option<Result<ChainedCommand, PipelineParseErr>> {
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
    } */
}
*/
