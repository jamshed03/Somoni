//! Runs every example in src/explain: a `Wrong:` example must give the code of its section,
//! a `Right:` example must run without any error.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One example of an explanation.
struct Example {
    code: String,
    wrong: bool,
    /// The name of the file it is in, like `page.template.smn`, if the text names one.
    file: Option<String>,
    text: String,
    /// The other files it needs, like `math.smn`, with their text.
    files: Vec<(String, String)>,
}

/// What a line of an explanation starts.
enum Label {
    Example { wrong: bool, file: Option<String> },
    File(String),
}

#[test]
fn every_example_of_the_explanations_does_what_it_says() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/explain");
    let mut texts: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ending| ending == "txt"))
        .collect();
    texts.sort();
    let examples: Vec<Example> = texts
        .iter()
        .flat_map(|path| examples(&fs::read_to_string(path).unwrap()))
        .collect();
    let problems: Vec<String> = examples
        .iter()
        .enumerate()
        .filter_map(|(number, example)| check(example, number))
        .collect();
    assert!(
        examples.len() > 300,
        "only {} examples found",
        examples.len()
    );
    assert!(
        problems.is_empty(),
        "{} of {} examples are wrong:\n\n{}",
        problems.len(),
        examples.len(),
        problems.join("\n\n")
    );
}

/// Returns the examples of all sections `## CODE ...` in `text`.
fn examples(text: &str) -> Vec<Example> {
    let mut examples: Vec<Example> = Vec::new();
    let mut code = String::new();
    let mut block: Option<Vec<String>> = None;
    let mut target: Option<Label> = None;
    for line in text.lines().chain(["## END"]) {
        let label = label(line);
        let ends_block = line.starts_with("## ") || label.is_some() || is_prose(line);
        if ends_block && let (Some(lines), Some(done)) = (block.take(), target.take()) {
            add(&mut examples, &code, done, lines);
        }
        if let Some(rest) = line.strip_prefix("## ") {
            code = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string();
        } else if label.is_some() {
            target = label;
            block = Some(Vec::new());
        } else if let Some(lines) = block.as_mut() {
            lines.push(line.strip_prefix("    ").unwrap_or(line).to_string());
        }
    }
    examples
}

/// Returns true for a line of text, which is neither empty nor part of an indented example.
fn is_prose(line: &str) -> bool {
    !line.is_empty() && !line.starts_with("    ")
}

/// Returns what `line` starts: an example like `Wrong (in page.template.smn):`, or a file like `File math.smn:`.
fn label(line: &str) -> Option<Label> {
    let head = line.strip_suffix(':')?;
    if let Some(name) = head.strip_prefix("File ") {
        return Some(Label::File(name.to_string()));
    }
    let (word, file) = match head.split_once(" (in ") {
        Some((word, rest)) => (word, Some(rest.strip_suffix(')')?.to_string())),
        None => (head, None),
    };
    match word {
        "Wrong" => Some(Label::Example { wrong: true, file }),
        "Right" => Some(Label::Example { wrong: false, file }),
        _ => None,
    }
}

/// Adds a finished block: a new example, or a file of the example before it.
fn add(examples: &mut Vec<Example>, code: &str, label: Label, lines: Vec<String>) {
    let text = lines.join("\n").trim().to_string() + "\n";
    match label {
        Label::Example { wrong, file } => examples.push(Example {
            code: code.to_string(),
            wrong,
            file,
            text,
            files: Vec::new(),
        }),
        Label::File(name) => {
            if let Some(example) = examples.last_mut() {
                example.files.push((name, text));
            }
        }
    }
}

/// Runs `example` in a folder of its own and returns what is wrong with it, if anything.
fn check(example: &Example, number: usize) -> Option<String> {
    let folder =
        std::env::temp_dir().join(format!("somoni_explain_{}_{}", std::process::id(), number));
    fs::create_dir_all(&folder).unwrap();
    let output = run(example, &folder);
    let _ = fs::remove_dir_all(&folder);
    let has_code = output.contains(&format!("[{}]", example.code));
    let kind = if example.wrong { "Wrong" } else { "Right" };
    let problem = if example.wrong {
        (!has_code).then(|| format!("does not give {}", example.code))
    } else {
        output
            .contains(" error [")
            .then(|| "has an error".to_string())
    }?;
    Some(format!(
        "{} {} {}:\n{}\n--- output ---\n{}",
        example.code, kind, problem, example.text, output
    ))
}

/// Writes the example and its files into `folder`, runs it and returns stdout and stderr.
fn run(example: &Example, folder: &Path) -> String {
    for (name, text) in &example.files {
        fs::write(folder.join(name), text).unwrap();
    }
    let is_template = example.text.starts_with(['<', '@']);
    let main = match &example.file {
        Some(file) => file.clone(),
        None if is_template => "main.template.smn".to_string(),
        None => "main.smn".to_string(),
    };
    fs::write(folder.join(&main), &example.text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_somoni"))
        .current_dir(folder)
        .arg(&main)
        .stdin(Stdio::null())
        .output()
        .expect("cannot start somoni");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
