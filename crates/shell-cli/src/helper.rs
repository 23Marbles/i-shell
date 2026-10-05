use std::borrow::Cow;

use rustyline::{
    Helper,
    completion::{self, Completer},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};

pub struct LineHelper {}

impl Hinter for LineHelper {
    type Hint = String;
}
impl Completer for LineHelper {
    type Candidate = completion::Pair;
}
impl Highlighter for LineHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let mut is_first = true;
        let mut q_single = false;
        let mut q_double = false;

        let mut buf = "\x1b[33m".to_string();

        for ch in line.chars() {
            match ch {
                '"' if q_double || !q_single => {
                    if q_double {
                        buf.push_str("\"\x1b[0m");
                        q_double = false;
                        continue;
                    } else {
                        buf.push_str("\x1b[32m");
                        q_double = true
                    }
                }
                '\'' if q_single || !q_double => {
                    if q_single {
                        buf.push_str("'\x1b[0m");
                        q_single = false;
                        continue;
                    } else {
                        buf.push_str("\x1b[32m");
                        q_single = true
                    }
                }
                ' ' if !q_double && !q_single && is_first => {
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
    fn highlight_char(
        &self,
        _line: &str,
        _pos: usize,
        _kind: rustyline::highlight::CmdKind,
    ) -> bool {
        true
    }
}

impl Validator for LineHelper {}
impl Helper for LineHelper {}
