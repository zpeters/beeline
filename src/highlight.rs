//! JSON syntax colouring: splits one line of JSON text into tokens. Pure.
//!
//! JSON strings cannot hold a raw newline, so each line is tokenized on
//! its own. Text that is not valid JSON still gets a best-effort result;
//! a string left open runs to the end of the line.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    /// An object key: a string followed by a colon.
    Key,
    String,
    Number,
    /// `true`, `false` or `null`.
    Literal,
    /// Braces, brackets, commas and colons.
    Punct,
}

/// The tokens of one line, as byte ranges in order. Whitespace and
/// unknown characters get no token.
pub fn line(text: &str) -> Vec<(Range<usize>, Token)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                let rest = text[i..].trim_start();
                let token = if rest.starts_with(':') {
                    Token::Key
                } else {
                    Token::String
                };
                out.push((start..i, token));
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                i += 1;
                out.push((start..i, Token::Punct));
            }
            b'-' | b'0'..=b'9' => {
                i += 1;
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                out.push((start..i, Token::Number));
            }
            b'a'..=b'z' => {
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if matches!(&text[start..i], "true" | "false" | "null") {
                    out.push((start..i, Token::Literal));
                }
            }
            // Skip a whole character, so `i` stays on a char boundary.
            _ => i += text[i..].chars().next().map_or(1, char::len_utf8),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, Token)> {
        line(text).into_iter().map(|(r, t)| (&text[r], t)).collect()
    }

    #[test]
    fn key_and_string_value() {
        assert_eq!(
            kinds(r#"  "name": "bee","#),
            vec![
                (r#""name""#, Token::Key),
                (":", Token::Punct),
                (r#""bee""#, Token::String),
                (",", Token::Punct),
            ]
        );
    }

    #[test]
    fn key_with_space_before_colon() {
        assert_eq!(kinds(r#""a" : 1"#)[0], (r#""a""#, Token::Key));
    }

    #[test]
    fn numbers_and_literals() {
        assert_eq!(
            kinds("[-1.5e3, true, false, null]"),
            vec![
                ("[", Token::Punct),
                ("-1.5e3", Token::Number),
                (",", Token::Punct),
                ("true", Token::Literal),
                (",", Token::Punct),
                ("false", Token::Literal),
                (",", Token::Punct),
                ("null", Token::Literal),
                ("]", Token::Punct),
            ]
        );
    }

    #[test]
    fn escaped_quote_stays_in_string() {
        assert_eq!(
            kinds(r#""say \"hi\"""#),
            vec![(r#""say \"hi\"""#, Token::String)]
        );
    }

    #[test]
    fn unclosed_string_runs_to_end_of_line() {
        assert_eq!(kinds(r#""open"#), vec![(r#""open"#, Token::String)]);
        // A trailing backslash must not step past the end.
        assert_eq!(kinds(r#""a\"#), vec![(r#""a\"#, Token::String)]);
    }

    #[test]
    fn unknown_words_and_unicode_get_no_token() {
        assert_eq!(kinds("nope é {"), vec![("{", Token::Punct)]);
        assert_eq!(kinds(r#""é": 1"#)[0], (r#""é""#, Token::Key));
    }
}
