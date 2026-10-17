use crate::ast::Stmt;
use crate::error::SomoniError;

/// More nested calls than this are runtime error R004, in the tree interpreter and in the VM.
pub const MAX_CALL_DEPTH: usize = 10_000;

/// Why a run stopped early.
#[derive(Debug)]
pub enum Stop {
    /// A normal Somoni error, e.g. a runtime error.
    Error(SomoniError),
}

impl From<SomoniError> for Stop {
    fn from(error: SomoniError) -> Stop {
        Stop::Error(error)
    }
}

/// Runs a checked program. The tree interpreter and the VM both do this.
pub trait Runner<'a> {
    fn run(&mut self, stmts: &'a [Stmt]) -> Result<(), Stop>;
}
