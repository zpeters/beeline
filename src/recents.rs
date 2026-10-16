//! The collections used most recently, newest first. Pure: the app saves
//! the list and checks which folders still exist.

use std::path::{Path, PathBuf};

use crate::collection::moved;

/// How many recent collections are kept.
pub const LIMIT: usize = 10;

/// Puts `path` at the front, removing any earlier copy, and keeps at most
/// `LIMIT` entries.
pub fn touch(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(LIMIT);
}

/// Drops entries that `exists` rejects, such as folders that were moved or
/// deleted outside the app.
pub fn prune(list: &mut Vec<PathBuf>, exists: impl Fn(&Path) -> bool) {
    list.retain(|p| exists(p));
}

/// Follows a rename or move of `from` to `to`, for `from` itself and any
/// entry inside it.
pub fn retarget(list: &mut [PathBuf], from: &Path, to: &Path) {
    for entry in list.iter_mut() {
        if let Some(new) = moved(entry, from, to) {
            *entry = new;
        }
    }
}

/// Removes `path` from the list.
pub fn forget(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|p| p != path);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn touch_puts_the_path_first_without_duplicates() {
        let mut list = vec![p("/a"), p("/b")];
        touch(&mut list, Path::new("/b"));
        assert_eq!(list, vec![p("/b"), p("/a")]);
    }

    #[test]
    fn touch_keeps_at_most_the_limit() {
        let mut list = Vec::new();
        for i in 0..(LIMIT + 3) {
            touch(&mut list, Path::new(&format!("/c{i}")));
        }
        assert_eq!(list.len(), LIMIT);
        assert_eq!(list[0], p(&format!("/c{}", LIMIT + 2)));
    }

    #[test]
    fn prune_drops_missing_paths() {
        let mut list = vec![p("/here"), p("/gone")];
        prune(&mut list, |path| path == Path::new("/here"));
        assert_eq!(list, vec![p("/here")]);
    }

    #[test]
    fn retarget_follows_a_rename() {
        let mut list = vec![p("/work/api"), p("/work/other"), p("/work/api-old/x")];
        retarget(&mut list, Path::new("/work/api"), Path::new("/work/api2"));
        assert_eq!(list[0], p("/work/api2"));
        assert_eq!(list[1], p("/work/other"));
    }

    #[test]
    fn forget_removes_the_path() {
        let mut list = vec![p("/a"), p("/b")];
        forget(&mut list, Path::new("/a"));
        assert_eq!(list, vec![p("/b")]);
    }
}
