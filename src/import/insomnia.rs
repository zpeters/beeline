//! An Insomnia export (format 4, JSON) as requests. Pure. The workspace is
//! the folder; `request_group`s become nested folders; each `request` is a
//! request. Insomnia's `{{ _.name }}` variables become beeline's
//! `{{name}}`. Environments are not imported. Disabled rows stay disabled.
//! Multipart bodies have no beeline equivalent and come in without a body.

use std::collections::HashMap;

use serde_json::Value;

use super::{Imported, Item};
use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};
use crate::query;

/// True when `doc` looks like an Insomnia export.
pub fn is_insomnia(doc: &Value) -> bool {
    doc.get("_type").and_then(Value::as_str) == Some("export")
        && doc.get("resources").is_some_and(Value::is_array)
}

pub fn parse(doc: &Value) -> Result<Imported, String> {
    let resources = doc
        .get("resources")
        .and_then(Value::as_array)
        .ok_or("Not an Insomnia export: no resources")?;
    let by_id: HashMap<&str, &Value> = resources
        .iter()
        .filter_map(|r| Some((r.get("_id")?.as_str()?, r)))
        .collect();
    let name = resources
        .iter()
        .find(|r| kind(r) == "workspace")
        .and_then(|w| w.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Insomnia import")
        .to_string();
    let items = resources
        .iter()
        .filter(|r| kind(r) == "request")
        .map(|r| Item {
            folders: folders_of(&by_id, r.get("parentId").and_then(Value::as_str)),
            name: r
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("request")
                .to_string(),
            req: request_of(r),
        })
        .collect();
    Ok(Imported { name, items })
}

fn kind(resource: &Value) -> &str {
    resource.get("_type").and_then(Value::as_str).unwrap_or("")
}

/// The request groups above a request, outermost first. The walk stops
/// after one step per resource, so a cyclic `parentId` cannot loop.
fn folders_of(by_id: &HashMap<&str, &Value>, parent: Option<&str>) -> Vec<String> {
    let mut names = Vec::new();
    let mut id = parent;
    for _ in 0..by_id.len() {
        let Some(group) = id
            .and_then(|i| by_id.get(i))
            .filter(|g| kind(g) == "request_group")
        else {
            break;
        };
        names.push(
            group
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("folder")
                .to_string(),
        );
        id = group.get("parentId").and_then(Value::as_str);
    }
    names.reverse();
    names
}

fn request_of(r: &Value) -> RequestFile {
    let method = r
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let raw = var_syntax(r.get("url").and_then(Value::as_str).unwrap_or(""));
    let base = raw.split_once('?').map_or(raw.as_str(), |(base, _)| base);
    let params = rows(r.get("parameters"));
    RequestFile {
        method,
        url: query::url_from_params(base, &params),
        params,
        headers: rows(r.get("headers")),
        body: body_of(r.get("body")),
        auth: auth_of(r.get("authentication")),
        ..RequestFile::default()
    }
}

/// Insomnia's `{{ _.name }}` is beeline's `{{name}}`.
fn var_syntax(text: &str) -> String {
    text.replace("{{ _.", "{{")
        .replace("{{_.", "{{")
        .replace(" }}", "}}")
}

/// Name/value rows with their disabled flag. Variables are converted.
fn rows(list: Option<&Value>) -> Vec<KeyValue> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let name = row.get("name").and_then(Value::as_str)?;
            let value = row.get("value").and_then(Value::as_str).unwrap_or("");
            let mut kv = KeyValue::new(var_syntax(name), var_syntax(value));
            kv.enabled = !row
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(kv)
        })
        .collect()
}

/// Bearer and basic auth. Other kinds beeline lacks come in as none.
fn auth_of(auth: Option<&Value>) -> Auth {
    let Some(auth) = auth else {
        return Auth::default();
    };
    let field = |key: &str| var_syntax(auth.get(key).and_then(Value::as_str).unwrap_or(""));
    match auth.get("type").and_then(Value::as_str) {
        Some("bearer") => Auth {
            kind: AuthKind::Bearer,
            token: field("token"),
            ..Auth::default()
        },
        Some("basic") => Auth {
            kind: AuthKind::Basic,
            username: field("username"),
            password: field("password"),
            ..Auth::default()
        },
        _ => Auth::default(),
    }
}

/// Form fields when the type is urlencoded, JSON or text otherwise. A
/// multipart upload has no beeline equivalent and comes in without a body.
fn body_of(body: Option<&Value>) -> Body {
    let Some(body) = body else {
        return Body::default();
    };
    let mime = body
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if mime.contains("multipart") {
        return Body::default();
    }
    if mime.contains("x-www-form-urlencoded") {
        let fields = rows(body.get("params"));
        if !fields.is_empty() {
            return Body {
                kind: BodyKind::Form,
                text: String::new(),
                fields,
            };
        }
    }
    let text = var_syntax(body.get("text").and_then(Value::as_str).unwrap_or(""));
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
        serde_json::from_str(include_str!(
            "../../fixtures/import/insomnia/shop-export.json"
        ))
        .unwrap()
    }

    fn shop() -> Imported {
        parse(&doc()).unwrap()
    }

    #[test]
    fn detects_an_insomnia_export() {
        assert!(is_insomnia(&doc()));
        assert!(!is_insomnia(&json!({"info": {}, "item": []})));
    }

    #[test]
    fn the_workspace_names_the_folder_and_groups_nest() {
        let imported = shop();
        assert_eq!(imported.name, "Shop");
        let paths: Vec<String> = imported
            .items
            .iter()
            .map(|i| [i.folders.clone(), vec![i.name.clone()]].concat().join("/"))
            .collect();
        assert_eq!(
            paths,
            vec![
                "Orders/List orders",
                "Orders/Create order",
                "Orders/Archive/Old search",
                "Login",
            ]
        );
    }

    #[test]
    fn environments_are_skipped() {
        assert_eq!(shop().items.len(), 4);
    }

    #[test]
    fn variables_and_disabled_rows_come_across() {
        let req = &shop().items[0].req;
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "{{base_url}}/api/orders?limit=20");
        assert_eq!(
            req.params,
            vec![
                KeyValue::new("limit", "20"),
                KeyValue {
                    enabled: false,
                    ..KeyValue::new("debug", "1")
                },
            ]
        );
        assert_eq!(req.headers.len(), 2);
        assert!(req.headers[0].enabled);
        assert!(!req.headers[1].enabled, "X-Trace stays off");
    }

    #[test]
    fn bearer_auth_comes_across_with_its_variable() {
        let req = &shop().items[0].req;
        assert_eq!(req.auth.kind, AuthKind::Bearer);
        assert_eq!(req.auth.token, "{{token}}");
    }

    #[test]
    fn basic_auth_comes_across() {
        let req = &shop().items[2].req;
        assert_eq!(req.auth.kind, AuthKind::Basic);
        assert_eq!(req.auth.username, "demo");
        assert_eq!(req.auth.password, "secret");
    }

    #[test]
    fn json_and_form_bodies() {
        let items = shop().items;
        assert_eq!(items[1].req.body.kind, BodyKind::Json);
        assert_eq!(items[1].req.body.text, r#"{"sku":"A1","qty":2}"#);
        let login = &items[3].req;
        assert_eq!(login.method, "POST");
        assert_eq!(login.body.kind, BodyKind::Form);
        assert_eq!(
            login.body.fields,
            vec![
                KeyValue::new("user", "demo"),
                KeyValue::new("pass", "secret")
            ]
        );
    }

    #[test]
    fn a_file_without_a_workspace_still_imports() {
        let doc = json!({
            "_type": "export",
            "resources": [
                {"_id": "req_1", "_type": "request", "name": "Ping", "method": "GET", "url": "https://x.test/ping"}
            ]
        });
        let imported = parse(&doc).unwrap();
        assert_eq!(imported.name, "Insomnia import");
        assert_eq!(imported.items[0].name, "Ping");
        assert!(imported.items[0].folders.is_empty());
    }
}
