//! Bruno's YAML format (OpenCollection) as requests. Pure: this reads
//! parsed YAML; walking the collection folder is in `import/mod.rs`.
//!
//! A collection folder has `opencollection.yml`, each subfolder may have a
//! `folder.yml`, and every other `.yml` file with an `http` section is a
//! request. A workspace folder has `workspace.yml` listing its collections.
//! Settings are handed down the same way as for `.bru` files, through
//! [`Inherited`].

use serde_json::Value;

use super::bruno::{AuthSpec, Inherited, apply_auth, layered, text_body};
use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};
use crate::query;

/// A scalar as text: strings as they are, numbers and booleans written out.
fn scalar(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// A list of `{name, value, disabled}` rows. `disabled: true` rows are off.
fn rows(v: Option<&Value>) -> Vec<KeyValue> {
    let Some(list) = v.and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|row| {
            let name = row.get("name")?.as_str()?;
            let mut kv = KeyValue::new(name, scalar(row.get("value")));
            kv.enabled = !row
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(kv)
        })
        .collect()
}

/// Only the rows that are on. Variables have no off state in beeline.
fn enabled(rows: Vec<KeyValue>) -> Vec<KeyValue> {
    rows.into_iter().filter(|kv| kv.enabled).collect()
}

/// `auth` as either a mode string (`inherit`, `none`) or a `{type, …}` map.
fn auth_spec(v: Option<&Value>) -> AuthSpec {
    let Some(v) = v else {
        return AuthSpec::Inherit;
    };
    if let Some(mode) = v.as_str() {
        return match mode {
            "inherit" => AuthSpec::Inherit,
            _ => AuthSpec::None,
        };
    }
    let field = |k: &str| scalar(v.get(k));
    match v.get("type").and_then(Value::as_str).unwrap_or("") {
        "inherit" => AuthSpec::Inherit,
        "bearer" => AuthSpec::Plain(Auth {
            kind: AuthKind::Bearer,
            token: field("token"),
            ..Auth::default()
        }),
        "basic" => AuthSpec::Plain(Auth {
            kind: AuthKind::Basic,
            username: field("username"),
            password: field("password"),
            ..Auth::default()
        }),
        "apikey" => AuthSpec::ApiKey {
            key: field("key"),
            value: field("value"),
            in_query: matches!(field("placement").as_str(), "query" | "queryparams"),
        },
        _ => AuthSpec::None,
    }
}

/// Reads `opencollection.yml` or a `folder.yml` and layers its `request`
/// section over `parent`, as [`super::bruno::inherit`] does for `.bru`.
pub fn inherit(parent: &Inherited, doc: &Value) -> Inherited {
    let request = doc.get("request");
    let section = |k: &str| request.and_then(|r| r.get(k));
    let auth = match auth_spec(section("auth")) {
        AuthSpec::Inherit => parent.auth.clone(),
        own => own,
    };
    Inherited {
        headers: layered(&parent.headers, rows(section("headers"))),
        vars: layered(&parent.vars, enabled(rows(section("variables")))),
        auth,
    }
}

/// A request file as (name, request), or None when it has no `http`
/// section. `fallback_name` is used when `info.name` is missing.
pub fn request(
    doc: &Value,
    fallback_name: &str,
    inherited: &Inherited,
) -> Option<(String, RequestFile)> {
    let http = doc.get("http")?.as_object()?;
    let name = doc
        .get("info")
        .and_then(|i| i.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| fallback_name.to_string());
    let method = http
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();

    let mut url = scalar(http.get("url"));
    let mut vars = layered(
        &inherited.vars,
        enabled(rows(doc.get("runtime").and_then(|r| r.get("variables")))),
    );
    let all_params = http.get("params").and_then(Value::as_array);
    let typed = |kind: &str| -> Vec<Value> {
        all_params
            .into_iter()
            .flatten()
            .filter(|p| p.get("type").and_then(Value::as_str).unwrap_or("query") == kind)
            .cloned()
            .collect()
    };
    // `:id` path parameters become `{{id}}`, valued through the request's vars.
    let path_rows = rows(Some(&Value::Array(typed("path"))));
    for kv in &path_rows {
        url = url.replace(&format!(":{}", kv.key), &format!("{{{{{}}}}}", kv.key));
    }
    vars = layered(&vars, path_rows);
    let mut params = if all_params.is_some() {
        rows(Some(&Value::Array(typed("query"))))
    } else {
        query::params_from_url(&url, &[])
    };
    let mut headers = layered(&inherited.headers, rows(http.get("headers")));

    let auth = match auth_spec(http.get("auth")) {
        AuthSpec::Inherit => inherited.auth.clone(),
        own => own,
    };
    let auth = apply_auth(auth, &mut url, &mut params, &mut headers);
    let body = body(http.get("body"));

    Some((
        name,
        RequestFile {
            method,
            url,
            params,
            headers,
            vars,
            body,
            auth,
        },
    ))
}

fn body(v: Option<&Value>) -> Body {
    let Some(v) = v else {
        return Body::default();
    };
    let data = v.get("data");
    match v.get("type").and_then(Value::as_str).unwrap_or("") {
        "json" => text_body(BodyKind::Json, scalar(data).trim().to_string()),
        "text" | "xml" => text_body(BodyKind::Text, scalar(data).trim().to_string()),
        "form-urlencoded" => Body {
            kind: BodyKind::Form,
            text: String::new(),
            fields: rows(data),
        },
        // Multipart and file bodies have no beeline equivalent.
        _ => Body::default(),
    }
}

/// The collections a `workspace.yml` lists, as (name, path relative to
/// the workspace folder).
pub fn workspace_collections(doc: &Value) -> Vec<(String, String)> {
    doc.get("collections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let path = c.get("path")?.as_str()?.to_string();
            let name = c
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| path.rsplit(['/', '\\']).next().unwrap_or(&path).into());
            Some((name, path))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(text: &str) -> Value {
        serde_yaml_ng::from_str(text).unwrap()
    }

    #[test]
    fn a_file_without_an_http_section_is_not_a_request() {
        let doc = yaml("info:\n  name: Docs\n  type: folder\n");
        assert!(request(&doc, "x", &Inherited::default()).is_none());
    }

    #[test]
    fn reads_method_url_headers_vars_and_json_body() {
        let doc = yaml(
            "info:\n  name: Add row\n  type: http\nhttp:\n  method: post\n  url: \"{{base}}/rows/{{id}}\"\n  headers:\n    - name: Content-Type\n      value: application/json\n    - name: X-Off\n      value: a\n      disabled: true\n  body:\n    type: json\n    data: |-\n      {\n        \"a\": 1\n      }\n  auth: inherit\nruntime:\n  variables:\n    - name: id\n      value: 932\n    - name: gone\n      value: x\n      disabled: true\n",
        );
        let (name, req) = request(&doc, "fallback", &Inherited::default()).unwrap();
        assert_eq!(name, "Add row");
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "{{base}}/rows/{{id}}");
        assert_eq!(req.headers.len(), 2);
        assert!(!req.headers[1].enabled);
        assert_eq!(req.vars, vec![KeyValue::new("id", "932")]);
        assert_eq!(req.body.kind, BodyKind::Json);
        assert_eq!(req.body.text, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn query_params_keep_disabled_rows_and_path_params_become_vars() {
        let doc = yaml(
            "http:\n  method: GET\n  url: \"https://x/t/:id?$top=1\"\n  params:\n    - name: $top\n      value: \"1\"\n      type: query\n    - name: $filter\n      value: a\n      type: query\n      disabled: true\n    - name: id\n      value: \"7\"\n      type: path\n",
        );
        let (name, req) = request(&doc, "Table", &Inherited::default()).unwrap();
        assert_eq!(name, "Table");
        assert_eq!(req.url, "https://x/t/{{id}}?$top=1");
        assert_eq!(req.params.len(), 2);
        assert!(!req.params[1].enabled);
        assert_eq!(req.vars, vec![KeyValue::new("id", "7")]);
    }

    #[test]
    fn collection_api_key_and_variables_reach_inheriting_requests() {
        let collection = inherit(
            &Inherited::default(),
            &yaml(
                "request:\n  auth:\n    type: apikey\n    key: Api-Key\n    value: k1\n    placement: header\n  variables:\n    - name: API_URL\n      value: https://a.test\n    - name: API_URL\n      value: http://localhost\n      disabled: true\n",
            ),
        );
        assert_eq!(
            collection.vars,
            vec![KeyValue::new("API_URL", "https://a.test")]
        );
        let doc = yaml("http:\n  method: GET\n  url: \"{{API_URL}}/health\"\n  auth: inherit\n");
        let (_, req) = request(&doc, "Health", &collection).unwrap();
        assert_eq!(req.headers, vec![KeyValue::new("Api-Key", "k1")]);
        assert_eq!(req.auth.kind, AuthKind::None);
        assert_eq!(req.vars, collection.vars);

        let own = yaml(
            "http:\n  method: GET\n  url: https://x\n  auth:\n    type: bearer\n    token: t\n",
        );
        let (_, req) = request(&own, "x", &collection).unwrap();
        assert_eq!(req.auth.kind, AuthKind::Bearer);
        assert!(req.headers.is_empty());
    }

    #[test]
    fn form_bodies_are_rows_and_multipart_is_skipped() {
        let form = yaml(
            "http:\n  method: POST\n  url: https://x\n  body:\n    type: form-urlencoded\n    data:\n      - name: a\n        value: \"1\"\n",
        );
        let (_, req) = request(&form, "x", &Inherited::default()).unwrap();
        assert_eq!(req.body.kind, BodyKind::Form);
        assert_eq!(req.body.fields, vec![KeyValue::new("a", "1")]);

        let multi = yaml(
            "http:\n  method: POST\n  url: https://x\n  body:\n    type: multipart-form\n    data: []\n",
        );
        let (_, req) = request(&multi, "x", &Inherited::default()).unwrap();
        assert_eq!(req.body.kind, BodyKind::None);
    }

    #[test]
    fn workspace_lists_its_collections() {
        let doc = yaml(
            "info:\n  name: W\n  type: workspace\ncollections:\n  - name: \"A (one)\"\n    path: \"collections/A\"\n  - path: collections/B\n",
        );
        assert_eq!(
            workspace_collections(&doc),
            vec![
                ("A (one)".to_string(), "collections/A".to_string()),
                ("B".to_string(), "collections/B".to_string()),
            ]
        );
    }
}
