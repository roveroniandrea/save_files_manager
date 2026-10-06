use std::path::PathBuf;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use uuid::Uuid;

/// Application-wide error type. Serialized to the UI as `{ kind, message }`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("config file {path} is invalid: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("config file {path} has schema version {found}, but this app supports up to {supported}; update the app")]
    UnsupportedConfigVersion {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("could not determine the user's application data directory")]
    NoAppDataDir,
    #[error("game {0} not found")]
    GameNotFound(Uuid),
    #[error("profile {0} not found")]
    ProfileNotFound(Uuid),
    #[error("a profile named '{0}' already exists")]
    NameTaken(String),
    #[error("the name cannot be empty")]
    EmptyName,
    #[error("the active profile cannot be deleted; switch to another profile first")]
    ProfileActive,
    #[error("backup '{0}' not found or incomplete")]
    BackupNotFound(String),
    #[error("save folder {path} cannot be swapped: {reason}")]
    InvalidSavePath { path: PathBuf, reason: String },
    #[error("another operation on this game is in progress or needs recovery ({0} exists)")]
    OperationInProgress(PathBuf),
    /// The live save folder(s) are empty while the active profile has stored
    /// data; the UI asks for confirmation and retries with `allow_empty_live`.
    #[error("live save folder(s) are empty but the active profile has stored data: {}", display_paths(.0))]
    LiveSavesEmpty(Vec<PathBuf>),
    #[error("{cause}; additionally, restoring the save folders failed ({rollback}). The backup is kept and recovery will be retried at startup")]
    RollbackFailed { cause: String, rollback: String },
    #[error("{0}")]
    Other(String),
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl AppError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        AppError::Io {
            path: path.into(),
            source,
        }
    }

    /// Stable machine-readable identifier for the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Io { .. } => "io",
            AppError::ConfigParse { .. } => "config_parse",
            AppError::UnsupportedConfigVersion { .. } => "unsupported_config_version",
            AppError::InvalidConfig(_) => "invalid_config",
            AppError::NoAppDataDir => "no_app_data_dir",
            AppError::GameNotFound(_) => "game_not_found",
            AppError::ProfileNotFound(_) => "profile_not_found",
            AppError::NameTaken(_) => "name_taken",
            AppError::EmptyName => "empty_name",
            AppError::ProfileActive => "profile_active",
            AppError::BackupNotFound(_) => "backup_not_found",
            AppError::InvalidSavePath { .. } => "invalid_save_path",
            AppError::OperationInProgress(_) => "operation_in_progress",
            AppError::LiveSavesEmpty(_) => "live_saves_empty",
            AppError::RollbackFailed { .. } => "rollback_failed",
            AppError::Other(_) => "other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
