use crate::token::Token;

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Lexer {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
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

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
                continue;
            }

            if c.is_ascii_digit() {
                tokens.push(self.read_number());
                continue;
            }

            if c.is_alphabetic() || c == '_' {
                tokens.push(self.read_word());
                continue;
            }

            match c {
                ':' => tokens.push(Token::Colon),
                '=' => tokens.push(Token::Equal),
                ';' => tokens.push(Token::Semicolon),
                _ => break,
            }
            self.advance();
        }

        tokens.push(Token::Eof);
        tokens
    }
}
