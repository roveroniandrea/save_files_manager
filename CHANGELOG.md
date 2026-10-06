# Changelog

All notable changes to this project are documented here.

## Unreleased
- Save folder suggestions from the Ludusavi manifest (`src-tauri/src/manifest/`): background download at startup (at most once per 7 days, ETag), compact cached index (`manifest\index.v1.json`), lookup by Steam appid or name/alias, placeholder expansion, glob matching, file entries collapsed to their folder, safety checks. The game editor shows suggestions (automatically for new Steam games, "Find" for others) with Found / Not found / unsafe markers, the database date and "Check for updates".
- Save folders are now stored with placeholder templates (`<winAppData>/…`, `<base>/…` for Steam games, …). Existing configs are converted at startup; a save folder whose template resolves elsewhere follows it when the old folder is gone and the new one exists (fixes KB-008). The template is shown as a tooltip on the game page.
- New commands `manifest_status`, `refresh_manifest`, `suggest_save_paths`; new error kind `manifest`. New crates `ureq`, `serde_yaml_ng`, `glob`.
- Steam integration (`src-tauri/src/steam/`): Steam install located via the registry, all library folders and installed games read from `libraryfolders.vdf` / `appmanifest_*.acf` (redistributables, Proton and runtimes hidden), Steam accounts, Steam library icons, Steam Cloud detection. Own small VDF parser (no new crate).
- "Add game" now starts with a searchable list of installed Steam games (already added ones disabled) plus "Other game…" for manual games. Steam games show their appid and install folder, default to Steam's icon ("From Steam" icon option), and show a warning banner when Steam Cloud syncs their saves.
- New commands `steam_scan`, `steam_cloud_status`; `add_game` and `preview_icon` take an optional `steamAppid`. New error kinds `game_already_added`, `steam_not_found`, `steam_app_not_found`.
- Save path safety checks also protect the Steam install, `userdata` and its account folders, and every Steam library (`steamapps`, `steamapps\common`).
- Steam games moved to another library get their new install folder at startup.
- First real UI: game list sidebar, game detail with profile cards (switch, rename, delete, new fresh/clone profile, size and last modification), add/edit game dialog (name, exe, icon with preview, save folders, first profile, backup limit), remove game dialog (keep data in `_orphaned` or delete), toasts, light/dark theme. All strings via the i18n catalog.
- Manual games: `add_game`, `update_game`, `remove_game`; profiles: `create_profile`, `rename_profile`, `delete_profile`, `profile_stats`; icons: `get_icon`, `preview_icon` (exe icon extracted as PNG, or a custom image). New error kinds `name_taken`, `empty_name`, `profile_active`.
- Save paths now have a stable `id`; stored profile data and backups are keyed by it (`data/<save-path-id>`), so adding, removing or relocating a save folder never mixes up data. Stored copies of removed save folders are kept.
- All file operations are serialized by an app-wide lock.
- Added `tauri-plugin-dialog` (file/folder pickers, confirmations).
- Swap engine (`src-tauri/src/swap/`): profile switch with pre-swap backup, store via staging/trash, journal-based rollback on any failure, backup pruning, restore-from-backup (active profile only, itself backed up first), automatic startup recovery of interrupted operations. Save path safety checks refuse folders that are/contain user or system folders, the game's install/exe folder, or overlap the app's data. Switching asks for confirmation (`live_saves_empty`) when the live save folders are empty but the active profile has stored data.
- Tauri commands `switch_profile`, `list_backups`, `restore_backup`, `take_recovery_reports`; new error kinds.
- Internationalization groundwork: message catalog `src/lib/i18n/` (English), `t()`/`tn()`, error messages localized by `kind`. Italian is planned as milestone 9.
- Project scaffold: Tauri 2 + SvelteKit (Svelte 5, TypeScript).
- Config model and persistence (`config.json` schema v1): versioned load with migration hook, validation, atomic write keeping `config.json.bak`; a config that fails to load is never overwritten (app runs read-only).
- On-disk layout helpers (`paths.rs`); `AppError` serialized to the UI as `{ kind, message }`.
- Tauri commands `get_config`, `update_settings`; typed frontend wrappers (`src/lib/api.ts`, `src/lib/types.ts`). Replaced the template `greet` demo with a placeholder page.
- Added specification (`docs/SPEC.md`), architecture notes, `CLAUDE.md`, `KNOWN_BUGS.md`, progress tracker (`docs/PROGRESS.md`).
