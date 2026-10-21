//! A pasted `curl` command as a request. Pure. Covers the flags people
//! copy from browsers' "Copy as cURL" and API docs; flags that only change
//! how curl runs (output, verbosity, timeouts) are skipped.

use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};
use crate::query;

/// Flags that take a value beeline has no use for. Their value is skipped.
const SKIP_WITH_VALUE: &[&str] = &[
    "-o",
    "--output",
    "-m",
    "--max-time",
    "--connect-timeout",
    "-w",
    "--write-out",
    "-x",
    "--proxy",
    "--retry",
    "-c",
    "--cookie-jar",
    "--cacert",
    "--cert",
    "--key",
    "-K",
    "--config",
];

/// Parses `command` into a suggested name and a request.
pub fn parse(command: &str) -> Result<(String, RequestFile), String> {
    // Line continuations: a backslash before a newline joins the lines.
    let joined = command.replace("\\\r\n", " ").replace("\\\n", " ");
    let words = shell_words::split(joined.trim())
        .map_err(|e| format!("Could not read the command: {e}"))?;
    let mut words = words.into_iter().skip_while(|w| w == "$");
    if words.next().as_deref() != Some("curl") {
        return Err("This is not a curl command".into());
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut user: Option<String> = None;
    let mut head = false;
    let mut get = false;

    let words: Vec<String> = words.collect();
    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        // `--flag=value` and `-Xvalue` forms are split into flag and value.
        let (flag, inline) = split_flag(word);
        let mut value = || -> Result<String, String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            i += 1;
            words
                .get(i)
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "-X" | "--request" => method = Some(value()?.to_ascii_uppercase()),
            "-H" | "--header" => {
                let v = value()?;
                if let Some((name, val)) = v.split_once(':') {
                    headers.push(KeyValue::new(name.trim(), val.trim()));
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii"
            | "--data-urlencode" => data.push(value()?),
            "--json" => {
                data.push(value()?);
                headers.push(KeyValue::new("Content-Type", "application/json"));
                headers.push(KeyValue::new("Accept", "application/json"));
            }
            "-u" | "--user" => user = Some(value()?),
            "-A" | "--user-agent" => headers.push(KeyValue::new("User-Agent", value()?)),
            "-e" | "--referer" => headers.push(KeyValue::new("Referer", value()?)),
            "-b" | "--cookie" => headers.push(KeyValue::new("Cookie", value()?)),
            "--url" => url = Some(value()?),
            "-I" | "--head" => head = true,
            "-G" | "--get" => get = true,
            "-F" | "--form" => {
                return Err("Multipart form uploads (-F) are not supported".into());
            }
            f if SKIP_WITH_VALUE.contains(&f) => {
                value()?;
            }
            f if f.starts_with('-') => {}
            _ => {
                if url.is_none() {
                    url = Some(word.clone());
                }
            }
        }
        i += 1;
    }

    let mut url = url.ok_or("The command has no URL")?;
    let body_text = data.join("&");
    if get && !body_text.is_empty() {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&body_text);
    }
    let method = method.unwrap_or_else(|| {
        if head {
            "HEAD".into()
        } else if !body_text.is_empty() && !get {
            "POST".into()
        } else {
            "GET".into()
        }
    });

    let mut req = RequestFile {
        method: method.clone(),
        params: query::params_from_url(&url, &[]),
        url,
        ..RequestFile::default()
    };

    // `Authorization: Bearer …` and `-u user:pass` become the Auth tab.
    if let Some(at) = headers.iter().position(|h| {
        h.key.eq_ignore_ascii_case("authorization")
            && h.value.to_ascii_lowercase().starts_with("bearer ")
    }) {
        let token = headers.remove(at).value["bearer ".len()..]
            .trim()
            .to_string();
        req.auth = Auth {
            kind: AuthKind::Bearer,
            token,
            ..Auth::default()
        };
    }
    if let Some(user) = user {
        let (username, password) = user.split_once(':').unwrap_or((&user, ""));
        req.auth = Auth {
            kind: AuthKind::Basic,
            username: username.into(),
            password: password.into(),
            ..Auth::default()
        };
    }

    if !body_text.is_empty() && !get {
        let content_type = headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.to_ascii_lowercase());
        let trimmed = body_text.trim_start();
        let looks_json = trimmed.starts_with('{') || trimmed.starts_with('[');
        req.body = match content_type.as_deref() {
            Some(ct) if ct.contains("json") => text_body(BodyKind::Json, body_text),
            Some(ct) if ct.contains("x-www-form-urlencoded") => form_body(&body_text),
            Some(_) => text_body(BodyKind::Text, body_text),
            // curl sends -d as a form unless told otherwise.
            None if looks_json => text_body(BodyKind::Json, body_text),
            None if body_text.contains('=') => form_body(&body_text),
            None => text_body(BodyKind::Text, body_text),
        };
    }
    req.headers = headers;

    Ok((name_for(&method, &req.url), req))
}

/// Splits `--flag=value` and short `-Xvalue` into the flag and its value.
fn split_flag(word: &str) -> (String, Option<String>) {
    if let Some(long) = word.strip_prefix("--")
        && let Some((flag, value)) = long.split_once('=')
    {
        return (format!("--{flag}"), Some(value.to_string()));
    }
    let short_with_value = [
        "-X", "-H", "-d", "-u", "-A", "-e", "-b", "-o", "-m", "-w", "-x",
    ];
    if word.len() > 2 && !word.starts_with("--") && short_with_value.contains(&&word[..2]) {
        return (word[..2].to_string(), Some(word[2..].to_string()));
    }
    (word.to_string(), None)
}

fn text_body(kind: BodyKind, text: String) -> Body {
    Body {
        kind,
        text,
        fields: vec![],
    }
}

fn form_body(text: &str) -> Body {
    Body {
        kind: BodyKind::Form,
        text: String::new(),
        fields: url::form_urlencoded::parse(text.as_bytes())
            .map(|(k, v)| KeyValue::new(k, v))
            .collect(),
    }
}

/// "GET users 42" from GET https://api.test/users/42?x=1. The host stands in
/// when the path is empty.
pub(super) fn name_for(method: &str, url: &str) -> String {
    let parsed = url::Url::parse(url).ok();
    let path = parsed
        .as_ref()
        .map(|u| u.path().trim_matches('/').replace('/', " "))
        .unwrap_or_default();
    let what = if path.is_empty() {
        parsed
            .as_ref()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "request".into())
    } else {
        path
    };
    format!("{method} {what}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{Resolved, resolve};
    use std::collections::BTreeMap;

    #[test]
    fn plain_get() {
        let (name, req) = parse("curl https://api.test/users/42").unwrap();
        assert_eq!(name, "GET users 42");
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://api.test/users/42");
    }

    #[test]
    fn query_string_fills_params() {
        let (_, req) = parse("curl 'https://api.test/s?q=bee&page=2'").unwrap();
        assert_eq!(
            req.params,
            vec![KeyValue::new("q", "bee"), KeyValue::new("page", "2")]
        );
    }

    #[test]
    fn json_post_over_several_lines() {
        let (_, req) = parse(
            "curl 'https://api.test/users' \\\n  -X POST \\\n  -H 'Content-Type: application/json' \\\n  --data-raw '{\"name\":\"bee\"}'",
        )
        .unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.body.kind, BodyKind::Json);
        assert_eq!(req.body.text, r#"{"name":"bee"}"#);
        assert_eq!(
            req.headers,
            vec![KeyValue::new("Content-Type", "application/json")]
        );
    }

    #[test]
    fn data_without_content_type_is_a_form_and_implies_post() {
        let (_, req) = parse("curl https://x/login -d user=bee -d 'pass=a b'").unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.body.kind, BodyKind::Form);
        assert_eq!(
            req.body.fields,
            vec![KeyValue::new("user", "bee"), KeyValue::new("pass", "a b")]
        );
    }

    #[test]
    fn get_flag_moves_data_into_the_query() {
        let (_, req) = parse("curl -G https://x/search -d q=bee").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://x/search?q=bee");
        assert_eq!(req.body.kind, BodyKind::None);
    }

    #[test]
    fn auth_flags_become_the_auth_tab() {
        let (_, basic) = parse("curl -u bee:secret https://x").unwrap();
        assert_eq!(basic.auth.kind, AuthKind::Basic);
        assert_eq!(
            (basic.auth.username.as_str(), basic.auth.password.as_str()),
            ("bee", "secret")
        );

        let (_, bearer) = parse("curl -H 'Authorization: Bearer t0k' https://x").unwrap();
        assert_eq!(bearer.auth.kind, AuthKind::Bearer);
        assert_eq!(bearer.auth.token, "t0k");
        assert!(bearer.headers.is_empty());
    }

    #[test]
    fn attached_and_equals_forms() {
        let (_, req) = parse("curl -XPUT --header=X-A:1 --url=https://x/a").unwrap();
        assert_eq!(req.method, "PUT");
        assert_eq!(req.url, "https://x/a");
        assert_eq!(req.headers, vec![KeyValue::new("X-A", "1")]);
    }

    #[test]
    fn runtime_flags_are_skipped_with_their_values() {
        let (_, req) = parse("curl -sSL --compressed -o out.json -m 5 https://x/a").unwrap();
        assert_eq!(req.url, "https://x/a");
    }

    #[test]
    fn rejects_what_it_cannot_import() {
        assert!(parse("wget https://x").is_err());
        assert!(parse("curl -s").is_err());
        assert!(parse("curl -F file=@a.png https://x").is_err());
    }

    #[test]
    fn our_own_copy_as_curl_reads_back_the_same() {
        let mut original = RequestFile {
            method: "POST".into(),
            url: "https://api.test/users?team=a".into(),
            headers: vec![KeyValue::new("X-Trace", "it's 1")],
            body: Body {
                kind: BodyKind::Json,
                text: r#"{"name":"bee"}"#.into(),
                fields: vec![],
            },
            ..RequestFile::default()
        };
        original.params = query::params_from_url(&original.url, &[]);
        let sent: Resolved = resolve(&original, &BTreeMap::new()).unwrap();
        let (_, back) = parse(&crate::curl::command(&sent)).unwrap();
        assert_eq!(resolve(&back, &BTreeMap::new()).unwrap(), sent);
    }
}
