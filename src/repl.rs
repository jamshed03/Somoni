//! The REPL: `somoni` without a file reads Somoni line by line and runs every input right away.
//! Errors go to stdout like results, because in the REPL they are part of the conversation, and end nothing.

use std::io::{self, BufRead, IsTerminal, Stdin, Write};
use std::path::PathBuf;

use crate::ast::{Expr, Stmt, VarDecl};
use crate::bytecode::Program;
use crate::compiler;
use crate::error::{ErrorCode, SomoniError};
use crate::lexer::Lexer;
use crate::loader::{Loader, MAIN_FILE, SourceFile};
use crate::parser::Parser;
use crate::token::{SpannedToken, Token};
use crate::typechecker::TypeChecker;
use crate::types::Type;
use crate::value::Quoted;
use crate::vm::{self, Session};

/// The prompt before an input, shown only when a person types into a terminal.
const PROMPT: &str = "> ";

/// The prompt before another line of an input that is not complete yet.
const MORE: &str = ". ";

/// Runs every input until the input ends (Ctrl+D).
pub fn run() {
    let stdin = io::stdin();
    let terminal = stdin.is_terminal();
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut repl = Repl::new(Loader::typed(dir));
    while let Some(input) = read_input(&stdin, terminal) {
        repl.run_input(input);
    }
    if terminal {
        println!();
    }
}

/// Reads one input, over several lines while it is not complete, or None when the input has ended.
/// An empty line sends an input that is not complete anyway, so a missing `)` cannot trap anyone.
fn read_input(stdin: &Stdin, terminal: bool) -> Option<String> {
    let mut input = String::new();
    loop {
        if terminal {
            print!("{}", if input.is_empty() { PROMPT } else { MORE });
            let _ = io::stdout().flush();
        }
        let mut line = String::new();
        if matches!(stdin.lock().read_line(&mut line), Ok(0) | Err(_)) {
            return (!input.is_empty()).then_some(input);
        }
        let blank = line.trim().is_empty();
        if blank && input.is_empty() {
            continue;
        }
        input.push_str(&line);
        if blank || is_complete(&input) {
            return Some(input);
        }
    }
}

/// Returns false while a bracket, a string or a `/* */` comment in `source` is still open.
fn is_complete(source: &str) -> bool {
    match Lexer::new(source, MAIN_FILE).tokenize() {
        Ok(tokens) => {
            let depth: i64 = tokens
                .iter()
                .map(|spanned| match spanned.token {
                    Token::LParen | Token::LBrace | Token::LBracket => 1,
                    Token::RParen | Token::RBrace | Token::RBracket => -1,
                    _ => 0,
                })
                .sum();
            depth <= 0
        }
        Err(error) => !matches!(
            error.code,
            ErrorCode::UnterminatedString | ErrorCode::UnterminatedComment
        ),
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
    /// The globals that have their values already, whose start values must not run again.
    known_globals: Vec<String>,
    /// The modules imported with `import * as name`, so `name.square(3)` reads as one name in later inputs.
    namespaces: Vec<String>,
    /// The typed main file and the files that inputs imported, not checked yet.
    loader: Loader,
    session: Session,
}

impl Repl {
    /// Starts with no inputs yet; `loader` holds the typed main file.
    fn new(loader: Loader) -> Repl {
        Repl {
            history: Vec::new(),
            sources: String::new(),
            lines: 0,
            known_globals: Vec::new(),
            namespaces: Vec::new(),
            loader,
            session: Session::default(),
        }
    }

    /// Runs one input, shows its value or its error, and keeps it if it ran without an error.
    /// It works on a copy of the loader, so a file that failed to load is read again by the next import.
    fn run_input(&mut self, source: String) {
        let mut loader = self.loader.clone();
        let ran = self.build(&source, &mut loader).and_then(|built| {
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
                self.namespaces
                    .extend(built.stmts.iter().filter_map(namespace_of));
                self.history.extend(built.stmts);
                self.lines += source.lines().count();
                self.sources.push_str(&source);
                self.known_globals = built.program.globals;
                self.loader = loader;
            }
            Err(error) => println!("{}", self.render(error, &source, &loader.files)),
        }
    }

    /// Reads the files one input imports, then checks and compiles it after the earlier ones; an expression alone
    /// goes into a hidden variable to show.
    fn build(&self, source: &str, loader: &mut Loader) -> Result<Built, SomoniError> {
        match self.parse(source)? {
            Input::Stmts(mut stmts) => {
                let order = loader.load_typed_imports(&mut stmts)?;
                self.compile(stmts, None, loader, &order)
            }
            Input::Expr(expr) => {
                let order = loader.load_typed_imports(&mut [])?;
                let shown = shown_var(expr.clone());
                let name = shown.name.clone();
                match self.compile(vec![Stmt::VarDecl(shown)], Some(&name), loader, &order) {
                    Err(error) if matches!(error.code, ErrorCode::VoidVariable) => {
                        let span = expr.span;
                        self.compile(vec![Stmt::Expr { expr, span }], None, loader, &order)
                    }
                    built => built,
                }
            }
        }
    }

    /// Reads the input as statements, else as one expression, else as statements with a `;` at the end, and shows
    /// the first error if none fits.
    fn parse(&self, source: &str) -> Result<Input, SomoniError> {
        let tokens = Lexer::new(source, MAIN_FILE)
            .starting_at_line(self.lines + 1)
            .tokenize()?;
        let parser = |tokens| Parser::new(tokens).with_namespaces(&self.namespaces);
        let stmts = match parser(tokens.clone()).parse_program(false) {
            Ok(stmts) => stmts,
            Err(error) => {
                if let Ok(expr) = parser(tokens.clone()).parse_lone_expr() {
                    return Ok(Input::Expr(expr));
                }
                parser(with_semicolon(tokens))
                    .parse_program(false)
                    .map_err(|_| error)?
            }
        };
        Ok(Input::Stmts(stmts))
    }

    /// Checks and compiles `stmts` after the earlier inputs, with the files of `loader` in `order`; `shown` names the
    /// variable whose value the REPL shows.
    fn compile(
        &self,
        stmts: Vec<Stmt>,
        shown: Option<&str>,
        loader: &Loader,
        order: &[usize],
    ) -> Result<Built, SomoniError> {
        let mut files = loader.files.clone();
        files[MAIN_FILE].stmts = self.history.iter().chain(&stmts).cloned().collect();
        let mut checker = TypeChecker::new();
        checker.check(&mut files, order)?;
        let shown = shown.and_then(|name| checker.top_level_var(name));
        let program =
            compiler::compile_from(&files, order, self.history.len(), &self.known_globals)?;
        Ok(Built {
            program,
            stmts,
            shown,
        })
    }

    /// Returns the error ready to show: in an imported file like in a program, else with its line counted inside the
    /// input, or in all inputs if it lies in an earlier one.
    fn render(&self, mut error: SomoniError, source: &str, files: &[SourceFile]) -> String {
        if error.span.is_some_and(|span| span.file != MAIN_FILE) {
            return crate::render(&error, files);
        }
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
        let main = SourceFile {
            source: text,
            ..files[MAIN_FILE].clone()
        };
        crate::render(&error, &[main])
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

/// Returns the name of the module that `import * as name` brings in, for other statements None.
fn namespace_of(stmt: &Stmt) -> Option<String> {
    match stmt {
        Stmt::Import(import) => import.namespace.as_ref().map(|(name, _)| name.clone()),
        _ => None,
    }
}

/// Returns `tokens` with a `;` before the end, for an input like `var x = 5` without it.
fn with_semicolon(mut tokens: Vec<SpannedToken>) -> Vec<SpannedToken> {
    let end = tokens.len() - 1;
    let semicolon = SpannedToken {
        token: Token::Semicolon,
        span: tokens[end].span,
    };
    tokens.insert(end, semicolon);
    tokens
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_brackets_strings_and_comments_wait_for_more_lines() {
        assert!(!is_complete("fn twice(n: int): int {\n"));
        assert!(!is_complete("var names = [\n    \"Ali\",\n"));
        assert!(!is_complete("print(\"a\"\n"));
        assert!(!is_complete("var text = \"\"\"\n    raw\n"));
        assert!(!is_complete("/* a comment\n"));
    }

    #[test]
    fn closed_or_too_many_brackets_are_complete() {
        assert!(is_complete("print(1);\n"));
        assert!(is_complete("fn f(): int {\n    return 1;\n}\n"));
        assert!(is_complete("print(\"(\");\n"));
        assert!(is_complete("}\n"));
        assert!(is_complete("print($);\n"));
    }

    #[test]
    fn a_semicolon_goes_before_the_end() {
        let tokens = Lexer::new("var x = 5", MAIN_FILE).tokenize().unwrap();
        let kinds: Vec<Token> = with_semicolon(tokens)
            .into_iter()
            .map(|spanned| spanned.token)
            .collect();
        assert_eq!(&kinds[kinds.len() - 2..], &[Token::Semicolon, Token::Eof]);
    }
}
