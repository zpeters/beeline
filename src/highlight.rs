//! JSON syntax colouring: splits one line of JSON text into tokens. Pure.
//!
//! JSON strings cannot hold a raw newline, so each line is tokenized on
//! its own. Text that is not valid JSON still gets a best-effort result;
//! a string left open runs to the end of the line.

use std::ops::Range;

use crate::vars;

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
    /// A `{{name}}` that has a value.
    VarOk,
    /// A `{{name}}` that has no value.
    VarMissing,
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

/// Like `line`, but each `{{name}}` gets its own token: `VarOk` when
/// `defined` says the name has a value, `VarMissing` when not. A variable
/// token replaces any JSON token it overlaps. With `json` false, only the
/// variables are coloured. Ranges are sorted and do not overlap.
pub fn line_with_vars(
    text: &str,
    json: bool,
    defined: impl Fn(&str) -> bool,
) -> Vec<(Range<usize>, Token)> {
    let mut classes: Vec<Option<Token>> = vec![None; text.len()];
    if json {
        for (range, token) in line(text) {
            classes[range].fill(Some(token));
        }
    }
    for (range, name) in vars::spans(text) {
        let token = if defined(name) {
            Token::VarOk
        } else {
            Token::VarMissing
        };
        classes[range].fill(Some(token));
    }
    // Join neighbouring bytes of one class into one range. Every boundary
    // is a token boundary, so each range starts and ends on a char boundary.
    let mut out = Vec::new();
    let mut i = 0;
    while i < classes.len() {
        let Some(token) = classes[i] else {
            i += 1;
            continue;
        };
        let start = i;
        while i < classes.len() && classes[i] == Some(token) {
            i += 1;
        }
        out.push((start..i, token));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, Token)> {
        line(text).into_iter().map(|(r, t)| (&text[r], t)).collect()
    }

    fn var_kinds(text: &str, json: bool) -> Vec<(&str, Token)> {
        line_with_vars(text, json, |name| name == "id")
            .into_iter()
            .map(|(r, t)| (&text[r], t))
            .collect()
    }

    #[test]
    fn variable_in_json_string_splits_the_string() {
        assert_eq!(
            var_kinds(r#""/users/{{id}}/x""#, true),
            vec![
                (r#""/users/"#, Token::String),
                ("{{id}}", Token::VarOk),
                (r#"/x""#, Token::String),
            ]
        );
    }

    #[test]
    fn undefined_variable_is_missing() {
        assert_eq!(
            var_kinds("{{nope}}", false),
            vec![("{{nope}}", Token::VarMissing)]
        );
    }

    #[test]
    fn plain_text_colours_only_variables() {
        assert_eq!(
            var_kinds("a {{id}} b", false),
            vec![("{{id}}", Token::VarOk)]
        );
    }

    #[test]
    fn variable_outside_a_string_keeps_json_tokens() {
        assert_eq!(
            var_kinds("[{{id}}, 1]", true),
            vec![
                ("[", Token::Punct),
                ("{{id}}", Token::VarOk),
                (",", Token::Punct),
                ("1", Token::Number),
                ("]", Token::Punct),
            ]
        );
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
