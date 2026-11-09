use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

pub use template::is_template;

use raw_string::RAW_QUOTES;

mod raw_string;
mod template;

/// Turns the source text into tokens, each with its place.
pub struct Lexer {
    file: usize,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    line_start: usize,
    /// The strings whose `{...}` is being read, the innermost last.
    open_strings: Vec<OpenString>,
}

/// A string inside whose `{...}` the lexer is, like `"Hello {name` before the `}`.
struct OpenString {
    /// The place of the opening `"`, for L003 if the string is never closed.
    start: Span,
    /// How many `{` inside the expression are still open, like the `{` of a map.
    braces: usize,
}

/// How the text of a string ended: with its closing `"` or with a `{` before an expression.
enum TextEnd {
    Quote,
    Brace,
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
            open_strings: Vec::new(),
        }
    }

    /// Counts the lines from `line` on instead of 1, for an input of the REPL that follows earlier ones.
    pub fn starting_at_line(mut self, line: usize) -> Lexer {
        self.line = line;
        self
    }

    /// Reads the whole source and returns its tokens, the last one always `Eof`.
    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, SomoniError> {
        let mut tokens = Vec::new();
        self.read_code(&mut tokens, None)?;
        self.finish(tokens)
    }

    /// Reads code into `tokens` up to the end, or up to `close` (like `}}` in a template) outside of any `{ }`;
    /// returns true if it found `close`.
    fn read_code(
        &mut self,
        tokens: &mut Vec<SpannedToken>,
        close: Option<&str>,
    ) -> Result<bool, SomoniError> {
        let mut braces = 0usize;
        while let Some(c) = self.peek() {
            if self.skip_space_or_comment(c)? {
                continue;
            }
            if let Some(close) = close
                && braces == 0
                && self.open_strings.is_empty()
                && self.at(close)
            {
                self.pos += close.chars().count();
                return Ok(true);
            }
            let (line, col) = (self.line, self.col());
            let token = self.read_token(c)?;
            match token {
                Token::LBrace => braces += 1,
                Token::RBrace => braces = braces.saturating_sub(1),
                _ => {}
            }
            let span = self.span_from(line, col);
            tokens.push(SpannedToken { token, span });
        }
        Ok(false)
    }

    /// Checks that no string is still open and adds the `Eof` token.
    fn finish(&self, mut tokens: Vec<SpannedToken>) -> Result<Vec<SpannedToken>, SomoniError> {
        if let Some(open) = self.open_strings.first() {
            return Err(self.unterminated(open.start));
        }
        let span = self.span_from(self.line, self.col());
        tokens.push(SpannedToken {
            token: Token::Eof,
            span,
        });
        Ok(tokens)
    }

    /// Returns true if `text` starts at the current character.
    fn at(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(offset, c)| self.chars.get(self.pos + offset) == Some(&c))
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
        if self.at(RAW_QUOTES) {
            return self.read_raw_string();
        }
        if c == '"' {
            return self.read_string();
        }
        if let Some(open) = self.open_strings.last_mut() {
            match (c, open.braces) {
                ('}', 0) => return self.continue_string(),
                ('}', _) => open.braces -= 1,
                ('{', _) => open.braces += 1,
                _ => {}
            }
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

    /// Reads a string `"..."`, or its start up to the first `{` of `"Hello {name}"`.
    fn read_string(&mut self) -> Result<Token, SomoniError> {
        let start = Span::in_line(self.file, self.line, self.col(), 1);
        self.advance();
        match self.read_text(start)? {
            (text, TextEnd::Quote) => Ok(Token::Str(text)),
            (text, TextEnd::Brace) => {
                self.open_strings.push(OpenString { start, braces: 0 });
                Ok(Token::StrStart(text))
            }
        }
    }

    /// Reads the rest of a string after the `}` that ends one of its `{...}`.
    fn continue_string(&mut self) -> Result<Token, SomoniError> {
        let start = self
            .open_strings
            .last()
            .map(|open| open.start)
            .ok_or_else(|| SomoniError::internal("'}' of a string without the string"))?;
        self.advance();
        match self.read_text(start)? {
            (text, TextEnd::Quote) => {
                self.open_strings.pop();
                Ok(Token::StrEnd(text))
            }
            (text, TextEnd::Brace) => Ok(Token::StrMiddle(text)),
        }
    }

    /// Reads text of a string up to its closing `"` or up to a `{`, which may go over several lines and hold escapes like `\n`.
    fn read_text(&mut self, start: Span) -> Result<(String, TextEnd), SomoniError> {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c == '"' {
                self.advance();
                return Ok((text, TextEnd::Quote));
            }
            if c == '{' {
                self.advance();
                return Ok((text, TextEnd::Brace));
            }
            if c == '\\' {
                text.push(self.read_escape(start)?);
                continue;
            }
            text.push(c);
            if c == '\n' {
                self.newline();
            } else {
                self.advance();
            }
        }
        Err(self.unterminated(start))
    }

    /// The error for a string that `start` opens and the file ends in: L007 at the outer string if one of its `{...}` is still open, else L002.
    fn unterminated(&self, start: Span) -> SomoniError {
        match self.open_strings.first() {
            Some(open) => SomoniError::at(open.start, ErrorCode::InterpolationNotClosed),
            None => SomoniError::at(start, ErrorCode::UnterminatedString),
        }
    }

    /// Reads an escape `\n`, `\t`, `\r`, `\"`, `\\`, `\{` or `\}` and returns its character, or L006 for another one.
    fn read_escape(&mut self, string_start: Span) -> Result<char, SomoniError> {
        let place = Span::in_line(self.file, self.line, self.col(), 2);
        self.advance();
        let Some(c) = self.peek() else {
            return Err(SomoniError::at(string_start, ErrorCode::UnterminatedString));
        };
        let meant = match c {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '"' => '"',
            '\\' => '\\',
            '{' => '{',
            '}' => '}',
            other => {
                let code = ErrorCode::UnknownEscape { found: other };
                return Err(SomoniError::at(place, code));
            }
        };
        self.advance();
        Ok(meant)
    }

    /// Reads a symbol like `+=` or `(`, or returns L001 for a character that is none.
    fn read_symbol(&mut self, c: char) -> Result<Token, SomoniError> {
        let Some((length, token)) = symbol(c, self.peek_next()) else {
            let span = Span::in_line(self.file, self.line, self.col(), 1);
            return Err(SomoniError::at(
                span,
                ErrorCode::UnexpectedCharacter { found: c },
            ));
        };
        self.pos += length;
        Ok(token)
    }

    /// Skips a comment `// ...` up to the end of the line.
    fn skip_line_comment(&mut self) {
        self.skip_while(|c| c != '\n');
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
        let start = self.pos;
        self.skip_while(wanted);
        self.chars[start..self.pos].iter().collect()
    }

    /// Skips characters as long as `wanted` is true for them.
    fn skip_while(&mut self, wanted: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&wanted) {
            self.advance();
        }
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
        Span::new(self.file, line, col, self.line, self.col())
    }
}

/// Returns the keyword token for `word`, or None if it is a name.
fn keyword(word: &str) -> Option<Token> {
    let token = match word {
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
        "null" => Token::Null,
        "in" => Token::In,
        "enum" => Token::Enum,
        "match" => Token::Match,
        "import" => Token::Import,
        "export" => Token::Export,
        "class" => Token::Class,
        "new" => Token::New,
        "this" => Token::This,
        "public" => Token::Public,
        "private" => Token::Private,
        "protected" => Token::Protected,
        "interface" => Token::Interface,
        "implements" => Token::Implements,
        "extends" => Token::Extends,
        "super" => Token::Super,
        "override" => Token::Override,
        "readonly" => Token::Readonly,
        "static" => Token::Static,
        "abstract" => Token::Abstract,
        "instanceof" => Token::Instanceof,
        _ => return None,
    };
    Some(token)
}

/// Returns the symbol that starts with `c` (and maybe `next`) and its length, the two-character ones like `+=` before `+`.
fn symbol(c: char, next: Option<char>) -> Option<(usize, Token)> {
    let two = match (c, next) {
        ('+', Some('=')) => Some(Token::PlusEqual),
        ('-', Some('=')) => Some(Token::MinusEqual),
        ('*', Some('=')) => Some(Token::StarEqual),
        ('/', Some('=')) => Some(Token::SlashEqual),
        ('%', Some('=')) => Some(Token::PercentEqual),
        ('=', Some('=')) => Some(Token::EqualEqual),
        ('=', Some('>')) => Some(Token::FatArrow),
        ('!', Some('=')) => Some(Token::BangEqual),
        ('<', Some('=')) => Some(Token::LessEqual),
        ('>', Some('=')) => Some(Token::GreaterEqual),
        ('&', Some('&')) => Some(Token::AndAnd),
        ('|', Some('|')) => Some(Token::OrOr),
        (':', Some(':')) => Some(Token::ColonColon),
        _ => None,
    };
    if let Some(token) = two {
        return Some((2, token));
    }
    let one = match c {
        ':' => Token::Colon,
        ',' => Token::Comma,
        '.' => Token::Dot,
        ';' => Token::Semicolon,
        '+' => Token::Plus,
        '-' => Token::Minus,
        '*' => Token::Star,
        '/' => Token::Slash,
        '%' => Token::Percent,
        '(' => Token::LParen,
        ')' => Token::RParen,
        '{' => Token::LBrace,
        '}' => Token::RBrace,
        '[' => Token::LBracket,
        ']' => Token::RBracket,
        '=' => Token::Equal,
        '!' => Token::Bang,
        '<' => Token::Less,
        '>' => Token::Greater,
        '?' => Token::Question,
        '|' => Token::Pipe,
        _ => return None,
    };
    Some((1, one))
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
    fn spans(source: &str) -> Vec<(u32, u32, u32, u32)> {
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

    /// Returns the text of the first token, which must be a string.
    fn string_text(source: &str) -> String {
        match &Lexer::new(source, 0).tokenize().unwrap()[0].token {
            Token::Str(text) => text.clone(),
            other => panic!("no string: {:?}", other),
        }
    }

    #[test]
    fn escapes_become_their_characters() {
        assert_eq!(string_text(r#""a\nb\tc\"d\\e\rf""#), "a\nb\tc\"d\\e\rf");
    }

    #[test]
    fn unknown_escape_is_an_error() {
        let error = Lexer::new(r#""a\qb""#, 0).tokenize().unwrap_err();
        assert!(error.to_string().contains("L006"), "{}", error);
    }

    /// Returns the tokens of `source` without their places and without `Eof`.
    fn tokens(source: &str) -> Vec<Token> {
        let mut tokens: Vec<Token> = Lexer::new(source, 0)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.token)
            .collect();
        tokens.pop();
        tokens
    }

    #[test]
    fn interpolation_splits_the_string_into_parts() {
        assert_eq!(
            tokens(r#""a{x}b{y}c""#),
            vec![
                Token::StrStart("a".to_string()),
                Token::Ident("x".to_string()),
                Token::StrMiddle("b".to_string()),
                Token::Ident("y".to_string()),
                Token::StrEnd("c".to_string()),
            ]
        );
    }

    #[test]
    fn braces_and_strings_inside_an_interpolation_stay_tokens() {
        assert_eq!(
            tokens(r#""{ {"k": "v"}["k"] }!""#),
            vec![
                Token::StrStart(String::new()),
                Token::LBrace,
                Token::Str("k".to_string()),
                Token::Colon,
                Token::Str("v".to_string()),
                Token::RBrace,
                Token::LBracket,
                Token::Str("k".to_string()),
                Token::RBracket,
                Token::StrEnd("!".to_string()),
            ]
        );
    }

    #[test]
    fn escaped_braces_are_text() {
        assert_eq!(string_text(r#""\{a\}""#), "{a}");
    }

    #[test]
    fn interpolation_without_end_is_an_error() {
        let error = Lexer::new(r#""a{x"#, 0).tokenize().unwrap_err();
        assert!(error.to_string().contains("L007"), "{}", error);
    }

    /// Returns the error code of lexing `source`, like `L002`.
    fn error_code(source: &str) -> &'static str {
        Lexer::new(source, 0).tokenize().unwrap_err().code.code()
    }

    #[test]
    fn the_longest_symbol_wins() {
        let name = |n: &str| Token::Ident(n.to_string());
        assert_eq!(
            tokens("a<=b == c => d += 1 :: x"),
            vec![
                name("a"),
                Token::LessEqual,
                name("b"),
                Token::EqualEqual,
                name("c"),
                Token::FatArrow,
                name("d"),
                Token::PlusEqual,
                Token::Int(1),
                Token::ColonColon,
                name("x"),
            ]
        );
    }

    #[test]
    fn keywords_are_whole_words() {
        assert_eq!(
            tokens("var variable if iffy"),
            vec![
                Token::Var,
                Token::Ident("variable".to_string()),
                Token::If,
                Token::Ident("iffy".to_string()),
            ]
        );
    }

    #[test]
    fn numbers_are_ints_or_floats() {
        assert_eq!(tokens("42 3.5"), vec![Token::Int(42), Token::Float(3.5)]);
    }

    #[test]
    fn a_tuple_index_after_a_dot_reads_as_a_float() {
        assert_eq!(
            tokens("p.0.1"),
            vec![Token::Ident("p".to_string()), Token::Dot, Token::Float(0.1)]
        );
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(
            tokens("a // to the end\n/* over\n lines */ b"),
            vec![Token::Ident("a".to_string()), Token::Ident("b".to_string())]
        );
    }

    #[test]
    fn errors_of_the_lexer() {
        assert_eq!(error_code("a $ b"), "L001");
        assert_eq!(error_code("\"abc"), "L002");
        assert_eq!(error_code("/* abc"), "L003");
        assert_eq!(error_code("9223372036854775808"), "L004");
    }

    #[test]
    fn lines_can_start_later() {
        let spans: Vec<u32> = Lexer::new("x\ny", 0)
            .starting_at_line(5)
            .tokenize()
            .unwrap()
            .iter()
            .map(|t| t.span.line)
            .collect();
        assert_eq!(spans, vec![5, 6, 6]);
    }

    #[test]
    fn string_over_two_lines() {
        assert_eq!(
            spans("\"a\nb\" x"),
            vec![(1, 1, 2, 3), (2, 4, 2, 5), (2, 5, 2, 5)]
        );
    }
}
