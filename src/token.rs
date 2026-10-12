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

    // Ende der Eingabe
    Eof,
}
