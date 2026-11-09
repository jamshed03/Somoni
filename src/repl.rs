//! The REPL: `somoni` without a file reads Somoni line by line and runs every input right away.
//! It keeps the parsed statements of every input that ran; a new input is checked and compiled together with them,
//! but only its own code runs, on top of the variables the VM kept (see docs on `compile_from` and `run_in`).
//! Errors are shown on stdout like results, because in the REPL they are part of the conversation, and end nothing.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use crate::ast::Stmt;
use crate::bytecode::Program;
use crate::compiler;
use crate::error::{ErrorCode, SomoniError};
use crate::lexer::Lexer;
use crate::loader::{MAIN_FILE, SourceFile};
use crate::parser::Parser;
use crate::typechecker::TypeChecker;
use crate::vm::{self, Session};

/// The prompt before an input, shown only when a person types into a terminal.
const PROMPT: &str = "> ";

/// Reads inputs until the input ends (Ctrl+D) and runs each one.
pub fn run() {
    let stdin = io::stdin();
    let terminal = stdin.is_terminal();
    let mut repl = Repl::default();
    loop {
        if terminal {
            print!("{}", PROMPT);
            let _ = io::stdout().flush();
        }
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if !line.trim().is_empty() {
            repl.run_input(line);
        }
    }
    if terminal {
        println!();
    }
}

/// What the REPL remembers from the inputs that ran.
#[derive(Default)]
struct Repl {
    /// The parsed statements of every input that ran, not checked yet: the type checker writes into a copy.
    history: Vec<Stmt>,
    /// The source of those inputs, for a message about one of their lines.
    sources: String,
    /// The number of their lines; the next input starts on the line after them.
    lines: usize,
    /// The names of the globals that have their values already, so their start values do not run again.
    globals: Vec<String>,
    /// The variables and globals of the VM.
    session: Session,
}

impl Repl {
    /// Reads, checks, compiles and runs one input, and keeps it if it ran without an error, else shows the error.
    fn run_input(&mut self, source: String) {
        let ran = self.build(&source).and_then(|(program, stmts)| {
            vm::run_in(&program, &mut self.session)?;
            Ok((program, stmts))
        });
        match ran {
            Ok((program, stmts)) => {
                self.history.extend(stmts);
                self.lines += source.lines().count();
                self.sources.push_str(&source);
                self.globals = program.globals;
            }
            Err(error) => println!("{}", self.render(error, &source)),
        }
    }

    /// Parses the input, checks and compiles it after the earlier ones, and returns the program with its statements;
    /// `import` is M008 for now.
    fn build(&self, source: &str) -> Result<(Program, Vec<Stmt>), SomoniError> {
        let tokens = Lexer::new(source, MAIN_FILE)
            .starting_at_line(self.lines + 1)
            .tokenize()?;
        let stmts = Parser::new(tokens).parse_program(false)?;
        if let Some(Stmt::Import(import)) =
            stmts.iter().find(|stmt| matches!(stmt, Stmt::Import(_)))
        {
            return Err(SomoniError::at(import.span, ErrorCode::ImportInRepl));
        }
        let mut files = [main_file(String::new())];
        files[MAIN_FILE].stmts = self.history.iter().chain(&stmts).cloned().collect();
        TypeChecker::new().check(&mut files, &[MAIN_FILE])?;
        let program =
            compiler::compile_from(&files, &[MAIN_FILE], self.history.len(), &self.globals)?;
        Ok((program, stmts))
    }

    /// Returns the error ready to show: its line counted inside the input, or in all inputs if it lies in an earlier one.
    fn render(&self, mut error: SomoniError, source: &str) -> String {
        let in_input = error.span.is_none_or(|span| span.line > self.lines);
        let text = if in_input {
            if let Some(span) = error.span.as_mut() {
                span.line -= self.lines;
                span.end_line -= self.lines;
            }
            source.to_string()
        } else {
            format!("{}{}", self.sources, source)
        };
        crate::render(&error, &[main_file(text)])
    }
}

/// Returns the main file of the REPL with `source` as its text; paths of `read_file` start at the current folder.
fn main_file(source: String) -> SourceFile {
    SourceFile {
        name: "<repl>".to_string(),
        dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        source,
        stmts: Vec::new(),
    }
}
