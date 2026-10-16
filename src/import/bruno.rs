//! Bruno `.bru` files as requests. Pure: this reads file text; walking the
//! collection folder is in `import/mod.rs`.
//!
//! A `.bru` file is a list of `name { … }` blocks with their content
//! indented two spaces. Key/value blocks hold `key: value` lines, where a
//! leading `~` switches a row off and deeper-indented lines continue the
//! value above. Body blocks hold raw text.
//!
//! `collection.bru` and `folder.bru` give headers, auth and variables that
//! requests inherit. beeline has no collection-level settings, so inherited
//! headers are copied into each request, and inherited variables into each
//! request's own variables (environments are not imported).

use serde_json::Value;

use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};
use crate::query;

/// One `name { … }` block: its name and its lines with the two-space
/// indent removed.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub lines: Vec<String>,
}

/// The blocks of a `.bru` file, in order.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut current: Option<Block> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        match &mut current {
            None => {
                if let Some(name) = line.strip_suffix('{').map(str::trim)
                    && !name.is_empty()
                    && !line.starts_with(char::is_whitespace)
                {
                    current = Some(Block {
                        name: name.to_string(),
                        lines: Vec::new(),
                    });
                }
            }
            Some(block) => {
                if line == "}" {
                    out.extend(current.take());
                } else {
                    block
                        .lines
                        .push(line.strip_prefix("  ").unwrap_or(line).to_string());
                }
            }
        }
    }
    out
}

fn find<'a>(blocks: &'a [Block], name: &str) -> Option<&'a Block> {
    blocks.iter().find(|b| b.name == name)
}

/// A block's `key: value` rows. `~key` rows are off. A line that does not
/// look like `key:` (indented deeper, or like `</Tag>` closing a multi-line
/// XML value) continues the previous value, and a value of `'''` runs to
/// the closing `'''` line.
pub fn rows(block: &Block) -> Vec<KeyValue> {
    let mut out: Vec<KeyValue> = Vec::new();
    let mut lines = block.lines.iter();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        if !starts_row(line) {
            if let Some(last) = out.last_mut() {
                last.value.push('\n');
                last.value.push_str(line.trim_start());
            }
            continue;
        }
        let (enabled, line) = match line.strip_prefix('~') {
            Some(rest) => (false, rest),
            None => (true, line.as_str()),
        };
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let mut value = value.trim().to_string();
        if value == "'''" {
            let body: Vec<&str> = lines
                .by_ref()
                .take_while(|l| l.trim() != "'''")
                .map(|l| l.strip_prefix("  ").unwrap_or(l))
                .collect();
            value = body.join("\n");
        }
        let mut kv = KeyValue::new(key.trim(), value);
        kv.enabled = enabled;
        out.push(kv);
    }
    out
}

/// True when `line` begins a new `key: value` row: no indent, and a key
/// before the first colon with no spaces or `<` in it.
fn starts_row(line: &str) -> bool {
    !line.starts_with(char::is_whitespace)
        && line.split_once(':').is_some_and(|(key, _)| {
            let key = key.trim_start_matches('~');
            !key.is_empty() && !key.contains(char::is_whitespace) && !key.contains('<')
        })
}

fn value_of(block: Option<&Block>, key: &str) -> Option<String> {
    rows(block?)
        .into_iter()
        .find(|kv| kv.enabled && kv.key == key)
        .map(|kv| kv.value)
}

fn text_of(block: Option<&Block>) -> String {
    block
        .map(|b| b.lines.join("\n").trim().to_string())
        .unwrap_or_default()
}

/// What `collection.bru` or `folder.bru` hands down to the requests in it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inherited {
    pub headers: Vec<KeyValue>,
    pub vars: Vec<KeyValue>,
    pub auth: AuthSpec,
}

/// Auth as Bruno describes it. API keys have no beeline auth kind; they
/// become a header or query row when applied to a request.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum AuthSpec {
    #[default]
    None,
    Inherit,
    Plain(Auth),
    ApiKey {
        key: String,
        value: String,
        in_query: bool,
    },
}

/// Reads a collection or folder file and layers it over `parent`: its
/// headers and variables add to the parent's (same keys replaced), and its
/// auth replaces the parent's unless it says to inherit.
pub fn inherit(parent: &Inherited, text: &str) -> Inherited {
    let blocks = blocks(text);
    let mode = value_of(find(&blocks, "auth"), "mode").unwrap_or_else(|| "inherit".into());
    let auth = match auth_spec(&blocks, &mode) {
        AuthSpec::Inherit => parent.auth.clone(),
        own => own,
    };
    Inherited {
        headers: layered(
            &parent.headers,
            find(&blocks, "headers").map(rows).unwrap_or_default(),
        ),
        vars: layered(
            &parent.vars,
            find(&blocks, "vars:pre-request")
                .map(rows)
                .unwrap_or_default(),
        ),
        auth,
    }
}

/// `base` with `over`'s rows added; a row in `over` replaces any row in
/// `base` with the same key.
fn layered(base: &[KeyValue], over: Vec<KeyValue>) -> Vec<KeyValue> {
    let mut out: Vec<KeyValue> = base
        .iter()
        .filter(|b| !over.iter().any(|o| o.key.eq_ignore_ascii_case(&b.key)))
        .cloned()
        .collect();
    out.extend(over);
    out
}

fn auth_spec(blocks: &[Block], mode: &str) -> AuthSpec {
    let block = |kind: &str| find(blocks, &format!("auth:{kind}"));
    match mode {
        "inherit" => AuthSpec::Inherit,
        "bearer" => AuthSpec::Plain(Auth {
            kind: AuthKind::Bearer,
            token: value_of(block("bearer"), "token").unwrap_or_default(),
            ..Auth::default()
        }),
        "basic" => AuthSpec::Plain(Auth {
            kind: AuthKind::Basic,
            username: value_of(block("basic"), "username").unwrap_or_default(),
            password: value_of(block("basic"), "password").unwrap_or_default(),
            ..Auth::default()
        }),
        "apikey" => AuthSpec::ApiKey {
            key: value_of(block("apikey"), "key").unwrap_or_default(),
            value: value_of(block("apikey"), "value").unwrap_or_default(),
            in_query: value_of(block("apikey"), "placement").as_deref() == Some("queryparams"),
        },
        _ => AuthSpec::None,
    }
}

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

/// A request file as (name, request), or None when the file is not a
/// request (no method block). `fallback_name` is used when `meta` has none.
pub fn request(
    text: &str,
    fallback_name: &str,
    inherited: &Inherited,
) -> Option<(String, RequestFile)> {
    let blocks = blocks(text);
    let (method, spec) = METHODS
        .iter()
        .find_map(|m| find(&blocks, m).map(|b| (m.to_ascii_uppercase(), b)))?;
    let name = value_of(find(&blocks, "meta"), "name").unwrap_or_else(|| fallback_name.to_string());

    let mut url = value_of(Some(spec), "url").unwrap_or_default();
    let mut vars = layered(
        &inherited.vars,
        find(&blocks, "vars:pre-request")
            .map(rows)
            .unwrap_or_default(),
    );
    // `:id` path parameters become `{{id}}`, valued through the request's vars.
    if let Some(path) = find(&blocks, "params:path") {
        let path_rows = rows(path);
        for kv in &path_rows {
            url = url.replace(&format!(":{}", kv.key), &format!("{{{{{}}}}}", kv.key));
        }
        vars = layered(&vars, path_rows);
    }
    let mut params = find(&blocks, "params:query")
        .or_else(|| find(&blocks, "query"))
        .map(rows)
        .unwrap_or_else(|| query::params_from_url(&url, &[]));
    let mut headers = layered(
        &inherited.headers,
        find(&blocks, "headers").map(rows).unwrap_or_default(),
    );

    let mode = value_of(Some(spec), "auth").unwrap_or_else(|| "none".into());
    let auth = match auth_spec(&blocks, &mode) {
        AuthSpec::Inherit => inherited.auth.clone(),
        own => own,
    };
    let auth = match auth {
        AuthSpec::None | AuthSpec::Inherit => Auth::default(),
        AuthSpec::Plain(a) => a,
        AuthSpec::ApiKey {
            key,
            value,
            in_query,
        } => {
            if in_query {
                params.push(KeyValue::new(key, value));
                url = query::url_from_params(&url, &params);
            } else {
                headers.push(KeyValue::new(key, value));
            }
            Auth::default()
        }
    };

    let body = match value_of(Some(spec), "body").as_deref() {
        Some("json") => text_body(BodyKind::Json, text_of(find(&blocks, "body:json"))),
        Some("text") => text_body(BodyKind::Text, text_of(find(&blocks, "body:text"))),
        Some("xml") => text_body(BodyKind::Text, text_of(find(&blocks, "body:xml"))),
        Some("formUrlEncoded") => Body {
            kind: BodyKind::Form,
            text: String::new(),
            fields: find(&blocks, "body:form-urlencoded")
                .map(rows)
                .unwrap_or_default(),
        },
        Some("graphql") => graphql_body(&blocks),
        // Multipart and file bodies have no beeline equivalent.
        _ => Body::default(),
    };

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

fn text_body(kind: BodyKind, text: String) -> Body {
    Body {
        kind,
        text,
        fields: vec![],
    }
}

/// GraphQL over HTTP is a JSON body: `{"query": …, "variables": …}`.
fn graphql_body(blocks: &[Block]) -> Body {
    let query = text_of(find(blocks, "body:graphql"));
    let variables: Value = serde_json::from_str(&text_of(find(blocks, "body:graphql:vars")))
        .unwrap_or(Value::Object(Default::default()));
    let json = serde_json::json!({ "query": query, "variables": variables });
    text_body(
        BodyKind::Json,
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_blocks_and_removes_the_indent() {
        let b = blocks("meta {\n  name: A\n}\n\nbody:json {\n  {\n    \"a\": 1\n  }\n}\n");
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].name, "meta");
        assert_eq!(b[1].lines, vec!["{", "  \"a\": 1", "}"]);
    }

    #[test]
    fn rows_handle_disabled_continued_and_triple_quoted_values() {
        let block = Block {
            name: "params:query".into(),
            lines: vec![
                "$select: a,b".into(),
                "~$top: 1".into(),
                "XML: <a>".into(),
                "    <b/>".into(),
                "</a>".into(),
                "note: '''".into(),
                "  line one".into(),
                "  line two".into(),
                "'''".into(),
                "url: http://x:8080/y".into(),
            ],
        };
        let r = rows(&block);
        assert_eq!(r[0], KeyValue::new("$select", "a,b"));
        assert!(!r[1].enabled);
        assert_eq!(
            r[2].value, "<a>\n<b/>\n</a>",
            "a closing tag continues the value"
        );
        assert_eq!(r[3].value, "line one\nline two");
        assert_eq!(r[4].value, "http://x:8080/y", "only the first colon splits");
    }

    #[test]
    fn a_file_without_a_method_block_is_not_a_request() {
        assert!(request("meta {\n  name: Docs\n}\n", "x", &Inherited::default()).is_none());
    }

    #[test]
    fn inherited_auth_headers_and_vars_layer_down() {
        let collection = inherit(
            &Inherited::default(),
            "headers {\n  Accept: application/json\n}\nauth {\n  mode: bearer\n}\nauth:bearer {\n  token: {{token}}\n}\nvars:pre-request {\n  base: https://a.test\n}\n",
        );
        let folder = inherit(
            &collection,
            "auth {\n  mode: inherit\n}\nvars:pre-request {\n  base: https://b.test\n}\n",
        );
        assert_eq!(folder.auth, collection.auth);
        assert_eq!(folder.vars, vec![KeyValue::new("base", "https://b.test")]);
        assert_eq!(folder.headers.len(), 1);

        let (_, req) = request(
            "get {\n  url: {{base}}/x\n  auth: inherit\n}\n",
            "x",
            &folder,
        )
        .unwrap();
        assert_eq!(req.auth.kind, AuthKind::Bearer);
        assert_eq!(req.vars, vec![KeyValue::new("base", "https://b.test")]);
        assert_eq!(
            req.headers,
            vec![KeyValue::new("Accept", "application/json")]
        );
    }

    #[test]
    fn api_key_auth_becomes_a_header_or_query_row() {
        let header = "get {\n  url: https://x/a\n  auth: apikey\n}\nauth:apikey {\n  key: X-Api-Key\n  value: k1\n  placement: header\n}\n";
        let (_, req) = request(header, "x", &Inherited::default()).unwrap();
        assert_eq!(req.headers, vec![KeyValue::new("X-Api-Key", "k1")]);
        assert_eq!(req.auth.kind, AuthKind::None);

        let query = header.replace("placement: header", "placement: queryparams");
        let (_, req) = request(&query, "x", &Inherited::default()).unwrap();
        assert_eq!(req.url, "https://x/a?X-Api-Key=k1");
    }

    #[test]
    fn graphql_becomes_a_json_body() {
        let text = "post {\n  url: https://x/graphql\n  body: graphql\n}\nbody:graphql {\n  { me { id } }\n}\nbody:graphql:vars {\n  {\"a\": 1}\n}\n";
        let (_, req) = request(text, "x", &Inherited::default()).unwrap();
        assert_eq!(req.body.kind, BodyKind::Json);
        let json: Value = serde_json::from_str(&req.body.text).unwrap();
        assert_eq!(json["query"], "{ me { id } }");
        assert_eq!(json["variables"]["a"], 1);
    }
}
