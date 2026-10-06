//! Pre-operation backups: `games/<id>/backups/<timestamp>_<profile-id>/<save-path-id>/`
//! plus `backup.json`, which is written last and marks the backup complete.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

use super::copy;
use super::journal::LivePath;

pub const META_FILE: &str = "backup.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupReason {
    Switch,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupMeta {
    /// Profile whose live saves were backed up.
    pub profile_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub reason: BackupReason,
    /// Save paths contained in the backup (one folder each).
    pub save_path_ids: Vec<Uuid>,
}

/// A complete backup, as listed to the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BackupInfo {
    /// Folder name; used as the backup id.
    pub id: String,
    #[serde(flatten)]
    pub meta: BackupMeta,
}

/// Unique, chronologically sortable backup folder name.
pub fn backup_name(at: DateTime<Utc>, profile: Uuid) -> String {
    format!("{}_{}", at.format("%Y%m%dT%H%M%S%3fZ"), profile)
}

/// Copies every live folder into `dir/<save-path-id>` and then writes the metadata.
pub fn create(dir: &Path, live: &[LivePath], meta: &BackupMeta) -> AppResult<()> {
    if dir.exists() {
        return Err(AppError::Other(format!("backup {} already exists", dir.display())));
    }
    for live in live {
        copy::copy_dir_contents(&live.path, &dir.join(live.id.to_string()))?;
    }
    let json = serde_json::to_vec_pretty(meta).map_err(|e| AppError::Other(e.to_string()))?;
    let meta_path = dir.join(META_FILE);
    fs::write(&meta_path, json).map_err(|e| AppError::io(&meta_path, e))
}

fn read_meta(dir: &Path) -> Option<BackupMeta> {
    let text = fs::read_to_string(dir.join(META_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Complete backups in `backups_dir`, newest first.
pub fn list(backups_dir: &Path) -> AppResult<Vec<BackupInfo>> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(backups_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(AppError::io(backups_dir, e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| AppError::io(backups_dir, e))?;
        if !entry.path().is_dir() {
            continue;
        }
        if let Some(meta) = read_meta(&entry.path()) {
            out.push(BackupInfo {
                id: entry.file_name().to_string_lossy().into_owned(),
                meta,
            });
        }
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(out)
}

/// Opens a complete backup by id.
pub fn find(backups_dir: &Path, id: &str) -> AppResult<(PathBuf, BackupMeta)> {
    // Ids are plain folder names; refuse anything that could escape `backups_dir`.
    let valid = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let dir = backups_dir.join(id);
    match (valid, read_meta(&dir)) {
        (true, Some(meta)) => Ok((dir, meta)),
        _ => Err(AppError::BackupNotFound(id.to_string())),
    }
}

/// Deletes the oldest complete backups so that at most `keep` remain.
/// Incomplete backups (no metadata) are left for crash recovery to handle.
pub fn prune(backups_dir: &Path, keep: usize) -> AppResult<()> {
    for old in list(backups_dir)?.into_iter().skip(keep) {
        copy::remove_path(&backups_dir.join(old.id))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swap::test_util::{assert_same_tree, write_file};
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn meta(profile: Uuid, live: &[LivePath]) -> BackupMeta {
        BackupMeta {
            profile_id: profile,
            created_at: Utc::now(),
            reason: BackupReason::Switch,
            save_path_ids: live.iter().map(|l| l.id).collect(),
        }
    }

    #[test]
    fn create_list_find_prune() {
        let tmp = tempdir().unwrap();
        let live0 = tmp.path().join("live0");
        write_file(&live0.join("s.sav"), b"save", 100);
        let live = [
            LivePath { id: Uuid::new_v4(), path: live0.clone() },
            LivePath { id: Uuid::new_v4(), path: tmp.path().join("live1-missing") },
        ];
        let backups = tmp.path().join("backups");
        let profile = Uuid::new_v4();

        let mut names = Vec::new();
        for i in 0..4 {
            let at = Utc.timestamp_opt(1_700_000_000 + i, 0).unwrap();
            let name = backup_name(at, profile);
            create(&backups.join(&name), &live, &meta(profile, &live)).unwrap();
            names.push(name);
        }
        // An incomplete backup is ignored by list and prune.
        fs::create_dir_all(backups.join("00000000T000000000Z_partial/0")).unwrap();

        let listed = list(&backups).unwrap();
        assert_eq!(listed.len(), 4);
        assert_eq!(listed[0].id, names[3], "newest first");
        let (dir, m) = find(&backups, &names[1]).unwrap();
        assert_eq!(m.profile_id, profile);
        assert_eq!(m.save_path_ids, vec![live[0].id, live[1].id]);
        assert_same_tree(&live0, &dir.join(live[0].id.to_string()));
        assert!(dir.join(live[1].id.to_string()).is_dir());

        assert!(matches!(find(&backups, "../x"), Err(AppError::BackupNotFound(_))));
        assert!(find(&backups, "00000000T000000000Z_partial").is_err());

        prune(&backups, 2).unwrap();
        let ids: Vec<_> = list(&backups).unwrap().into_iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![names[3].clone(), names[2].clone()]);
        assert!(backups.join("00000000T000000000Z_partial").exists());
    }
}
