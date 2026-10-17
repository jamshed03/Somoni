use crate::ast::Stmt;
use crate::error::SomoniError;

/// Why a run stopped early.
#[derive(Debug)]
pub enum Stop {
    /// A normal Somoni error, e.g. a runtime error.
    Error(SomoniError),
    /// The program uses something the VM cannot run yet.
    NotInVm(String),
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
