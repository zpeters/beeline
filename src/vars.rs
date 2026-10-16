//! `{{name}}` interpolation.

use std::collections::BTreeMap;

/// Replace every `{{name}}` (whitespace inside the braces is allowed)
/// with its value. Unknown names and unclosed braces are left as written,
/// so the problem stays visible in the sent request.
pub fn interpolate(input: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        match vars.get(after[..end].trim()) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Names in `{{...}}` that `vars` does not define, in order of first use.
pub fn missing(input: &str, vars: &BTreeMap<String, String>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if !vars.contains_key(name) && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_missing_names_once() {
        assert_eq!(
            missing("{{a}}/{{id}}/{{ a }}/{{b}}", &vars()),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(missing("{{base}}", &vars()).is_empty());
    }

    fn vars() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("base".to_string(), "https://api.test".to_string()),
            ("id".to_string(), "42".to_string()),
        ])
    }

    #[test]
    fn replaces_known_names() {
        assert_eq!(
            interpolate("{{base}}/users/{{id}}", &vars()),
            "https://api.test/users/42"
        );
    }

    #[test]
    fn allows_whitespace_inside_braces() {
        assert_eq!(interpolate("{{ id }}", &vars()), "42");
    }

    #[test]
    fn keeps_unknown_names() {
        assert_eq!(interpolate("a{{nope}}b{{id}}", &vars()), "a{{nope}}b42");
    }

    #[test]
    fn keeps_unclosed_braces() {
        assert_eq!(interpolate("x{{id", &vars()), "x{{id");
    }

    #[test]
    fn values_are_not_reinterpolated() {
        let v = BTreeMap::from([("a".to_string(), "{{b}}".to_string())]);
        assert_eq!(interpolate("{{a}}", &v), "{{b}}");
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(interpolate("no vars { here }", &vars()), "no vars { here }");
    }
}
