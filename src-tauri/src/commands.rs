//! Tauri commands. Keep these thin: logic lives in the modules they call.
//!
//! Commands that touch files or may wait for the operation lock are
//! `async`, so Tauri runs them off the main thread and the UI never freezes.

use std::path::PathBuf;

use tauri::State;
use uuid::Uuid;

use crate::config::{Config, ConfigStore, Settings};
use crate::error::{AppError, AppResult};
use crate::games::{self, GameInput};
use crate::icons::{self, IconChoice};
use crate::paths::AppPaths;
use crate::swap::manage::{self, NewProfileSource, ProfileStats};
use crate::swap::{self, backup::BackupInfo, RecoveryReport, SwapContext, SwapOptions};
use crate::StartupRecovery;

#[tauri::command]
pub fn get_config(store: State<'_, ConfigStore>) -> AppResult<Config> {
    store.get()
}

#[tauri::command]
pub fn update_settings(store: State<'_, ConfigStore>, settings: Settings) -> AppResult<Config> {
    store.update(|config| {
        if settings.storage_root != config.settings.storage_root && !config.games.is_empty() {
            return Err(AppError::Other(
                "the storage folder cannot be changed while games are configured (see KNOWN_BUGS KB-004)".into(),
            ));
        }
        config.settings = settings;
        Ok(config.clone())
    })
}

#[tauri::command(async)]
pub fn switch_profile(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
    profile_id: Uuid,
    options: Option<SwapOptions>,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    swap::switch_profile(&store, &ctx, game_id, profile_id, options.unwrap_or_default())?;
    store.get()
}

#[tauri::command]
pub fn list_backups(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
) -> AppResult<Vec<BackupInfo>> {
    swap::list_backups(&SwapContext::new(&paths, &store)?, game_id)
}

#[tauri::command(async)]
pub fn restore_backup(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
    backup_id: String,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    swap::restore_backup(&store, &ctx, game_id, &backup_id)?;
    store.get()
}

/// Returns the startup recovery reports once (subsequent calls return none).
#[tauri::command]
pub fn take_recovery_reports(recovery: State<'_, StartupRecovery>) -> Vec<RecoveryReport> {
    std::mem::take(&mut *recovery.0.lock().unwrap_or_else(|e| e.into_inner()))
}

#[tauri::command(async)]
pub fn add_game(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    input: GameInput,
    first_profile_name: String,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    games::add_manual_game(&store, &ctx, &paths.icons_dir(), input, &first_profile_name)?;
    store.get()
}

#[tauri::command(async)]
pub fn update_game(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
    input: GameInput,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    games::update_game(&store, &ctx, &paths.icons_dir(), game_id, input)?;
    store.get()
}

#[tauri::command(async)]
pub fn remove_game(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
    keep_data: bool,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    let icon = store.get()?.game(game_id).and_then(|g| g.icon.clone());
    manage::remove_game(&store, &ctx, game_id, keep_data)?;
    icons::remove(icon.as_deref());
    store.get()
}

#[tauri::command(async)]
pub fn create_profile(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
    name: String,
    source: NewProfileSource,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    manage::create_profile(&store, &ctx, game_id, &name, source)?;
    store.get()
}

#[tauri::command]
pub fn rename_profile(
    store: State<'_, ConfigStore>,
    game_id: Uuid,
    profile_id: Uuid,
    name: String,
) -> AppResult<Config> {
    manage::rename_profile(&store, game_id, profile_id, &name)?;
    store.get()
}

#[tauri::command(async)]
pub fn delete_profile(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
    profile_id: Uuid,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    manage::delete_profile(&store, &ctx, game_id, profile_id)?;
    store.get()
}

#[tauri::command(async)]
pub fn profile_stats(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    game_id: Uuid,
) -> AppResult<Vec<ProfileStats>> {
    manage::profile_stats(&store, &SwapContext::new(&paths, &store)?, game_id)
}

/// The game's icon as a `data:` URL, if it has one.
#[tauri::command]
pub fn get_icon(store: State<'_, ConfigStore>, game_id: Uuid) -> AppResult<Option<String>> {
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    match &game.icon {
        Some(icon) if icon.exists() => icons::data_url(icon).map(Some),
        _ => Ok(None),
    }
}

/// Preview of an icon choice in the game editor, before saving.
#[tauri::command(async)]
pub fn preview_icon(choice: IconChoice, exe_path: Option<PathBuf>) -> AppResult<Option<String>> {
    icons::preview(&choice, exe_path.as_deref())
}
