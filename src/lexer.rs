use crate::token::{SpannedToken, Token};

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Lexer {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn read_number(&mut self) -> Result<Token, String> {
        let line = self.line;
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
        let next_is_digit = self
            .chars
            .get(self.pos + 1)
            .map_or(false, |c| c.is_ascii_digit());

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
            return text
                .parse::<f64>()
                .map(Token::Float)
                .map_err(|_| format!("Line {}: invalid float '{}'", line, text));
        }

        text.parse::<i64>()
            .map(Token::Int)
            .map_err(|_| format!("Line {}: integer too large '{}'", line, text))
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
            _ => Token::Ident(text),
        }
    }

    fn read_string(&mut self) -> Result<Token, String> {
        let start_line = self.line;
        self.advance(); // opening "
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c == '"' {
                self.advance(); // closing "
                return Ok(Token::Str(text));
            }
            if c == '\n' {
                self.line += 1;
            }
            text.push(c);
            self.advance();
        }
        Err(format!("Line {}: unterminated string", start_line))
    }

    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, String> {
        let mut tokens = Vec::new();

        while let Some(c) = self.peek() {
            if c == '\n' {
                self.line += 1;
                self.advance();
                continue;
            }

            if c.is_whitespace() {
                self.advance();
                continue;
            }

            let line = self.line;

            if c.is_ascii_digit() {
                let token = self.read_number()?;
                tokens.push(SpannedToken { token, line });
                continue;
            }

            if c.is_alphabetic() || c == '_' {
                let token = self.read_word();
                tokens.push(SpannedToken { token, line });
                continue;
            }

            if c == '"' {
                let token = self.read_string()?;
                tokens.push(SpannedToken { token, line });
                continue;
            }

            let token = match c {
                ':' => Token::Colon,
                '=' => Token::Equal,
                ';' => Token::Semicolon,
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => Token::Slash,
                '(' => Token::LParen,
                ')' => Token::RParen,
                _ => return Err(format!("Line {}: unexpected character '{}'", line, c)),
            };
            tokens.push(SpannedToken { token, line });
            self.advance();
        }

        tokens.push(SpannedToken { token: Token::Eof, line: self.line });
        Ok(tokens)
    }
}
