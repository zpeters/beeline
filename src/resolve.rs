//! Turn a `RequestFile` plus environment variables into exactly what goes
//! on the wire. Pure: no network, no clock.

use std::collections::BTreeMap;

use base64::Engine;

use crate::model::{AuthKind, BodyKind, KeyValue, RequestFile};
use crate::vars::{interpolate, missing};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

fn enabled(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter()
        .filter(|kv| kv.enabled && !kv.key.trim().is_empty())
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// The variables a request sees: the environment's, plus the request's own
/// enabled ones. A request's value wins when the names collide. Values are
/// literal; they are not interpolated again.
pub fn effective(req: &RequestFile, env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut vars = env.clone();
    for kv in enabled(&req.vars) {
        vars.insert(kv.key.trim().to_string(), kv.value.clone());
    }
    vars
}

pub fn resolve(req: &RequestFile, env: &BTreeMap<String, String>) -> Result<Resolved, String> {
    let vars = &effective(req, env);
    let sub = |s: &str| interpolate(s, vars);

    let method = req.method.trim().to_ascii_uppercase();
    if method.is_empty() {
        return Err("Method is empty".into());
    }

    // An unknown variable in the URL would otherwise surface as a
    // confusing DNS or parse error, so name it instead.
    let unknown = missing(&req.url, vars);
    if !unknown.is_empty() {
        let names: Vec<String> = unknown.iter().map(|n| format!("{{{{{n}}}}}")).collect();
        return Err(format!("Unknown variable in URL: {}", names.join(", ")));
    }
    let mut raw_url = sub(req.url.trim());
    if raw_url.is_empty() {
        return Err("URL is empty".into());
    }
    if !raw_url.contains("://") {
        raw_url.insert_str(0, "http://");
    }
    let url = url::Url::parse(&raw_url).map_err(|e| format!("Invalid URL \"{raw_url}\": {e}"))?;

    let mut headers: Vec<(String, String)> = enabled(&req.headers)
        .map(|kv| (sub(kv.key.trim()), sub(&kv.value)))
        .collect();

    if !has_header(&headers, "authorization") {
        let auth = &req.auth;
        match auth.kind {
            AuthKind::None => {}
            AuthKind::Bearer => {
                headers.push((
                    "Authorization".into(),
                    format!("Bearer {}", sub(&auth.token)),
                ));
            }
            AuthKind::Basic => {
                let creds = format!("{}:{}", sub(&auth.username), sub(&auth.password));
                let encoded = base64::engine::general_purpose::STANDARD.encode(creds);
                headers.push(("Authorization".into(), format!("Basic {encoded}")));
            }
        }
    }

    let (body, content_type) = match req.body.kind {
        BodyKind::None => (Vec::new(), None),
        BodyKind::Json => (sub(&req.body.text).into_bytes(), Some("application/json")),
        BodyKind::Text => (
            sub(&req.body.text).into_bytes(),
            Some("text/plain; charset=utf-8"),
        ),
        BodyKind::Form => {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            for kv in enabled(&req.body.fields) {
                form.append_pair(&sub(kv.key.trim()), &sub(&kv.value));
            }
            (
                form.finish().into_bytes(),
                Some("application/x-www-form-urlencoded"),
            )
        }
    };
    if let Some(ct) = content_type
        && !has_header(&headers, "content-type")
    {
        headers.push(("Content-Type".into(), ct.into()));
    }

    Ok(Resolved {
        method,
        url: url.into(),
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Auth, Body};

    fn vars() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("base".to_string(), "https://api.test".to_string()),
            ("token".to_string(), "t0k".to_string()),
        ])
    }

    fn get(url: &str) -> RequestFile {
        RequestFile {
            url: url.into(),
            ..RequestFile::default()
        }
    }

    fn header<'a>(r: &'a Resolved, name: &str) -> Option<&'a str> {
        r.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn interpolates_and_normalizes_url() {
        let r = resolve(&get("{{base}}/a b?q=x y"), &vars()).unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://api.test/a%20b?q=x%20y");
    }

    #[test]
    fn adds_http_when_scheme_missing() {
        let r = resolve(&get("localhost:3000/health"), &vars()).unwrap();
        assert_eq!(r.url, "http://localhost:3000/health");
    }

    #[test]
    fn empty_url_is_an_error() {
        assert!(resolve(&get("  "), &vars()).is_err());
    }

    #[test]
    fn unknown_var_in_host_is_an_error() {
        let err = resolve(&get("{{nope}}/x"), &vars()).unwrap_err();
        assert!(err.contains("{{nope}}"), "{err}");
    }

    #[test]
    fn keeps_only_enabled_headers() {
        let mut req = get("https://x");
        req.headers = vec![
            KeyValue::new("X-A", "{{token}}"),
            KeyValue {
                enabled: false,
                ..KeyValue::new("X-B", "2")
            },
            KeyValue::new("  ", "blank key"),
        ];
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(r.headers, vec![("X-A".to_string(), "t0k".to_string())]);
    }

    #[test]
    fn bearer_auth_adds_header() {
        let mut req = get("https://x");
        req.auth = Auth {
            kind: AuthKind::Bearer,
            token: "{{token}}".into(),
            ..Auth::default()
        };
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(header(&r, "authorization"), Some("Bearer t0k"));
    }

    #[test]
    fn basic_auth_is_base64() {
        let mut req = get("https://x");
        req.auth = Auth {
            kind: AuthKind::Basic,
            username: "user".into(),
            password: "pass".into(),
            ..Auth::default()
        };
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(header(&r, "Authorization"), Some("Basic dXNlcjpwYXNz"));
    }

    #[test]
    fn explicit_authorization_header_wins() {
        let mut req = get("https://x");
        req.headers = vec![KeyValue::new("authorization", "Custom 1")];
        req.auth.kind = AuthKind::Bearer;
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(r.headers.len(), 1);
        assert_eq!(header(&r, "Authorization"), Some("Custom 1"));
    }

    #[test]
    fn json_body_sets_content_type() {
        let mut req = get("https://x");
        req.body = Body {
            kind: BodyKind::Json,
            text: r#"{"t":"{{token}}"}"#.into(),
            fields: vec![],
        };
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(r.body, br#"{"t":"t0k"}"#);
        assert_eq!(header(&r, "content-type"), Some("application/json"));
    }

    #[test]
    fn explicit_content_type_wins() {
        let mut req = get("https://x");
        req.headers = vec![KeyValue::new("Content-Type", "application/vnd.x+json")];
        req.body.kind = BodyKind::Json;
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(r.headers.len(), 1);
    }

    #[test]
    fn form_body_is_url_encoded() {
        let mut req = get("https://x");
        req.body = Body {
            kind: BodyKind::Form,
            text: "ignored".into(),
            fields: vec![
                KeyValue::new("name", "a b&c"),
                KeyValue {
                    enabled: false,
                    ..KeyValue::new("skip", "1")
                },
            ],
        };
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(r.body, b"name=a+b%26c");
        assert_eq!(
            header(&r, "content-type"),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[test]
    fn request_var_overrides_env_var() {
        let mut req = get("{{base}}/a");
        req.vars = vec![KeyValue::new("base", "https://staging.test")];
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(r.url, "https://staging.test/a");
    }

    #[test]
    fn request_var_can_define_a_new_name() {
        let mut req = get("{{host}}/a");
        req.vars = vec![KeyValue::new("host", "https://only-here.test")];
        let r = resolve(&req, &vars()).unwrap();
        assert_eq!(r.url, "https://only-here.test/a");
    }

    #[test]
    fn disabled_and_blank_request_vars_are_ignored() {
        let mut req = get("{{base}}/a");
        req.vars = vec![
            KeyValue {
                enabled: false,
                ..KeyValue::new("base", "https://off.test")
            },
            KeyValue::new("  ", "blank key"),
        ];
        assert_eq!(effective(&req, &vars()), vars());
    }

    #[test]
    fn request_var_values_are_not_reinterpolated() {
        let mut req = get("https://x");
        req.vars = vec![KeyValue::new("a", "{{base}}")];
        assert_eq!(effective(&req, &vars())["a"], "{{base}}");
    }

    #[test]
    fn none_body_is_empty_even_with_text() {
        let mut req = get("https://x");
        req.body.text = "left over".into();
        let r = resolve(&req, &vars()).unwrap();
        assert!(r.body.is_empty());
        assert!(r.headers.is_empty());
    }
}
