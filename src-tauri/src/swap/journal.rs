//! `swap.journal.json`: records an in-flight operation so a crash can be
//! recovered at startup. It is self-contained (holds the live save paths), so
//! recovery does not depend on the config.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// Switch the active profile from `from` to `to`.
    Switch,
    /// Restore a backup into the live folders (active profile unchanged).
    Restore,
}

/// Progress of an operation. Live save folders are only modified from
/// `Loading` on; before that only app-owned folders are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Copying live folders into the backup (backup may be incomplete).
    BackingUp,
    /// Backup complete; replacing the `from` profile's stored data.
    Storing,
    /// Clearing live folders and copying the new content in.
    Loading,
    /// Live folders hold the new content; config update pending.
    Loaded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub operation: Operation,
    pub game_id: Uuid,
    /// Profile active when the operation started; it stays active on recovery.
    pub from: Uuid,
    /// Target profile (switch only).
    pub to: Option<Uuid>,
    /// Name of the pre-operation backup folder under `backups/`.
    pub backup: String,
    /// Live save folders, in save path index order.
    pub save_paths: Vec<PathBuf>,
    pub step: Step,
    pub started_at: DateTime<Utc>,
}

impl Journal {
    /// Creates the journal file; fails with `OperationInProgress` if one
    /// already exists (acts as a per-game lock).
    pub fn create(&self, path: &Path) -> AppResult<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| AppError::io(dir, e))?;
        }
        let mut file = match fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(AppError::OperationInProgress(path.to_path_buf()))
            }
            Err(e) => return Err(AppError::io(path, e)),
        };
        let json = serde_json::to_vec_pretty(self).map_err(|e| AppError::Other(e.to_string()))?;
        file.write_all(&json).map_err(|e| AppError::io(path, e))?;
        file.sync_all().map_err(|e| AppError::io(path, e))
    }

    /// Persists a new step (atomic replace).
    pub fn set_step(&mut self, path: &Path, step: Step) -> AppResult<()> {
        self.step = step;
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(self).map_err(|e| AppError::Other(e.to_string()))?;
        {
            let mut file = fs::File::create(&tmp).map_err(|e| AppError::io(&tmp, e))?;
            file.write_all(&json).map_err(|e| AppError::io(&tmp, e))?;
            file.sync_all().map_err(|e| AppError::io(&tmp, e))?;
        }
        fs::rename(&tmp, path).map_err(|e| AppError::io(path, e))
    }

    /// Reads the journal if present.
    pub fn read(path: &Path) -> AppResult<Option<Journal>> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::io(path, e)),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| AppError::Other(format!("journal {} is invalid: {e}", path.display())))
    }

    pub fn remove(path: &Path) -> AppResult<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::io(path, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample() -> Journal {
        Journal {
            operation: Operation::Switch,
            game_id: Uuid::new_v4(),
            from: Uuid::new_v4(),
            to: Some(Uuid::new_v4()),
            backup: "20261006T120000000Z_x".into(),
            save_paths: vec![PathBuf::from(r"C:\saves")],
            step: Step::BackingUp,
            started_at: Utc::now(),
        }
    }

    #[test]
    fn create_is_exclusive_and_steps_persist() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("g").join("swap.journal.json");
        let mut journal = sample();
        journal.create(&path).unwrap();
        assert!(matches!(
            sample().create(&path),
            Err(AppError::OperationInProgress(_))
        ));
        journal.set_step(&path, Step::Loading).unwrap();
        assert_eq!(Journal::read(&path).unwrap().unwrap(), journal);
        Journal::remove(&path).unwrap();
        assert_eq!(Journal::read(&path).unwrap(), None);
        Journal::remove(&path).unwrap();
    }
}
