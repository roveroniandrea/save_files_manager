use std::path::PathBuf;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

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
    #[error("{0}")]
    Other(String),
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
