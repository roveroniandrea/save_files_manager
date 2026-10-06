//! On-disk layout. All paths are built here so the layout lives in one place.
//!
//! - App dir (fixed, `%APPDATA%\SaveFilesManager`): `config.json`, `manifest\`, `icons\`.
//! - Storage root (`settings.storage_root`, defaults to the app dir): `games\<game-id>\...`.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::config::Settings;
use crate::error::{AppError, AppResult};

pub const APP_DIR_NAME: &str = "SaveFilesManager";

#[derive(Debug, Clone)]
pub struct AppPaths {
    app_dir: PathBuf,
}

impl AppPaths {
    pub fn new(app_dir: impl Into<PathBuf>) -> Self {
        AppPaths {
            app_dir: app_dir.into(),
        }
    }

    /// `%APPDATA%\SaveFilesManager` (roaming app data).
    pub fn from_system() -> AppResult<Self> {
        let base = dirs::config_dir().ok_or(AppError::NoAppDataDir)?;
        Ok(Self::new(base.join(APP_DIR_NAME)))
    }

    pub fn app_dir(&self) -> &Path {
        &self.app_dir
    }

    pub fn config_file(&self) -> PathBuf {
        self.app_dir.join("config.json")
    }

    pub fn manifest_dir(&self) -> PathBuf {
        self.app_dir.join("manifest")
    }

    pub fn icons_dir(&self) -> PathBuf {
        self.app_dir.join("icons")
    }

    /// Storage layout for the given settings.
    pub fn storage(&self, settings: &Settings) -> StorageLayout {
        StorageLayout {
            root: settings
                .storage_root
                .clone()
                .unwrap_or_else(|| self.app_dir.clone()),
        }
    }
}

/// Layout of per-game data under the storage root. Profiles are addressed by
/// id, never by name, so renames never move data.
#[derive(Debug, Clone)]
pub struct StorageLayout {
    root: PathBuf,
}

impl StorageLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        StorageLayout { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn game_dir(&self, game: Uuid) -> PathBuf {
        self.root.join("games").join(game.to_string())
    }

    pub fn profile_dir(&self, game: Uuid, profile: Uuid) -> PathBuf {
        self.game_dir(game).join("profiles").join(profile.to_string())
    }

    /// Stored copy of save path number `index` for a profile.
    pub fn profile_data_dir(&self, game: Uuid, profile: Uuid, index: usize) -> PathBuf {
        self.profile_dir(game, profile).join("data").join(index.to_string())
    }

    pub fn backups_dir(&self, game: Uuid) -> PathBuf {
        self.game_dir(game).join("backups")
    }

    pub fn journal_file(&self, game: Uuid) -> PathBuf {
        self.game_dir(game).join("swap.journal.json")
    }

    /// Where data of removed games is kept when the user chooses not to delete it.
    pub fn orphaned_dir(&self) -> PathBuf {
        self.root.join("games").join("_orphaned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_root_defaults_to_app_dir() {
        let paths = AppPaths::new(r"C:\data\SaveFilesManager");
        let mut settings = Settings::default();
        assert_eq!(paths.storage(&settings).root(), paths.app_dir());
        settings.storage_root = Some(PathBuf::from(r"D:\Saves"));
        assert_eq!(paths.storage(&settings).root(), Path::new(r"D:\Saves"));
    }

    #[test]
    fn profile_paths_use_ids() {
        let layout = StorageLayout::new(r"C:\root");
        let game = Uuid::nil();
        let profile = Uuid::max();
        let dir = layout.profile_data_dir(game, profile, 2);
        assert_eq!(
            dir,
            Path::new(r"C:\root")
                .join("games")
                .join(game.to_string())
                .join("profiles")
                .join(profile.to_string())
                .join("data")
                .join("2")
        );
        assert!(layout.journal_file(game).starts_with(layout.game_dir(game)));
    }
}
