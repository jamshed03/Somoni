use crate::span::Span;

/// The smallest parts of the source: keywords, names, values and symbols.
#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    // keywords
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
    Fn,
    Return,
    Struct,
    Null,
    In,
    Enum,
    Match,
    Import,
    Export,
    Class,
    New,
    This,
    Public,
    Private,
    Protected,
    Interface,
    Implements,
    Extends,
    Super,
    Override,
    Readonly,
    Static,
    Abstract,

    // names and values
    Ident(String),
    Int(i64),
    Str(String),
    Float(f64),

    // symbols
    Colon,
    ColonColon,
    Comma,
    Dot,
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
    LBracket,
    RBracket,

    EqualEqual,
    BangEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    AndAnd,
    OrOr,
    Bang,
    Question,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    FatArrow,

    /// The end of the input.
    Eof,
}

/// A token with its place in the source.
#[derive(Debug, PartialEq, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}
