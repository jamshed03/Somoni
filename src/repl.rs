//! The REPL: `somoni` without a file reads Somoni line by line and runs every input right away.
//! It keeps the parsed statements of every input that ran; a new input is checked and compiled together with them,
//! but only its own code runs, on top of the variables the VM kept (see docs on `compile_from` and `run_in`).
//! Errors are shown on stdout like results, because in the REPL they are part of the conversation, and end nothing.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use crate::ast::{Expr, Stmt, VarDecl};
use crate::bytecode::Program;
use crate::compiler;
use crate::error::{ErrorCode, SomoniError};
use crate::lexer::Lexer;
use crate::loader::{MAIN_FILE, SourceFile};
use crate::parser::Parser;
use crate::typechecker::TypeChecker;
use crate::types::Type;
use crate::value::Quoted;
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
    /// Reads, checks, compiles and runs one input, shows the value of an expression, and keeps the input if it ran
    /// without an error, else shows the error.
    fn run_input(&mut self, source: String) {
        let ran = self.build(&source).and_then(|built| {
            vm::run_in(&built.program, &mut self.session)?;
            Ok(built)
        });
        match ran {
            Ok(built) => {
                if let Some((ty, slot)) = &built.shown
                    && let Some(value) = self.session.local(*slot)
                {
                    println!("{}: {}", Quoted(value), ty);
                }
                self.history.extend(built.stmts);
                self.lines += source.lines().count();
                self.sources.push_str(&source);
                self.globals = built.program.globals;
            }
            Err(error) => println!("{}", self.render(error, &source)),
        }
    }

    /// Parses the input, checks and compiles it after the earlier ones. An input that is one expression is kept in
    /// a hidden variable whose value the REPL shows; a call without a value, like `print(x)`, just runs.
    fn build(&self, source: &str) -> Result<Built, SomoniError> {
        match self.parse(source)? {
            Input::Stmts(stmts) => self.compile(stmts, None),
            Input::Expr(expr) => {
                let shown = shown_var(expr.clone());
                let name = shown.name.clone();
                match self.compile(vec![Stmt::VarDecl(shown)], Some(&name)) {
                    Err(error) if matches!(error.code, ErrorCode::VoidVariable) => {
                        let span = expr.span;
                        self.compile(vec![Stmt::Expr { expr, span }], None)
                    }
                    built => built,
                }
            }
        }
    }

    /// Reads the input as statements or, if that fails, as one expression; `import` is M008 for now.
    fn parse(&self, source: &str) -> Result<Input, SomoniError> {
        let tokens = Lexer::new(source, MAIN_FILE)
            .starting_at_line(self.lines + 1)
            .tokenize()?;
        let stmts = match Parser::new(tokens.clone()).parse_program(false) {
            Ok(stmts) => stmts,
            Err(error) => {
                return Parser::new(tokens)
                    .parse_lone_expr()
                    .map(Input::Expr)
                    .map_err(|_| error);
            }
        };
        if let Some(Stmt::Import(import)) =
            stmts.iter().find(|stmt| matches!(stmt, Stmt::Import(_)))
        {
            return Err(SomoniError::at(import.span, ErrorCode::ImportInRepl));
        }
        Ok(Input::Stmts(stmts))
    }

    /// Checks and compiles `stmts` after the earlier inputs; `shown` names the variable whose value the REPL shows.
    fn compile(&self, stmts: Vec<Stmt>, shown: Option<&str>) -> Result<Built, SomoniError> {
        let mut files = [main_file(String::new())];
        files[MAIN_FILE].stmts = self.history.iter().chain(&stmts).cloned().collect();
        let mut checker = TypeChecker::new();
        checker.check(&mut files, &[MAIN_FILE])?;
        let shown = shown.and_then(|name| checker.top_level_var(name));
        let program =
            compiler::compile_from(&files, &[MAIN_FILE], self.history.len(), &self.globals)?;
        Ok(Built {
            program,
            stmts,
            shown,
        })
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

/// What one input holds: statements, or one expression whose value the REPL shows.
enum Input {
    Stmts(Vec<Stmt>),
    Expr(Expr),
}

/// One input ready to run: the program, the statements to keep, and the type and slot of a value to show.
struct Built {
    program: Program,
    stmts: Vec<Stmt>,
    shown: Option<(Type, usize)>,
}

/// Returns `var <it line:col> = expr;`, the hidden variable that holds the value of an expression typed alone.
fn shown_var(expr: Expr) -> VarDecl {
    VarDecl {
        exported: false,
        name: format!("<it {}:{}>", expr.span.line, expr.span.col),
        name_span: expr.span,
        type_name: None,
        span: expr.span,
        value: expr,
        mutable: true,
        slot: None,
        global: None,
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
