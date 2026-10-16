//! Sample requests against httpbin.org, added to a folder from its menu.
//! Each request shows one feature: a method, a body, auth, query params,
//! a status code, or a slow response to test the timeout.

use std::fs;
use std::path::{Path, PathBuf};

use crate::collection;
use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, RequestFile};

/// The folder the samples are written into.
pub const FOLDER: &str = "httpbin";

fn get(url: &str) -> RequestFile {
    RequestFile {
        url: url.into(),
        ..RequestFile::default()
    }
}

/// The sample files as (file name, request). Names become the request names.
pub fn requests() -> Vec<(&'static str, RequestFile)> {
    let base = "https://httpbin.org";
    vec![
        ("get.json", get(&format!("{base}/get"))),
        (
            "headers.json",
            RequestFile {
                headers: vec![KeyValue::new("X-Demo", "beeline")],
                ..get(&format!("{base}/headers"))
            },
        ),
        (
            "query params.json",
            RequestFile {
                url: format!("{base}/anything?page=1&size=20"),
                params: vec![KeyValue::new("page", "1"), KeyValue::new("size", "20")],
                ..RequestFile::default()
            },
        ),
        (
            "post json.json",
            RequestFile {
                method: "POST".into(),
                body: Body {
                    kind: BodyKind::Json,
                    text: "{\n  \"name\": \"beeline\",\n  \"tags\": [\"api\", \"test\"]\n}\n"
                        .into(),
                    fields: vec![],
                },
                ..get(&format!("{base}/post"))
            },
        ),
        (
            "post form.json",
            RequestFile {
                method: "POST".into(),
                body: Body {
                    kind: BodyKind::Form,
                    text: String::new(),
                    fields: vec![
                        KeyValue::new("name", "beeline"),
                        KeyValue::new("kind", "client"),
                    ],
                },
                ..get(&format!("{base}/post"))
            },
        ),
        (
            "put.json",
            RequestFile {
                method: "PUT".into(),
                body: Body {
                    kind: BodyKind::Text,
                    text: "replaced".into(),
                    fields: vec![],
                },
                ..get(&format!("{base}/put"))
            },
        ),
        (
            "patch.json",
            RequestFile {
                method: "PATCH".into(),
                body: Body {
                    kind: BodyKind::Json,
                    text: "{\"enabled\": false}".into(),
                    fields: vec![],
                },
                ..get(&format!("{base}/patch"))
            },
        ),
        (
            "delete.json",
            RequestFile {
                method: "DELETE".into(),
                ..get(&format!("{base}/delete"))
            },
        ),
        (
            "bearer auth.json",
            RequestFile {
                auth: Auth {
                    kind: AuthKind::Bearer,
                    token: "beeline-demo-token".into(),
                    ..Auth::default()
                },
                ..get(&format!("{base}/bearer"))
            },
        ),
        (
            "basic auth.json",
            RequestFile {
                auth: Auth {
                    kind: AuthKind::Basic,
                    username: "user".into(),
                    password: "passwd".into(),
                    ..Auth::default()
                },
                ..get(&format!("{base}/basic-auth/user/passwd"))
            },
        ),
        ("status 404.json", get(&format!("{base}/status/404"))),
        ("slow response.json", get(&format!("{base}/delay/2"))),
    ]
}

/// Writes the samples into a new `httpbin` folder inside `dir`. Files that
/// already exist are left alone. Returns the folder's path.
pub fn write(dir: &Path) -> Result<PathBuf, String> {
    let folder = dir.join(FOLDER);
    fs::create_dir_all(&folder)
        .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
    for (name, req) in requests() {
        let path = folder.join(name);
        if !path.exists() {
            collection::write_request(&path, &req)?;
        }
    }
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{request_count, scan};
    use crate::resolve::resolve;
    use std::collections::BTreeMap;

    #[test]
    fn every_sample_resolves_without_variables() {
        for (name, req) in requests() {
            assert!(
                resolve(&req, &BTreeMap::new()).is_ok(),
                "{name} does not resolve"
            );
        }
    }

    #[test]
    fn sample_names_are_unique() {
        let mut names: Vec<&str> = requests().iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before);
    }

    #[test]
    fn writes_a_folder_that_scans_and_keeps_edits() {
        let root = std::env::temp_dir().join(format!("beeline-samples-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let folder = write(&root).unwrap();
        assert_eq!(folder, root.join(FOLDER));
        let tree = scan(&root).unwrap();
        assert_eq!(request_count(&tree), requests().len());

        // A second write must not overwrite an edited sample.
        let edited = folder.join("get.json");
        fs::write(&edited, "{\"url\": \"https://mine.test\"}").unwrap();
        write(&root).unwrap();
        assert!(fs::read_to_string(&edited).unwrap().contains("mine.test"));

        let _ = fs::remove_dir_all(&root);
    }
}
