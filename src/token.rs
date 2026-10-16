use crate::span::Span;

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    // Schlüsselwörter
    Var,
    Const,
    True,
    False,
    If,
    Else,
    While,
    For,
    Break,
    Continue,

    // Namen und Werte
    Ident(String),
    Int(i64),
    Str(String),
    Float(f64),

    // Symbole
    Colon,
    Equal,
    Semicolon,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    LBrace,
    RBrace,

    EqualEqual,
    BangEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    AndAnd,
    OrOr,
    Bang,

    // Ende der Eingabe
    Eof,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}
