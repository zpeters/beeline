//! A collection as a Postman v2.1 collection file. Pure. Folders stay
//! folders; `{{vars}}` keep their syntax, which Postman shares with beeline.
//! Per-request variables have no place in a Postman collection and are
//! left out.

use serde_json::{Value, json};

use super::Entry;
use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};

const SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// The collection file for `name` holding `entries`.
pub fn collection(name: &str, entries: &[Entry]) -> Value {
    json!({
        "info": {"name": name, "schema": SCHEMA},
        "item": items(entries),
    })
}

fn items(entries: &[Entry]) -> Vec<Value> {
    entries.iter().map(item).collect()
}

fn item(entry: &Entry) -> Value {
    match entry {
        Entry::Folder { name, items: inner } => json!({"name": name, "item": items(inner)}),
        Entry::Request { name, req } => json!({"name": name, "request": request(req)}),
    }
}

fn request(req: &RequestFile) -> Value {
    let mut out = json!({
        "method": req.method,
        "header": rows(&req.headers),
        // The URL holds only the enabled params; `query` has them all.
        "url": {"raw": req.url, "query": rows(&req.params)},
        "auth": auth(&req.auth),
    });
    if let Some(body) = body(&req.body) {
        out["body"] = body;
    }
    out
}

/// Postman key/value rows. A disabled row says so; enabled rows leave it out.
fn rows(list: &[KeyValue]) -> Value {
    list.iter()
        .map(|kv| {
            let mut row = json!({"key": kv.key, "value": kv.value});
            if !kv.enabled {
                row["disabled"] = json!(true);
            }
            row
        })
        .collect()
}

fn auth(auth: &Auth) -> Value {
    match auth.kind {
        AuthKind::None => json!({"type": "noauth"}),
        AuthKind::Bearer => json!({
            "type": "bearer",
            "bearer": [{"key": "token", "value": auth.token, "type": "string"}],
        }),
        AuthKind::Basic => json!({
            "type": "basic",
            "basic": [
                {"key": "username", "value": auth.username, "type": "string"},
                {"key": "password", "value": auth.password, "type": "string"},
            ],
        }),
    }
}

/// `None` for no body. Multipart has no beeline equivalent, so it never
/// comes out of an import; a raw body keeps its JSON or text type.
fn body(body: &Body) -> Option<Value> {
    match body.kind {
        BodyKind::None => None,
        BodyKind::Json => Some(json!({
            "mode": "raw",
            "raw": body.text,
            "options": {"raw": {"language": "json"}},
        })),
        BodyKind::Text => Some(json!({"mode": "raw", "raw": body.text})),
        BodyKind::Form => Some(json!({"mode": "urlencoded", "urlencoded": rows(&body.fields)})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import;
    use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};
    use serde_json::json;

    fn get(url: &str) -> RequestFile {
        RequestFile {
            url: url.into(),
            ..RequestFile::default()
        }
    }

    #[test]
    fn a_collection_has_postman_info_and_items() {
        let doc = collection("Shop", &[]);
        assert_eq!(doc["info"]["name"], "Shop");
        assert_eq!(
            doc["info"]["schema"],
            "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        );
        assert_eq!(doc["item"], json!([]));
    }

    #[test]
    fn folders_nest_and_requests_keep_their_names() {
        let entries = vec![Entry::Folder {
            name: "Orders".into(),
            items: vec![Entry::Request {
                name: "List".into(),
                req: get("https://x.test/orders"),
            }],
        }];
        let doc = collection("Shop", &entries);
        assert_eq!(doc["item"][0]["name"], "Orders");
        assert_eq!(doc["item"][0]["item"][0]["name"], "List");
        assert_eq!(
            doc["item"][0]["item"][0]["request"]["url"]["raw"],
            "https://x.test/orders"
        );
    }

    #[test]
    fn method_headers_and_disabled_rows() {
        let req = RequestFile {
            method: "POST".into(),
            headers: vec![
                KeyValue::new("Accept", "application/json"),
                KeyValue {
                    enabled: false,
                    ..KeyValue::new("X-Trace", "off")
                },
            ],
            ..get("https://x.test")
        };
        let doc = collection(
            "S",
            &[Entry::Request {
                name: "r".into(),
                req,
            }],
        );
        let request = &doc["item"][0]["request"];
        assert_eq!(request["method"], "POST");
        assert_eq!(
            request["header"],
            json!([
                {"key": "Accept", "value": "application/json"},
                {"key": "X-Trace", "value": "off", "disabled": true}
            ])
        );
    }

    #[test]
    fn auth_becomes_bearer_basic_or_noauth() {
        let with = |auth: Auth| {
            let doc = collection(
                "S",
                &[Entry::Request {
                    name: "r".into(),
                    req: RequestFile {
                        auth,
                        ..get("https://x.test")
                    },
                }],
            );
            doc["item"][0]["request"]["auth"].clone()
        };
        assert_eq!(with(Auth::default())["type"], "noauth");
        assert_eq!(
            with(Auth {
                kind: AuthKind::Bearer,
                token: "t".into(),
                ..Auth::default()
            }),
            json!({"type": "bearer", "bearer": [{"key": "token", "value": "t", "type": "string"}]})
        );
        assert_eq!(
            with(Auth {
                kind: AuthKind::Basic,
                username: "u".into(),
                password: "p".into(),
                ..Auth::default()
            }),
            json!({"type": "basic", "basic": [
                {"key": "username", "value": "u", "type": "string"},
                {"key": "password", "value": "p", "type": "string"}
            ]})
        );
    }

    #[test]
    fn bodies_raw_json_text_form_and_none() {
        let body = |body: Body| {
            let doc = collection(
                "S",
                &[Entry::Request {
                    name: "r".into(),
                    req: RequestFile {
                        body,
                        ..get("https://x.test")
                    },
                }],
            );
            doc["item"][0]["request"].get("body").cloned()
        };
        assert_eq!(body(Body::default()), None);
        assert_eq!(
            body(Body {
                kind: BodyKind::Json,
                text: "{}".into(),
                fields: vec![]
            }),
            Some(json!({"mode": "raw", "raw": "{}", "options": {"raw": {"language": "json"}}}))
        );
        assert_eq!(
            body(Body {
                kind: BodyKind::Text,
                text: "hi".into(),
                fields: vec![]
            }),
            Some(json!({"mode": "raw", "raw": "hi"}))
        );
        assert_eq!(
            body(Body {
                kind: BodyKind::Form,
                text: String::new(),
                fields: vec![KeyValue::new("a", "1")]
            }),
            Some(json!({"mode": "urlencoded", "urlencoded": [{"key": "a", "value": "1"}]}))
        );
    }

    #[test]
    fn the_shop_fixture_survives_an_import_and_export() {
        // Import the Postman fixture, export what came out, import again.
        // The two imports must agree, so nothing beeline reads is lost.
        let text = include_str!("../../fixtures/import/postman/shop.postman_collection.json");
        let first = import::parse_file(text).unwrap();
        let entries = to_entries(&first);
        let exported = collection(&first.name, &entries);
        let second = import::parse_file(&exported.to_string()).unwrap();
        assert_eq!(second, first);
    }

    /// The imported flat list as a tree, for the round trip.
    fn to_entries(imported: &import::Imported) -> Vec<Entry> {
        fn insert(entries: &mut Vec<Entry>, folders: &[String], name: &str, req: &RequestFile) {
            match folders.split_first() {
                None => entries.push(Entry::Request {
                    name: name.into(),
                    req: req.clone(),
                }),
                Some((first, rest)) => {
                    let index = entries
                        .iter()
                        .position(|e| matches!(e, Entry::Folder { name, .. } if name == first));
                    let index = index.unwrap_or_else(|| {
                        entries.push(Entry::Folder {
                            name: first.clone(),
                            items: vec![],
                        });
                        entries.len() - 1
                    });
                    if let Entry::Folder { items, .. } = &mut entries[index] {
                        insert(items, rest, name, req);
                    }
                }
            }
        }
        let mut entries = Vec::new();
        for item in &imported.items {
            insert(&mut entries, &item.folders, &item.name, &item.req);
        }
        entries
    }
}
