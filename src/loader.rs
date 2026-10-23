use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::ast::{Import, Stmt};
use crate::error::{ErrorCode, SomoniError};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::span::Span;

/// The number of the main file, where the program starts.
const MAIN_FILE: usize = 0;

/// One file of the program: its name for messages, its text and its statements.
pub struct SourceFile {
    pub name: String,
    pub source: String,
    pub stmts: Vec<Stmt>,
}

/// Reads the main file and every file it imports once, and puts each file after its imports.
#[derive(Default)]
pub struct Loader {
    pub files: Vec<SourceFile>,
    number_by_path: HashMap<PathBuf, usize>,
    loading_now: Vec<usize>,
    check_order: Vec<usize>,
}

impl Loader {
    /// Reads the program that starts in file `name` and returns the order to check its files.
    pub fn load(&mut self, name: &str, source: String) -> Result<Vec<usize>, SomoniError> {
        let path = fs::canonicalize(name).map_err(|_| cannot_read(name))?;
        self.add_file(path, name.to_string(), source)?;
        Ok(std::mem::take(&mut self.check_order))
    }

    /// Reads one file and the files it imports, and returns its number.
    fn add_file(
        &mut self,
        path: PathBuf,
        name: String,
        source: String,
    ) -> Result<usize, SomoniError> {
        let number = self.register(path.clone(), name, source);
        let mut stmts = self.parse(number)?;
        self.load_imports(number, &path, &mut stmts)?;
        self.files[number].stmts = stmts;
        self.check_order.push(number);
        Ok(number)
    }

    /// Adds a file without statements yet and returns its number.
    fn register(&mut self, path: PathBuf, name: String, source: String) -> usize {
        let number = self.files.len();
        self.number_by_path.insert(path, number);
        self.files.push(SourceFile {
            name,
            source,
            stmts: Vec::new(),
        });
        number
    }

    /// Lexes and parses one file.
    fn parse(&self, number: usize) -> Result<Vec<Stmt>, SomoniError> {
        let tokens = Lexer::new(&self.files[number].source, number).tokenize()?;
        Parser::new(tokens).parse_program(number != MAIN_FILE)
    }

    /// Reads the files that a file imports and writes their numbers into its imports.
    fn load_imports(
        &mut self,
        number: usize,
        path: &Path,
        stmts: &mut [Stmt],
    ) -> Result<(), SomoniError> {
        self.loading_now.push(number);
        let dir = path.parent().unwrap_or(Path::new(""));
        for stmt in stmts.iter_mut() {
            if let Stmt::Import(import) = stmt {
                import.file = Some(self.import(dir, number, import)?);
            }
        }
        self.loading_now.pop();
        Ok(())
    }

    /// Returns the number of the file an import points to, and reads that file if it is new.
    fn import(&mut self, dir: &Path, from: usize, import: &Import) -> Result<usize, SomoniError> {
        check_import_path(import)?;
        let name = self.import_name(from, import);
        let not_found = || SomoniError::at(import.path_span, cannot_read(&name));
        let path = fs::canonicalize(dir.join(&import.path)).map_err(|_| not_found())?;
        if let Some(&number) = self.number_by_path.get(&path) {
            self.check_no_cycle(number, &name, import.path_span)?;
            return Ok(number);
        }
        let source = fs::read_to_string(&path).map_err(|_| not_found())?;
        self.add_file(path, name, source)
    }

    /// Returns the name of an imported file for messages, seen from the main file.
    fn import_name(&self, from: usize, import: &Import) -> String {
        let from_dir = Path::new(&self.files[from].name)
            .parent()
            .unwrap_or(Path::new(""));
        tidy(&from_dir.join(&import.path))
    }

    /// Returns M002 if file `number` is still loading, because then the import closes a cycle.
    fn check_no_cycle(&self, number: usize, name: &str, span: Span) -> Result<(), SomoniError> {
        let Some(start) = self.loading_now.iter().position(|&n| n == number) else {
            return Ok(());
        };
        let mut cycle: Vec<&str> = self.loading_now[start..]
            .iter()
            .map(|&n| self.files[n].name.as_str())
            .collect();
        cycle.push(name);
        Err(SomoniError::at(
            span,
            ErrorCode::ImportCycle {
                cycle: cycle.join(" -> "),
            },
        ))
    }
}

/// Returns M004 for an import path that starts with the reserved `/` or `@`.
fn check_import_path(import: &Import) -> Result<(), SomoniError> {
    if import.path.starts_with('/') || import.path.starts_with('@') {
        return Err(SomoniError::at(
            import.path_span,
            ErrorCode::ReservedPath {
                path: import.path.clone(),
            },
        ));
    }
    Ok(())
}

/// Returns M001 for a file that cannot be read.
fn cannot_read(name: &str) -> ErrorCode {
    ErrorCode::CannotReadFile {
        name: name.to_string(),
    }
}

/// Removes `.` and `x/..` from a path for messages, so `a/b/../c.smn` becomes `a/c.smn`.
fn tidy(path: &Path) -> String {
    let mut parts: Vec<Component> = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir if matches!(parts.last(), Some(Component::Normal(_))) => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts
        .iter()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}
