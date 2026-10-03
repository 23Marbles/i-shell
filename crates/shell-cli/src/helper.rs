use std::borrow::Cow;

use rustyline::{
    Helper,
    completion::{self, Completer},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};
use shell_core::parse::ParseState;

pub struct LineHelper {}

impl Hinter for LineHelper {
    type Hint = String;
}
impl Completer for LineHelper {
    type Candidate = completion::Pair;
}
impl Highlighter for LineHelper {
    fn highlight_char(
        &self,
        _line: &str,
        _pos: usize,
        _kind: rustyline::highlight::CmdKind,
    ) -> bool {
        true
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let mut is_first = true;
        let mut state = ParseState::default();

        let mut buf = "\x1b[33m".to_string();

        for ch in line.chars() {
            match ch {
                '"' if state.q_double || !state.q_single => {
                    if state.q_double {
                        buf.push_str("\"\x1b[0m");
                        state.q_double = false;
                        continue;
                    } else {
                        buf.push_str("\x1b[32m");
                        state.q_double = true
                    }
                }
                '\'' if state.q_single || !state.q_double => {
                    if state.q_single {
                        buf.push_str("'\x1b[0m");
                        state.q_single = false;
                        continue;
                    } else {
                        buf.push_str("\x1b[32m");
                        state.q_single = true
                    }
                }
                ' ' if !state.q_double && !state.q_single && is_first => {
                    buf.push_str("\x1b[0m");
                    is_first = false
                }
                _ => (),
            };

            buf.push(ch);
        }

        buf.push_str("\x1b[0m");

        Cow::Owned(buf)
    }
}
impl Validator for LineHelper {}
impl Helper for LineHelper {}
