/// A place in source file number `file`, from (line, col) up to (end_line, end_col), counted from 1.
/// The numbers are `u32` so that every token and syntax tree node stays small.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub file: u32,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Span {
    /// A place in file `file` from (line, col) up to (end_line, end_col).
    pub fn new(file: usize, line: usize, col: usize, end_line: usize, end_col: usize) -> Span {
        Span {
            file: narrow(file),
            line: narrow(line),
            col: narrow(col),
            end_line: narrow(end_line),
            end_col: narrow(end_col),
        }
    }

    /// A place in one line, `width` characters wide.
    pub fn in_line(file: usize, line: usize, col: usize, width: usize) -> Span {
        Span::new(file, line, col, line, col + width)
    }

    /// The place from the start of `self` to the end of `other`.
    pub fn to(self, other: Span) -> Span {
        Span {
            end_line: other.end_line,
            end_col: other.end_col,
            ..self
        }
    }

    /// The place of one character right after `self`, e.g. where a missing `;` belongs.
    pub fn after(self) -> Span {
        Span {
            line: self.end_line,
            col: self.end_col,
            end_col: self.end_col + 1,
            ..self
        }
    }

    /// The same place counted `lines` lines higher, for an input of the REPL after earlier ones.
    pub fn up(self, lines: usize) -> Span {
        let line = self.line as usize - lines;
        let end_line = self.end_line as usize - lines;
        Span::new(
            self.file as usize,
            line,
            self.col as usize,
            end_line,
            self.end_col as usize,
        )
    }
}

/// Turns a line, column or file number into a `u32`; a source with more than four billion lines cannot be read anyway.
fn narrow(number: usize) -> u32 {
    u32::try_from(number).unwrap_or(u32::MAX)
}
