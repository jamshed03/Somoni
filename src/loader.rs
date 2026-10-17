use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::ast::{Import, Stmt};
use crate::error::{ErrorCode, SomoniError};
use crate::lexer::Lexer;
use crate::parser::Parser;

/// One file of the program: its name for messages, its text and its statements.
pub struct SourceFile {
    pub name: String,
    pub source: String,
    pub stmts: Vec<Stmt>,
}

/// Reads the main file and every file it imports, each only once.
/// The number of a file is its place in `files`; 0 is the main file.
#[derive(Default)]
pub struct Loader {
    /// All files read so far. After an error they stay here, so the message can show the line.
    pub files: Vec<SourceFile>,
    /// The number of each file, by its full path.
    numbers: HashMap<PathBuf, usize>,
    /// The files whose imports are being read right now, to find cycles.
    loading: Vec<usize>,
    /// The files in the order to check them: every file after the files it imports.
    order: Vec<usize>,
}

impl Loader {
    /// Reads the program that starts in the file `name` with the text `source`.
    /// Returns the order in which the files must be checked.
    pub fn load(&mut self, name: &str, source: String) -> Result<Vec<usize>, SomoniError> {
        let path = fs::canonicalize(name).map_err(|_| cannot_read(name))?;
        self.add_file(path, name.to_string(), source)?;
        Ok(std::mem::take(&mut self.order))
    }

    /// Lexes and parses one file, then reads the files it imports. Returns its number.
    fn add_file(
        &mut self,
        path: PathBuf,
        name: String,
        source: String,
    ) -> Result<usize, SomoniError> {
        let number = self.files.len();
        self.numbers.insert(path.clone(), number);
        self.files.push(SourceFile {
            name,
            source,
            stmts: Vec::new(),
        });
        let tokens = Lexer::new(&self.files[number].source, number).tokenize()?;
        let mut stmts = Parser::new(tokens).parse_program(number != 0)?;

        self.loading.push(number);
        let dir = path.parent().unwrap_or(Path::new(""));
        for stmt in &mut stmts {
            if let Stmt::Import(import) = stmt {
                import.file = Some(self.import(dir, number, import)?);
            }
        }
        self.loading.pop();

        self.files[number].stmts = stmts;
        self.order.push(number);
        Ok(number)
    }

    /// Finds the file of an import in file `from`, which lies in `dir`, and reads it if it is new.
    /// Returns its number.
    fn import(&mut self, dir: &Path, from: usize, import: &Import) -> Result<usize, SomoniError> {
        if import.path.starts_with('/') || import.path.starts_with('@') {
            return Err(SomoniError::at(
                import.path_span,
                ErrorCode::ReservedPath {
                    path: import.path.clone(),
                },
            ));
        }
        let from_dir = Path::new(&self.files[from].name)
            .parent()
            .unwrap_or(Path::new(""));
        let name = tidy(&from_dir.join(&import.path));
        let not_found = || SomoniError::at(import.path_span, cannot_read(&name));
        let path = fs::canonicalize(dir.join(&import.path)).map_err(|_| not_found())?;

        if let Some(&number) = self.numbers.get(&path) {
            if let Some(start) = self.loading.iter().position(|&n| n == number) {
                let mut cycle: Vec<&str> = self.loading[start..]
                    .iter()
                    .map(|&n| self.files[n].name.as_str())
                    .collect();
                cycle.push(&name);
                return Err(SomoniError::at(
                    import.path_span,
                    ErrorCode::ImportCycle {
                        cycle: cycle.join(" -> "),
                    },
                ));
            }
            return Ok(number);
        }
        let source = fs::read_to_string(&path).map_err(|_| not_found())?;
        self.add_file(path, name, source)
    }
}

/// The error M001 for a file that cannot be read.
fn cannot_read(name: &str) -> ErrorCode {
    ErrorCode::CannotReadFile {
        name: name.to_string(),
    }
}

/// Removes `.` and `x/..` from a path for messages: `modules/text/../zahlen.smn` is
/// `modules/zahlen.smn`.
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
