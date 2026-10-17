use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Exit code of somoni when the VM cannot run a program yet.
const EXIT_NOT_IN_VM: i32 = 3;

/// How a program is run: by the tree interpreter, or by the VM with `--vm`.
#[derive(Clone, Copy)]
enum Mode {
    Tree,
    Vm,
}

/// The result of one run: what is wrong (empty if all is fine), or skipped by the VM.
enum Outcome {
    Checked(Vec<String>),
    Skipped,
}

/// Runs every program in tests/cases with the tree interpreter and with the VM,
/// and compares each run with the .out file of the program.
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
    let mut skipped = 0;
    for program in &programs {
        let name = program.file_name().unwrap().to_string_lossy();
        for (mode, label) in [(Mode::Tree, "tree"), (Mode::Vm, "vm")] {
            match check_program(program, mode) {
                Outcome::Checked(problems) if !problems.is_empty() => {
                    failures.push(format!("{} ({})\n{}", name, label, problems.join("\n")));
                }
                Outcome::Checked(_) => {}
                Outcome::Skipped => skipped += 1,
            }
        }
    }

    eprintln!(
        "VM: {} of {} programs skipped (not in the VM yet)",
        skipped,
        programs.len()
    );
    assert!(
        failures.is_empty(),
        "{} runs failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Runs one program and returns what is wrong: other output or another exit code.
/// `err_*` programs must end with exit code 1, all others with 0.
fn check_program(program: &Path, mode: Mode) -> Outcome {
    let Ok(expected) = fs::read_to_string(program.with_extension("out")) else {
        return Outcome::Checked(vec!["missing .out file".to_string()]);
    };

    let mut command = Command::new(env!("CARGO_BIN_EXE_somoni"));
    if let Mode::Vm = mode {
        command.arg("--vm");
    }
    let output = command.arg(program).output().expect("cannot start somoni");
    if let (Mode::Vm, Some(EXIT_NOT_IN_VM)) = (mode, output.status.code()) {
        return Outcome::Skipped;
    }
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
    Outcome::Checked(problems)
}
