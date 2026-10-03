use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Name,
    Arg,
    SingleQuoted,
    DoubleQuoted,
    Pipe,
    RightAngle,
}

impl TokenKind {
    pub fn colour(&self) -> &'static str {
        match self {
            TokenKind::Name => todo!(),
            TokenKind::Arg => todo!(),
            TokenKind::SingleQuoted => todo!(),
            TokenKind::DoubleQuoted => todo!(),
            TokenKind::Pipe => todo!(),
            TokenKind::RightAngle => todo!(),
        }
    }
}

pub struct ScanError {
    source: String,
    span: Range<usize>,
    kind: ScanErrorKind,
}

pub enum ScanErrorKind {
    UnclosedSingleQuote,
    UnclosedDoubleQuote,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub lexeme: &'a str,
    pub span: Range<usize>,
}

pub struct Scanner<'a> {
    source: &'a str,
    start: usize,
    current: usize,
    had_name: bool,
}

impl<'a> Scanner<'a> {
    fn error(&self, kind: ScanErrorKind) -> ScanError {
        ScanError {
            source: self.source.to_owned(),
            span: self.start..self.current,
            kind,
        }
    }

    fn error_with_span(&self, kind: ScanErrorKind, span: Range<usize>) -> ScanError {
        ScanError {
            source: self.source.to_owned(),
            span,
            kind,
        }
    }

    fn lexeme_with_bounds(&self, start: usize, end: usize) -> &'a str {
        &self.source[start..end]
    }

    fn make_token(&self, kind: TokenKind) -> Token<'a> {
        self.make_token_with_slice(kind, self.start, self.current)
    }

    fn make_token_with_slice(&self, kind: TokenKind, start: usize, end: usize) -> Token<'a> {
        Token {
            kind,
            lexeme: self.lexeme_with_bounds(start, end),
            span: start..end,
        }
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source[self.current..].chars().next()?;
        self.current += ch.len_utf8();
        Some(ch)
    }

    fn span_through_match(&self, find_ch: char) -> Option<Range<usize>> {
        let idx = self.source[self.current..].find(find_ch)?;

        Some(self.start..self.source.ceil_char_boundary(self.current + idx + 1))
    }

    fn span_before_any_match(&self, find_chs: &[char]) -> Option<Range<usize>> {
        let idx = self.source[self.current..].find(|ch| find_chs.contains(&ch))?;

        Some(self.start..self.current + idx)
    }
}

impl<'a> Iterator for Scanner<'a> {
    type Item = Result<Token<'a>, ScanError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.start = self.current;

        match self.advance()? {
            '|' => Some(Ok(self.make_token(TokenKind::Pipe))),
            '>' => Some(Ok(self.make_token(TokenKind::RightAngle))),
            '\'' => Some(
                self.span_through_match('\'')
                    .map(|span| {
                        self.current = span.end;
                        let token = self.make_token_with_slice(
                            TokenKind::SingleQuoted,
                            self.start + 1,
                            self.current - 1,
                        );

                        token
                    })
                    .ok_or(self.error_with_span(
                        ScanErrorKind::UnclosedSingleQuote,
                        self.start..self.source.len(),
                    )),
            ),
            '"' => Some(
                self.span_through_match('"')
                    .map(|span| {
                        self.current = span.end;
                        let token = self.make_token_with_slice(
                            TokenKind::DoubleQuoted,
                            self.start + 1,
                            self.current - 1,
                        );

                        token
                    })
                    .ok_or(self.error_with_span(
                        ScanErrorKind::UnclosedDoubleQuote,
                        self.start..self.source.len(),
                    )),
            ),
            _ => {
                self.current = self
                    .span_before_any_match(&[' ', '\'', '"', '|', '>'])
                    .map_or(self.source.len(), |range| range.end);

                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn span_through_match_includes_the_first_matching_character() {
        let scanner = Scanner {
            source: "1'2'3456'78",
            start: 3,
            current: 4,
            had_name: false,
        };

        let span = scanner.span_through_match('\'').unwrap();

        assert_eq!(3..9, span);
        assert_eq!("'3456'", &scanner.source[span]);
    }

    #[test]
    pub fn span_through_match_includes_a_match_at_current() {
        let scanner = Scanner {
            source: "''",
            start: 0,
            current: 1,
            had_name: false,
        };

        assert_eq!(Some(0..2), scanner.span_through_match('\''));
    }

    #[test]
    pub fn span_through_match_returns_none_if_no_match_follows_current() {
        let scanner = Scanner {
            source: "'a'",
            start: 0,
            current: 3,
            had_name: false,
        };

        assert_eq!(None, scanner.span_through_match('\''));
    }

    #[test]
    pub fn span_through_match_uses_utf8_byte_offsets() {
        let scanner = Scanner {
            source: "'猫🐈 tail",
            start: 0,
            current: 1,
            had_name: false,
        };

        let span = scanner.span_through_match('🐈').unwrap();

        assert_eq!(0..8, span);
        assert_eq!("'猫🐈", &scanner.source[span]);
    }

    #[test]
    pub fn span_before_any_match_excludes_the_first_matching_character() {
        let scanner = Scanner {
            source: "word | next",
            start: 0,
            current: 1,
            had_name: false,
        };

        let span = scanner.span_before_any_match(&['|', ' ']).unwrap();

        assert_eq!(0..4, span);
        assert_eq!("word", &scanner.source[span]);
    }

    #[test]
    pub fn span_before_any_match_returns_empty_span_when_match_is_at_current() {
        let scanner = Scanner {
            source: "|next",
            start: 0,
            current: 0,
            had_name: false,
        };

        assert_eq!(Some(0..0), scanner.span_before_any_match(&['|', ' ']));
    }

    #[test]
    pub fn span_before_any_match_returns_none_when_no_match_follows_current() {
        let scanner = Scanner {
            source: "word",
            start: 0,
            current: 1,
            had_name: false,
        };

        assert_eq!(None, scanner.span_before_any_match(&['|', ' ']));
    }

    #[test]
    pub fn span_before_any_match_uses_utf8_byte_offsets() {
        let scanner = Scanner {
            source: "猫🐈|next",
            start: 0,
            current: 0,
            had_name: false,
        };

        let span = scanner.span_before_any_match(&['|', ' ']).unwrap();

        assert_eq!(0..7, span);
        assert_eq!("猫🐈", &scanner.source[span]);
    }
}
