//! An OpenAPI 3 spec (JSON or YAML) as requests. Pure. One request per
//! operation, in a folder per first tag. Security schemes are not mapped
//! to auth; set auth on the requests after importing.

use serde_json::Value;

use super::{Imported, Item};
use crate::model::{Body, BodyKind, KeyValue, RequestFile};
use crate::query;

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

/// Reads `text` as JSON, or as YAML when it is not JSON.
pub fn read(text: &str) -> Result<Value, String> {
    serde_json::from_str(text)
        .or_else(|_| serde_yaml_ng::from_str(text))
        .map_err(|e| format!("Not JSON or YAML: {e}"))
}

/// True when `doc` looks like an OpenAPI 3 spec.
pub fn is_openapi(doc: &Value) -> bool {
    doc.get("openapi").is_some() || doc.get("swagger").is_some()
}

pub fn parse(doc: &Value) -> Result<Imported, String> {
    if doc.get("swagger").is_some() {
        return Err("Swagger 2.0 specs are not supported; convert to OpenAPI 3 first".into());
    }
    let version = doc.get("openapi").and_then(Value::as_str).unwrap_or("");
    if !version.starts_with('3') {
        return Err(format!(
            "Unsupported OpenAPI version \"{version}\"; expected 3.x"
        ));
    }
    let name = doc
        .pointer("/info/title")
        .and_then(Value::as_str)
        .unwrap_or("OpenAPI import")
        .to_string();
    let base = server_url(doc);

    let mut items = Vec::new();
    let paths = doc.get("paths").and_then(Value::as_object);
    for (path, path_item) in paths.into_iter().flatten() {
        let path_item = deref(doc, path_item);
        let shared = params_of(doc, path_item);
        for method in METHODS {
            let Some(op) = path_item.get(method) else {
                continue;
            };
            let op = deref(doc, op);
            let mut params: Vec<&Value> = shared.clone();
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
                req: request(doc, method, &base, path, &params, op),
            });
        }
    }
    Ok(Imported { name, items })
}

fn request(
    doc: &Value,
    method: &str,
    base: &str,
    path: &str,
    params: &[&Value],
    op: &Value,
) -> RequestFile {
    // `{id}` in an OpenAPI path is `{{id}}` in beeline.
    let url = format!(
        "{}{}",
        base.trim_end_matches('/'),
        path.replace('{', "{{").replace('}', "}}")
    );
    let row = |p: &Value| {
        let mut kv = KeyValue::new(
            p.get("name").and_then(Value::as_str).unwrap_or(""),
            example_text(
                p.get("example")
                    .or_else(|| p.pointer("/schema/example"))
                    .or_else(|| p.pointer("/schema/default")),
            ),
        );
        // Optional parameters come in switched off, so the request sends
        // only what the API requires until the user turns more on.
        kv.enabled = p.get("required").and_then(Value::as_bool).unwrap_or(false);
        kv
    };
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
    if let Some(body) = op.get("requestBody").map(|b| deref(doc, b)) {
        req.body = request_body(doc, body);
    }
    req
}

/// The first server's URL with its variables filled from their defaults,
/// or `{{baseUrl}}` when the spec names no server.
fn server_url(doc: &Value) -> String {
    let Some(server) = doc.pointer("/servers/0") else {
        return "{{baseUrl}}".into();
    };
    let mut url = server
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(vars) = server.get("variables").and_then(Value::as_object) {
        for (name, var) in vars {
            if let Some(default) = var.get("default").and_then(Value::as_str) {
                url = url.replace(&format!("{{{name}}}"), default);
            }
        }
    }
    url
}

fn params_of<'a>(doc: &'a Value, node: &'a Value) -> Vec<&'a Value> {
    node.get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|p| deref(doc, p))
        .collect()
}

fn request_body(doc: &Value, body: &Value) -> Body {
    let Some(content) = body.get("content").and_then(Value::as_object) else {
        return Body::default();
    };
    let example = |media: &Value| {
        media
            .get("example")
            .or_else(|| media.pointer("/examples").and_then(first_example))
            .or_else(|| {
                media
                    .get("schema")
                    .map(|s| deref(doc, s))
                    .and_then(|s| s.get("example"))
            })
            .cloned()
    };
    if let Some((_, media)) = content.iter().find(|(t, _)| t.contains("json")) {
        let text = example(media)
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| "{}".into());
        return Body {
            kind: BodyKind::Json,
            text,
            fields: vec![],
        };
    }
    if let Some(media) = content.get("application/x-www-form-urlencoded") {
        let schema = media.get("schema").map(|s| deref(doc, s));
        let fields = schema
            .and_then(|s| s.get("properties"))
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(name, prop)| KeyValue::new(name.clone(), example_text(prop.get("example"))))
            .collect();
        return Body {
            kind: BodyKind::Form,
            text: String::new(),
            fields,
        };
    }
    let text = content
        .values()
        .next()
        .and_then(example)
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default();
    Body {
        kind: BodyKind::Text,
        text,
        fields: vec![],
    }
}

fn first_example(examples: &Value) -> Option<&Value> {
    examples.as_object()?.values().next()?.get("value")
}

/// An example value as row text: strings as written, other JSON as JSON.
fn example_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// Follows `$ref`s within the document, a few levels deep.
fn deref<'a>(doc: &'a Value, mut v: &'a Value) -> &'a Value {
    for _ in 0..8 {
        match v
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix('#'))
            .and_then(|pointer| doc.pointer(pointer))
        {
            Some(target) => v = target,
            None => break,
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const PETS: &str = r##"{
      "openapi": "3.0.3",
      "info": {"title": "Pet Store"},
      "servers": [{"url": "https://{env}.pets.test/v1", "variables": {"env": {"default": "api"}}}],
      "paths": {
        "/pets": {
          "get": {
            "summary": "List pets",
            "tags": ["pets"],
            "parameters": [
              {"name": "limit", "in": "query", "schema": {"default": 20}},
              {"name": "species", "in": "query", "required": true, "example": "cat"}
            ]
          },
          "post": {
            "operationId": "createPet",
            "tags": ["pets"],
            "parameters": [{"$ref": "#/components/parameters/Trace"}],
            "requestBody": {"content": {"application/json": {"example": {"name": "Tom"}}}}
          }
        },
        "/pets/{petId}": {
          "parameters": [{"name": "petId", "in": "path", "required": true}],
          "delete": {}
        }
      },
      "components": {
        "parameters": {"Trace": {"name": "X-Trace", "in": "header", "required": true, "example": "abc"}}
      }
    }"##;

    fn pets() -> Imported {
        parse(&read(PETS).unwrap()).unwrap()
    }

    #[test]
    fn names_the_folder_after_the_title_and_groups_by_tag() {
        let imported = pets();
        assert_eq!(imported.name, "Pet Store");
        let names: Vec<(&[String], &str)> = imported
            .items
            .iter()
            .map(|i| (i.folders.as_slice(), i.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                (&["pets".to_string()][..], "List pets"),
                (&["pets".to_string()][..], "createPet"),
                (&[][..], "DELETE /pets/{petId}"),
            ]
        );
    }

    #[test]
    fn query_params_required_on_optional_off_and_mirrored_in_the_url() {
        let list = &pets().items[0].req;
        assert_eq!(list.method, "GET");
        assert_eq!(list.url, "https://api.pets.test/v1/pets?species=cat");
        assert_eq!(list.params.len(), 2);
        assert!(!list.params[0].enabled, "optional limit is off");
        assert_eq!(list.params[0].value, "20");
        assert!(list.params[1].enabled);
    }

    #[test]
    fn json_body_uses_the_example_and_refs_resolve() {
        let create = &pets().items[1].req;
        assert_eq!(create.body.kind, BodyKind::Json);
        assert!(create.body.text.contains("\"name\": \"Tom\""));
        assert_eq!(create.headers, vec![KeyValue::new("X-Trace", "abc")]);
    }

    #[test]
    fn path_parameters_become_beeline_variables() {
        assert_eq!(
            pets().items[2].req.url,
            "https://api.pets.test/v1/pets/{{petId}}"
        );
    }

    #[test]
    fn reads_yaml_and_falls_back_to_a_base_url_variable() {
        let yaml = "openapi: 3.1.0\ninfo:\n  title: Mini\npaths:\n  /health:\n    get:\n      summary: Health\n";
        let imported = parse(&read(yaml).unwrap()).unwrap();
        assert_eq!(imported.items[0].req.url, "{{baseUrl}}/health");
    }

    #[test]
    fn swagger_two_is_refused_with_a_reason() {
        let err = parse(&read(r#"{"swagger": "2.0"}"#).unwrap()).unwrap_err();
        assert!(err.contains("Swagger 2.0"), "{err}");
    }
}
