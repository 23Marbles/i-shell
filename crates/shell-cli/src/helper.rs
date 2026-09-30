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
                '"' if state.double || !state.single => {
                    if state.double {
                        buf.push_str("\"\x1b[0m");
                        state.double = false;
                        continue;
                    } else {
                        buf.push_str("\x1b[32m");
                        state.double = true
                    }
                }
                '\'' if state.single || !state.double => {
                    if state.single {
                        buf.push_str("'\x1b[0m");
                        state.single = false;
                        continue;
                    } else {
                        buf.push_str("\x1b[32m");
                        state.single = true
                    }
                }
                ' ' if !state.double && !state.single && is_first => {
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
