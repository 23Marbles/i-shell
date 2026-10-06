use std::{borrow::Cow, path::PathBuf};

use rustyline::{
    Context, Helper,
    completion::{Completer, Pair},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};
use shell_core::command::builtin::BUILTINS;

pub struct LineHelper {}

impl Hinter for LineHelper {
    type Hint = String;
}

impl Completer for LineHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Only complete the text before the cursor.
        let input = &line[..pos];

        // Find the beginning of the current word.
        let start = input.rfind(char::is_whitespace).unwrap_or(0);

        if start != 0 {
            return Ok((0, Vec::new()));
        }

        let partial = &input[start..];

        let commands = BUILTINS;

        let path_env = std::env::var("PATH").unwrap_or_default();

        let dir_entries = path_env
            .split([';', ':'])
            .filter_map(|s| {
                let path = PathBuf::from(s);

                (path.exists() && path.is_dir())
                    .then_some(std::fs::read_dir(path).ok()?.filter_map(Result::ok))
            })
            .flatten();

        let programs = dir_entries.filter_map(|entry| {
            Some(Cow::Owned(
                entry
                    .path()
                    .file_name()
                    .map(|o| o.to_owned().into_string().ok())??,
            ))
        });

        let mut candidates: Vec<Pair> = commands
            .iter()
            .map(|command| Cow::Borrowed(command.name))
            .chain(programs)
            .filter(|name| name.starts_with(partial))
            .map(|command| Pair {
                display: command.to_string(),
                replacement: format!("{command} "),
            })
            .collect();

        candidates.dedup_by(|p1, p2| p1.display == p2.display);

        Ok((start, candidates))
    }
}

impl Highlighter for LineHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let mut is_first = true;
        let mut q_single = false;
        let mut q_double = false;
        let mut optional_arg = false;

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
                '-' if !q_double && !q_single && !is_first => {
                    optional_arg = true;
                    buf.push_str("\x1b[2m");
                }
                ' ' if !q_double && !q_single && (is_first || optional_arg) => {
                    buf.push_str("\x1b[0m");
                    is_first = false;
                    optional_arg = false;
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
