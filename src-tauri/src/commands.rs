//! Tauri commands. Keep these thin: logic lives in the modules they call.

use tauri::State;

use crate::config::{Config, ConfigStore, Settings};
use crate::error::{AppError, AppResult};

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
