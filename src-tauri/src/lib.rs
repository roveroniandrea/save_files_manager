mod commands;
pub mod config;
pub mod error;
pub mod games;
pub mod icons;
pub mod launch;
pub mod manifest;
pub mod paths;
pub mod process;
pub mod steam;
pub mod swap;

use std::sync::Mutex;

use config::ConfigStore;
use manifest::Manifest;
use tauri::Manager;
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
    // Moved Steam installs, save path templates, relocated save folders.
    if store.get().is_ok() {
        let steam = steam::Steam::locate();
        let apps = steam.as_ref().map(|s| s.apps()).unwrap_or_default();
        let places = manifest::placeholders::Places::from_system(steam.map(|s| s.root().to_path_buf()));
        if let Err(e) = games::refresh_paths(&store, &apps, &places) {
            eprintln!("could not refresh game folders: {e}");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Manifest::new(paths.manifest_dir()))
        .manage(paths)
        .manage(store)
        .manage(StartupRecovery(Mutex::new(recovered)))
        .setup(|app| {
            // Check for a newer Ludusavi manifest (at most once per 7 days).
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let manifest = handle.state::<Manifest>();
                if let Err(e) = manifest.refresh(&manifest::fetch::HttpFetcher, false) {
                    eprintln!("manifest refresh failed: {e}");
                }
            });
            Ok(())
        })
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
            commands::manifest_status,
            commands::refresh_manifest,
            commands::suggest_save_paths,
            commands::game_running,
            commands::launch_game,
            commands::open_storage_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
