mod display_impls {
    use super::*;

    impl Display for Part<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self.kind {
                TokenPartKind::Simple => f.write_str(self.lexme),
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
            }
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

    impl Display for Pipe {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_char('|')
        }
    }

    impl Display for CommandChain<'_> {
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

    impl Display for LocationPart<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self.kind {
                TokenPartKind::Simple => f.write_str(self.lexme),
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
            }
        }
    }

    impl Display for Location<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.sock.fmt(f)?;

            if let Some(p) = &self.path {
                f.write_char('@')?;
                p.fmt(f)?;
            }

            Ok(())
        }
    }

    impl Display for LocatedChain<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_char('[')?;

            self.location.fmt(f)?;

            f.write_char(':')?;

            self.chain.fmt(f)?;

            f.write_char(']')?;

            Ok(())
        }
    }

    impl Display for Chain<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Chain::LocatedChain(located_chain) => located_chain.fmt(f),
                Chain::CommandChain(command_chain) => command_chain.fmt(f),
            }
        }
    }

    impl Display for Pipeline<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.first_chain.fmt(f)?;

            for (pipe, chain) in &self.opt_chains {
                f.write_char(' ')?;
                pipe.fmt(f)?;
                f.write_char(' ')?;
                chain.fmt(f)?;
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

pub enum Chain<'a> {
    LocatedChain(LocatedChain<'a>),

    CommandChain(CommandChain<'a>),
}

impl<'a> ParseToken<'a> for Chain<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        if parser.peek().ok_or(ParseError::UnexpectedEof)? == '[' {
            Ok(Self::LocatedChain(LocatedChain::parse_token(parser)?))
        } else {
            Ok(Self::CommandChain(CommandChain::parse_token(parser)?))
        }
    }
}

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
                '|' | '[' | ']' => break,
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

pub struct Pipe {
    span: Range<usize>,
}

impl ParseToken<'_> for Pipe {
    fn parse_token(parser: &mut super::Parser<'_>) -> Result<Self, ParseError> {
        let start = parser.pos;

        if parser.bump() != Some('|') {
            return Err(ParseError::ExpectedChar {
                ch: '|',
                reason: "expected `|` at the start of a pipe invocation".to_string(),
            });
        }

        Ok(Self {
            span: start..parser.pos,
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
            '[' => Err(ParseError::UnexpectedChar {
                ch: '[',
                reason: "`[` is the start of a located chain and cannot be part of a simple, unquoted arg"
                    .to_string(),
            }),
            ']' => Err(ParseError::UnexpectedChar {
                ch: ']',
                reason: "`]` is the end of a located chain and cannot be part of a simple, unquoted arg"
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
                let mut end = parser.pos;

                while let Some(ch) = parser.peek() {
                    if ch.is_whitespace()
                        || matches!(ch, '@' | ':' | '[' | ']' | '|')
                    {
                        break;
                    }

                    parser.bump();
                    end = parser.pos;
                }

                let range = start..end;

                Ok(Self {
                    kind: TokenPartKind::Simple,
                    span: range.clone(),
                    lexme: &parser.source[range],
                })
            }
        }
    }
}

pub struct Pipeline<'a> {
    opt_chains: Vec<(Pipe, Chain<'a>)>,

    pub first_chain: Chain<'a>,
}

impl<'a> ParseToken<'a> for Pipeline<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let first_chain = Chain::parse_token(parser)?;

        let mut opt_chains = Vec::new();

        while let Some(ch) = parser.peek_past_whitspace() {
            if ch == '|' {
                parser.skip_whitespace();
                let pipe = Pipe::parse_token(parser)?;
                parser.skip_whitespace();
                let chain = Chain::parse_token(parser)?;
                opt_chains.push((pipe, chain));
            } else {
                return Err(ParseError::ExpectedChar {
                    ch: '|',
                    reason: "Expected pipe before next chain".to_string(),
                });
            }
        }

        Ok(Self {
            opt_chains,
            first_chain,
        })
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
                None | Some('|') | Some('[') | Some(']') => break,
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

pub struct CommandChain<'a> {
    span: Range<usize>,

    pub opt_commands: Vec<(Pipe, Command<'a>)>,

    pub first_command: Command<'a>,
}

impl<'a> ParseToken<'a> for CommandChain<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let start = parser.pos;
        let first_command = Command::parse_token(parser)?;

        let mut opt_commands = Vec::new();

        while let Some(ch) = parser.peek_past_whitspace() {
            match ch {
                '[' | ']' => break,
                '|' => {
                    let start = parser.pos;
                    parser.skip_whitespace();
                    let pipe = Pipe::parse_token(parser)?;
                    parser.skip_whitespace();
                    match parser.peek() {
                        Some('[' | ']') => {
                            parser.pos = start;
                            break;
                        }
                        Some(_) => {}
                        None => return Err(ParseError::UnexpectedEof),
                    }
                    let cmd = Command::parse_token(parser)?;
                    opt_commands.push((pipe, cmd));
                }
                _ => {
                    return Err(ParseError::ExpectedChar {
                        ch: '|',
                        reason: "commands must have a pipe or other terminating character following them".to_string(),
                    });
                }
            }
        }

        Ok(Self {
            span: start..parser.pos,
            opt_commands,
            first_command,
        })
    }
}

pub struct Location<'a> {
    span: Range<usize>,

    pub sock: LocationPart<'a>,

    pub path: Option<LocationPart<'a>>,
}

impl<'a> ParseToken<'a> for Location<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let start = parser.pos;

        let sock = LocationPart::parse_token(parser)?;
        parser.skip_whitespace();

        let path = match parser.peek() {
            Some('@') => {
                parser.bump();
                parser.skip_whitespace();
                Some(LocationPart::parse_token(parser)?)
            }
            Some(':') => None,
            Some(ch) => {
                return Err(ParseError::UnexpectedChar {
                    ch,
                    reason: "the first location must have a `:` after it or indicate a second location with `@`".to_string(),
                });
            }
            None => return Err(ParseError::UnexpectedEof),
        };

        parser.expect(
            ':',
            "locations must end with a `:` to indicate the command follows".to_string(),
        )?;

        Ok(Self {
            span: start..parser.pos,
            sock,
            path,
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
                if first.is_whitespace() || matches!(first, '@' | ':') {
                    return Err(ParseError::UnexpectedChar {
                        ch: first,
                        reason: "expected a path or IP location part".to_string(),
                    });
                }

                let mut end = parser.pos;

                while let Some(ch) = parser.peek() {
                    if ch.is_whitespace() || matches!(ch, '@' | ':' | '[' | ']' | '|') {
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

pub struct LocatedChain<'a> {
    span: Range<usize>,

    chain: CommandChain<'a>,

    location: Location<'a>,
}

impl<'a> ParseToken<'a> for LocatedChain<'a> {
    fn parse_token(parser: &mut super::Parser<'a>) -> Result<Self, ParseError> {
        let start = parser.pos;

        parser.expect(
            '[',
            "Expected '[' at the start of located chain".to_string(),
        )?;

        parser.skip_whitespace();

        let location = Location::parse_token(parser)?;

        parser.skip_whitespace();

        let chain = CommandChain::parse_token(parser)?;

        parser.skip_whitespace();

        parser.expect(']', "Expected ']' at the end of located chain".to_string())?;

        Ok(Self {
            span: start..parser.pos,
            location,
            chain,
        })
    }
}
#[cfg(test)]
mod tests {
    use crate::parse::token::CommandChain;
    use crate::parse::{ParseToken, Parser};

    #[test]
    fn parses_simple_command_with_quoted_args() {
        let mut parser = Parser::new("echo \"hello world\" | grep -i 'there'");

        let pipeline = parser.parse().unwrap();

        assert_eq!(parser.pos, "echo \"hello world\" | grep -i 'there'".len());
        assert_eq!(
            pipeline.first_chain.to_string(),
            "echo \"hello world\" | grep -i 'there'"
        );
    }

    #[test]
    fn command_chain_stops_before_closing_bracket() {
        let mut parser = Parser::new("echo hello]");

        let chain = CommandChain::parse_token(&mut parser).unwrap();

        assert_eq!(chain.first_command.to_string(), "echo hello");
        assert_eq!(parser.peek(), Some(']'));
    }

    #[test]
    fn parses_located_chain_and_consumes_the_delimiter() {
        let mut parser = Parser::new("[127.0.0.1: echo hello]");

        let pipeline = parser.parse().unwrap();

        assert_eq!(parser.pos, "[127.0.0.1: echo hello]".len());
        assert_eq!(pipeline.first_chain.to_string(), "[127.0.0.1:echo hello]");
    }
}
