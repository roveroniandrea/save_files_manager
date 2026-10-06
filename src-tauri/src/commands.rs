//! Tauri commands. Keep these thin: logic lives in the modules they call.
//!
//! Swap commands are synchronous on purpose: Tauri runs them off the main
//! thread, and the per-game journal prevents overlapping operations.

use tauri::State;
use uuid::Uuid;

use crate::config::{Config, ConfigStore, Settings};
use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;
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
