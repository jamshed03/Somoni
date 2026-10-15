// Runs every tests/cases/*.smn

use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
        let name = program.file_name().unwrap().to_string_lossy().to_string();

        let expected = match fs::read_to_string(program.with_extension("out")) {
            Ok(text) => text,
            Err(_) => {
                failures.push(format!("{}: missing .out file", name));
                continue;
            }
        };

        // CARGO_BIN_EXE_somoni = path to the compiled binary (set by cargo for tests)
        let output = Command::new(env!("CARGO_BIN_EXE_somoni"))
            .arg(program)
            .output()
            .expect("cannot start somoni");
        let actual = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        if actual != expected {
            failures.push(format!(
                "{}\n--- expected ---\n{}--- actual ---\n{}",
                name, expected, actual
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} programs failed:\n\n{}",
        failures.len(),
        programs.len(),
        failures.join("\n")
    );
}
