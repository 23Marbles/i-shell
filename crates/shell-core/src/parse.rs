pub mod token;

use std::ops::Range;

use crate::parse::token::Pipeline;

pub trait ParseToken<'a>: Sized {
    fn parse_token(parser: &mut Parser<'a>) -> Result<Self, ParseError>;
}
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("the range specified was empty")]
    EmptyRange,

    #[error("expected char '{ch}': {reason}")]
    ExpectedChar { ch: char, reason: String },

    #[error("attempted to parse past the end of the line")]
    UnexpectedEof,
    #[error("did not expect char '{ch}': {reason}")]
    UnexpectedChar { ch: char, reason: String },

    #[error("the range indexed (`{}..{}`) is out of bounds (bounds: `{}..{}`)", range.start, range.end, bounds.start, bounds.end)]
    RangeOutOfBounds {
        range: Range<usize>,

        bounds: Range<usize>,
    },
}

#[derive(Debug, Clone)]
pub struct Parser<'a> {
    pos: usize,

    source: &'a str,
}

impl<'a> Parser<'a> {
    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn expect(&mut self, ch: char, reason: String) -> Result<(), ParseError> {
        (self.bump().ok_or(ParseError::UnexpectedEof)? == ch)
            .ok_or(ParseError::ExpectedChar { ch, reason })
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }
    fn peek_past_whitespace(&self) -> Option<char> {
        let mut clone = self.clone();
        clone.skip_whitespace();
        clone.peek()
    }

    #[must_use]
    fn consume_while(&mut self, mut predicate: impl FnMut(char) -> bool) -> Range<usize> {
        let start = self.pos;

        while let Some(ch) = self.peek() {
            if !predicate(ch) {
                break;
            }

            self.bump();
        }

        start..self.pos
    }
    #[must_use]
    fn consume_until(&mut self, ch: char) -> Option<Range<usize>> {
        let start = self.pos;
        let end = self.source[start..].find(ch)? + start;

        self.pos = end + ch.len_utf8();

        Some(start..end)
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { pos: 0, source }
    }

    pub fn parse(&mut self) -> Result<Pipeline<'a>, ParseError> {
        Pipeline::parse_token(self)
    }
}
