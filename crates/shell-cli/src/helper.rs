use std::{borrow::Cow, fmt::Write, path::PathBuf};

use anstyle::{AnsiColor, Color, Style};
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

#[derive(Debug, Default)]
struct HighlighterState {
    current_colour: Style,
    is_single_quote: bool,
    is_double_quote: bool,
    // None = Not currently expected
    // Some(false) = Expected but not encountered;
    // Some(true) = Expected and encountered;
    is_command_name: Option<bool>,
    bracket_depth: usize,
}

enum HighlightCommand {
    HighlightChar { highlight: Style, reset: Style },
    ChangeColour(Style),
    Reset,
    ResetAfter,
    Continue,
}

impl HighlighterState {
    const FIRST_COMMAND: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
    const QUOTE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
    const OPTIONAL_ARG: Style = Style::new().dimmed();
    const PIPE_COLOUR: Style = Style::new();

    fn begin() -> Self {
        let mut this = Self::default();
        this.start_new_command();
        this
    }

    fn start_new_command(&mut self) -> Style {
        self.is_command_name = Some(false);
        self.current_colour = Self::FIRST_COMMAND;
        Self::FIRST_COMMAND
    }

    fn change_colour(&mut self, colour: Style) -> Style {
        self.current_colour = colour;
        colour
    }

    fn advance_state(&mut self, ch: char) -> HighlightCommand {
        match ch {
            '|' if !self.is_double_quote || !self.is_single_quote => {
                HighlightCommand::HighlightChar {
                    highlight: Self::PIPE_COLOUR,
                    reset: self.start_new_command(),
                }
            }
            '"' if self.is_double_quote || !self.is_single_quote => {
                if self.is_double_quote {
                    self.is_double_quote = false;
                    HighlightCommand::ResetAfter
                } else {
                    self.is_double_quote = true;
                    HighlightCommand::ChangeColour(Self::QUOTE)
                }
            }
            '\'' if self.is_single_quote || !self.is_double_quote => {
                if self.is_single_quote {
                    self.is_single_quote = false;
                    HighlightCommand::ResetAfter
                } else {
                    self.is_single_quote = true;
                    HighlightCommand::ChangeColour(Self::QUOTE)
                }
            }
            '-' if !self.is_double_quote
                && !self.is_single_quote
                && !self.is_command_name.is_some() =>
            {
                HighlightCommand::ChangeColour(self.change_colour(Self::OPTIONAL_ARG))
            }
            w if w.is_whitespace()
                && !self.is_double_quote
                && !self.is_single_quote
                && self.is_command_name != Some(false) =>
            {
                self.is_command_name = None;
                HighlightCommand::Reset
            }
            _ => {
                if let Some(encountered) = &mut self.is_command_name {
                    *encountered = true
                };
                HighlightCommand::Continue
            }
        }
    }
}

impl Highlighter for LineHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let mut state = HighlighterState::begin();

        const RESET: &str = "\x1b[0m";

        let mut buf = state.current_colour.to_string();

        for ch in line.chars() {
            match state.advance_state(ch) {
                HighlightCommand::HighlightChar { highlight, reset } => {
                    buf.push_str(RESET);
                    let _ = write!(buf, "{}", highlight);
                    buf.push(ch);
                    buf.push_str(RESET);
                    let _ = write!(buf, "{}", reset);
                }
                HighlightCommand::ChangeColour(style) => {
                    buf.push_str(RESET);
                    let _ = write!(buf, "{}", style);
                    buf.push(ch);
                }
                HighlightCommand::Reset => {
                    buf.push_str(RESET);
                    buf.push(ch);
                }
                HighlightCommand::ResetAfter => {
                    buf.push(ch);
                    buf.push_str(RESET);
                }
                HighlightCommand::Continue => {
                    buf.push(ch);
                }
            }
        }

        buf.push_str(RESET);

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
