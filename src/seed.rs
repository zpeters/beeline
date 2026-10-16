//! The example collection a first-time user starts with. The files are
//! compiled into the binary, so the app works from any location.

use std::fs;
use std::path::Path;

const FILES: &[(&str, &str)] = &[
    (
        "echo/basic auth.json",
        include_str!("../examples/demo/echo/basic auth.json"),
    ),
    (
        "echo/bearer auth.json",
        include_str!("../examples/demo/echo/bearer auth.json"),
    ),
    (
        "echo/form post.json",
        include_str!("../examples/demo/echo/form post.json"),
    ),
    (
        "posts/create post.json",
        include_str!("../examples/demo/posts/create post.json"),
    ),
    (
        "posts/get post.json",
        include_str!("../examples/demo/posts/get post.json"),
    ),
    (
        "posts/list posts.json",
        include_str!("../examples/demo/posts/list posts.json"),
    ),
    (
        "environments/public.json",
        include_str!("../examples/demo/environments/public.json"),
    ),
];

/// Writes the example collection into `dir`. Files that already exist are
/// left alone, so a user's edits are never overwritten.
pub fn write_examples(dir: &Path) -> Result<(), String> {
    for (rel, text) in FILES {
        let path = dir.join(rel);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
        }
        fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Node, environments, scan};

    #[test]
    fn writes_a_collection_that_scans_and_has_an_environment() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Default");
        write_examples(&root).unwrap();
        let tree = scan(&root).unwrap();
        let names: Vec<&str> = tree
            .iter()
            .filter_map(|n| match n {
                Node::Folder { name, .. } => Some(name.as_str()),
                Node::Request { .. } => None,
            })
            .collect();
        assert_eq!(names, ["echo", "posts"]);
        assert_eq!(environments(&root).unwrap().len(), 1);
    }

    #[test]
    fn existing_files_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments/public.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "mine").unwrap();
        write_examples(dir.path()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "mine");
    }
}
