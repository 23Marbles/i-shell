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
    /// None = Not currently expected;
    /// Some(false) = Expected but not encountered;
    /// Some(true) = Expected and encountered;
    is_command_name: Option<bool>,
    /// None = Not currently in chain;
    /// Some(false) = In chain but has not started commands yet;
    /// Some(true) = In chain;
    chain: Option<bool>,

    char_queue: Vec<FutureCommand>,
}

type Predicate = fn(char) -> bool;
type StateCmd = fn(&mut HighlighterState) -> HighlightCommand;

#[derive(Debug)]
enum FutureCommand {
    Single {
        predicate: fn(char) -> bool,
        command: fn(&mut HighlighterState) -> HighlightCommand,
    },
    Options(
        Vec<(
            fn(char) -> bool,
            fn(&mut HighlighterState) -> HighlightCommand,
        )>,
    ),
}

impl FutureCommand {
    fn new(
        predicate: fn(char) -> bool,
        command: fn(&mut HighlighterState) -> HighlightCommand,
    ) -> Self {
        Self::Single { predicate, command }
    }

    fn add_option(
        self,
        predicate: fn(char) -> bool,
        command: fn(&mut HighlighterState) -> HighlightCommand,
    ) -> Self {
        match self {
            FutureCommand::Single {
                predicate: predicate1,
                command: command1,
            } => Self::Options(vec![(predicate1, command1), (predicate, command)]),
            FutureCommand::Options(mut items) => {
                items.push((predicate, command));
                FutureCommand::Options(items)
            }
        }
    }

    fn find_match(&self, ch: char) -> Option<fn(&mut HighlighterState) -> HighlightCommand> {
        match self {
            FutureCommand::Single { predicate, command } => {
                if predicate(ch) {
                    Some(*command)
                } else {
                    None
                }
            }
            FutureCommand::Options(items) => items.iter().find(|(p, _)| p(ch)).map(|(_, c)| *c),
        }
    }
}

#[derive(Debug)]
enum HighlightCommand {
    HighlightChar { highlight: Style, reset: Style },
    HighlightCharReset { highlight: Style },
    ChangeColour(Style),
    Reset,
    ResetAfter,
    Continue,
}

impl HighlighterState {
    const COMMAND_NAME: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
    const QUOTE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
    const OPTIONAL_ARG: Style = Style::new().dimmed();
    const PIPE: Style = Style::new();
    const BRACKET: Style = Style::new();
    const LOCATION: Style = Style::new().underline();
    const ERROR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));

    fn handle_pipe(&mut self) -> HighlightCommand {
        HighlightCommand::HighlightChar {
            highlight: Self::PIPE,
            reset: self.start_new_command(),
        }
    }

    fn handle_command_name(&mut self) -> HighlightCommand {
        self
    }

    fn handle_open_bracket(&mut self) -> HighlightCommand {
        self.char_queue.push((,));
    }

    fn handle_arg_whitespace(&mut self) -> HighlightCommand {
        {
            self.char_queue.push(FutureCommand::new(
                |ch| ch == '|',
                HighlighterState::handle_pipe,
            ).add_option(|w| w.is_whitespace(), |_| HighlightCommand::Reset).add_option(|ch| ch == '[', Self::handle_open_bracket));
            HighlightCommand::Continue
        }
    }

    fn begin() -> Self {
        let mut this = Self::default();
        this.start_new_command();
        this
    }

    fn start_new_command(&mut self) -> Style {
        self.current_colour = Self::COMMAND_NAME;
        Self::COMMAND_NAME
    }

    fn change_colour(&mut self, colour: Style) -> Style {
        self.current_colour = colour;
        colour
    }

    fn reset_style(&mut self) {
        self.current_colour = Style::new()
    }

    fn advance_state(&mut self, ch: char) -> HighlightCommand {
        if let Some(command) = {
            let mut command = None;

            let _ = self.char_queue.pop_if(|item| {
                command = item.find_match(ch);
                command.is_some()
            });

            command
        } {
            return command(self);
        }

        if !self.char_queue.is_empty() {
            return HighlightCommand::Continue;
        }

        match ch {
            '"' => {
                self.char_queue.push(FutureCommand::new(
                    |c| c == '"',
                    |_| HighlightCommand::ResetAfter,
                ));
                HighlightCommand::ChangeColour(Self::QUOTE)
            }
            '\'' => {
                self.char_queue.push(FutureCommand::new(
                    |c| c == '\'',
                    |_| HighlightCommand::ResetAfter,
                ));
                HighlightCommand::ChangeColour(Self::QUOTE)
            }
            ']' if !(self.is_double_quote || self.is_single_quote) => {
                self.reset_style();
                self.chain = None;
                self.is_command_name = None;
                HighlightCommand::HighlightCharReset {
                    highlight: Self::BRACKET,
                }
            }
            '[' => {
                self.current_colour = Self::LOCATION;
                self.chain = Some(false);
                self.is_command_name = None;
                HighlightCommand::HighlightChar {
                    highlight,
                    reset: Self::LOCATION,
                }
            }
            ':' if !(self.is_double_quote || self.is_single_quote) => {
                let highlight = if matches!((self.chain, self.is_command_name), (None, Some(false)))
                {
                    Self::BRACKET
                } else {
                    Self::ERROR
                };
                self.chain = Some(true);
                HighlightCommand::HighlightChar {
                    highlight,
                    reset: self.start_new_command(),
                }
            }
            '|' if !(self.is_double_quote || self.is_single_quote) => {
                HighlightCommand::HighlightChar {
                    highlight: Self::PIPE,
                    reset: self.start_new_command(),
                }
            }
            '-' if !self.is_double_quote
                && !self.is_single_quote
                && !self.is_command_name.is_some() =>
            {
                HighlightCommand::ChangeColour(self.change_colour(Self::OPTIONAL_ARG))
            }
            w if w.is_whitespace() && self.is_command_name != Some(false) => {
                HighlightCommand::Reset
            }
            _ => {
                if let Some(encountered) = &mut self.is_command_name {
                    if !*encountered {
                        *encountered = true;
                        self.char_queue
                            .push(FutureCommand::new(|ch| ch.is_whitespace(), |this| th));
                        self.current_colour = Self::COMMAND_NAME;
                        return HighlightCommand::ChangeColour(Self::COMMAND_NAME);
                    }
                }
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
                HighlightCommand::HighlightCharReset { highlight } => {
                    buf.push_str(RESET);
                    let _ = write!(buf, "{}", highlight);
                    buf.push(ch);
                    buf.push_str(RESET);
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
