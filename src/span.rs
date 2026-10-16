// A place in the source code: from (line, col) up to, but not including, (end_line, end_col).
// Lines and columns start at 1 and count characters, not bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
}
