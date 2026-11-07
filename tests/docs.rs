//! Runs every ```smn block of the documentation site: each must run without an error, so no example gets old.
//! Blocks marked ```text or ```template are only shown, not run.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn every_smn_block_of_the_documentation_runs() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/projects/docs_site/content");
    let mut pages: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ending| ending == "md"))
        .collect();
    pages.sort();
    let blocks: Vec<(String, String)> = pages
        .iter()
        .flat_map(|page| {
            let name = page.file_name().unwrap().to_string_lossy().to_string();
            smn_blocks(&fs::read_to_string(page).unwrap())
                .into_iter()
                .map(move |block| (name.clone(), block))
        })
        .collect();
    let problems: Vec<String> = blocks
        .iter()
        .enumerate()
        .filter_map(|(number, (page, block))| {
            let output = run(block, number);
            output
                .contains(" error [")
                .then(|| format!("{}:\n{}\n--- output ---\n{}", page, block, output))
        })
        .collect();
    assert!(!blocks.is_empty(), "no ```smn blocks found");
    assert!(
        problems.is_empty(),
        "{} of {} blocks fail:\n\n{}",
        problems.len(),
        blocks.len(),
        problems.join("\n\n")
    );
}

/// Returns the code of every block that starts with ```smn in the Markdown `text`.
fn smn_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<Vec<&str>> = None;
    for line in text.lines() {
        match (&mut open, line.trim_end()) {
            (Some(lines), "```") => {
                blocks.push(lines.join("\n") + "\n");
                open = None;
            }
            (Some(lines), _) => lines.push(line),
            (None, "```smn") => open = Some(Vec::new()),
            (None, _) => {}
        }
    }
    blocks
}

/// Runs `code` as a program in a folder of its own and returns stdout and stderr.
fn run(code: &str, number: usize) -> String {
    let folder =
        std::env::temp_dir().join(format!("somoni_docs_{}_{}", std::process::id(), number));
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("main.smn"), code).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_somoni"))
        .current_dir(&folder)
        .arg("main.smn")
        .stdin(Stdio::null())
        .output()
        .expect("cannot start somoni");
    let _ = fs::remove_dir_all(&folder);
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
