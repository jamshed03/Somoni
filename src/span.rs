/// A place in the source: from (line, col) up to (end_line, end_col), columns start at 1.
/// `file` is the number of the source file; 0 is the file given to `somoni`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub file: usize,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    /// A place in one line, `width` characters wide.
    pub fn in_line(file: usize, line: usize, col: usize, width: usize) -> Span {
        Span {
            file,
            line,
            col,
            end_line: line,
            end_col: col + width,
        }
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
        Span::in_line(self.file, self.end_line, self.end_col, 1)
    }
}
