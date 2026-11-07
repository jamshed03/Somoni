//! The parts of an address like `/my%20page?q=a+b&x=1`: the decoded path and the decoded query.

/// Splits a request target into its path and its query, both still encoded; a `#...` at the end is dropped.
pub(super) fn split_target(target: &str) -> (&str, &str) {
    let without_fragment = target.split('#').next().unwrap_or_default();
    without_fragment
        .split_once('?')
        .unwrap_or((without_fragment, ""))
}

/// Returns the pairs of a query like `q=a+b&x=1`, decoded, in their order; None if one of them is no valid text.
pub(super) fn parse_query(query: &str) -> Option<Vec<(String, String)>> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((decode(name, true)?, decode(value, true)?))
        })
        .collect()
}

/// Decodes `%20`, `%C3%BC` and, with `plus_is_space`, `+` as a space; None for a broken `%` or no valid UTF-8.
pub(super) fn decode(text: &str, plus_is_space: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut place = 0;
    while place < bytes.len() {
        match bytes[place] {
            b'%' => {
                let hex = text.get(place + 1..place + 3)?;
                decoded.push(u8::from_str_radix(hex, 16).ok()?);
                place += 3;
            }
            b'+' if plus_is_space => {
                decoded.push(b' ');
                place += 1;
            }
            byte => {
                decoded.push(byte);
                place += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_splits_into_path_and_query() {
        assert_eq!(split_target("/search?q=a#top"), ("/search", "q=a"));
        assert_eq!(split_target("/about"), ("/about", ""));
    }

    #[test]
    fn percent_and_plus_are_decoded() {
        assert_eq!(decode("my%20page", false), Some("my page".to_string()));
        assert_eq!(decode("a+b", false), Some("a+b".to_string()));
        assert_eq!(decode("a+b", true), Some("a b".to_string()));
        assert_eq!(decode("%C3%BC", true), Some("ü".to_string()));
        assert_eq!(decode("%2e%2e", false), Some("..".to_string()));
    }

    #[test]
    fn broken_encodings_are_none() {
        assert_eq!(decode("100%", true), None);
        assert_eq!(decode("%zz", true), None);
        assert_eq!(decode("%FF", true), None);
    }

    #[test]
    fn query_gives_its_pairs_in_order() {
        let pairs = parse_query("q=string+inter&x=1&flag&q=second").unwrap();
        let expected = [
            ("q", "string inter"),
            ("x", "1"),
            ("flag", ""),
            ("q", "second"),
        ];
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(pairs, expected);
    }
}
