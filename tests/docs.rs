//! Runs every Somoni block of the documentation site, so no example gets old: ```smn and ```smn-bad must run without
//! an error, ```smn-wrong T014 must give exactly that error. Blocks marked ```text or ```template are only shown.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// What a block must do when it runs.
enum Expect {
    Run,
    Fail(String),
}

/// One block of a page, with the page it is on and what it must do.
struct Block {
    page: String,
    code: String,
    expect: Expect,
}

#[test]
fn every_smn_block_of_the_documentation_does_what_it_says() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/projects/docs_site/content");
    let mut pages: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ending| ending == "md"))
        .collect();
    pages.sort();
    let mut blocks = Vec::new();
    let mut problems = Vec::new();
    for page in &pages {
        let name = page.file_name().unwrap().to_string_lossy().to_string();
        for (info, code) in fenced_blocks(&fs::read_to_string(page).unwrap()) {
            match expectation(&info) {
                Some(expect) => blocks.push(Block {
                    page: name.clone(),
                    code,
                    expect,
                }),
                None if info.starts_with("smn") => {
                    problems.push(format!("{}: unknown block kind ```{}", name, info))
                }
                None => {}
            }
        }
    }
    problems.extend(
        blocks
            .iter()
            .enumerate()
            .filter_map(|(number, block)| problem(block, &run(&block.code, number))),
    );
    assert!(!blocks.is_empty(), "no ```smn blocks found");
    assert!(
        problems.is_empty(),
        "{} of {} blocks fail:\n\n{}",
        problems.len(),
        blocks.len(),
        problems.join("\n\n")
    );
}

/// Returns what a block with this info string (the text after ```) must do, or None for one that is only shown.
fn expectation(info: &str) -> Option<Expect> {
    let words: Vec<&str> = info.split_whitespace().collect();
    match words.as_slice() {
        ["smn"] | ["smn-bad"] => Some(Expect::Run),
        ["smn-wrong", code] => Some(Expect::Fail(code.to_string())),
        _ => None,
    }
}

/// Returns why `block` did not do what it must, judged by its `output`, or None if it did.
fn problem(block: &Block, output: &str) -> Option<String> {
    let (failed, wanted) = match &block.expect {
        Expect::Run => (output.contains(" error ["), "no error".to_string()),
        Expect::Fail(code) => (
            !output.contains(&format!(" error [{}]", code)),
            format!("error {}", code),
        ),
    };
    failed.then(|| {
        format!(
            "{} (wanted {}):\n{}\n--- output ---\n{}",
            block.page, wanted, block.code, output
        )
    })
}

/// Returns the info string and the code of every fenced block (```info ... ```) in the Markdown `text`.
fn fenced_blocks(text: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(String, Vec<&str>)> = None;
    for line in text.lines() {
        match (&mut open, line.trim_end()) {
            (Some((info, lines)), "```") => {
                blocks.push((info.clone(), lines.join("\n") + "\n"));
                open = None;
            }
            (Some((_, lines)), _) => lines.push(line),
            (None, fence) if fence.starts_with("```") => {
                open = Some((fence[3..].trim().to_string(), Vec::new()))
            }
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
