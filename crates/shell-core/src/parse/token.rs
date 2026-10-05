mod display_impls {
    use super::*;

    impl Display for LocationPart<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.lexme)
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

        let loc_res = Location::parse_token(parser);

        if parser.bump().ok_or(ParseError::UnexpectedEof)? == '>' {
            Ok(Self {
                span: start..parser.pos,
                location: Some(loc_res?),
            })
        } else {
            parser.pos = start;

            parser.skip_whitespace();

            if parser.peek() == Some('>') {
                parser.bump();
            }

            Ok(Self {
                span: start..parser.pos,
                location: None,
            })
        }
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
                parser.consume_until('\'').ok_or(ParseError::ExpectedChar {
                    ch: '\'',
                    reason: "expected closing quote".to_string(),
                })?;
                let end = parser.pos;
                parser.bump();

                Ok(Self {
                    kind: TokenPartKind::SingleQuote,
                    span: start..parser.pos,
                    lexme: &parser.source[start + 1..end],
                })
            }
            '"' => {
                let start = parser.pos;
                parser.bump();
                parser.consume_until('"').ok_or(ParseError::ExpectedChar {
                    ch: '"',
                    reason: "expected closing quote".to_string(),
                })?;
                let end = parser.pos;
                parser.bump();

                Ok(Self {
                    kind: TokenPartKind::SingleQuote,
                    span: start..parser.pos,
                    lexme: &parser.source[start + 1..end],
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

        let mut end = parser.pos;

        while let Some(ch) = parser.peek() {
            parser.skip_whitespace();

            if ch == '|' {
                break;
            }

            args.push(Arg::parse_token(parser)?);

            end = parser.pos;
        }

        Ok(Self {
            span: start..end,
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

        while parser.peek().is_some() {
            let pipe = Pipe::parse_token(parser)?;
            parser.skip_whitespace();
            let cmd = Command::parse_token(parser)?;
            parser.skip_whitespace();

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
            Some('>') => {
                parser.bump();
                None
            }
            Some(ch) => {
                return Err(ParseError::UnexpectedChar {
                    ch,
                    reason: "the first location must have a `>` after it or indicate a second location with `@`".to_string(),
                });
            }
            None => return Err(ParseError::UnexpectedEof),
        };

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
                    span: range,
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
                    span: range,
                })
            }
            _ => {
                let mut end = parser.pos;

                while let Some(ch) = parser.peek() {
                    if ch.is_whitespace() || ch == '@' || ch == '>' {
                        break;
                    }

                    parser.bump();
                    end = parser.pos;
                }

                let range = start..end;

                if range.is_empty() {
                    return Err(ParseError::EmptyRange);
                }

                Ok(LocationPart {
                    kind: TokenPartKind::Simple,
                    lexme: &parser.source[range.clone()],
                    span: range,
                })
            }
        }
    }
}

fn get_source_span(source: &str, range: Range<usize>) -> Result<&str, ParseError> {
    let part = source
        .get(range.clone())
        .ok_or(ParseError::RangeOutOfBounds {
            range: range.clone(),
            bounds: 0..source.len(),
        })?;

    if part.is_empty() {
        return Err(ParseError::EmptyRange);
    };

    Ok(part)
}
