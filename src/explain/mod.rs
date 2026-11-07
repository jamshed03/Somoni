//! `somoni --explain CODE`: a longer explanation of an error code with an example, from the text files next to this one.

use crate::error::ErrorKind;

/// The explanations, one text per kind of error, by the first letter of its codes.
const TEXTS: &[(char, &str)] = &[
    ('L', include_str!("lexer.txt")),
    ('P', include_str!("parser.txt")),
    ('M', include_str!("modules.txt")),
    ('T', include_str!("types.txt")),
    ('R', include_str!("runtime.txt")),
];

/// Returns the explanation of `code` (like `t003` or `T003`) ready to print, or why there is none.
pub fn explain(code: &str) -> Result<String, String> {
    let code = code.trim().to_ascii_uppercase();
    let Some((letter, kind)) = kind_of(&code) else {
        return Err(format!(
            "'{}' is no error code; codes look like T003 or R001",
            code
        ));
    };
    let section = TEXTS
        .iter()
        .find(|(found, _)| *found == letter)
        .and_then(|(_, text)| section(text, &code));
    match section {
        Some((title, body)) => Ok(format!(
            "{} ({}): {}\n\n{}\n",
            code,
            kind.label(),
            title,
            body
        )),
        None => Err(format!("{} has no explanation", code)),
    }
}

/// Returns the letter and the kind of a code like `T003`: a known letter and three digits.
fn kind_of(code: &str) -> Option<(char, ErrorKind)> {
    let mut chars = code.chars();
    let letter = chars.next()?;
    let digits: Vec<char> = chars.collect();
    let kind = ErrorKind::from_letter(letter)?;
    (digits.len() == 3 && digits.iter().all(char::is_ascii_digit)).then_some((letter, kind))
}

/// Returns the title and the text of the section `## CODE title` in `text`, up to the next `## `.
fn section(text: &str, code: &str) -> Option<(String, String)> {
    let mut lines = text.lines();
    let title = lines.find_map(|line| {
        let rest = line.strip_prefix("## ")?;
        let (found, title) = rest.split_once(' ').unwrap_or((rest, ""));
        (found == code).then(|| title.trim().to_string())
    })?;
    let body: Vec<&str> = lines.take_while(|line| !line.starts_with("## ")).collect();
    Some((title, body.join("\n").trim().to_string()))
}

/// Returns the codes of all sections in `text`, like `L001`.
#[cfg(test)]
fn codes_in(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("## "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kinds of errors whose codes all have an explanation already.
    const FINISHED: &str = "LPMR";

    /// Returns every code that `error.rs` gives to an error, like `L001`.
    fn codes_of_errors() -> Vec<String> {
        include_str!("../error.rs")
            .split("=> \"")
            .skip(1)
            .filter_map(|rest| rest.get(..4))
            .filter(|code| kind_of(code).is_some())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn every_error_of_a_finished_kind_has_an_explanation() {
        let missing: Vec<String> = codes_of_errors()
            .into_iter()
            .filter(|code| code.starts_with(|c| FINISHED.contains(c)))
            .filter(|code| explain(code).is_err())
            .collect();
        assert!(missing.is_empty(), "no explanation for {:?}", missing);
    }

    #[test]
    fn every_explanation_belongs_to_an_error() {
        let codes = codes_of_errors();
        for (_, text) in TEXTS {
            for code in codes_in(text) {
                assert!(
                    codes.contains(&code),
                    "{} is explained but no error has it",
                    code
                );
            }
        }
    }

    #[test]
    fn codes_are_read_in_any_case() {
        assert!(explain("l001").unwrap().starts_with("L001 (Lex error): "));
        assert!(explain("X001").is_err());
        assert!(explain("L01").is_err());
    }
}
