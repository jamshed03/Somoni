#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    pub fn in_line(line: usize, col: usize, width: usize) -> Span {
        Span {
            line,
            col,
            end_line: line,
            end_col: col + width,
        }
    }

    pub fn to(self, other: Span) -> Span {
        Span {
            end_line: other.end_line,
            end_col: other.end_col,
            ..self
        }
    }

    pub fn after(self) -> Span {
        Span::in_line(self.end_line, self.end_col, 1)
    }
}
