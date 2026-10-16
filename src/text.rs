use std::fmt;

// A Somoni string: the text plus its number of characters.
// len counts characters, not bytes ("ä" is 1), and counting means reading the whole text.
// So every Text knows its count from the start, and len only reads a number.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    text: String,
    chars: usize,
}

impl Text {
    // counts once
    pub fn new(text: String) -> Text {
        let chars = text.chars().count();
        Text { text, chars }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn char_count(&self) -> usize {
        self.chars
    }

    // appends without counting again: both counts are known
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
