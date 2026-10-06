//! Tauri commands. Keep these thin: logic lives in the modules they call.
//!
//! Commands that touch files or may wait for the operation lock are
//! `async`, so Tauri runs them off the main thread and the UI never freezes.

use std::path::PathBuf;

use tauri::State;
use uuid::Uuid;

use crate::config::{Config, ConfigStore, GameSource, Settings};
use crate::error::{AppError, AppResult};
use crate::games::{self, GameInput};
use crate::icons::{self, IconChoice, IconSources};
use crate::manifest::fetch::HttpFetcher;
use crate::manifest::{Manifest, ManifestStatus, SuggestQuery, Suggestions};
use crate::paths::AppPaths;
use crate::steam::{CloudStatus, Steam, SteamScan};
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
        if settings.storage_root != config.settings.storage_root {
            if !config.games.is_empty() {
                return Err(AppError::Other(
                    "the storage folder cannot be changed while games are configured (see KNOWN_BUGS KB-004)".into(),
                ));
            }
            if let Some(root) = &settings.storage_root {
                if !root.is_absolute() {
                    return Err(AppError::Other(format!("{} is not an absolute path", root.display())));
                }
                std::fs::create_dir_all(root).map_err(|e| AppError::io(root, e))?;
            }
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

/// Whether the game is running now (swaps are refused while it is).
#[tauri::command(async)]
pub fn game_running(store: State<'_, ConfigStore>, game_id: Uuid) -> AppResult<bool> {
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    Ok(crate::process::is_running(game))
}

/// Starts the game (Steam: `steam://rungameid/<appid>`; manual: its exe).
#[tauri::command(async)]
pub fn launch_game(store: State<'_, ConfigStore>, game_id: Uuid) -> AppResult<()> {
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    crate::launch::launch(game)
}

/// Opens the folder holding profiles and backups in Explorer.
#[tauri::command(async)]
pub fn open_storage_folder(paths: State<'_, AppPaths>, store: State<'_, ConfigStore>) -> AppResult<()> {
    let layout = paths.storage(&store.get()?.settings);
    crate::launch::open_folder(layout.root())
}

/// Returns the startup recovery reports once (subsequent calls return none).
#[tauri::command]
pub fn take_recovery_reports(recovery: State<'_, StartupRecovery>) -> Vec<RecoveryReport> {
    std::mem::take(&mut *recovery.0.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Adds a game; `steam_appid` set = a Steam game (its install folder is read
/// from Steam, not trusted from the UI).
#[tauri::command(async)]
pub fn add_game(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    input: GameInput,
    first_profile_name: String,
    steam_appid: Option<u32>,
) -> AppResult<Config> {
    let ctx = SwapContext::new(&paths, &store)?;
    let (source, steam_icon) = match steam_appid {
        None => (GameSource::Manual, None),
        Some(appid) => {
            let steam = Steam::locate().ok_or(AppError::SteamNotFound)?;
            let app = steam.app(appid).ok_or(AppError::SteamAppNotFound(appid))?;
            let source = GameSource::Steam {
                appid,
                install_dir: app.install_dir,
            };
            (source, steam.library_icon(appid))
        }
    };
    games::add_game(
        &store,
        &ctx,
        &paths.icons_dir(),
        input,
        &first_profile_name,
        source,
        steam_icon.as_deref(),
    )?;
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
    let appid = store.get()?.game(game_id).and_then(games::steam_appid);
    let steam_icon = appid.and_then(steam_library_icon);
    games::update_game(&store, &ctx, &paths.icons_dir(), game_id, input, steam_icon.as_deref())?;
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
pub fn preview_icon(
    choice: IconChoice,
    exe_path: Option<PathBuf>,
    steam_appid: Option<u32>,
) -> AppResult<Option<String>> {
    let steam_icon = steam_appid.and_then(steam_library_icon);
    let sources = IconSources {
        exe: exe_path.as_deref(),
        steam: steam_icon.as_deref(),
    };
    icons::preview(&choice, sources)
}

#[tauri::command]
pub fn manifest_status(manifest: State<'_, Manifest>) -> ManifestStatus {
    manifest.status()
}

/// Checks for a newer Ludusavi manifest now (ignores the 7-day interval).
#[tauri::command(async)]
pub fn refresh_manifest(manifest: State<'_, Manifest>) -> AppResult<ManifestStatus> {
    manifest.refresh(&HttpFetcher, true)
}

/// Save folder suggestions from the Ludusavi manifest.
#[tauri::command(async)]
pub fn suggest_save_paths(
    paths: State<'_, AppPaths>,
    store: State<'_, ConfigStore>,
    manifest: State<'_, Manifest>,
    query: SuggestQuery,
) -> AppResult<Suggestions> {
    let ctx = SwapContext::new(&paths, &store)?;
    let include_config = store.get()?.settings.include_config;
    let install_dir = query
        .steam_appid
        .and_then(|appid| Steam::locate()?.app(appid))
        .map(|app| app.install_dir);
    Ok(manifest.suggest(&query, install_dir.as_deref(), &ctx.places, &ctx.guards, include_config))
}

fn steam_library_icon(appid: u32) -> Option<PathBuf> {
    Steam::locate()?.library_icon(appid)
}

/// Installed Steam games (`root: null` if Steam is not installed).
#[tauri::command(async)]
pub fn steam_scan() -> SteamScan {
    Steam::locate().map(|s| s.scan()).unwrap_or_default()
}

/// Steam Cloud status of a Steam game (`null` for other games or without Steam).
#[tauri::command(async)]
pub fn steam_cloud_status(store: State<'_, ConfigStore>, game_id: Uuid) -> AppResult<Option<CloudStatus>> {
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    Ok(games::steam_appid(game).and_then(|appid| Some(Steam::locate()?.cloud_status(appid))))
}
