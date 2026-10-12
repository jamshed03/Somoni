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

    fn read_number(&mut self) -> Token {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.advance();
            } else {
                break;
            }
        }
        Token::Int(text.parse().unwrap())
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
            _ => Token::Ident(text),
        }
    }

    pub fn tokenize(&mut self) -> Vec<SpannedToken> {
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
                let token = self.read_number();
                tokens.push(SpannedToken { token, line });
                continue;
            }

            if c.is_alphabetic() || c == '_' {
                let token = self.read_word();
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
                _ => break, 
            };
            tokens.push(SpannedToken { token, line });
            self.advance();
        }

        tokens.push(SpannedToken { token: Token::Eof, line: self.line });
        tokens
    }
}
