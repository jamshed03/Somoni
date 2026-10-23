use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

/// The keywords of Somoni; every other word is a name.
const KEYWORDS: &[(&str, Token)] = &[
    ("var", Token::Var),
    ("const", Token::Const),
    ("true", Token::True),
    ("false", Token::False),
    ("if", Token::If),
    ("else", Token::Else),
    ("while", Token::While),
    ("for", Token::For),
    ("break", Token::Break),
    ("continue", Token::Continue),
    ("fn", Token::Fn),
    ("return", Token::Return),
    ("struct", Token::Struct),
    ("null", Token::Null),
    ("in", Token::In),
    ("enum", Token::Enum),
    ("match", Token::Match),
    ("import", Token::Import),
    ("export", Token::Export),
    ("class", Token::Class),
    ("new", Token::New),
    ("this", Token::This),
    ("public", Token::Public),
    ("private", Token::Private),
    ("protected", Token::Protected),
    ("interface", Token::Interface),
    ("implements", Token::Implements),
    ("extends", Token::Extends),
    ("super", Token::Super),
    ("override", Token::Override),
    ("readonly", Token::Readonly),
    ("static", Token::Static),
    ("abstract", Token::Abstract),
];

/// The symbols of Somoni, the two-character ones first so `+=` is found before `+`.
const SYMBOLS: &[(&str, Token)] = &[
    ("+=", Token::PlusEqual),
    ("-=", Token::MinusEqual),
    ("*=", Token::StarEqual),
    ("/=", Token::SlashEqual),
    ("%=", Token::PercentEqual),
    ("==", Token::EqualEqual),
    ("=>", Token::FatArrow),
    ("!=", Token::BangEqual),
    ("<=", Token::LessEqual),
    (">=", Token::GreaterEqual),
    ("&&", Token::AndAnd),
    ("||", Token::OrOr),
    ("::", Token::ColonColon),
    (":", Token::Colon),
    (",", Token::Comma),
    (".", Token::Dot),
    (";", Token::Semicolon),
    ("+", Token::Plus),
    ("-", Token::Minus),
    ("*", Token::Star),
    ("/", Token::Slash),
    ("%", Token::Percent),
    ("(", Token::LParen),
    (")", Token::RParen),
    ("{", Token::LBrace),
    ("}", Token::RBrace),
    ("[", Token::LBracket),
    ("]", Token::RBracket),
    ("=", Token::Equal),
    ("!", Token::Bang),
    ("<", Token::Less),
    (">", Token::Greater),
    ("?", Token::Question),
];

/// Turns the source text into tokens, each with its place.
pub struct Lexer {
    file: usize,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    line_start: usize,
}

impl Lexer {
    /// Starts at the first character of the text of source file number `file`.
    pub fn new(input: &str, file: usize) -> Lexer {
        Lexer {
            file,
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            line_start: 0,
        }
    }

    /// Reads the whole source and returns its tokens, the last one always `Eof`.
    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, SomoniError> {
        let mut tokens = Vec::new();
        while let Some(c) = self.peek() {
            if self.skip_space_or_comment(c)? {
                continue;
            }
            let (line, col) = (self.line, self.col());
            let token = self.read_token(c)?;
            let span = self.span_from(line, col);
            tokens.push(SpannedToken { token, span });
        }
        let span = self.span_from(self.line, self.col());
        tokens.push(SpannedToken {
            token: Token::Eof,
            span,
        });
        Ok(tokens)
    }

    /// Skips a line break, a space or a comment that starts with `c`, and returns true if it did.
    fn skip_space_or_comment(&mut self, c: char) -> Result<bool, SomoniError> {
        match (c, self.peek_next()) {
            ('\n', _) => self.newline(),
            (c, _) if c.is_whitespace() => self.advance(),
            ('/', Some('/')) => self.skip_line_comment(),
            ('/', Some('*')) => self.skip_block_comment()?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Reads the token that starts with `c`.
    fn read_token(&mut self, c: char) -> Result<Token, SomoniError> {
        if c.is_ascii_digit() {
            return self.read_number();
        }
        if c.is_alphabetic() || c == '_' {
            return Ok(self.read_word());
        }
        if c == '"' {
            return self.read_string();
        }
        self.read_symbol(c)
    }

    /// Reads an int like `42` or a float like `1.5`.
    fn read_number(&mut self) -> Result<Token, SomoniError> {
        let (line, col) = (self.line, self.col());
        let mut text = self.read_while(|c| c.is_ascii_digit());
        let is_float =
            self.peek() == Some('.') && self.peek_next().is_some_and(|c| c.is_ascii_digit());
        if is_float {
            self.advance();
            text.push('.');
            text.push_str(&self.read_while(|c| c.is_ascii_digit()));
        }
        let span = self.span_from(line, col);
        number_token(text, is_float).map_err(|code| SomoniError::at(span, code))
    }

    /// Reads a keyword like `if` or a name like `count`.
    fn read_word(&mut self) -> Token {
        let word = self.read_while(|c| c.is_alphanumeric() || c == '_');
        keyword(&word).unwrap_or(Token::Ident(word))
    }

    /// Reads a string `"..."`, which may go over several lines.
    fn read_string(&mut self) -> Result<Token, SomoniError> {
        let start = Span::in_line(self.file, self.line, self.col(), 1);
        self.advance();
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c == '"' {
                self.advance();
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

    /// Reads a symbol like `+=` or `(`, or returns L001 for a character that is none.
    fn read_symbol(&mut self, c: char) -> Result<Token, SomoniError> {
        let Some((text, token)) = symbol(c, self.peek_next()) else {
            let span = Span::in_line(self.file, self.line, self.col(), 1);
            return Err(SomoniError::at(
                span,
                ErrorCode::UnexpectedCharacter { found: c },
            ));
        };
        self.pos += text.len();
        Ok(token)
    }

    /// Skips a comment `// ...` up to the end of the line.
    fn skip_line_comment(&mut self) {
        self.read_while(|c| c != '\n');
    }

    /// Skips a comment `/* ... */`, also over several lines.
    fn skip_block_comment(&mut self) -> Result<(), SomoniError> {
        let start = Span::in_line(self.file, self.line, self.col(), 2);
        self.pos += 2;
        loop {
            match self.peek() {
                None => return Err(SomoniError::at(start, ErrorCode::UnterminatedComment)),
                Some('*') if self.peek_next() == Some('/') => {
                    self.pos += 2;
                    return Ok(());
                }
                Some('\n') => self.newline(),
                Some(_) => self.advance(),
            }
        }
    }

    /// Reads characters as long as `wanted` is true for them and returns them.
    fn read_while(&mut self, wanted: impl Fn(char) -> bool) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek().filter(|&c| wanted(c)) {
            text.push(c);
            self.advance();
        }
        text
    }

    /// Returns the current character, None at the end.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Returns the character after the current one.
    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    /// Goes to the next character in the same line.
    fn advance(&mut self) {
        self.pos += 1;
    }

    /// Skips a line break and starts the next line.
    fn newline(&mut self) {
        self.advance();
        self.line += 1;
        self.line_start = self.pos;
    }

    /// Returns the column of the current character, starting at 1.
    fn col(&self) -> usize {
        self.pos - self.line_start + 1
    }

    /// Returns the place from (line, col) up to the current character.
    fn span_from(&self, line: usize, col: usize) -> Span {
        Span {
            file: self.file,
            line,
            col,
            end_line: self.line,
            end_col: self.col(),
        }
    }
}

/// Returns the keyword token for `word`, or None if it is a name.
fn keyword(word: &str) -> Option<Token> {
    KEYWORDS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, token)| token.clone())
}

/// Returns the symbol that starts with `c` (and maybe `next`) and its token.
fn symbol(c: char, next: Option<char>) -> Option<(&'static str, Token)> {
    SYMBOLS
        .iter()
        .find(|(text, _)| starts_symbol(text, c, next))
        .map(|(text, token)| (*text, token.clone()))
}

/// Returns true if the characters `c` and `next` start with the symbol `text`.
fn starts_symbol(text: &str, c: char, next: Option<char>) -> bool {
    let mut chars = text.chars();
    chars.next() == Some(c) && chars.next().is_none_or(|second| Some(second) == next)
}

/// Turns the digits of a number into an int or float token, or L004/L005 if they do not fit.
fn number_token(text: String, is_float: bool) -> Result<Token, ErrorCode> {
    if is_float {
        return text
            .parse()
            .map(Token::Float)
            .map_err(|_| ErrorCode::InvalidFloat { text });
    }
    text.parse()
        .map(Token::Int)
        .map_err(|_| ErrorCode::IntegerTooLarge { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns (line, col, end_line, end_col) of every token, Eof included.
    fn spans(source: &str) -> Vec<(usize, usize, usize, usize)> {
        Lexer::new(source, 0)
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
