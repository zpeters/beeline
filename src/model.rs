//! The on-disk JSON format. Every field has a default, so a hand-written
//! file can be as short as `{"url": "https://example.com"}`.

use serde::{Deserialize, Serialize};

pub const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestFile {
    pub method: String,
    /// Holds the query string. `params` mirrors it (see `query.rs`).
    pub url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    pub body: Body,
    pub auth: Auth,
}

impl Default for RequestFile {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            url: String::new(),
            params: Vec::new(),
            headers: Vec::new(),
            body: Body::default(),
            auth: Auth::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl Default for KeyValue {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyKind {
    #[default]
    None,
    Json,
    Text,
    Form,
}

impl BodyKind {
    pub const ALL: [BodyKind; 4] = [Self::None, Self::Json, Self::Text, Self::Form];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Json => "JSON",
            Self::Text => "Text",
            Self::Form => "Form URL-encoded",
        }
    }
}

/// `text` serves JSON and Text; `fields` serves Form. Both are kept when
/// the kind changes, so switching back and forth loses nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Body {
    #[serde(rename = "type")]
    pub kind: BodyKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<KeyValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    #[default]
    None,
    Bearer,
    Basic,
}

impl AuthKind {
    pub const ALL: [AuthKind; 3] = [Self::None, Self::Bearer, Self::Basic];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Bearer => "Bearer token",
            Self::Basic => "Basic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Auth {
    #[serde(rename = "type")]
    pub kind: AuthKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub password: String,
}

/// Pretty JSON (2-space indent) with a trailing newline, for small diffs.
#[cfg(test)]
pub fn to_json<T: Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("model types always serialize");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_file_gets_defaults() {
        let req: RequestFile = serde_json::from_str(r#"{"url": "https://x"}"#).unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://x");
        assert_eq!(req.body.kind, BodyKind::None);
        assert_eq!(req.auth.kind, AuthKind::None);
    }

    #[test]
    fn key_value_enabled_defaults_true() {
        let kv: KeyValue = serde_json::from_str(r#"{"key": "a", "value": "b"}"#).unwrap();
        assert!(kv.enabled);
    }

    #[test]
    fn round_trips() {
        let req = RequestFile {
            method: "POST".into(),
            url: "{{base}}/users?x=1".into(),
            params: vec![KeyValue::new("x", "1")],
            headers: vec![KeyValue {
                enabled: false,
                ..KeyValue::new("Accept", "*/*")
            }],
            body: Body {
                kind: BodyKind::Json,
                text: "{}".into(),
                fields: vec![],
            },
            auth: Auth {
                kind: AuthKind::Bearer,
                token: "{{token}}".into(),
                ..Auth::default()
            },
        };
        let back: RequestFile = serde_json::from_str(&to_json(&req)).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn empty_fields_are_omitted() {
        let json = to_json(&RequestFile::default());
        assert!(!json.contains("params"));
        assert!(!json.contains("token"));
        assert!(json.contains(r#""type": "none""#));
        assert!(json.ends_with("}\n"));
    }
}
