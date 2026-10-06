mod commands;
pub mod config;
pub mod error;
pub mod paths;

use config::ConfigStore;
use paths::AppPaths;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = AppPaths::from_system().expect("cannot locate the application data directory");
    let store = ConfigStore::open(paths.config_file());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(paths)
        .manage(store)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
