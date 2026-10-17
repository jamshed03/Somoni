use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs every program in tests/cases and compares it with its .out file.
/// Files in subfolders (e.g. tests/cases/modules) are only imported, not run.
#[test]
fn smn_programs_match_expected_output() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases");

    let mut programs: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("tests/cases not found")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "smn"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty(), "no .smn files in tests/cases");

    let mut failures = Vec::new();
    for program in &programs {
        let problems = check_program(program);
        if !problems.is_empty() {
            let name = program.file_name().unwrap().to_string_lossy();
            failures.push(format!("{}\n{}", name, problems.join("\n")));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} programs failed:\n\n{}",
        failures.len(),
        programs.len(),
        failures.join("\n\n")
    );
}

/// Runs one program and returns what is wrong: other output or another exit code.
/// `err_*` programs must end with exit code 1, all others with 0.
/// It runs inside tests/cases, so error messages show short paths like `modules/mathe.smn`.
fn check_program(program: &Path) -> Vec<String> {
    let Ok(expected) = fs::read_to_string(program.with_extension("out")) else {
        return vec!["missing .out file".to_string()];
    };

    let (Some(dir), Some(file)) = (program.parent(), program.file_name()) else {
        return vec!["program has no directory".to_string()];
    };
    let output = Command::new(env!("CARGO_BIN_EXE_somoni"))
        .current_dir(dir)
        .arg(file)
        .output()
        .expect("cannot start somoni");
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
