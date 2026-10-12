#![allow(dead_code)]

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    // Schlüsselwörter
    Var,
    Const,

    // Namen und Werte
    Ident(String),
    Int(i64),

    // Symbole
    Colon,
    Equal,
    Semicolon,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,

    // Ende der Eingabe
    Eof,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
}
