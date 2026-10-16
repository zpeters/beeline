//! A collection is a folder of request files. All filesystem access lives
//! here; errors come back as user-facing strings.

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Environment, RequestFile, to_json};

/// Reserved folder at the collection root; not shown in the request tree.
pub const ENV_DIR: &str = "environments";
const EXT: &str = "json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Folder {
        name: String,
        path: PathBuf,
        children: Vec<Node>,
    },
    Request {
        name: String,
        path: PathBuf,
    },
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn err(action: &str, path: &Path, e: impl std::fmt::Display) -> String {
    format!("Could not {action} {}: {e}", path.display())
}

/// The request tree under `root`: folders first, then requests, each
/// sorted case-insensitively. Hidden entries and the root's
/// `environments/` folder are skipped.
pub fn scan(root: &Path) -> Result<Vec<Node>, String> {
    scan_dir(root, true)
}

fn scan_dir(dir: &Path, is_root: bool) -> Result<Vec<Node>, String> {
    let entries = fs::read_dir(dir).map_err(|e| err("read", dir, e))?;
    let mut folders = Vec::new();
    let mut requests = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| err("read", dir, e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_hidden(&name) {
            continue;
        }
        if path.is_dir() {
            if is_root && name == ENV_DIR {
                continue;
            }
            let children = scan_dir(&path, false)?;
            folders.push(Node::Folder {
                name,
                path,
                children,
            });
        } else if path.extension().is_some_and(|e| e == EXT) {
            let name = stem(&path);
            requests.push(Node::Request { name, path });
        }
    }
    let key = |n: &Node| match n {
        Node::Folder { name, .. } | Node::Request { name, .. } => name.to_lowercase(),
    };
    folders.sort_by_key(key);
    requests.sort_by_key(key);
    folders.extend(requests);
    Ok(folders)
}

/// A request's display name: its file stem.
pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Check a user-typed request or folder name and return it trimmed.
pub fn validate_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name is empty".into());
    }
    if name.contains(['/', '\\', ':']) {
        return Err("Name cannot contain / \\ or :".into());
    }
    if is_hidden(name) {
        return Err("Name cannot start with a dot".into());
    }
    Ok(name)
}

pub fn read_request(path: &Path) -> Result<RequestFile, String> {
    let text = fs::read_to_string(path).map_err(|e| err("read", path, e))?;
    serde_json::from_str(&text).map_err(|e| err("parse", path, e))
}

/// Write via a temp file and rename, so a crash never leaves half a file.
fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, contents).map_err(|e| err("write", path, e))?;
    fs::rename(&tmp, path).map_err(|e| err("write", path, e))
}

pub fn write_request(path: &Path, req: &RequestFile) -> Result<(), String> {
    write_atomic(path, &to_json(req))
}

/// Create `<dir>/<name>.json` holding `req`. Fails if it exists.
pub fn create_request(dir: &Path, name: &str, req: &RequestFile) -> Result<PathBuf, String> {
    let path = dir.join(format!("{}.{EXT}", validate_name(name)?));
    if path.exists() {
        return Err(format!("\"{}\" already exists", stem(&path)));
    }
    write_request(&path, req)?;
    Ok(path)
}

pub fn create_folder(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(validate_name(name)?);
    if path.exists() {
        return Err(format!("\"{}\" already exists", name.trim()));
    }
    fs::create_dir(&path).map_err(|e| err("create", &path, e))?;
    Ok(path)
}

/// Rename a request file (keeping `.json`) or a folder. Returns the new path.
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let name = validate_name(new_name)?;
    let file_name = if path.is_dir() {
        name.to_string()
    } else {
        format!("{name}.{EXT}")
    };
    let target = path.with_file_name(file_name);
    if target == path {
        return Ok(target);
    }
    // A case-only rename on a case-insensitive disk "exists" already.
    let case_only =
        target.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase();
    if target.exists() && !case_only {
        return Err(format!("\"{name}\" already exists"));
    }
    fs::rename(path, &target).map_err(|e| err("rename", path, e))?;
    Ok(target)
}

/// Environments as `(name, path)`, sorted by name. No `environments/`
/// folder means no environments.
pub fn environments(root: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let dir = root.join(ENV_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut envs: Vec<(String, PathBuf)> = fs::read_dir(&dir)
        .map_err(|e| err("read", &dir, e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == EXT))
        .filter(|p| !is_hidden(&stem(p)))
        .map(|p| (stem(&p), p))
        .collect();
    envs.sort_by_key(|(name, _)| name.to_lowercase());
    Ok(envs)
}

pub fn read_env(path: &Path) -> Result<Environment, String> {
    let text = fs::read_to_string(path).map_err(|e| err("read", path, e))?;
    serde_json::from_str(&text).map_err(|e| err("parse", path, e))
}

pub fn write_env(path: &Path, env: &Environment) -> Result<(), String> {
    write_atomic(path, &to_json(env))
}

/// Create an empty `environments/<name>.json`. Fails if it exists.
pub fn create_env(root: &Path, name: &str) -> Result<PathBuf, String> {
    let dir = root.join(ENV_DIR);
    fs::create_dir_all(&dir).map_err(|e| err("create", &dir, e))?;
    let path = dir.join(format!("{}.{EXT}", validate_name(name)?));
    if path.exists() {
        return Err(format!("Environment \"{}\" already exists", stem(&path)));
    }
    write_env(&path, &Environment::default())?;
    Ok(path)
}

/// Move a request file or folder to the system trash (recoverable).
pub fn delete(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| err("move to trash", path, e))
}

/// Where `path` lives after `from` was renamed to `to`, if it was inside
/// (or is) `from`.
pub fn moved(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rest| {
        if rest.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(rest)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn names(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                Node::Folder { name, .. } => format!("{name}/"),
                Node::Request { name, .. } => name.clone(),
            })
            .collect()
    }

    #[test]
    fn scans_folders_first_and_skips_reserved_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("b.json"), "{}");
        touch(&root.join("A.json"), "{}");
        touch(&root.join("notes.txt"), "");
        touch(&root.join(".hidden.json"), "{}");
        touch(&root.join("environments/dev.json"), "{}");
        touch(&root.join("users/list.json"), "{}");
        touch(&root.join(".git/config"), "");

        let tree = scan(root).unwrap();
        assert_eq!(names(&tree), vec!["users/", "A", "b"]);
        let Node::Folder { children, .. } = &tree[0] else {
            panic!("expected folder")
        };
        assert_eq!(names(children), vec!["list"]);
    }

    #[test]
    fn nested_environments_folder_is_a_normal_folder() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("x/environments/a.json"), "{}");
        let tree = scan(dir.path()).unwrap();
        let Node::Folder { children, .. } = &tree[0] else {
            panic!("expected folder")
        };
        assert_eq!(names(children), vec!["environments/"]);
    }

    #[test]
    fn create_write_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let req = RequestFile {
            url: "https://x".into(),
            ..RequestFile::default()
        };
        let path = create_request(dir.path(), " get thing ", &req).unwrap();
        assert_eq!(path, dir.path().join("get thing.json"));
        assert_eq!(read_request(&path).unwrap(), req);
        assert!(!dir.path().join("get thing.json.tmp").exists());

        let err = create_request(dir.path(), "get thing", &req).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[test]
    fn read_reports_bad_json_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        touch(&path, "{ nope");
        let err = read_request(&path).unwrap_err();
        assert!(err.contains("bad.json"), "{err}");
    }

    #[test]
    fn validates_names() {
        assert_eq!(validate_name("  ok  "), Ok("ok"));
        assert!(validate_name("  ").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name(".x").is_err());
    }

    #[test]
    fn renames_requests_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        let req = dir.path().join("old.json");
        touch(&req, "{}");
        let new = rename(&req, "new").unwrap();
        assert_eq!(new, dir.path().join("new.json"));
        assert!(new.exists() && !req.exists());

        let folder = create_folder(dir.path(), "f").unwrap();
        let moved_folder = rename(&folder, "g").unwrap();
        assert!(moved_folder.is_dir());

        touch(&dir.path().join("taken.json"), "{}");
        assert!(rename(&new, "taken").is_err());
    }

    #[test]
    fn case_only_rename_works() {
        let dir = tempfile::tempdir().unwrap();
        let req = dir.path().join("users.json");
        touch(&req, "{}");
        let new = rename(&req, "Users").unwrap();
        assert_eq!(scan(dir.path()).unwrap().len(), 1);
        assert_eq!(stem(&new), "Users");
    }

    #[test]
    fn lists_creates_and_round_trips_environments() {
        let dir = tempfile::tempdir().unwrap();
        assert!(environments(dir.path()).unwrap().is_empty());

        let prod = create_env(dir.path(), "prod").unwrap();
        create_env(dir.path(), "Dev").unwrap();
        touch(&dir.path().join("environments/notes.txt"), "");
        let names: Vec<String> = environments(dir.path())
            .unwrap()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, vec!["Dev", "prod"]);
        assert!(create_env(dir.path(), "prod").is_err());

        let mut env = read_env(&prod).unwrap();
        assert!(env.variables.is_empty());
        env.variables.insert("base".into(), "https://x".into());
        write_env(&prod, &env).unwrap();
        assert_eq!(read_env(&prod).unwrap(), env);
    }

    /// Every request in `examples/demo` parses and resolves with its
    /// environment, so the sample collection cannot rot.
    #[test]
    fn example_collection_resolves() {
        fn walk(nodes: &[Node], out: &mut Vec<PathBuf>) {
            for n in nodes {
                match n {
                    Node::Folder { children, .. } => walk(children, out),
                    Node::Request { path, .. } => out.push(path.clone()),
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
        let envs = environments(&root).unwrap();
        assert_eq!(envs.len(), 1);
        let vars = read_env(&envs[0].1).unwrap().variables;

        let mut paths = Vec::new();
        walk(&scan(&root).unwrap(), &mut paths);
        assert_eq!(paths.len(), 6);
        for path in paths {
            let req = read_request(&path).unwrap();
            let r = crate::resolve::resolve(&req, &vars)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(r.url.starts_with("https://"), "{}", r.url);
        }
    }

    #[test]
    fn maps_paths_after_a_move() {
        let from = Path::new("/c/users");
        let to = Path::new("/c/people");
        assert_eq!(
            moved(Path::new("/c/users/list.json"), from, to),
            Some(PathBuf::from("/c/people/list.json"))
        );
        assert_eq!(moved(from, from, to), Some(to.to_path_buf()));
        assert_eq!(moved(Path::new("/c/other.json"), from, to), None);
    }
}
