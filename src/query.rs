//! Keep the URL's query string and the params table in sync. The URL is
//! the source of truth for enabled params; disabled params live only in
//! the table (and the saved file).

use crate::model::KeyValue;

/// Split `base?query#fragment` into `(base, query, "#fragment")`.
fn split(url: &str) -> (&str, Option<&str>, &str) {
    let (rest, fragment) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    match rest.find('?') {
        Some(i) => (&rest[..i], Some(&rest[i + 1..]), fragment),
        None => (rest, None, fragment),
    }
}

fn parse(query: &str) -> Vec<KeyValue> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => KeyValue::new(k, v),
            None => KeyValue::new(pair, ""),
        })
        .collect()
}

/// The URL was edited: enabled params become what the URL says, and
/// disabled params are kept after them.
pub fn params_from_url(url: &str, params: &[KeyValue]) -> Vec<KeyValue> {
    let (_, query, _) = split(url);
    let mut out = query.map(parse).unwrap_or_default();
    out.extend(params.iter().filter(|p| !p.enabled).cloned());
    out
}

/// The params table was edited: rebuild the URL's query from the
/// enabled, non-blank params. Text is copied as typed (no encoding), so
/// `{{vars}}` survive; encoding happens when the request is resolved.
pub fn url_from_params(url: &str, params: &[KeyValue]) -> String {
    let (base, _, fragment) = split(url);
    let pairs: Vec<String> = params
        .iter()
        .filter(|p| p.enabled && !(p.key.is_empty() && p.value.is_empty()))
        .map(|p| {
            if p.value.is_empty() {
                p.key.clone()
            } else {
                format!("{}={}", p.key, p.value)
            }
        })
        .collect();
    if pairs.is_empty() {
        format!("{base}{fragment}")
    } else {
        format!("{base}?{}{fragment}", pairs.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled(k: &str, v: &str) -> KeyValue {
        KeyValue {
            enabled: false,
            ..KeyValue::new(k, v)
        }
    }

    #[test]
    fn parses_params_from_url() {
        let got = params_from_url("https://x/a?q=1&flag&empty=#top", &[]);
        assert_eq!(
            got,
            vec![
                KeyValue::new("q", "1"),
                KeyValue::new("flag", ""),
                KeyValue::new("empty", "")
            ]
        );
    }

    #[test]
    fn url_edit_keeps_disabled_params() {
        let old = vec![KeyValue::new("a", "1"), disabled("b", "2")];
        let got = params_from_url("https://x?a=9", &old);
        assert_eq!(got, vec![KeyValue::new("a", "9"), disabled("b", "2")]);
    }

    #[test]
    fn no_query_means_no_enabled_params() {
        assert!(params_from_url("https://x", &[KeyValue::new("a", "1")]).is_empty());
    }

    #[test]
    fn rebuilds_url_from_enabled_params() {
        let params = vec![
            KeyValue::new("a", "1"),
            disabled("b", "2"),
            KeyValue::new("flag", ""),
            KeyValue::new("", ""),
        ];
        assert_eq!(
            url_from_params("https://x/p?old=1#frag", &params),
            "https://x/p?a=1&flag#frag"
        );
    }

    #[test]
    fn removes_query_when_no_params() {
        assert_eq!(url_from_params("https://x/p?old=1", &[]), "https://x/p");
    }

    #[test]
    fn keeps_vars_unencoded() {
        let params = vec![KeyValue::new("id", "{{id}}")];
        assert_eq!(
            url_from_params("{{base}}/u", &params),
            "{{base}}/u?id={{id}}"
        );
    }
}
