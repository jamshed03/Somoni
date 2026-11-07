use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Runs every program in tests/cases and compares it with its .out file.
/// Files in subfolders (e.g. tests/cases/modules) are only imported, not run.
#[test]
fn smn_programs_match_expected_output() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut programs = smn_files(&dir);
    programs.sort();
    check_all(&programs, "tests/cases");
}

/// Runs every program in examples and its subfolders and compares it with its .out file.
/// Files in folders named `lib` are only imported, not run.
#[test]
fn example_programs_match_expected_output() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut programs = smn_files_below(&dir);
    programs.sort();
    check_all(&programs, "examples");
}

/// Returns the .smn files directly in `dir`.
fn smn_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("{} not found", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "smn"))
        .collect()
}

/// Returns the .smn files in `dir` and all its subfolders except those named `lib`.
fn smn_files_below(dir: &Path) -> Vec<PathBuf> {
    let mut files = smn_files(dir);
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() && !path.ends_with("lib") {
            files.extend(smn_files_below(&path));
        }
    }
    files
}

/// Checks all `programs` and fails with every problem found; `place` names the folder in the messages.
fn check_all(programs: &[PathBuf], place: &str) {
    assert!(!programs.is_empty(), "no .smn files in {}", place);
    let mut failures = Vec::new();
    for program in programs {
        let problems = check_program(program);
        if !problems.is_empty() {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let name = program.strip_prefix(root).unwrap_or(program).display();
            failures.push(format!("{}\n{}", name, problems.join("\n")));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} programs in {} failed:\n\n{}",
        failures.len(),
        programs.len(),
        place,
        failures.join("\n\n")
    );
}

/// Runs somoni in `dir` with `file` and gives it `input` as if it were typed, e.g. from `spiel.in` next to `spiel.smn`.
fn run_somoni(dir: &Path, file: &std::ffi::OsStr, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_somoni"))
        .current_dir(dir)
        .arg(file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cannot start somoni");
    let mut stdin = child.stdin.take().expect("no stdin");
    stdin
        .write_all(input.as_bytes())
        .expect("cannot write the input");
    drop(stdin);
    child.wait_with_output().expect("somoni did not finish")
}

/// Runs one program and returns what is wrong: other output or another exit code.
/// `err_*` programs must end with exit code 1, all others with 0.
/// It runs inside tests/cases, so error messages show short paths like `modules/math.smn`.
fn check_program(program: &Path) -> Vec<String> {
    let Ok(expected) = fs::read_to_string(program.with_extension("out")) else {
        return vec!["missing .out file".to_string()];
    };

    let (Some(dir), Some(file)) = (program.parent(), program.file_name()) else {
        return vec!["program has no directory".to_string()];
    };
    let input = fs::read_to_string(program.with_extension("in")).unwrap_or_default();
    let output = run_somoni(dir, file, &input);
    let actual = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut problems = Vec::new();
    if actual != expected {
        problems.push(format!(
            "--- expected ---\n{}--- actual ---\n{}",
            expected, actual
        ));
    }

    let name = program.file_name().unwrap().to_string_lossy();
    let expected_code = if name.starts_with("err_") { 1 } else { 0 };
    if output.status.code() != Some(expected_code) {
        problems.push(format!(
            "exit code {:?}, expected {}",
            output.status.code(),
            expected_code
        ));
    }
    problems
}
