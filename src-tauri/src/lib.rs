mod commands;
pub mod config;
pub mod error;
pub mod paths;
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
