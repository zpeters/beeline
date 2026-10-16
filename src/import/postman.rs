//! A Postman collection (v2.1, and v2.0's auth layout) as requests. Pure.
//! Folders stay folders; `{{vars}}` keep their syntax, which beeline shares.
//! Multipart form-data and file bodies have no beeline equivalent and come
//! in without a body.

use serde_json::Value;

use super::{Imported, Item};
use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};
use crate::query;

/// True when `doc` looks like a Postman collection.
pub fn is_postman(doc: &Value) -> bool {
    doc.get("info").is_some() && doc.get("item").is_some()
}

pub fn parse(doc: &Value) -> Result<Imported, String> {
    let name = doc
        .pointer("/info/name")
        .and_then(Value::as_str)
        .unwrap_or("Postman import")
        .to_string();
    let mut items = Vec::new();
    walk(
        doc.get("item").unwrap_or(&Value::Null),
        &[],
        auth_of(doc.get("auth")),
        &mut items,
    );
    Ok(Imported { name, items })
}

/// Collects requests depth-first. `inherited` is the nearest folder's or
/// the collection's auth, which a request without its own auth uses.
fn walk(nodes: &Value, folders: &[String], inherited: Option<Auth>, out: &mut Vec<Item>) {
    for node in nodes.as_array().into_iter().flatten() {
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("request")
            .to_string();
        if let Some(children) = node.get("item") {
            let mut inner = folders.to_vec();
            inner.push(name);
            let auth = auth_of(node.get("auth")).or_else(|| inherited.clone());
            walk(children, &inner, auth, out);
        } else if let Some(request) = node.get("request") {
            out.push(Item {
                folders: folders.to_vec(),
                name,
                req: request_of(request, inherited.clone()),
            });
        }
    }
}

fn request_of(request: &Value, inherited: Option<Auth>) -> RequestFile {
    // A request may be just its URL.
    if let Some(url) = request.as_str() {
        return RequestFile {
            params: query::params_from_url(url, &[]),
            url: url.to_string(),
            auth: inherited.unwrap_or_default(),
            ..RequestFile::default()
        };
    }
    let (url, params) = url_of(request.get("url"));
    RequestFile {
        method: request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_ascii_uppercase(),
        url,
        params,
        headers: rows(request.get("header")),
        body: body_of(request.get("body")),
        auth: auth_of(request.get("auth"))
            .or(inherited)
            .unwrap_or_default(),
        ..RequestFile::default()
    }
}

/// The URL with only enabled query params in it, and all params as rows.
/// Postman's `:id` path variables become `{{id}}`.
fn url_of(url: Option<&Value>) -> (String, Vec<KeyValue>) {
    let raw = match url {
        Some(Value::String(s)) => s.clone(),
        Some(u) => u
            .get("raw")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| built(u)),
        None => String::new(),
    };
    let (base, _) = raw.split_once('?').unwrap_or((&raw, ""));
    let base = path_vars(base);
    match url.and_then(|u| u.get("query")) {
        Some(query) => {
            let params = rows(Some(query));
            (query::url_from_params(&base, &params), params)
        }
        None => {
            let full = if raw.contains('?') {
                format!("{base}?{}", raw.split_once('?').map_or("", |(_, q)| q))
            } else {
                base
            };
            let params = query::params_from_url(&full, &[]);
            (full, params)
        }
    }
}

/// A URL from Postman's parts, for files that omit `raw`.
fn built(u: &Value) -> String {
    let join = |key: &str, sep: &str| -> String {
        match u.get(key) {
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(sep),
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        }
    };
    let protocol = u.get("protocol").and_then(Value::as_str).unwrap_or("https");
    format!("{protocol}://{}/{}", join("host", "."), join("path", "/"))
}

/// `/users/:id` → `/users/{{id}}`, leaving the scheme's `://` and ports alone.
fn path_vars(url: &str) -> String {
    url.split('/')
        .map(|seg| match seg.strip_prefix(':') {
            Some(name) if !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()) => {
                format!("{{{{{name}}}}}")
            }
            _ => seg.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Postman key/value rows, keeping their on/off state.
fn rows(list: Option<&Value>) -> Vec<KeyValue> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let key = row.get("key").and_then(Value::as_str)?;
            let mut kv = KeyValue::new(key, row.get("value").and_then(Value::as_str).unwrap_or(""));
            kv.enabled = !row
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(kv)
        })
        .collect()
}

fn body_of(body: Option<&Value>) -> Body {
    let Some(body) = body else {
        return Body::default();
    };
    match body.get("mode").and_then(Value::as_str) {
        Some("raw") => {
            let text = body
                .get("raw")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let json = body
                .pointer("/options/raw/language")
                .and_then(Value::as_str)
                == Some("json");
            Body {
                kind: if json { BodyKind::Json } else { BodyKind::Text },
                text,
                fields: vec![],
            }
        }
        Some("urlencoded") => Body {
            kind: BodyKind::Form,
            text: String::new(),
            fields: rows(body.get("urlencoded")),
        },
        _ => Body::default(),
    }
}

/// Bearer or basic auth, from v2.1's `[{key, value}]` lists or v2.0's
/// objects. `noauth` is an explicit "none" that stops inheritance; other
/// kinds beeline lacks also come in as none.
fn auth_of(auth: Option<&Value>) -> Option<Auth> {
    let auth = auth?;
    let kind = auth.get("type").and_then(Value::as_str)?;
    let field = |section: &str, key: &str| -> String {
        let value = match auth.get(section) {
            Some(Value::Array(list)) => list
                .iter()
                .find(|r| r.get("key").and_then(Value::as_str) == Some(key))
                .and_then(|r| r.get("value")),
            Some(Value::Object(map)) => map.get(key),
            _ => None,
        };
        value.and_then(Value::as_str).unwrap_or("").to_string()
    };
    Some(match kind {
        "bearer" => Auth {
            kind: AuthKind::Bearer,
            token: field("bearer", "token"),
            ..Auth::default()
        },
        "basic" => Auth {
            kind: AuthKind::Basic,
            username: field("basic", "username"),
            password: field("basic", "password"),
            ..Auth::default()
        },
        _ => Auth::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLLECTION: &str = r#"{
      "info": {"name": "Shop", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
      "auth": {"type": "bearer", "bearer": [{"key": "token", "value": "{{token}}"}]},
      "item": [
        {"name": "Orders", "item": [
          {"name": "Get order", "request": {
            "method": "GET",
            "url": {"raw": "{{base}}/orders/:id?expand=items&debug=1",
                    "query": [{"key": "expand", "value": "items"}, {"key": "debug", "value": "1", "disabled": true}]},
            "header": [{"key": "Accept", "value": "application/json"}]
          }},
          {"name": "Create order", "request": {
            "method": "post",
            "url": "{{base}}/orders",
            "body": {"mode": "raw", "raw": "{\"sku\": 1}", "options": {"raw": {"language": "json"}}},
            "auth": {"type": "noauth"}
          }}
        ]},
        {"name": "Login", "request": {
          "method": "POST",
          "url": {"raw": "https://shop.test/login"},
          "body": {"mode": "urlencoded", "urlencoded": [{"key": "user", "value": "bee"}]},
          "auth": {"type": "basic", "basic": {"username": "bee", "password": "pw"}}
        }},
        {"name": "Ping", "request": "https://shop.test/ping"}
      ]
    }"#;

    fn shop() -> Imported {
        parse(&serde_json::from_str(COLLECTION).unwrap()).unwrap()
    }

    #[test]
    fn keeps_the_collection_name_and_folders() {
        let imported = shop();
        assert_eq!(imported.name, "Shop");
        let names: Vec<(Vec<String>, &str)> = imported
            .items
            .iter()
            .map(|i| (i.folders.clone(), i.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                (vec!["Orders".to_string()], "Get order"),
                (vec!["Orders".to_string()], "Create order"),
                (vec![], "Login"),
                (vec![], "Ping"),
            ]
        );
    }

    #[test]
    fn query_rows_keep_their_state_and_path_vars_convert() {
        let get = &shop().items[0].req;
        assert_eq!(get.url, "{{base}}/orders/{{id}}?expand=items");
        assert_eq!(get.params.len(), 2);
        assert!(!get.params[1].enabled);
        assert_eq!(
            get.headers,
            vec![KeyValue::new("Accept", "application/json")]
        );
    }

    #[test]
    fn auth_inherits_from_the_collection_unless_overridden() {
        let items = shop().items;
        assert_eq!(items[0].req.auth.kind, AuthKind::Bearer);
        assert_eq!(items[0].req.auth.token, "{{token}}");
        assert_eq!(
            items[1].req.auth.kind,
            AuthKind::None,
            "noauth stops inheritance"
        );
        assert_eq!(
            items[2].req.auth.kind,
            AuthKind::Basic,
            "v2.0 object layout"
        );
        assert_eq!(items[2].req.auth.password, "pw");
    }

    #[test]
    fn bodies_raw_json_and_urlencoded() {
        let items = shop().items;
        assert_eq!(items[1].req.method, "POST");
        assert_eq!(items[1].req.body.kind, BodyKind::Json);
        assert_eq!(items[1].req.body.text, r#"{"sku": 1}"#);
        assert_eq!(items[2].req.body.kind, BodyKind::Form);
        assert_eq!(items[2].req.body.fields, vec![KeyValue::new("user", "bee")]);
    }

    #[test]
    fn a_bare_url_request_is_a_get() {
        let ping = &shop().items[3].req;
        assert_eq!(ping.method, "GET");
        assert_eq!(ping.url, "https://shop.test/ping");
    }

    #[test]
    fn ports_and_schemes_are_not_path_vars() {
        assert_eq!(
            path_vars("http://localhost:3000/a/:b"),
            "http://localhost:3000/a/{{b}}"
        );
    }
}
