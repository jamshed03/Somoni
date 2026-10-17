use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    // position of the first character of the current line, for the column
    line_start: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Lexer {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            line_start: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    // skip a '\n' and start the next line
    fn newline(&mut self) {
        self.advance();
        self.line += 1;
        self.line_start = self.pos;
    }

    fn col(&self) -> usize {
        self.pos - self.line_start + 1
    }

    // from (line, col) up to the current position
    fn span_from(&self, line: usize, col: usize) -> Span {
        Span {
            line,
            col,
            end_line: self.line,
            end_col: self.col(),
        }
    }

    fn read_number(&mut self) -> Result<Token, SomoniError> {
        let (line, col) = (self.line, self.col());
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // a '.' only belongs to the number if a digit follows
        let next_is_digit = self.peek_next().is_some_and(|c| c.is_ascii_digit());

        if self.peek() == Some('.') && next_is_digit {
            text.push('.');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            let span = self.span_from(line, col);
            return text
                .parse::<f64>()
                .map(Token::Float)
                .map_err(|_| SomoniError::at(span, ErrorCode::InvalidFloat { text }));
        }

        let span = self.span_from(line, col);
        text.parse::<i64>()
            .map(Token::Int)
            .map_err(|_| SomoniError::at(span, ErrorCode::IntegerTooLarge { text }))
    }

    fn read_word(&mut self) -> Token {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                text.push(c);
                self.advance();
            } else {
                break;
            }
        }
        match text.as_str() {
            "var" => Token::Var,
            "const" => Token::Const,
            "true" => Token::True,
            "false" => Token::False,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "for" => Token::For,
            "break" => Token::Break,
            "continue" => Token::Continue,
            "fn" => Token::Fn,
            "return" => Token::Return,
            "struct" => Token::Struct,
            _ => Token::Ident(text),
        }
    }

    fn read_string(&mut self) -> Result<Token, SomoniError> {
        let start = Span::in_line(self.line, self.col(), 1);
        self.advance(); // opening "
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c == '"' {
                self.advance(); // closing "
                return Ok(Token::Str(text));
            }
            text.push(c);
            if c == '\n' {
                self.newline();
            } else {
                self.advance();
            }
        }
        Err(SomoniError::at(start, ErrorCode::UnterminatedString))
    }

    // `// ...` up to the end of the line; the '\n' itself is left for the line counter
    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    // `/* ... */` over several lines, not nested
    fn skip_block_comment(&mut self) -> Result<(), SomoniError> {
        let start = Span::in_line(self.line, self.col(), 2); // the '/*'
        self.advance(); // '/'
        self.advance(); // '*'
        loop {
            match self.peek() {
                None => return Err(SomoniError::at(start, ErrorCode::UnterminatedComment)),
                Some('*') if self.peek_next() == Some('/') => {
                    self.advance(); // '*'
                    self.advance(); // '/'
                    return Ok(());
                }
                Some('\n') => self.newline(),
                Some(_) => self.advance(),
            }
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, SomoniError> {
        let mut tokens = Vec::new();

        while let Some(c) = self.peek() {
            if c == '\n' {
                self.newline();
                continue;
            }

            if c.is_whitespace() {
                self.advance();
                continue;
            }

            if c == '/' && self.peek_next() == Some('/') {
                self.skip_line_comment();
                continue;
            }

            if c == '/' && self.peek_next() == Some('*') {
                self.skip_block_comment()?;
                continue;
            }

            let line = self.line;
            let col = self.col();

            if c.is_ascii_digit() {
                let token = self.read_number()?;
                tokens.push(SpannedToken {
                    token,
                    span: self.span_from(line, col),
                });
                continue;
            }

            if c.is_alphabetic() || c == '_' {
                let token = self.read_word();
                tokens.push(SpannedToken {
                    token,
                    span: self.span_from(line, col),
                });
                continue;
            }

            if c == '"' {
                let token = self.read_string()?;
                tokens.push(SpannedToken {
                    token,
                    span: self.span_from(line, col),
                });
                continue;
            }

            let (token, len) = match c {
                ':' => (Token::Colon, 1),
                ',' => (Token::Comma, 1),
                // a '.' after a digit is already part of the float (read_number)
                '.' => (Token::Dot, 1),
                ';' => (Token::Semicolon, 1),
                '+' => (Token::Plus, 1),
                '-' => (Token::Minus, 1),
                '*' => (Token::Star, 1),
                '/' => (Token::Slash, 1),
                '%' => (Token::Percent, 1),
                '(' => (Token::LParen, 1),
                ')' => (Token::RParen, 1),
                '{' => (Token::LBrace, 1),
                '}' => (Token::RBrace, 1),
                '[' => (Token::LBracket, 1),
                ']' => (Token::RBracket, 1),
                '=' if self.peek_next() == Some('=') => (Token::EqualEqual, 2),
                '=' => (Token::Equal, 1),
                '!' if self.peek_next() == Some('=') => (Token::BangEqual, 2),
                '!' => (Token::Bang, 1),
                '<' if self.peek_next() == Some('=') => (Token::LessEqual, 2),
                '<' => (Token::Less, 1),
                '>' if self.peek_next() == Some('=') => (Token::GreaterEqual, 2),
                '>' => (Token::Greater, 1),
                '&' if self.peek_next() == Some('&') => (Token::AndAnd, 2),
                '|' if self.peek_next() == Some('|') => (Token::OrOr, 2),
                _ => {
                    return Err(SomoniError::at(
                        Span::in_line(line, col, 1),
                        ErrorCode::UnexpectedCharacter { found: c },
                    ));
                }
            };
            for _ in 0..len {
                self.advance();
            }
            tokens.push(SpannedToken {
                token,
                span: self.span_from(line, col),
            });
        }

        // Eof has no width: it sits right after the last character
        tokens.push(SpannedToken {
            token: Token::Eof,
            span: self.span_from(self.line, self.col()),
        });
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // (line, col, end_line, end_col) of every token, Eof included
    fn spans(source: &str) -> Vec<(usize, usize, usize, usize)> {
        Lexer::new(source)
            .tokenize()
            .unwrap()
            .iter()
            .map(|t| (t.span.line, t.span.col, t.span.end_line, t.span.end_col))
            .collect()
    }

    #[test]
    fn columns_count_characters() {
        // "ä" is one character (but two bytes in UTF-8)
        assert_eq!(
            spans("x == \"ä\";"),
            vec![
                (1, 1, 1, 2),
                (1, 3, 1, 5),
                (1, 6, 1, 9),
                (1, 9, 1, 10),
                (1, 10, 1, 10)
            ]
        );
    }

    #[test]
    fn new_line_starts_at_column_one() {
        // line 4 is " y */ c": the space counts, so `c` is in column 7
        assert_eq!(
            spans("a\n\tb // comment\n/* x\n y */ c"),
            vec![(1, 1, 1, 2), (2, 2, 2, 3), (4, 7, 4, 8), (4, 8, 4, 8)]
        );
    }

    #[test]
    fn string_over_two_lines() {
        assert_eq!(
            spans("\"a\nb\" x"),
            vec![(1, 1, 2, 3), (2, 4, 2, 5), (2, 5, 2, 5)]
        );
    }
}
