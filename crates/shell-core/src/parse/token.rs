mod display_impls {
    use super::*;

    impl Display for LocationPart<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self.kind {
                TokenPartKind::SingleQuote => {
                    f.write_char('\'')?;
                    f.write_str(self.lexme)?;
                    f.write_char('\'')
                }
                TokenPartKind::DoubleQuote => {
                    f.write_char('"')?;
                    f.write_str(self.lexme)?;
                    f.write_char('"')
                }
                TokenPartKind::Simple => f.write_str(self.lexme),
            }
        }
    }

    impl Display for Location<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.location.fmt(f)?;

            if let Some(loc) = &self.opt_location {
                f.write_char('@')?;
                loc.fmt(f)?;
            }

            Ok(())
        }
    }

    impl Display for Pipeline<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.first_command.fmt(f)?;

            for (pipe, cmd) in &self.opt_commands {
                f.write_char(' ')?;
                pipe.fmt(f)?;
                f.write_char(' ')?;
                cmd.fmt(f)?;
            }

            Ok(())
        }
    }

    impl Display for Part<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self.kind {
                TokenPartKind::SingleQuote => {
                    f.write_char('\'')?;
                    f.write_str(self.lexme)?;
                    f.write_char('\'')?;
                }

                TokenPartKind::DoubleQuote => {
                    f.write_char('"')?;
                    f.write_str(self.lexme)?;
                    f.write_char('"')?;
                }

                TokenPartKind::Simple => f.write_str(self.lexme)?,
            }

            Ok(())
        }
    }

    impl Display for Pipe<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_char('|')?;

            if let Some(loc) = &self.location {
                loc.fmt(f)?;
                f.write_char('>')?;
            }

            Ok(())
        }
    }

    impl Display for Arg<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.part.fmt(f)?;

            for part in &self.opt_parts {
                part.fmt(f)?;
            }

            Ok(())
        }
    }

    impl Display for Command<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.name.fmt(f)?;

            for arg in &self.args {
                f.write_char(' ')?;

                arg.fmt(f)?;
            }

            Ok(())
        }
    }
}

use std::{
    fmt::{Display, Write},
    ops::Range,
};

use crate::parse::{ParseError, ParseToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPartKind {
    Simple,

    SingleQuote,

    DoubleQuote,
}

pub struct Arg<'a> {
    span: Range<usize>,

    part: Part<'a>,

    opt_parts: Vec<Part<'a>>,
}

impl Arg<'_> {
    pub fn raw_to_string(&self) -> String {
        let mut string = String::new();

        string.push_str(self.part.lexme);

        for part in &self.opt_parts {
            string.push_str(part.lexme);
        }

        string
    }
}

impl<'a> ParseToken<'a> for Arg<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let start = parser.pos;

        let part = Part::parse_token(parser)?;

        let mut opt_parts = Vec::new();

        while let Some(ch) = parser.peek() {
            match ch {
                '|' => break,
                w if w.is_whitespace() => break,
                _ => {
                    opt_parts.push(Part::parse_token(parser)?);
                }
            }
        }

        Ok(Self {
            span: start..parser.pos,
            part,
            opt_parts,
        })
    }
}

pub struct Pipe<'a> {
    span: Range<usize>,

    location: Option<Location<'a>>,
}

impl<'a> ParseToken<'a> for Pipe<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let start = parser.pos;

        if parser.bump() != Some('|') {
            return Err(ParseError::ExpectedChar {
                ch: '|',
                reason: "expected `|` at the start of a pipe invocation".to_string(),
            });
        }

        parser.skip_whitespace();
        let location = if parser.has_location_terminator() {
            Some(Location::parse_token(parser)?)
        } else {
            None
        };

        Ok(Self {
            span: start..parser.pos,
            location,
        })
    }
}

pub struct Part<'a> {
    kind: TokenPartKind,

    span: Range<usize>,

    pub lexme: &'a str,
}

impl<'a> ParseToken<'a> for Part<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        match parser.peek().ok_or(ParseError::UnexpectedEof)? {
            '|' => Err(ParseError::UnexpectedChar {
                ch: '|',
                reason: "`|` is the start of a pipe and cannot be part of a simple, unquoted arg"
                    .to_string(),
            }),
            w if w.is_whitespace() => Err(ParseError::UnexpectedChar {
                ch: w,
                reason: "cannot have whitespace within a command".to_string(),
            }),
            '\'' => {
                let start = parser.pos;
                parser.bump();
                let content = parser.consume_until('\'').ok_or(ParseError::ExpectedChar {
                    ch: '\'',
                    reason: "expected closing quote".to_string(),
                })?;

                Ok(Self {
                    kind: TokenPartKind::SingleQuote,
                    span: start..parser.pos,
                    lexme: &parser.source[content],
                })
            }
            '"' => {
                let start = parser.pos;
                parser.bump();
                let content = parser.consume_until('"').ok_or(ParseError::ExpectedChar {
                    ch: '"',
                    reason: "expected closing quote".to_string(),
                })?;

                Ok(Self {
                    kind: TokenPartKind::DoubleQuote,
                    span: start..parser.pos,
                    lexme: &parser.source[content],
                })
            }
            _ => {
                let start = parser.pos;
                let _ = parser.consume_while(|ch| {
                    !ch.is_whitespace() && ch != '|' && ch != '"' && ch != '\''
                });

                Ok(Self {
                    kind: TokenPartKind::Simple,
                    span: start..parser.pos,
                    lexme: &parser.source[start..parser.pos],
                })
            }
        }
    }
}

pub struct Command<'a> {
    span: Range<usize>,

    pub name: Part<'a>,

    pub args: Vec<Arg<'a>>,
}

impl<'a> ParseToken<'a> for Command<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        parser.skip_whitespace();

        let start = parser.pos;

        let name = Part::parse_token(parser)?;

        let mut args = Vec::new();

        loop {
            parser.skip_whitespace();

            match parser.peek() {
                None | Some('|') => break,
                Some(_) => args.push(Arg::parse_token(parser)?),
            }
        }

        Ok(Self {
            span: start..parser.pos,
            name,
            args,
        })
    }
}

pub struct Pipeline<'a> {
    opt_commands: Vec<(Pipe<'a>, Command<'a>)>,

    pub first_command: Command<'a>,
}

impl<'a> ParseToken<'a> for Pipeline<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let first_command = Command::parse_token(parser)?;

        let mut opt_commands = Vec::new();

        loop {
            parser.skip_whitespace();

            if parser.peek().is_none() {
                break;
            }

            let pipe = Pipe::parse_token(parser)?;
            parser.skip_whitespace();
            let cmd = Command::parse_token(parser)?;

            opt_commands.push((pipe, cmd));
        }

        Ok(Self {
            opt_commands,
            first_command,
        })
    }
}

pub struct Location<'a> {
    span: Range<usize>,

    location: LocationPart<'a>,

    opt_location: Option<LocationPart<'a>>,
}

impl<'a> ParseToken<'a> for Location<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let start = parser.pos;

        let location = LocationPart::parse_token(parser)?;
        parser.skip_whitespace();

        let opt_location = match parser.peek() {
            Some('@') => {
                parser.bump();
                parser.skip_whitespace();
                Some(LocationPart::parse_token(parser)?)
            }
            Some('>') => None,
            Some(ch) => {
                return Err(ParseError::UnexpectedChar {
                    ch,
                    reason: "the first location must have a `>` after it or indicate a second location with `@`".to_string(),
                });
            }
            None => return Err(ParseError::UnexpectedEof),
        };

        parser.skip_whitespace();
        if parser.bump() != Some('>') {
            return Err(ParseError::ExpectedChar {
                ch: '>',
                reason: "locations must end with a `>` to indicate the command follows".to_string(),
            });
        }

        Ok(Self {
            span: start..parser.pos,
            location,
            opt_location,
        })
    }
}

pub struct LocationPart<'a> {
    span: Range<usize>,

    kind: TokenPartKind,

    pub lexme: &'a str,
}

impl<'a> ParseToken<'a> for LocationPart<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let start = parser.pos;

        match parser.bump().ok_or(ParseError::UnexpectedEof)? {
            '\'' => {
                let range = parser.consume_until('\'').ok_or(ParseError::ExpectedChar {
                    ch: '\'',
                    reason: "Location part requires closing '".to_string(),
                })?;

                if range.is_empty() {
                    return Err(ParseError::EmptyRange);
                }

                Ok(LocationPart {
                    kind: TokenPartKind::SingleQuote,
                    lexme: &parser.source[range.clone()],
                    span: start..parser.pos,
                })
            }
            '"' => {
                let range = parser.consume_until('"').ok_or(ParseError::ExpectedChar {
                    ch: '"',
                    reason: "Location part requires closing \"".to_string(),
                })?;

                if range.is_empty() {
                    return Err(ParseError::EmptyRange);
                }

                Ok(LocationPart {
                    kind: TokenPartKind::DoubleQuote,
                    lexme: &parser.source[range.clone()],
                    span: start..parser.pos,
                })
            }
            first => {
                if first.is_whitespace() || matches!(first, '@' | '>') {
                    return Err(ParseError::UnexpectedChar {
                        ch: first,
                        reason: "expected a path or IP location part".to_string(),
                    });
                }

                let mut end = parser.pos;

                while let Some(ch) = parser.peek() {
                    if ch.is_whitespace() || ch == '@' || ch == '>' {
                        break;
                    }

                    parser.bump();
                    end = parser.pos;
                }

                let range = start..end;

                Ok(LocationPart {
                    kind: TokenPartKind::Simple,
                    lexme: &parser.source[range.clone()],
                    span: range,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Parser;

    #[test]
    fn parses_command_and_trailing_whitespace() {
        let mut parser = Parser::new("echo hello  \t");
        let pipeline = parser.parse().unwrap();

        assert_eq!(pipeline.first_command.name.lexme, "echo");
        assert_eq!(pipeline.first_command.args.len(), 1);
        assert_eq!(pipeline.first_command.args[0].raw_to_string(), "hello");
        assert_eq!(pipeline.to_string(), "echo hello");
    }

    #[test]
    fn parses_quoted_and_concatenated_argument_parts() {
        let mut parser = Parser::new("echo 'two words' pre\"猫\"");
        let pipeline = parser.parse().unwrap();

        let args = &pipeline.first_command.args;
        assert_eq!(args[0].raw_to_string(), "two words");
        assert_eq!(args[1].raw_to_string(), "pre猫");
        assert_eq!(args[1].part.kind, TokenPartKind::Simple);
        assert_eq!(args[1].opt_parts[0].kind, TokenPartKind::DoubleQuote);
    }

    #[test]
    fn parses_pipeline_without_location() {
        let mut parser = Parser::new("echo one | cat two");
        let pipeline = parser.parse().unwrap();

        assert_eq!(pipeline.opt_commands.len(), 1);
        assert_eq!(pipeline.opt_commands[0].1.name.lexme, "cat");
        assert_eq!(pipeline.to_string(), "echo one | cat two");
    }

    #[test]
    fn parses_three_command_pipeline_in_order() {
        let mut parser = Parser::new("echo first | tr a-z A-Z | wc -c");
        let pipeline = parser.parse().unwrap();

        assert_eq!(pipeline.first_command.name.lexme, "echo");
        assert_eq!(
            pipeline
                .first_command
                .args
                .iter()
                .map(Arg::raw_to_string)
                .collect::<Vec<_>>(),
            ["first"]
        );
        assert_eq!(pipeline.opt_commands.len(), 2);
        assert_eq!(pipeline.opt_commands[0].1.name.lexme, "tr");
        assert_eq!(
            pipeline.opt_commands[0]
                .1
                .args
                .iter()
                .map(Arg::raw_to_string)
                .collect::<Vec<_>>(),
            ["a-z", "A-Z"]
        );
        assert_eq!(pipeline.opt_commands[1].1.name.lexme, "wc");
        assert_eq!(
            pipeline.opt_commands[1]
                .1
                .args
                .iter()
                .map(Arg::raw_to_string)
                .collect::<Vec<_>>(),
            ["-c"]
        );
        assert_eq!(pipeline.to_string(), "echo first | tr a-z A-Z | wc -c");
    }

    #[test]
    fn parses_three_command_pipeline_with_middle_remote_location() {
        let mut parser = Parser::new("one |/tmp@host> two | three");
        let pipeline = parser.parse().unwrap();

        assert_eq!(pipeline.opt_commands.len(), 2);

        let (located_pipe, second_command) = &pipeline.opt_commands[0];
        let location = located_pipe.location.as_ref().unwrap();
        assert_eq!(location.location.lexme, "/tmp");
        assert_eq!(location.opt_location.as_ref().unwrap().lexme, "host");
        assert_eq!(second_command.name.lexme, "two");

        let (plain_pipe, third_command) = &pipeline.opt_commands[1];
        assert!(plain_pipe.location.is_none());
        assert_eq!(third_command.name.lexme, "three");
        assert_eq!(pipeline.to_string(), "one |/tmp@host> two | three");
    }

    #[test]
    fn parses_three_command_pipeline_with_quoted_locations_and_spacing() {
        let mut parser = Parser::new("one  | 'first path' > two\t|\"second path\"> three");
        let pipeline = parser.parse().unwrap();

        assert_eq!(pipeline.opt_commands.len(), 2);
        assert_eq!(
            pipeline.opt_commands[0]
                .0
                .location
                .as_ref()
                .unwrap()
                .location
                .lexme,
            "first path"
        );
        assert_eq!(
            pipeline.opt_commands[1]
                .0
                .location
                .as_ref()
                .unwrap()
                .location
                .lexme,
            "second path"
        );
        assert_eq!(pipeline.opt_commands[0].1.name.lexme, "two");
        assert_eq!(pipeline.opt_commands[1].1.name.lexme, "three");
        assert_eq!(
            pipeline.to_string(),
            "one |'first path'> two |\"second path\"> three"
        );
    }

    #[test]
    fn parses_pipeline_with_quoted_location_and_ip() {
        let mut parser = Parser::new("echo |'path with spaces'@host> cat");
        let pipeline = parser.parse().unwrap();

        let (pipe, command) = &pipeline.opt_commands[0];
        let location = pipe.location.as_ref().unwrap();
        assert_eq!(location.location.lexme, "path with spaces");
        assert_eq!(location.location.kind, TokenPartKind::SingleQuote);
        assert_eq!(location.opt_location.as_ref().unwrap().lexme, "host");
        assert_eq!(command.name.lexme, "cat");
        assert_eq!(pipeline.to_string(), "echo |'path with spaces'@host> cat");
    }

    #[test]
    fn reports_unterminated_quoted_argument() {
        let mut parser = Parser::new("echo 'unfinished");

        assert!(matches!(
            parser.parse(),
            Err(ParseError::ExpectedChar { ch: '\'', .. })
        ));
    }

    #[test]
    fn rejects_location_without_terminating_greater_than() {
        let mut parser = Parser::new("/tmp@host cat");

        assert!(matches!(
            Location::parse_token(&mut parser),
            Err(ParseError::ExpectedChar { ch: '>', .. })
        ));
    }
}
