//! Runs the REPL (`somoni` without a file) with the lines of every `tests/repl/name.in` as input and compares what it
//! shows with `name.out`. Without a terminal the REPL shows no prompt, so the output is only results and errors.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn every_repl_session_matches_its_expected_output() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/repl");
    let mut sessions: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ending| ending == "in"))
        .collect();
    sessions.sort();
    assert!(!sessions.is_empty(), "no .in files in tests/repl");
    let problems: Vec<String> = sessions
        .iter()
        .filter_map(|session| {
            let expected = fs::read_to_string(session.with_extension("out")).unwrap_or_default();
            let actual = run_repl(&dir, &fs::read_to_string(session).unwrap());
            let name = session.file_name().unwrap().to_string_lossy();
            (actual != expected).then(|| {
                format!(
                    "{}\n--- expected ---\n{}--- actual ---\n{}",
                    name, expected, actual
                )
            })
        })
        .collect();
    assert!(
        problems.is_empty(),
        "{} of {} sessions fail:\n\n{}",
        problems.len(),
        sessions.len(),
        problems.join("\n\n")
    );
}

/// Starts the REPL in `dir`, types `input` into it and returns stdout followed by stderr; it must end with code 0.
fn run_repl(dir: &PathBuf, input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_somoni"))
        .current_dir(dir)
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
    let output = child.wait_with_output().expect("somoni did not finish");
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    match output.status.code() {
        Some(0) => shown,
        code => format!("{}(exit code {:?})\n", shown, code),
    }
}
