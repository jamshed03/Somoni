use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::ast::{Import, Stmt};
use crate::error::{ErrorCode, SomoniError};
use crate::lexer::{Lexer, is_template};
use crate::parser::Parser;
use crate::span::Span;

/// The number of the main file, where the program starts.
pub const MAIN_FILE: usize = 0;

/// One file of the program: its name for messages, its folder for `read_file`, its text and its statements.
#[derive(Clone)]
pub struct SourceFile {
    pub name: String,
    pub dir: PathBuf,
    pub source: String,
    pub stmts: Vec<Stmt>,
}

/// Reads the main file and every file it imports once, and puts each file after its imports.
#[derive(Default, Clone)]
pub struct Loader {
    pub files: Vec<SourceFile>,
    number_by_path: HashMap<PathBuf, usize>,
    /// The file number for a folder joined with an import path, so `canonicalize` runs once for each.
    number_by_import: HashMap<PathBuf, usize>,
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

    /// Starts a program whose main file is typed in, like in the REPL; its imports start at the folder `dir`.
    pub fn typed(dir: PathBuf) -> Loader {
        let main = SourceFile {
            name: "<repl>".to_string(),
            dir,
            source: String::new(),
            stmts: Vec::new(),
        };
        Loader {
            files: vec![main],
            ..Loader::default()
        }
    }

    /// Reads the files that the typed statements `stmts` import and returns the order to check all files, main last.
    pub fn load_typed_imports(&mut self, stmts: &mut [Stmt]) -> Result<Vec<usize>, SomoniError> {
        let dir = self.files[MAIN_FILE].dir.clone();
        self.load_imports(MAIN_FILE, &dir, stmts)?;
        Ok(self
            .check_order
            .iter()
            .copied()
            .chain([MAIN_FILE])
            .collect())
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
        self.load_imports(number, path.parent().unwrap_or(Path::new("")), &mut stmts)?;
        self.files[number].stmts = stmts;
        self.check_order.push(number);
        Ok(number)
    }

    /// Adds a file without statements yet and returns its number.
    fn register(&mut self, path: PathBuf, name: String, source: String) -> usize {
        let number = self.files.len();
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        self.number_by_path.insert(path, number);
        self.files.push(SourceFile {
            name,
            dir,
            source,
            stmts: Vec::new(),
        });
        number
    }

    /// Lexes and parses one file.
    fn parse(&self, number: usize) -> Result<Vec<Stmt>, SomoniError> {
        let file = &self.files[number];
        let mut lexer = Lexer::new(&file.source, number);
        let tokens = if is_template(&file.name) {
            lexer.tokenize_template(&file.name)?
        } else {
            lexer.tokenize()?
        };
        Parser::new(tokens).parse_program(number != MAIN_FILE)
    }

    /// Reads the files that file `number` in the folder `dir` imports and writes their numbers into its imports.
    fn load_imports(
        &mut self,
        number: usize,
        dir: &Path,
        stmts: &mut [Stmt],
    ) -> Result<(), SomoniError> {
        self.loading_now.push(number);
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
        let joined = dir.join(&import.path);
        if let Some(&number) = self.number_by_import.get(&joined) {
            self.check_no_cycle(number, &name, import.path_span)?;
            return Ok(number);
        }
        let path = fs::canonicalize(&joined).map_err(|_| not_found())?;
        if let Some(&number) = self.number_by_path.get(&path) {
            self.check_no_cycle(number, &name, import.path_span)?;
            self.number_by_import.insert(joined, number);
            return Ok(number);
        }
        let source = fs::read_to_string(&path).map_err(|_| not_found())?;
        let number = self.add_file(path, name, source)?;
        self.number_by_import.insert(joined, number);
        Ok(number)
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

/// Removes `.` and `x/..` from a path for messages, so `a/b/../c.smn` becomes `a/c.smn`; always with `/`, also on
/// Windows, so a message is the same on every system.
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
        .map(|part| match part {
            Component::RootDir => String::new(),
            other => other.as_os_str().to_string_lossy().into_owned(),
        })
        .collect::<Vec<String>>()
        .join("/")
}

/// Returns the relative path from the folder `from` to the folder `to`, like `"../../"`, with `/` at the end and `""`
/// for the same folder; both must be canonical. Without a shared start (another drive on Windows) it is `to` itself.
pub fn relative_dir(from: &Path, to: &Path) -> String {
    let from_parts: Vec<Component> = from.components().collect();
    let to_parts: Vec<Component> = to.components().collect();
    let shared = from_parts
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if shared == 0 {
        return format!("{}/", to.to_string_lossy());
    }
    let ups = (shared..from_parts.len()).map(|_| "../".to_string());
    let downs = to_parts[shared..]
        .iter()
        .map(|part| format!("{}/", part.as_os_str().to_string_lossy()));
    ups.chain(downs).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_dir_goes_up_and_down() {
        let dir = |text: &str| PathBuf::from(text);
        assert_eq!(relative_dir(&dir("/app"), &dir("/app")), "");
        assert_eq!(
            relative_dir(&dir("/app/lib/adapters"), &dir("/app")),
            "../../"
        );
        assert_eq!(
            relative_dir(&dir("/shared/lib"), &dir("/app/web")),
            "../../app/web/"
        );
        assert_eq!(relative_dir(&dir("/app"), &dir("/app/web")), "web/");
    }

    #[test]
    fn tidy_removes_dots_and_joins_with_slashes() {
        assert_eq!(tidy(Path::new("a/b/../c.smn")), "a/c.smn");
        assert_eq!(tidy(Path::new("./modules/./x.smn")), "modules/x.smn");
        assert_eq!(tidy(Path::new("../x.smn")), "../x.smn");
        assert_eq!(tidy(Path::new("/home/ali/x.smn")), "/home/ali/x.smn");
    }
}
