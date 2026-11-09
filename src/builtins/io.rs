//! Talking to the outside of the program: `input()` reads a line that the user types, `read_file`, `write_file`,
//! `append_file` and `copy_file` work with whole files, `make_dir` makes folders, `now()` reads the clock. `print` and `write` are written by the VM (`vm/output.rs`).
//! Under `somoni serve`, `refusal` keeps the file functions inside the served folder.

use std::io::{BufRead, ErrorKind, Write};
use std::path::{Component, Path};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{ErrorCode, SomoniError};
use crate::text::Text;
use crate::value::Value;

use super::string_value;

/// Reads one line without its line break, or returns `null` when the input has ended (Ctrl+D or the end of a file).
pub(super) fn read_line() -> Result<Value, SomoniError> {
    let mut line = String::new();
    let read = std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| ErrorCode::InputFailed {
            reason: error.to_string(),
        })?;
    if read == 0 {
        return Ok(Value::Null);
    }
    let text = line.strip_suffix('\n').unwrap_or(&line);
    let text = text.strip_suffix('\r').unwrap_or(text);
    Ok(string_value(text.to_string()))
}

/// Reads the file `path`, which starts at the folder `dir`, as `ok(text)`, or `err(reason)` if that is not possible.
pub(super) fn read_file(dir: &str, path: &str) -> Value {
    match std::fs::read_to_string(Path::new(dir).join(path)) {
        Ok(text) => Value::Ok(Rc::new(string_value(text))),
        Err(error) => failed("read", path, error.kind()),
    }
}

/// Writes `text` into the file `path`, which starts at the folder `dir`, as `ok(characters)`, or `err(reason)`.
pub(super) fn write_file(dir: &str, path: &str, text: &Text) -> Value {
    match std::fs::write(Path::new(dir).join(path), text.as_str()) {
        Ok(()) => Value::Ok(Rc::new(Value::Int(text.char_count() as i64))),
        Err(error) => failed("write", path, error.kind()),
    }
}

/// Adds `text` to the end of the file `path`, which starts at the folder `dir` and is made if it is missing, as
/// `ok(characters)`, or `err(reason)`.
pub(super) fn append_file(dir: &str, path: &str, text: &Text) -> Value {
    let appended = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(Path::new(dir).join(path))
        .and_then(|mut file| file.write_all(text.as_str().as_bytes()));
    match appended {
        Ok(()) => Value::Ok(Rc::new(Value::Int(text.char_count() as i64))),
        Err(error) => failed("write", path, error.kind()),
    }
}

/// Makes the folder `path` and the folders above it that are missing, as `ok(true)`, `ok(false)` if it was there
/// already, or `err(reason)`.
pub(super) fn make_dir(dir: &str, path: &str) -> Value {
    let full = Path::new(dir).join(path);
    if full.is_dir() {
        return Value::Ok(Rc::new(Value::Bool(false)));
    }
    match std::fs::create_dir_all(full) {
        Ok(()) => Value::Ok(Rc::new(Value::Bool(true))),
        Err(error) => failed("make", path, error.kind()),
    }
}

/// Copies the file `from` to `to`, both starting at the folder `dir`, as `ok(bytes)`, or `err(reason)`.
/// Copies raw bytes rather than text, so it also works for files like `.png` that are not valid UTF-8.
pub(super) fn copy_file(dir: &str, from: &str, to: &str) -> Value {
    let bytes = match std::fs::read(Path::new(dir).join(from)) {
        Ok(bytes) => bytes,
        Err(error) => return failed("read", from, error.kind()),
    };
    match std::fs::write(Path::new(dir).join(to), &bytes) {
        Ok(()) => Value::Ok(Rc::new(Value::Int(bytes.len() as i64))),
        Err(error) => failed("write", to, error.kind()),
    }
}

/// Returns the milliseconds since 1970-01-01 (UTC), or 0 if the clock of the system stands before that.
pub(super) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// Returns `err("cannot read 'path': not found")` and the like, with a reason that is the same on every system.
fn failed(action: &str, path: &str, kind: ErrorKind) -> Value {
    let reason = match kind {
        ErrorKind::NotFound => "not found",
        ErrorKind::PermissionDenied => "no permission",
        ErrorKind::InvalidData => "not UTF-8 text",
        ErrorKind::IsADirectory => "it is a folder",
        _ => "the system refused",
    };
    cannot(action, path, reason)
}

/// Returns `err("cannot <action> '<path>': <reason>")`.
pub(super) fn cannot(action: &str, path: &str, reason: &str) -> Value {
    let message = format!("cannot {} '{}': {}", action, path, reason);
    Value::Err(Rc::new(string_value(message)))
}

/// How a file function uses a path, which decides what `refusal` checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Access {
    Read,
    MakeFolder,
    WriteFile,
}

impl Access {
    /// Returns the verb for the message, like `read` in `cannot read 'x'`.
    pub(super) fn action(self) -> &'static str {
        match self {
            Access::Read => "read",
            Access::MakeFolder => "make",
            Access::WriteFile => "write",
        }
    }
}

/// The reason for a path that leaves the served folder.
pub(super) const OUTSIDE: &str = "outside the served folder";

/// The reason for writing a file whose path holds `.smn`, which could then run as a page.
pub(super) const SOURCE: &str = "somoni serve does not write .smn files";

/// Returns why `path`, which starts at the folder `dir`, may not be used under `somoni serve` with the served folder
/// `root`, or None if it may. A written path may not hold `.smn` in any case, because Windows makes `hack.smn` also
/// from `hack.smn.` and `hack.smn::$DATA`.
pub(super) fn refusal(root: &Path, dir: &str, path: &str, access: Access) -> Option<&'static str> {
    if access == Access::WriteFile && path.to_lowercase().contains(".smn") {
        return Some(SOURCE);
    }
    (!stays_inside(root, &Path::new(dir).join(path))).then_some(OUTSIDE)
}

/// Returns true if `path` lies in `root`: its nearest part that exists (a link too, even one that leads nowhere) must
/// really be inside `root`, and the rest that does not exist yet may not hold `..`.
fn stays_inside(root: &Path, path: &Path) -> bool {
    let Some(existing) = path
        .ancestors()
        .find(|part| part.symlink_metadata().is_ok())
    else {
        return false;
    };
    let rest = path.strip_prefix(existing).unwrap_or(path);
    let rest_is_plain = rest
        .components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    let real_inside = existing
        .canonicalize()
        .is_ok_and(|real| real.starts_with(root));
    rest_is_plain && real_inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Makes a fresh served folder `name` with `notes.txt`, `sub/` and `data/` next to a file `outside.txt` that
    /// lies outside of it, and returns the real path of the served folder; each test has its own.
    fn site(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("somoni_io_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("site");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("notes.txt"), "x").unwrap();
        fs::write(base.join("outside.txt"), "x").unwrap();
        root.canonicalize().unwrap()
    }

    /// Returns the reason for `path` from the folder `root`, or None if it may be used.
    fn check(root: &Path, path: &str, access: Access) -> Option<&'static str> {
        refusal(root, &root.to_string_lossy(), path, access)
    }

    #[test]
    fn paths_inside_the_served_folder_are_allowed() {
        let root = site("inside");
        assert_eq!(check(&root, "notes.txt", Access::Read), None);
        assert_eq!(check(&root, "sub/../notes.txt", Access::Read), None);
        assert_eq!(check(&root, "data/new.txt", Access::WriteFile), None);
        assert_eq!(check(&root, "a/b/c", Access::MakeFolder), None);
        let from_sub = refusal(
            &root,
            &root.join("sub").to_string_lossy(),
            "../notes.txt",
            Access::Read,
        );
        assert_eq!(from_sub, None);
    }

    #[test]
    fn paths_out_of_the_served_folder_are_refused() {
        let root = site("outside");
        assert_eq!(check(&root, "../outside.txt", Access::Read), Some(OUTSIDE));
        assert_eq!(check(&root, "/etc/passwd", Access::Read), Some(OUTSIDE));
        assert_eq!(
            check(&root, "new/../../x.txt", Access::WriteFile),
            Some(OUTSIDE)
        );
        assert_eq!(check(&root, "../made", Access::MakeFolder), Some(OUTSIDE));
    }

    #[test]
    fn source_files_are_never_written() {
        let root = site("source");
        for path in [
            "hack.smn",
            "Hack.SMN",
            "hack.template.smn",
            "data/hack.smn",
            "hack.smn.",
            "hack.smn::$DATA",
        ] {
            assert_eq!(
                check(&root, path, Access::WriteFile),
                Some(SOURCE),
                "{}",
                path
            );
        }
        assert_eq!(check(&root, "hack.smn", Access::Read), None);
    }

    #[cfg(unix)]
    #[test]
    fn links_out_of_the_served_folder_are_refused() {
        let root = site("links");
        let outside = root.parent().unwrap();
        std::os::unix::fs::symlink(outside, root.join("away")).unwrap();
        std::os::unix::fs::symlink(outside.join("new.txt"), root.join("dangling.txt")).unwrap();
        assert_eq!(
            check(&root, "away/outside.txt", Access::Read),
            Some(OUTSIDE)
        );
        assert_eq!(
            check(&root, "away/new.txt", Access::WriteFile),
            Some(OUTSIDE)
        );
        assert_eq!(
            check(&root, "dangling.txt", Access::WriteFile),
            Some(OUTSIDE)
        );
    }
}
