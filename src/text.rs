use std::fmt;

/// A Somoni string: the text and its number of characters, so `len` needs no counting.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Text {
    text: String,
    chars: usize,
}

impl Text {
    /// Makes a text and counts its characters.
    pub fn new(text: String) -> Text {
        let chars = text.chars().count();
        Text { text, chars }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the number of characters (not bytes: "ä" is 1).
    pub fn char_count(&self) -> usize {
        self.chars
    }

    /// Appends another text.
    pub fn push(&mut self, other: &Text) {
        self.text.push_str(&other.text);
        self.chars += other.chars;
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_characters_not_bytes() {
        let text = Text::new("äöü".to_string());
        assert_eq!(text.char_count(), 3);
        assert_eq!(text.as_str().len(), 6);
    }

    #[test]
    fn push_adds_the_counts() {
        let mut text = Text::new("ä".to_string());
        text.push(&Text::new("bß".to_string()));
        assert_eq!(text.as_str(), "äbß");
        assert_eq!(text.char_count(), 3);
    }
}
