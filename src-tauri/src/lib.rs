mod commands;
pub mod config;
pub mod error;
pub mod games;
pub mod icons;
pub mod paths;
pub mod steam;
pub mod swap;

use std::sync::Mutex;

use config::ConfigStore;
use paths::AppPaths;
use swap::RecoveryReport;

/// Results of startup crash recovery, shown once by the UI.
pub struct StartupRecovery(pub Mutex<Vec<RecoveryReport>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = AppPaths::from_system().expect("cannot locate the application data directory");
    let store = ConfigStore::open(paths.config_file());
    // Finish or undo operations interrupted by a crash before any new one can start.
    let recovered = match store.get() {
        Ok(config) => swap::recover_all(&store, &paths.storage(&config.settings)),
        Err(_) => Vec::new(),
    };
    // Steam games moved to another library get their new install folder.
    if let Some(steam) = store.get().ok().and_then(|_| steam::Steam::locate()) {
        if let Err(e) = games::refresh_steam_install_dirs(&store, &steam.apps()) {
            eprintln!("could not refresh Steam install folders: {e}");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(paths)
        .manage(store)
        .manage(StartupRecovery(Mutex::new(recovered)))
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::update_settings,
            commands::switch_profile,
            commands::list_backups,
            commands::restore_backup,
            commands::take_recovery_reports,
            commands::add_game,
            commands::update_game,
            commands::remove_game,
            commands::create_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::profile_stats,
            commands::get_icon,
            commands::preview_icon,
            commands::steam_scan,
            commands::steam_cloud_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
