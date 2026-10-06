//! Helpers for swap tests (temp dirs only).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use filetime::FileTime;
use walkdir::WalkDir;

/// Writes `bytes` to `path` (creating parents) and sets its mtime.
pub fn write_file(path: &Path, bytes: &[u8], mtime_secs: i64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    if path.exists() {
        let mut perms = fs::metadata(path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(path, perms).unwrap();
    }
    fs::write(path, bytes).unwrap();
    filetime::set_file_mtime(path, FileTime::from_unix_time(mtime_secs, 0)).unwrap();
}

pub fn set_readonly(path: &Path) {
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms).unwrap();
}

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    /// File contents; `None` for folders.
    pub bytes: Option<Vec<u8>>,
    /// Files only: NTFS updates folder times lazily after their contents
    /// change, so folder times are not stable enough to compare.
    pub mtime: Option<FileTime>,
    pub readonly: bool,
}

/// Everything under `dir` (relative path → entry). Missing dir = empty.
pub fn snapshot(dir: &Path) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return out;
    }
    for entry in WalkDir::new(dir).min_depth(1) {
        let entry = entry.unwrap();
        let meta = entry.metadata().unwrap();
        let rel = entry.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned();
        out.insert(
            rel,
            Entry {
                bytes: meta.is_file().then(|| fs::read(entry.path()).unwrap()),
                mtime: meta.is_file().then(|| FileTime::from_last_modification_time(&meta)),
                readonly: meta.is_file() && meta.permissions().readonly(),
            },
        );
    }
    out
}

pub fn assert_same_tree(a: &Path, b: &Path) {
    assert_eq!(snapshot(a), snapshot(b), "{} != {}", a.display(), b.display());
}
