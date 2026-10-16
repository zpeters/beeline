//! Importing requests from other tools. Each format's parser is pure and
//! returns an [`Imported`]; [`write`] puts it on disk inside a collection.

pub mod curl;
pub mod openapi;
pub mod postman;

use std::fs;
use std::path::{Path, PathBuf};

use crate::collection;
use crate::model::RequestFile;

/// One request found by a parser: the folders it sits in (outermost first),
/// its name, and the request.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub folders: Vec<String>,
    pub name: String,
    pub req: RequestFile,
}

/// What a parser found: a name for the new folder, and its requests.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub name: String,
    pub items: Vec<Item>,
}

/// Writes `imported` into a new folder inside `root`. The folder takes the
/// imported name, or the name with " 2", " 3"… when that is taken. Returns
/// the folder and how many requests were written.
pub fn write(root: &Path, imported: &Imported) -> Result<(PathBuf, usize), String> {
    let folder = unique(root, &sanitize(&imported.name), "");
    for item in &imported.items {
        let mut dir = folder.clone();
        for part in &item.folders {
            dir.push(sanitize(part));
        }
        fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        let path = unique(&dir, &sanitize(&item.name), ".json");
        collection::write_request(&path, &item.req)?;
    }
    // An import with no requests still leaves its folder, so the user sees it.
    fs::create_dir_all(&folder)
        .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
    Ok((folder, imported.items.len()))
}

/// Reads an API file and parses it as whichever format it is.
pub fn parse_file(text: &str) -> Result<Imported, String> {
    let doc = openapi::read(text)?;
    if openapi::is_openapi(&doc) {
        return openapi::parse(&doc);
    }
    if postman::is_postman(&doc) {
        return postman::parse(&doc);
    }
    Err("Not an OpenAPI 3 spec or a Postman collection".into())
}

/// Writes one request at the top level of `root`. Returns its path.
pub fn write_one(root: &Path, name: &str, req: &RequestFile) -> Result<PathBuf, String> {
    let path = unique(root, &sanitize(name), ".json");
    collection::write_request(&path, req)?;
    Ok(path)
}

/// A file or folder name made safe: characters that file systems or the
/// tree treat specially become "-", and a leading "." (hidden) is dropped.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        "request".to_string()
    } else {
        cleaned.to_string()
    }
}

/// `dir/stem+ext`, or `dir/stem 2+ext`, `dir/stem 3+ext`… whichever is free.
fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}{ext}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} {n}{ext}")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Node, request_count, scan};

    fn item(folders: &[&str], name: &str) -> Item {
        Item {
            folders: folders.iter().map(|s| s.to_string()).collect(),
            name: name.into(),
            req: RequestFile::default(),
        }
    }

    #[test]
    fn sanitize_replaces_path_characters_and_hidden_dots() {
        assert_eq!(sanitize("GET /users/:id"), "GET -users--id");
        assert_eq!(sanitize(".env"), "env");
        assert_eq!(sanitize("  "), "request");
    }

    #[test]
    fn writes_nested_folders_and_numbers_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let imported = Imported {
            name: "Pet Store".into(),
            items: vec![
                item(&["pets"], "list"),
                item(&["pets"], "list"),
                item(&[], "health"),
            ],
        };
        let (folder, count) = write(dir.path(), &imported).unwrap();
        assert_eq!(folder, dir.path().join("Pet Store"));
        assert_eq!(count, 3);
        assert!(folder.join("pets/list.json").exists());
        assert!(folder.join("pets/list 2.json").exists());
        assert!(folder.join("health.json").exists());
        assert_eq!(request_count(&scan(dir.path()).unwrap()), 3);
    }

    #[test]
    fn parse_file_picks_the_format_from_the_content() {
        let openapi = parse_file("openapi: 3.0.0\ninfo: {title: A}\npaths: {}\n").unwrap();
        assert_eq!(openapi.name, "A");
        let postman = parse_file(r#"{"info": {"name": "B"}, "item": []}"#).unwrap();
        assert_eq!(postman.name, "B");
        assert!(parse_file(r#"{"hello": 1}"#).is_err());
    }

    #[test]
    fn a_taken_folder_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("API")).unwrap();
        let imported = Imported {
            name: "API".into(),
            items: vec![item(&[], "x")],
        };
        let (folder, _) = write(dir.path(), &imported).unwrap();
        assert_eq!(folder, dir.path().join("API 2"));
        let tree = scan(dir.path()).unwrap();
        assert!(matches!(&tree[0], Node::Folder { name, .. } if name == "API"));
    }
}

/// The sample files in `fixtures/import/`, run through each parser. Unit
/// tests above use tiny inline cases; these check whole, realistic files.
#[cfg(test)]
mod fixtures {
    use super::*;
    use crate::collection::{request_count, scan};
    use crate::model::{AuthKind, BodyKind, KeyValue};

    fn curl(name: &str) -> RequestFile {
        let text = match name {
            "chrome" => include_str!("../../fixtures/import/curl/chrome-copy.sh"),
            "github" => include_str!("../../fixtures/import/curl/github-docs.sh"),
            "stripe" => include_str!("../../fixtures/import/curl/stripe-docs.sh"),
            _ => unreachable!(),
        };
        curl::parse(text).unwrap().1
    }

    #[test]
    fn curl_from_chrome_copy() {
        let req = curl("chrome");
        assert_eq!(req.method, "POST");
        assert_eq!(
            req.url,
            "https://api.example.test/graph/v1/orders?expand=items&limit=20"
        );
        assert_eq!(req.params.len(), 2);
        assert_eq!(req.auth.kind, AuthKind::Bearer);
        assert_eq!(req.auth.token, "eyJhbGciOiJIUzI1NiJ9.demo.sig");
        assert_eq!(req.headers.len(), 5, "authorization moved to the Auth tab");
        assert_eq!(req.body.kind, BodyKind::Json);
        assert!(req.body.text.contains("it's urgent"));
    }

    #[test]
    fn curl_from_github_docs() {
        let req = curl("github");
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "https://api.github.com/repos/OWNER/REPO/issues");
        assert_eq!(req.auth.token, "<YOUR-TOKEN>");
        assert_eq!(
            req.headers,
            vec![
                KeyValue::new("Accept", "application/vnd.github+json"),
                KeyValue::new("X-GitHub-Api-Version", "2022-11-28"),
            ]
        );
        assert_eq!(req.body.kind, BodyKind::Json);
        assert!(req.body.text.contains("I'm having a problem"));
    }

    #[test]
    fn curl_from_stripe_docs() {
        let req = curl("stripe");
        assert_eq!(req.method, "POST");
        assert_eq!(req.auth.kind, AuthKind::Basic);
        assert_eq!(req.auth.username, "sk_test_EXAMPLE");
        assert_eq!(req.auth.password, "");
        assert_eq!(
            req.body.fields,
            vec![
                KeyValue::new("name", "Jenny Rosen"),
                KeyValue::new("email", "jenny.rosen@example.com"),
            ]
        );
    }

    fn petstore() -> Imported {
        parse_file(include_str!(
            "../../fixtures/import/openapi/petstore-expanded.yaml"
        ))
        .unwrap()
    }

    #[test]
    fn openapi_petstore_expanded() {
        let imported = petstore();
        assert_eq!(imported.name, "Swagger Petstore");
        let names: Vec<&str> = imported.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["findPets", "addPet", "find pet by id", "deletePet"]
        );

        let find = &imported.items[0].req;
        assert_eq!(find.url, "https://petstore.swagger.io/v2/pets");
        assert_eq!(find.params.len(), 2);
        assert!(find.params.iter().all(|p| !p.enabled), "both are optional");

        let add = &imported.items[1].req;
        assert_eq!(add.method, "POST");
        assert_eq!(add.body.kind, BodyKind::Json);

        assert_eq!(
            imported.items[2].req.url,
            "https://petstore.swagger.io/v2/pets/{{id}}"
        );
        assert_eq!(imported.items[3].req.method, "DELETE");
    }

    fn shop() -> Imported {
        parse_file(include_str!(
            "../../fixtures/import/postman/shop.postman_collection.json"
        ))
        .unwrap()
    }

    #[test]
    fn postman_shop_export() {
        let imported = shop();
        assert_eq!(imported.name, "Shop API");
        let paths: Vec<String> = imported
            .items
            .iter()
            .map(|i| [i.folders.clone(), vec![i.name.clone()]].concat().join("/"))
            .collect();
        assert_eq!(
            paths,
            vec![
                "Orders/List orders",
                "Orders/Get order",
                "Orders/Create order",
                "Orders/Upload receipt",
                "Auth/Log in",
                "Health",
            ]
        );

        let list = &imported.items[0].req;
        assert_eq!(list.url, "{{baseUrl}}/orders?status=open&page=1");
        assert_eq!(list.params.len(), 3);
        assert!(!list.params[2].enabled);
        assert!(!list.headers[1].enabled);
        assert_eq!(list.auth.token, "{{orderToken}}", "folder auth");

        assert_eq!(imported.items[1].req.url, "{{baseUrl}}/orders/{{orderId}}");
        assert!(imported.items[2].req.body.text.contains("BEE-1"));
        assert_eq!(
            imported.items[3].req.body.kind,
            BodyKind::None,
            "form-data skipped"
        );

        let login = &imported.items[4].req;
        assert_eq!(login.auth.kind, AuthKind::None, "noauth override");
        assert_eq!(login.body.fields.len(), 3);
        assert!(!login.body.fields[2].enabled);

        let health = &imported.items[5].req;
        assert_eq!(health.auth.kind, AuthKind::Basic, "collection auth");
        assert_eq!(health.auth.username, "admin");
    }

    #[test]
    fn fixtures_write_and_scan_back() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &petstore()).unwrap();
        write(dir.path(), &shop()).unwrap();
        assert_eq!(request_count(&scan(dir.path()).unwrap()), 4 + 6);
    }
}
