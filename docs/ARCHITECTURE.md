# Architecture

> Keep this file in sync with the code (see `CLAUDE.md`). Items marked *(planned)* are not implemented yet.

## Overview
```
Svelte UI (src/)  ──invoke()──▶  Tauri commands (src-tauri/src/commands.rs)
                                   │
                                   ├─ config     config.json model, load/migrate/validate, atomic save, ConfigStore
                                   ├─ paths      app dir + storage layout (all on-disk paths built here)
                                   ├─ error      AppError, serialized to the UI as { kind, message }
                                   ├─ swap       profile swap engine, backups, journal, startup recovery
                                   ├─ steam      registry, libraries, appmanifests, users, cloud, icons
                                   ├─ manifest   Ludusavi manifest download/index, placeholders, suggestions
                                   ├─ process    running-game detection (Win32 process list)
                                   ├─ games      add/edit games (config + icon)
                                   ├─ icons      exe icon extraction (Win32 → PNG), custom images, data URLs
                                   └─ launch     Play: steam:// URL or exe via ShellExecuteW; open folder
```

## Frontend
- SvelteKit with `adapter-static` (SPA, `ssr = false` in `src/routes/+layout.ts`).
- `src/lib/api.ts` — typed wrappers over `invoke()` plus `errorMessage()` (localized from the error `kind`) / `errorDetail()`; `src/lib/types.ts` — shared types mirroring Rust structs (keep in sync with `config.rs`, `error.rs`, `swap/`, `games.rs`, `icons.rs`).
- `src/lib/state.svelte.ts` — global `app` state: config snapshot (sets `data-theme` on `<html>` from the theme setting and applies the language setting), selected game, `view` (game / settings), `busy`, toasts; `app.run(command)` runs a config-returning command and turns errors into toasts.
- `src/lib/format.ts` — locale-aware sizes and dates.
- `src/lib/components/` — `Settings` (settings page), `GameDetail` (Play with "play as" profile selector, running banner (polled every 3 s), profiles: switch with the `live_saves_empty` confirm, new fresh/clone, rename, delete; backups with Restore; save folders; Steam appid/install folder and Steam Cloud warning banner), `SteamPicker` (first step of "Add game": searchable list of installed Steam games, "Other game…" for manual games), `GameEditor` (add/edit modal: name, exe, icon with preview incl. "From Steam", save folders with `SaveFolderSuggestions`, first profile / backup limit; `steam` prop = adding that Steam game), `SaveFolderSuggestions` (Ludusavi suggestions: automatic for a new Steam game, "Find" otherwise; "Did you mean…" chips when nothing matched; "Not the right game? Search the database" opens a search box (250 ms debounce, Enter picks the first result, Esc closes) to pick any entry; add buttons; database date and "Check for updates"), `RemoveGameDialog`, `GameIcon`, `Modal` (native `<dialog>`), `Toasts`.
- `src/routes/+layout.svelte` — global CSS; release builds suppress the WebView context menu outside text fields.
- `src/routes/+page.svelte` — layout: game list sidebar + detail; add-game flow (picker → editor); shows startup recovery reports. `src/app.css` — theme tokens: light by default, dark via `prefers-color-scheme` unless `data-theme="light"`, or forced by `data-theme="dark"`.
- Native dialogs (file/folder pickers, confirmations) come from `@tauri-apps/plugin-dialog`; their labels are passed through `t()`.
- `src/lib/i18n/` — `en.ts` (reference catalog; flat keys, `{placeholders}`, `_one`/`_other` plural suffixes), `it.ts` (Italian, typed `Messages` so it must have exactly the same keys), `i18n.svelte.ts` (`t()`, `tn()`, reactive locale; `applyLanguage(setting)` resolves "system" from `navigator.language`, falls back to English, sets `<html lang>`; before the config loads the system language is used). Every UI string goes through it. `format.ts` formats dates/sizes with the current locale. To add a language: new catalog file, add it to `catalogs` and `Locale`, the Rust `Language` enum, `types.ts`, the Settings list and a `settings.language.<code>` key (language names are written in their own language in every catalog).

## Backend modules
- `config.rs` — `Config`/`Settings`/`Game`/`GameSource`/`SavePath`/`Profile` (serde, snake_case JSON). `load` (missing file → default, migrate, validate), `save` (validate, tmp + fsync, `.bak`, rename). `ConfigStore` is the Tauri-managed state: `get()` returns a snapshot, `update(f)` applies `f` to a copy, saves, then commits; if the file could not be loaded the store is read-only.
- `paths.rs` — `AppPaths` (fixed app dir: `config.json`, `manifest\`, `icons\`) and `StorageLayout` (storage root: `games\<id>\profiles\<id>\data\<save-path-id>`, `backups\`, `swap.journal.json`, `_orphaned\`).
- `error.rs` — `AppError` (thiserror); every command returns `AppResult<T>`.
- `swap/` — the swap engine (never loses data; see "Swap algorithm" in `docs/SPEC.md`):
  - `mod.rs` — `switch_profile`, `restore_backup`, `list_backups`, `recover_all` (startup), rollback logic, `SwapContext` (storage layout + `Guards` + `Places` for templates), test-only fail points (`fail_point(name)`, no-op in release builds).
  - `copy.rs` — copy folder contents preserving times/read-only, `clear_dir` (empties but keeps the folder), `remove_path`.
  - `journal.rs` — `swap.journal.json` (exclusive create = per-game lock, atomic step updates).
  - `backup.rs` — create/list/find/prune backups; `backup.json` marks completion.
  - `validate.rs` — save path safety checks (`Guards`: protected and reserved folders); `live_paths` returns `LivePath { id, path }`.
  - `manage.rs` — `create_profile` (fresh/clone), `rename_profile`, `delete_profile`, `remove_game`, `profile_stats`.
  - `op_lock()` — app-wide mutex serializing every operation that touches save/profile folders.
  - `tests.rs`, `test_util.rs` — engine tests on temp dirs (round trip, every failure point, locked files, each crash state).
- `games.rs` — `add_game` (manual or Steam source; a Steam appid only once), `update_game` (`GameInput`; save paths keep their id when edited; the source never changes), `refresh_paths` (startup: moved Steam installs, plain templates converted, relocated save folders followed when the old one is gone and the new one exists). Save path templates come from `manifest::placeholders::to_template`.
- `manifest/` — Ludusavi manifest (see SPEC "Save location discovery"):
  - `fetch.rs` — `Fetcher` trait, `HttpFetcher` (`ureq`, `If-None-Match`, 304 handling, size limit).
  - `index.rs` — `build` (YAML → `Index`, one game at a time via a serde map visitor), `Lookup` (by Steam id, normalized name, aliases; `entry` by exact name; `similar` / `search` fuzzy over names + aliases).
  - `fuzzy.rs` — `Key` (normalized chars + words), `score` (word overlap / edit distance (OSA) / containment), `rank` (threshold, dedupe by game, top N). `SIMILAR_MIN` 0.55, `SEARCH_MIN` 0.45.
  - `placeholders.rs` — `Places` (known folders), `GameContext` (`<base>`, `<root>`, `<game>`, `<storeGameId>`), `resolve`, `glob_pattern`, `to_template`.
  - `resolve.rs` — `suggest`: expand, glob, collapse to folders, drop nested, safety-check, sort.
  - `mod.rs` — `Manifest` (Tauri state): `refresh(fetcher, force)` (7-day interval, index cache, meta), `lookup` (waits for a running refresh), `status`, `suggest` (picked `entry` or automatic match; `similar` top 5 when nothing matched), `search` (top 20).
  - Read-only checks with a real manifest: `MANIFEST_YAML=<file> cargo test real_manifest -- --ignored --nocapture` (also `real_suggestions`, which uses this PC's Steam, and `real_fuzzy`, which prints "did you mean"/search results and timings for sample names).
- `icons.rs` — `IconChoice` (keep/exe/steam/custom/none), `IconSources` (exe + Steam library icon), `apply` (writes `icons/<game-id>.<ext>`), `exe_icon_png` (Windows: `SHDefExtractIconW` + GDI → PNG via `png`), `data_url`, `preview`.
- `steam/` — read-only access to Steam's files; `Steam::at(root)` works on fixtures, `Steam::locate()` finds the real install (registry `SteamPath` via `RegGetValueW`, fallback `%ProgramFiles(x86)%\Steam`):
  - `vdf.rs` — small text KeyValues parser (quoted/unquoted tokens, escapes, comments, conditionals; case-insensitive keys).
  - `library.rs` — `libraries()` (`libraryfolders.vdf`), `apps()` / `app(appid)` (`appmanifest_*.acf`, tools filtered by appid/name).
  - `users.rs` — `users()` (`config/loginusers.vdf`, SteamID64 → account id), `userdata_ids()`.
  - `cloud.rs` — `cloud_status(appid)`: accounts with Steam Cloud active for the app (see SPEC).
  - `mod.rs` — `library_icon(appid)`, `protected_folders()` (added to the swap guards), `scan()` (games + icon data URLs).
  - Read-only smoke test against the real install: `cargo test real_steam -- --ignored --nocapture`.
- `lib.rs` — builds the app (plugins: single-instance (a second start focuses the existing window; the op lock and the in-memory config are per process), opener, dialog); runs `swap::recover_all` at startup (if the config loaded) and keeps the reports in `StartupRecovery`, then `games::refresh_paths`; manages `Manifest` and starts the background manifest check in `setup`.
- `process.rs` — `running_exes` (`EnumProcesses` + `QueryFullProcessImageNameW`), `game_matches` (manual: exe path; Steam: under install dir), `is_running`. The swap engine gets it through `swap::RunningCheck` on `SwapContext` (tests inject their own; default = never running) and refuses switch/restore with `game_running`.
- `launch.rs` — `target` (Steam URL / exe + working dir; `no_executable`, `launch_failed`), `launch`, `open_folder` (all via `ShellExecuteW`).
- `commands.rs` — thin `#[tauri::command]` layer.

## Tauri commands
| Command | Args | Returns | Notes |
|---|---|---|---|
| `get_config` | — | `Config` | Fails if `config.json` could not be loaded. |
| `update_settings` | `settings: Settings` | `Config` | Rejects changing `storage_root` while games exist; a new storage root must be absolute and is created. |
| `switch_profile` | `gameId`, `profileId`, `options?: { allow_empty_live }` | `Config` | Async (off the main thread). Errors: `live_saves_empty` (ask, then retry with `allow_empty_live`), `invalid_save_path`, `operation_in_progress`, `rollback_failed`, … |
| `list_backups` | `gameId` | `BackupInfo[]` | Complete backups, newest first. |
| `restore_backup` | `gameId`, `backupId` | `Config` | Async. Only backups of the active profile. |
| `take_recovery_reports` | — | `RecoveryReport[]` | Startup recovery results; returned once. |
| `add_game` | `input: GameInput`, `firstProfileName`, `steamAppid?` | `Config` | Async. `steamAppid` = Steam game (install folder read from Steam; `steam_not_found`, `steam_app_not_found`, `game_already_added`), else manual. Validates save folders; default icon = Steam icon / exe icon. |
| `update_game` | `gameId`, `input: GameInput` | `Config` | Async. Save paths with `id` keep their data. |
| `remove_game` | `gameId`, `keepData` | `Config` | Async. Live folders untouched; data → `_orphaned` or deleted. |
| `create_profile` | `gameId`, `name`, `source: {kind:"fresh"} \| {kind:"clone", profile_id}` | `Config` | Async. |
| `rename_profile` | `gameId`, `profileId`, `name` | `Config` | `name_taken`, `empty_name`. |
| `delete_profile` | `gameId`, `profileId` | `Config` | Async. `profile_active` for the active one. |
| `profile_stats` | `gameId` | `ProfileStats[]` | Async. Size + newest mtime per profile. |
| `get_icon` | `gameId` | `string \| null` | `data:` URL. |
| `preview_icon` | `choice: IconChoice`, `exePath`, `steamAppid?` | `string \| null` | Async. Preview in the editor. |
| `steam_scan` | — | `SteamScan { root, games: ScannedGame[] }` | Async. Installed Steam games with icon data URLs; `root: null` without Steam. |
| `steam_cloud_status` | `gameId` | `CloudStatus \| null` | Async. Accounts with Steam Cloud active; `null` for manual games or without Steam. |
| `manifest_status` | — | `ManifestStatus` | `{ updating, games, checked_at, updated_at, error }`. |
| `refresh_manifest` | — | `ManifestStatus` | Async. Forced check/download; error kind `manifest`. |
| `suggest_save_paths` | `query: { name, steam_appid, exe_path, entry? }` | `Suggestions { status, matched, items, similar }` | Async. Waits for a running download. `entry` = database entry picked by the user (used instead of the appid/name match). Items: `{ template, path, exists, save, config, problem }`. `similar`: up to 5 entry names when nothing matched. |
| `search_manifest` | `text` | `string[]` | Async. Database entry names matching the text, best first (top 20; empty without an index). |
| `game_running` | `gameId` | `boolean` | Async. Polled by the game page. |
| `launch_game` | `gameId` | — | Async. `no_executable`, `launch_failed`. |
| `open_storage_folder` | — | — | Async. Opens the storage root in Explorer (created if missing). |

## Build & packaging
- `npm run tauri build` → release exe (`src-tauri/target/release/save-files-manager.exe`, ~6.5 MB; LTO, `opt-level = 3`, stripped, `panic = "abort"`) and an NSIS installer (`bundle/nsis/Save Files Manager_<version>_x64-setup.exe`, ~2.3 MB; English/Italian with a language selector).
- `tauri.conf.json`: product name/version/identifier (`com.savefilesmanager.app`), window 1100×720 (min 820×540), bundle target `nsis` only, `installMode: currentUser` (no admin rights), WebView2 bootstrapper (default), publisher/description metadata. MSI is not built (would need WiX and admin installs).
- Icons: `src-tauri/icons/app-icon.svg` is the source; `npx tauri icon src-tauri/icons/app-icon.svg` regenerates the PNG/ICO/ICNS set (delete the generated `android/` and `ios/` folders).
- Version: kept in sync in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `tauri.conf.json` by `npm run release X.Y.Z` (`scripts/release.mjs`: bumps them, moves `CHANGELOG.md` `Unreleased` into `## X.Y.Z — <date>`, commits `Release vX.Y.Z`, tags `vX.Y.Z`, never pushes). Its other modes are used by the workflows: `--check-tag vX.Y.Z` (every version file and a changelog section must match), `--notes vX.Y.Z` (prints the section), `--self-test` (tests the text transformations).
- Updater artifacts: `bundle.createUpdaterArtifacts: true`, so `npm run tauri build` also signs the installer (`…-setup.exe.sig`) with the minisign key from `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` and fails without it. `npm run build:unsigned` merges `src-tauri/tauri.unsigned.conf.json` (`createUpdaterArtifacts: false`) for local builds. The public key and the endpoint (`https://github.com/roveroniandrea/save_files_manager/releases/latest/download/latest.json`) are already in `plugins.updater` for milestone 12 (the updater plugin itself is not added yet).
- CI: `.github/workflows/ci.yml` (push to `main`, pull requests, manual): `windows-latest`, Node 22 + `npm ci`, stable Rust (`dtolnay/rust-toolchain`, `Swatinem/rust-cache`), `npm run check`, `release.mjs --self-test`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.
- Release: `.github/workflows/release.yml` (tag `v*.*.*`, `contents: write`): fails early if the signing secret is missing or the tag does not match the version, extracts the changelog notes, runs the tests, then `tauri-apps/tauri-action@v0` builds and publishes a non-draft release named "Save Files Manager vX.Y.Z" with the installer, its `.sig` and `latest.json` (`includeUpdaterJson`). No Authenticode signing (KB-019).

## Data layout
App dir `%APPDATA%\SaveFilesManager\` (fixed) and storage root (`settings.storage_root`, defaults to the app dir):
```
config.json, config.json.bak                          (app dir)
manifest\index.v1.json, manifest.meta.json            (app dir)
icons\<game-id>.<png|jpg|…>                           (app dir)
games\<game-id>\                                      (storage root)
  profiles\<profile-id>\data\<save-path-id>\...
  profiles\<profile-id>\staging\, trash\            (only while storing)
  backups\<timestamp>_<profile-id>\<save-path-id>\..., backup.json
games\_orphaned\<game-id>_<timestamp>\               (storage root; data of removed games)
  swap.journal.json   (only while an operation is in progress, or after a failed rollback)
```
See `docs/SPEC.md` for the config schema and the swap algorithm.
