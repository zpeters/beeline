//! A HAR file (HTTP Archive, from a browser's network tab or a proxy) as
//! requests. Pure. Each entry becomes one request, all in one folder named
//! after the first entry's host. Pseudo-headers (`:authority`) are dropped,
//! and a bearer or basic Authorization header moves to the Auth tab, as
//! cURL import does. Multipart bodies have no beeline equivalent and come
//! in without a body.

use base64::Engine;
use serde_json::Value;

use super::curl::name_for;
use super::{Imported, Item};
use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};
use crate::query;

/// True when `doc` looks like a HAR file.
pub fn is_har(doc: &Value) -> bool {
    doc.pointer("/log/entries").is_some()
}

pub fn parse(doc: &Value) -> Result<Imported, String> {
    let entries = doc
        .pointer("/log/entries")
        .and_then(Value::as_array)
        .ok_or("Not a HAR file: no log.entries")?;
    let name = entries
        .first()
        .and_then(|e| e.pointer("/request/url"))
        .and_then(Value::as_str)
        .and_then(|url| url::Url::parse(url).ok())
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| "HAR import".into());
    let items = entries
        .iter()
        .filter_map(|e| e.get("request"))
        .map(|request| {
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_ascii_uppercase();
            let url = request
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Item {
                folders: vec![],
                name: name_for(&method, &url),
                req: request_of(request, &method, &url),
            }
        })
        .collect();
    Ok(Imported { name, items })
}

fn request_of(request: &Value, method: &str, url: &str) -> RequestFile {
    let base = url.split_once('?').map_or(url, |(base, _)| base);
    let params = rows(request.get("queryString"));
    let full = query::url_from_params(base, &params);

    // Pseudo-headers are HTTP/2 framing, not headers to send.
    let mut headers = rows(request.get("headers"));
    headers.retain(|h| !h.key.starts_with(':'));
    let mut auth = Auth::default();
    if let Some(at) = headers
        .iter()
        .position(|h| h.key.eq_ignore_ascii_case("authorization"))
        && let Some(found) = auth_from(&headers[at].value)
    {
        auth = found;
        headers.remove(at);
    }

    RequestFile {
        method: method.into(),
        url: full,
        params,
        headers,
        body: body_of(request.get("postData")),
        auth,
        ..RequestFile::default()
    }
}

/// Name/value rows from a HAR list, all enabled (HAR has no disabled rows).
fn rows(list: Option<&Value>) -> Vec<KeyValue> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let name = row.get("name").and_then(Value::as_str)?;
            Some(KeyValue::new(
                name,
                row.get("value").and_then(Value::as_str).unwrap_or(""),
            ))
        })
        .collect()
}

/// A bearer or basic Authorization header as the Auth tab. Other schemes
/// stay as headers.
fn auth_from(value: &str) -> Option<Auth> {
    let (scheme, rest) = value.trim().split_once(' ')?;
    let rest = rest.trim();
    match scheme.to_ascii_lowercase().as_str() {
        "bearer" => Some(Auth {
            kind: AuthKind::Bearer,
            token: rest.into(),
            ..Auth::default()
        }),
        "basic" => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(rest)
                .ok()?;
            let creds = String::from_utf8(bytes).ok()?;
            let (username, password) = creds.split_once(':').unwrap_or((creds.as_str(), ""));
            Some(Auth {
                kind: AuthKind::Basic,
                username: username.into(),
                password: password.into(),
                ..Auth::default()
            })
        }
        _ => None,
    }
}

/// Form fields when the type is urlencoded, JSON or text otherwise. A
/// multipart upload has no beeline equivalent and comes in without a body.
fn body_of(post: Option<&Value>) -> Body {
    let Some(post) = post else {
        return Body::default();
    };
    let mime = post
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if mime.contains("multipart") {
        return Body::default();
    }
    if mime.contains("x-www-form-urlencoded") {
        let fields = rows(post.get("params"));
        if !fields.is_empty() {
            return Body {
                kind: BodyKind::Form,
                text: String::new(),
                fields,
            };
        }
    }
    let text = post
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if text.is_empty() {
        return Body::default();
    }
    let kind = if mime.contains("json") {
        BodyKind::Json
    } else {
        BodyKind::Text
    };
    Body {
        kind,
        text,
        fields: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc() -> Value {
        serde_json::from_str(include_str!("../../fixtures/import/har/shop-session.har")).unwrap()
    }

    fn shop() -> Imported {
        parse(&doc()).unwrap()
    }

    #[test]
    fn detects_a_har_file() {
        assert!(is_har(&doc()));
        assert!(!is_har(&json!({"info": {"name": "x"}, "item": []})));
    }

    #[test]
    fn each_entry_is_a_request_in_a_folder_named_after_the_host() {
        let imported = shop();
        assert_eq!(imported.name, "shop.example.test");
        let names: Vec<&str> = imported.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "GET api orders",
                "POST api orders",
                "POST login",
                "POST upload",
                "GET api me"
            ]
        );
        assert!(imported.items.iter().all(|i| i.folders.is_empty()));
    }

    #[test]
    fn query_string_becomes_params_and_stays_in_the_url() {
        let req = &shop().items[0].req;
        assert_eq!(req.method, "GET");
        assert_eq!(
            req.url,
            "https://shop.example.test/api/orders?limit=20&status=open"
        );
        assert_eq!(
            req.params,
            vec![
                KeyValue::new("limit", "20"),
                KeyValue::new("status", "open")
            ]
        );
    }

    #[test]
    fn pseudo_headers_go_and_bearer_authorization_moves_to_auth() {
        let req = &shop().items[0].req;
        assert_eq!(
            req.headers,
            vec![KeyValue::new("accept", "application/json")]
        );
        assert_eq!(req.auth.kind, AuthKind::Bearer);
        assert_eq!(req.auth.token, "demo.t0k3n");
    }

    #[test]
    fn basic_authorization_is_decoded_into_the_auth_tab() {
        let req = &shop().items[4].req;
        assert_eq!(req.auth.kind, AuthKind::Basic);
        assert_eq!(req.auth.username, "demo");
        assert_eq!(req.auth.password, "secret");
        assert!(req.headers.is_empty());
    }

    #[test]
    fn json_and_form_bodies_come_in_and_multipart_does_not() {
        let items = shop().items;
        assert_eq!(items[1].req.body.kind, BodyKind::Json);
        assert_eq!(items[1].req.body.text, r#"{"sku":"A1","qty":2}"#);
        let form = &items[2].req.body;
        assert_eq!(form.kind, BodyKind::Form);
        assert_eq!(
            form.fields,
            vec![
                KeyValue::new("user", "demo"),
                KeyValue::new("pass", "secret")
            ]
        );
        assert_eq!(items[3].req.body.kind, BodyKind::None);
    }

    #[test]
    fn a_file_without_entries_is_an_error() {
        assert!(parse(&json!({"log": {}})).is_err());
    }
}
