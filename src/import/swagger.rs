//! A Swagger 2.0 spec as requests. Pure. The same requests the OpenAPI
//! importer makes: one per operation, in folders by first tag, with `{id}`
//! path parameters written as `{{id}}`. Body parameters become a JSON or text
//! body from their example. `formData` becomes a form, unless the request
//! takes multipart, which has no beeline equivalent.

use serde_json::Value;

use super::openapi::{METHODS, deref, example_text};
use super::{Imported, Item};
use crate::model::{Body, BodyKind, KeyValue, RequestFile};
use crate::query;

/// True when `doc` is a Swagger 2.0 spec.
pub fn is_swagger(doc: &Value) -> bool {
    doc.get("swagger")
        .and_then(Value::as_str)
        .is_some_and(|v| v.starts_with('2'))
}

pub fn parse(doc: &Value) -> Result<Imported, String> {
    let name = doc
        .pointer("/info/title")
        .and_then(Value::as_str)
        .unwrap_or("Swagger import")
        .to_string();
    let base = base_url(doc);
    let mut items = Vec::new();
    for (path, path_item) in doc
        .get("paths")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let path_item = deref(doc, path_item);
        let shared = params_of(doc, path_item);
        for method in METHODS {
            let Some(op) = path_item.get(method) else {
                continue;
            };
            let op = deref(doc, op);
            let mut params = shared.clone();
            for p in params_of(doc, op) {
                // An operation's parameter replaces a path-level one of the same name.
                params.retain(|q| q.get("name") != p.get("name") || q.get("in") != p.get("in"));
                params.push(p);
            }
            items.push(Item {
                folders: op
                    .pointer("/tags/0")
                    .and_then(Value::as_str)
                    .map(|t| vec![t.to_string()])
                    .unwrap_or_default(),
                name: op
                    .get("summary")
                    .or_else(|| op.get("operationId"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{} {path}", method.to_uppercase())),
                req: request(doc, op, method, &base, path, &params),
            });
        }
    }
    Ok(Imported { name, items })
}

/// `scheme://host` plus the base path. With no host, the base URL variable
/// stands in for it, as the OpenAPI importer does.
fn base_url(doc: &Value) -> String {
    let base_path = doc
        .get("basePath")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_end_matches('/');
    match doc.get("host").and_then(Value::as_str) {
        Some(host) => {
            let scheme = doc
                .pointer("/schemes/0")
                .and_then(Value::as_str)
                .unwrap_or("https");
            format!("{scheme}://{host}{base_path}")
        }
        None => format!("{{{{baseUrl}}}}{base_path}"),
    }
}

fn params_of<'a>(doc: &'a Value, node: &'a Value) -> Vec<&'a Value> {
    node.get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|p| deref(doc, p))
        .collect()
}

fn request(
    doc: &Value,
    op: &Value,
    method: &str,
    base: &str,
    path: &str,
    params: &[&Value],
) -> RequestFile {
    // `{id}` in a Swagger path is `{{id}}` in beeline.
    let url = format!("{base}{}", path.replace('{', "{{").replace('}', "}}"));
    let located = |place: &str| -> Vec<KeyValue> {
        params
            .iter()
            .filter(|p| p.get("in").and_then(Value::as_str) == Some(place))
            .map(|p| row(p))
            .collect()
    };
    let query_rows = located("query");
    let mut req = RequestFile {
        method: method.to_uppercase(),
        url: query::url_from_params(&url, &query_rows),
        params: query_rows,
        headers: located("header"),
        ..RequestFile::default()
    };
    let consumes = consumes_of(doc, op);
    if let Some(body) = params
        .iter()
        .find(|p| p.get("in").and_then(Value::as_str) == Some("body"))
    {
        req.body = body_param(doc, body, &consumes);
    } else if params
        .iter()
        .any(|p| p.get("in").and_then(Value::as_str) == Some("formData"))
    {
        req.body = form_data(params, &consumes);
    }
    req
}

/// A parameter as a row. Optional ones come in switched off, as the OpenAPI
/// importer does.
fn row(p: &Value) -> KeyValue {
    let mut kv = KeyValue::new(
        p.get("name").and_then(Value::as_str).unwrap_or(""),
        example_text(
            p.get("x-example")
                .or_else(|| p.get("example"))
                .or_else(|| p.get("default")),
        ),
    );
    kv.enabled = p.get("required").and_then(Value::as_bool).unwrap_or(false);
    kv
}

/// The media type the operation takes, from its `consumes` or the spec's.
fn consumes_of(doc: &Value, op: &Value) -> String {
    op.get("consumes")
        .or_else(|| doc.get("consumes"))
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// A body parameter's example as JSON (pretty), or its text for other types.
fn body_param(doc: &Value, body: &Value, consumes: &str) -> Body {
    let schema = body.get("schema").map(|s| deref(doc, s));
    let example = body
        .get("x-example")
        .or_else(|| schema.and_then(|s| s.get("example")));
    if consumes.contains("multipart") {
        return Body::default();
    }
    let json = consumes.is_empty() || consumes.contains("json");
    if json {
        let text = example
            .and_then(|v| serde_json::to_string_pretty(v).ok())
            .unwrap_or_else(|| "{}".into());
        return Body {
            kind: BodyKind::Json,
            text,
            fields: vec![],
        };
    }
    let text = example.map(|v| example_text(Some(v))).unwrap_or_default();
    Body {
        kind: BodyKind::Text,
        text,
        fields: vec![],
    }
}

/// `formData` parameters as form fields. A multipart request has file
/// uploads, which beeline cannot send, so it comes in without a body.
fn form_data(params: &[&Value], consumes: &str) -> Body {
    if consumes.contains("multipart") {
        return Body::default();
    }
    let fields = params
        .iter()
        .filter(|p| {
            p.get("in").and_then(Value::as_str) == Some("formData")
                && p.get("type").and_then(Value::as_str) != Some("file")
        })
        .map(|p| row(p))
        .collect();
    Body {
        kind: BodyKind::Form,
        text: String::new(),
        fields,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BodyKind, KeyValue};
    use serde_json::json;

    fn doc() -> Value {
        serde_json::from_str(include_str!("../../fixtures/import/swagger/pets-v2.json")).unwrap()
    }

    #[test]
    fn detects_swagger_2_only() {
        assert!(is_swagger(&doc()));
        assert!(!is_swagger(&json!({"openapi": "3.0.0"})));
    }

    #[test]
    fn one_request_per_operation_in_tag_folders() {
        let imported = parse(&doc()).unwrap();
        assert_eq!(imported.name, "Pets (Swagger 2.0)");
        let names: Vec<(Vec<String>, String)> = imported
            .items
            .iter()
            .map(|i| (i.folders.clone(), i.name.clone()))
            .collect();
        assert_eq!(
            names,
            vec![
                (vec!["pet".to_string()], "List pets".to_string()),
                (vec!["pet".to_string()], "addPet".to_string()),
                (vec!["pet".to_string()], "deletePet".to_string()),
                (vec!["store".to_string()], "Place an order".to_string()),
            ]
        );
    }

    #[test]
    fn host_scheme_and_base_path_form_the_url() {
        let items = parse(&doc()).unwrap().items;
        assert_eq!(
            items[2].req.url,
            "https://pets.example.test/v2/pets/{{petId}}"
        );
        assert_eq!(items[2].req.method, "DELETE");
    }

    #[test]
    fn query_and_header_parameters_with_their_state() {
        let req = &parse(&doc()).unwrap().items[0].req;
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://pets.example.test/v2/pets?tag=dog");
        assert_eq!(
            req.params,
            vec![
                KeyValue {
                    enabled: false,
                    ..KeyValue::new("limit", "20")
                },
                KeyValue::new("tag", "dog"),
            ],
            "required params are on, the rest are off"
        );
        assert_eq!(req.headers.len(), 1);
        assert!(!req.headers[0].enabled);
    }

    #[test]
    fn body_parameter_example_becomes_a_json_body() {
        let req = &parse(&doc()).unwrap().items[1].req;
        assert_eq!(req.method, "POST");
        assert_eq!(req.body.kind, BodyKind::Json);
        assert!(req.body.text.contains("\"Rex\""), "{}", req.body.text);
    }

    #[test]
    fn form_data_becomes_a_form_body() {
        let req = &parse(&doc()).unwrap().items[3].req;
        assert_eq!(req.body.kind, BodyKind::Form);
        assert_eq!(
            req.body.fields,
            vec![
                KeyValue::new("petId", "7"),
                KeyValue {
                    enabled: false,
                    ..KeyValue::new("quantity", "1")
                },
            ]
        );
    }

    #[test]
    fn no_host_means_the_base_url_variable() {
        let spec = json!({
            "swagger": "2.0",
            "info": {"title": "Bare"},
            "basePath": "/api",
            "paths": {"/ping": {"get": {"responses": {}}}}
        });
        let imported = parse(&spec).unwrap();
        assert_eq!(imported.name, "Bare");
        assert_eq!(imported.items[0].req.url, "{{baseUrl}}/api/ping");
        assert_eq!(imported.items[0].name, "GET /ping");
    }
}
