//! `config.json` model and persistence.
//!
//! The file is versioned (`version`), migrated on load, validated before every
//! write, and written atomically (temp file + rename, previous file kept as
//! `config.json.bak`). A file that cannot be read is never overwritten.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Current schema version of `config.json`.
pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_BACKUP_LIMIT: u32 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub games: Vec<Game>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CONFIG_VERSION,
            settings: Settings::default(),
            games: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where game profiles and backups are stored. `None` = the app data dir.
    pub storage_root: Option<PathBuf>,
    /// Backups kept per game unless the game overrides it.
    pub backup_limit: u32,
    /// Also suggest Ludusavi paths tagged `config` (not only `save`).
    pub include_config: bool,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            storage_root: None,
            backup_limit: DEFAULT_BACKUP_LIMIT,
            include_config: false,
            theme: Theme::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub id: Uuid,
    pub name: String,
    pub source: GameSource,
    #[serde(default)]
    pub exe_path: Option<PathBuf>,
    /// Icon file (absolute path), usually `icons/<game-id>.png`.
    #[serde(default)]
    pub icon: Option<PathBuf>,
    #[serde(default)]
    pub save_paths: Vec<SavePath>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub active_profile_id: Option<Uuid>,
    /// Per-game override of `Settings::backup_limit`.
    #[serde(default)]
    pub backup_limit: Option<u32>,
}

impl Game {
    pub fn new(name: impl Into<String>, source: GameSource) -> Self {
        Game {
            id: Uuid::new_v4(),
            name: name.into(),
            source,
            exe_path: None,
            icon: None,
            save_paths: Vec::new(),
            profiles: Vec::new(),
            active_profile_id: None,
            backup_limit: None,
        }
    }

    pub fn profile(&self, id: Uuid) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn profile_mut(&mut self, id: Uuid) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        self.active_profile_id.and_then(|id| self.profile(id))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameSource {
    Steam { appid: u32, install_dir: PathBuf },
    Manual,
}

/// A save folder of a game. `template` may contain Ludusavi-style placeholders
/// (e.g. `<winAppData>/Game`); `resolved` is the absolute path used for swaps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavePath {
    pub template: String,
    pub resolved: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_used_at: Option<DateTime<Utc>>,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Profile {
            id: Uuid::new_v4(),
            name: name.into(),
            created_at: Utc::now(),
            last_used_at: None,
        }
    }
}

impl Config {
    pub fn game(&self, id: Uuid) -> Option<&Game> {
        self.games.iter().find(|g| g.id == id)
    }

    pub fn game_mut(&mut self, id: Uuid) -> Option<&mut Game> {
        self.games.iter_mut().find(|g| g.id == id)
    }

    /// Effective number of backups to keep for `game`.
    pub fn backup_limit_for(&self, game: &Game) -> u32 {
        game.backup_limit.unwrap_or(self.settings.backup_limit)
    }

    /// Checks invariants that the rest of the app relies on.
    pub fn validate(&self) -> AppResult<()> {
        let invalid = |msg: String| Err(AppError::InvalidConfig(msg));
        if self.version != CONFIG_VERSION {
            return invalid(format!("unexpected version {}", self.version));
        }
        if self.settings.backup_limit == 0 {
            return invalid("backup limit must be at least 1".into());
        }
        let mut game_ids = HashSet::new();
        for game in &self.games {
            if !game_ids.insert(game.id) {
                return invalid(format!("duplicate game id {}", game.id));
            }
            if game.name.trim().is_empty() {
                return invalid(format!("game {} has an empty name", game.id));
            }
            if game.backup_limit == Some(0) {
                return invalid(format!("game '{}': backup limit must be at least 1", game.name));
            }
            let mut profile_ids = HashSet::new();
            for profile in &game.profiles {
                if !profile_ids.insert(profile.id) {
                    return invalid(format!("game '{}': duplicate profile id {}", game.name, profile.id));
                }
                if profile.name.trim().is_empty() {
                    return invalid(format!("game '{}': profile {} has an empty name", game.name, profile.id));
                }
            }
            match game.active_profile_id {
                Some(id) if !profile_ids.contains(&id) => {
                    return invalid(format!("game '{}': active profile {} does not exist", game.name, id));
                }
                None if !game.profiles.is_empty() => {
                    return invalid(format!("game '{}' has profiles but no active profile", game.name));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Loads `config.json`. A missing file yields the default config.
pub fn load(path: &Path) -> AppResult<Config> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(AppError::io(path, e)),
    };
    let parse_err = |source| AppError::ConfigParse {
        path: path.to_path_buf(),
        source,
    };
    let value: serde_json::Value = serde_json::from_str(&text).map_err(parse_err)?;
    let value = migrate(path, value)?;
    let config: Config = serde_json::from_value(value).map_err(parse_err)?;
    config.validate()?;
    Ok(config)
}

/// Upgrades an older config document to `CONFIG_VERSION`.
fn migrate(path: &Path, value: serde_json::Value) -> AppResult<serde_json::Value> {
    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| AppError::InvalidConfig(format!("{} has no schema version", path.display())))?;
    let version = u32::try_from(version).unwrap_or(u32::MAX);
    if version > CONFIG_VERSION {
        return Err(AppError::UnsupportedConfigVersion {
            path: path.to_path_buf(),
            found: version,
            supported: CONFIG_VERSION,
        });
    }
    if version == 0 {
        return Err(AppError::InvalidConfig(format!("{} has schema version 0", path.display())));
    }
    // Future migrations go here: `if version < 2 { value = v1_to_v2(value); }`.
    Ok(value)
}

/// Validates and atomically writes `config` to `path`.
pub fn save(path: &Path, config: &Config) -> AppResult<()> {
    config.validate()?;
    let dir = path
        .parent()
        .ok_or_else(|| AppError::Other(format!("{} has no parent directory", path.display())))?;
    fs::create_dir_all(dir).map_err(|e| AppError::io(dir, e))?;

    let json = serde_json::to_vec_pretty(config).map_err(|e| AppError::Other(e.to_string()))?;
    let tmp = sibling(path, "tmp");
    {
        let mut file = fs::File::create(&tmp).map_err(|e| AppError::io(&tmp, e))?;
        file.write_all(&json).map_err(|e| AppError::io(&tmp, e))?;
        file.sync_all().map_err(|e| AppError::io(&tmp, e))?;
    }
    if path.exists() {
        let bak = sibling(path, "bak");
        fs::copy(path, &bak).map_err(|e| AppError::io(&bak, e))?;
    }
    // On Windows `rename` replaces the destination atomically (MoveFileEx + REPLACE_EXISTING).
    fs::rename(&tmp, path).map_err(|e| AppError::io(path, e))?;
    Ok(())
}

/// `config.json` -> `config.json.<ext>`
fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(ext);
    path.with_file_name(name)
}

/// In-memory config shared by Tauri commands, persisted on every change.
pub struct ConfigStore {
    path: PathBuf,
    config: Mutex<Config>,
    /// Set when the file on disk could not be loaded; the store is then
    /// read-only so the broken file is never overwritten.
    load_error: Option<String>,
}

impl ConfigStore {
    /// Opens the store. Never fails: a load error is recorded and reported by
    /// [`ConfigStore::get`] / [`ConfigStore::update`].
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let (config, load_error) = match load(&path) {
            Ok(config) => (config, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        };
        ConfigStore {
            path,
            config: Mutex::new(config),
            load_error,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn check_loaded(&self) -> AppResult<()> {
        match &self.load_error {
            Some(e) => Err(AppError::Other(format!("configuration could not be loaded: {e}"))),
            None => Ok(()),
        }
    }

    /// Returns a snapshot of the current config.
    pub fn get(&self) -> AppResult<Config> {
        self.check_loaded()?;
        Ok(self.config.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    /// Applies `f` to a copy of the config, saves it, and only then makes it
    /// current. If `f` or the save fails, nothing changes.
    pub fn update<T>(&self, f: impl FnOnce(&mut Config) -> AppResult<T>) -> AppResult<T> {
        self.check_loaded()?;
        let mut guard = self.config.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        let result = f(&mut next)?;
        save(&self.path, &next)?;
        *guard = next;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample() -> Config {
        let mut config = Config::default();
        let mut game = Game::new(
            "Hollow Knight",
            GameSource::Steam {
                appid: 367520,
                install_dir: PathBuf::from(r"C:\Steam\steamapps\common\Hollow Knight"),
            },
        );
        game.save_paths.push(SavePath {
            template: "<winLocalAppDataLow>/Team Cherry/Hollow Knight".into(),
            resolved: PathBuf::from(r"C:\Users\x\AppData\LocalLow\Team Cherry\Hollow Knight"),
        });
        let andrea = Profile::new("Andrea");
        game.active_profile_id = Some(andrea.id);
        game.profiles.push(andrea);
        game.profiles.push(Profile::new("Silvia"));
        config.games.push(game);
        config.games.push(Game::new("Manual game", GameSource::Manual));
        config
    }

    #[test]
    fn missing_file_loads_default() {
        let dir = tempdir().unwrap();
        let config = load(&dir.path().join("config.json")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");
        let config = sample();
        save(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), config);
        assert!(!sibling(&path, "tmp").exists());
    }

    #[test]
    fn save_keeps_previous_file_as_bak() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let first = Config::default();
        save(&path, &first).unwrap();
        let second = sample();
        save(&path, &second).unwrap();
        assert_eq!(load(&sibling(&path, "bak")).unwrap(), first);
        assert_eq!(load(&path).unwrap(), second);
    }

    #[test]
    fn json_shape_matches_spec() {
        let value = serde_json::to_value(sample()).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["settings"]["theme"], "system");
        assert_eq!(value["games"][0]["source"]["kind"], "steam");
        assert_eq!(value["games"][0]["source"]["appid"], 367520);
        assert_eq!(value["games"][1]["source"]["kind"], "manual");
    }

    #[test]
    fn missing_optional_fields_use_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"version":1,"games":[{"id":"6f1c2b9e-0d7a-4c1e-9a55-2f0d1b3c4e5f","name":"G","source":{"kind":"manual"}}]}"#).unwrap();
        let config = load(&path).unwrap();
        assert_eq!(config.settings, Settings::default());
        assert!(config.games[0].profiles.is_empty());
        assert_eq!(config.backup_limit_for(&config.games[0]), DEFAULT_BACKUP_LIMIT);
    }

    #[test]
    fn newer_version_is_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"version":99}"#).unwrap();
        assert!(matches!(
            load(&path),
            Err(AppError::UnsupportedConfigVersion { found: 99, .. })
        ));
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(load(&path), Err(AppError::ConfigParse { .. })));
        fs::write(&path, r#"{"games":[]}"#).unwrap();
        assert!(matches!(load(&path), Err(AppError::InvalidConfig(_))));
    }

    #[test]
    fn validate_rejects_broken_invariants() {
        let mut c = sample();
        c.games[0].active_profile_id = Some(Uuid::new_v4());
        assert!(c.validate().is_err());

        let mut c = sample();
        c.games[0].active_profile_id = None;
        assert!(c.validate().is_err());

        let mut c = sample();
        let dup = c.games[0].clone();
        c.games.push(dup);
        assert!(c.validate().is_err());

        let mut c = sample();
        let dup = c.games[0].profiles[0].clone();
        c.games[0].profiles.push(dup);
        assert!(c.validate().is_err());

        let mut c = sample();
        c.settings.backup_limit = 0;
        assert!(c.validate().is_err());

        let mut c = sample();
        c.games[0].profiles[1].name = "  ".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn invalid_config_is_not_written() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        save(&path, &sample()).unwrap();
        let mut bad = sample();
        bad.settings.backup_limit = 0;
        assert!(save(&path, &bad).is_err());
        assert_eq!(load(&path).unwrap().settings.backup_limit, DEFAULT_BACKUP_LIMIT);
    }

    #[test]
    fn store_update_persists_and_failed_update_changes_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::open(&path);
        store
            .update(|c| {
                c.settings.backup_limit = 7;
                Ok(())
            })
            .unwrap();
        assert_eq!(load(&path).unwrap().settings.backup_limit, 7);

        // Closure error.
        let r: AppResult<()> = store.update(|c| {
            c.settings.backup_limit = 9;
            Err(AppError::Other("nope".into()))
        });
        assert!(r.is_err());
        // Validation error on save.
        assert!(store
            .update(|c| {
                c.settings.backup_limit = 0;
                Ok(())
            })
            .is_err());
        assert_eq!(store.get().unwrap().settings.backup_limit, 7);
        assert_eq!(load(&path).unwrap().settings.backup_limit, 7);
    }

    #[test]
    fn store_with_unreadable_file_is_read_only() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "garbage").unwrap();
        let store = ConfigStore::open(&path);
        assert!(store.get().is_err());
        assert!(store.update(|_| Ok(())).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "garbage");
    }
}
