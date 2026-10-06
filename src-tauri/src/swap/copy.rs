//! Folder copy / clear primitives used by the swap engine.
//!
//! - Copies preserve file and folder modification/access times and the
//!   read-only attribute.
//! - Symlinks/junctions are refused (copying them could leak or lose data);
//!   the error happens during the backup step, before anything is modified.
//! - Long paths: Rust's std adds the `\\?\` prefix automatically on Windows
//!   when a path exceeds `MAX_PATH`, so no special handling is needed here.

use std::fs;
use std::path::Path;

use filetime::FileTime;
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};

use super::fail_point;

/// True if `dir` does not exist or contains no entries.
pub fn is_empty_or_missing(dir: &Path) -> AppResult<bool> {
    match fs::read_dir(dir) {
        Ok(mut entries) => Ok(entries.next().is_none()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(AppError::io(dir, e)),
    }
}

/// Copies the contents of `src` into `dst` (created if needed). A missing
/// `src` copies nothing. Existing files in `dst` with the same name are
/// overwritten; other files in `dst` are left alone.
pub fn copy_dir_contents(src: &Path, dst: &Path) -> AppResult<()> {
    fs::create_dir_all(dst).map_err(|e| AppError::io(dst, e))?;
    if !src.exists() {
        return Ok(());
    }
    // Folder times are applied after their contents are written, deepest first.
    let mut dirs = Vec::new();
    for entry in WalkDir::new(src).min_depth(1).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(|e| walk_error(src, e))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .expect("walkdir yields paths under its root");
        let target = dst.join(rel);
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(AppError::Other(format!(
                "{} is a symbolic link or junction, which is not supported in save folders",
                entry.path().display()
            )));
        } else if file_type.is_dir() {
            fs::create_dir_all(&target).map_err(|e| AppError::io(&target, e))?;
            dirs.push((entry.path().to_path_buf(), target));
        } else {
            fail_point("copy_file")?;
            copy_file(entry.path(), &target)?;
        }
    }
    for (src_dir, dst_dir) in dirs.iter().rev() {
        copy_times(src_dir, dst_dir)?;
    }
    Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> AppResult<()> {
    if let Ok(meta) = fs::symlink_metadata(dst) {
        if meta.permissions().readonly() {
            make_writable(dst)?;
        }
    }
    // Also copies attributes (including read-only) on Windows.
    fs::copy(src, dst).map_err(|e| AppError::io(src, e))?;
    copy_times(src, dst)
}

fn copy_times(src: &Path, dst: &Path) -> AppResult<()> {
    let meta = fs::metadata(src).map_err(|e| AppError::io(src, e))?;
    let atime = FileTime::from_last_access_time(&meta);
    let mtime = FileTime::from_last_modification_time(&meta);
    set_times(dst, atime, mtime).map_err(|e| AppError::io(dst, e))
}

/// Sets times without needing write access to the file contents, so it works
/// on read-only files and on directories.
#[cfg(windows)]
fn set_times(path: &Path, atime: FileTime, mtime: FileTime) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let file = fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    filetime::set_file_handle_times(&file, Some(atime), Some(mtime))
}

#[cfg(not(windows))]
fn set_times(path: &Path, atime: FileTime, mtime: FileTime) -> std::io::Result<()> {
    filetime::set_file_times(path, atime, mtime)
}

fn make_writable(path: &Path) -> AppResult<()> {
    let mut perms = fs::symlink_metadata(path)
        .map_err(|e| AppError::io(path, e))?
        .permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(path, perms).map_err(|e| AppError::io(path, e))
}

/// Removes everything inside `dir` but keeps `dir` itself (its permissions and
/// any links pointing to it stay intact). Creates `dir` if missing.
pub fn clear_dir(dir: &Path) -> AppResult<()> {
    fs::create_dir_all(dir).map_err(|e| AppError::io(dir, e))?;
    for entry in fs::read_dir(dir).map_err(|e| AppError::io(dir, e))? {
        let entry = entry.map_err(|e| AppError::io(dir, e))?;
        fail_point("clear_entry")?;
        remove_path(&entry.path())?;
    }
    Ok(())
}

/// Removes a file or a whole folder tree, clearing read-only attributes first.
/// A missing path is not an error.
pub fn remove_path(path: &Path) -> AppResult<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(AppError::io(path, e)),
    };
    if meta.is_dir() {
        for entry in WalkDir::new(path).follow_links(false) {
            let entry = entry.map_err(|e| walk_error(path, e))?;
            let readonly = entry
                .metadata()
                .map(|m| m.permissions().readonly())
                .unwrap_or(false);
            if readonly {
                make_writable(entry.path())?;
            }
        }
        fs::remove_dir_all(path).map_err(|e| AppError::io(path, e))
    } else if meta.file_type().is_symlink() {
        // Remove the link itself, never its target.
        fs::remove_file(path)
            .or_else(|_| fs::remove_dir(path))
            .map_err(|e| AppError::io(path, e))
    } else {
        if meta.permissions().readonly() {
            make_writable(path)?;
        }
        fs::remove_file(path).map_err(|e| AppError::io(path, e))
    }
}

/// Replaces the contents of `dst` with the contents of `src`.
pub fn replace_dir_contents(src: &Path, dst: &Path) -> AppResult<()> {
    clear_dir(dst)?;
    copy_dir_contents(src, dst)
}

fn walk_error(root: &Path, e: walkdir::Error) -> AppError {
    let path = e.path().unwrap_or(root).to_path_buf();
    match e.into_io_error() {
        Some(io) => AppError::io(path, io),
        None => AppError::Other(format!("filesystem loop under {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swap::test_util::{assert_same_tree, write_file};
    use tempfile::tempdir;

    #[test]
    fn copy_preserves_bytes_times_and_readonly() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("src");
        write_file(&src.join("a.sav"), b"alpha", 1_000_000);
        write_file(&src.join("sub/deeper/b.sav"), b"beta", 2_000_000);
        let ro = src.join("sub/ro.cfg");
        write_file(&ro, b"read only", 3_000_000);
        let mut perms = fs::metadata(&ro).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&ro, perms).unwrap();
        filetime::set_file_mtime(src.join("sub"), FileTime::from_unix_time(4_000_000, 0)).unwrap();

        let dst = tmp.path().join("dst");
        copy_dir_contents(&src, &dst).unwrap();
        assert_same_tree(&src, &dst);
        assert!(fs::metadata(dst.join("sub/ro.cfg")).unwrap().permissions().readonly());

        // Copying again over read-only files works.
        copy_dir_contents(&src, &dst).unwrap();
        assert_same_tree(&src, &dst);
    }

    #[test]
    fn copy_of_missing_source_creates_empty_target() {
        let tmp = tempdir().unwrap();
        let dst = tmp.path().join("dst");
        copy_dir_contents(&tmp.path().join("missing"), &dst).unwrap();
        assert!(dst.is_dir());
        assert!(is_empty_or_missing(&dst).unwrap());
    }

    #[test]
    fn clear_keeps_folder_and_removes_readonly_content() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("live");
        write_file(&dir.join("x/y.sav"), b"1", 1);
        let ro = dir.join("ro.sav");
        write_file(&ro, b"2", 1);
        let mut perms = fs::metadata(&ro).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&ro, perms).unwrap();

        clear_dir(&dir).unwrap();
        assert!(dir.is_dir());
        assert!(is_empty_or_missing(&dir).unwrap());
    }

    #[test]
    fn replace_removes_stale_files() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write_file(&src.join("new.sav"), b"new", 10);
        write_file(&dst.join("old.sav"), b"old", 20);
        replace_dir_contents(&src, &dst).unwrap();
        assert_same_tree(&src, &dst);
    }

    #[test]
    fn remove_path_handles_missing_and_files() {
        let tmp = tempdir().unwrap();
        remove_path(&tmp.path().join("missing")).unwrap();
        let f = tmp.path().join("f");
        write_file(&f, b"x", 1);
        remove_path(&f).unwrap();
        assert!(!f.exists());
    }
}
