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
                                   ├─ steam      registry, libraryfolders.vdf, appmanifests    (planned)
                                   ├─ manifest   Ludusavi manifest fetch/index/resolve         (planned)
                                   ├─ process    running-game detection                        (planned)
                                   ├─ icons      exe icon extraction                           (planned)
                                   └─ launch     steam:// URL or exe spawn                     (planned)
```

## Frontend
- SvelteKit with `adapter-static` (SPA, `ssr = false` in `src/routes/+layout.ts`).
- `src/lib/api.ts` — typed wrappers over `invoke()` plus `errorMessage()` (localized from the error `kind`) / `errorDetail()`; `src/lib/types.ts` — shared types mirroring Rust structs (keep in sync with `config.rs`, `error.rs`, `swap/`).
- `src/lib/i18n/` — `en.ts` (reference catalog; flat keys, `{placeholders}`, `_one`/`_other` plural suffixes), `i18n.svelte.ts` (`t()`, `tn()`, reactive locale). Every UI string goes through it; English only until milestone 9 adds `it.ts`.

## Backend modules
- `config.rs` — `Config`/`Settings`/`Game`/`GameSource`/`SavePath`/`Profile` (serde, snake_case JSON). `load` (missing file → default, migrate, validate), `save` (validate, tmp + fsync, `.bak`, rename). `ConfigStore` is the Tauri-managed state: `get()` returns a snapshot, `update(f)` applies `f` to a copy, saves, then commits; if the file could not be loaded the store is read-only.
- `paths.rs` — `AppPaths` (fixed app dir: `config.json`, `manifest\`, `icons\`) and `StorageLayout` (storage root: `games\<id>\profiles\<id>\data\<i>`, `backups\`, `swap.journal.json`, `_orphaned\`).
- `error.rs` — `AppError` (thiserror); every command returns `AppResult<T>`.
- `swap/` — the swap engine (never loses data; see "Swap algorithm" in `docs/SPEC.md`):
  - `mod.rs` — `switch_profile`, `restore_backup`, `list_backups`, `recover_all` (startup), rollback logic, `SwapContext` (storage layout + `Guards`), test-only fail points (`fail_point(name)`, no-op in release builds).
  - `copy.rs` — copy folder contents preserving times/read-only, `clear_dir` (empties but keeps the folder), `remove_path`.
  - `journal.rs` — `swap.journal.json` (exclusive create = per-game lock, atomic step updates).
  - `backup.rs` — create/list/find/prune backups; `backup.json` marks completion.
  - `validate.rs` — save path safety checks (`Guards`: protected and reserved folders).
  - `tests.rs`, `test_util.rs` — engine tests on temp dirs (round trip, every failure point, locked files, each crash state).
- `lib.rs` — builds the app; runs `swap::recover_all` at startup (if the config loaded) and keeps the reports in `StartupRecovery`.
- `commands.rs` — thin `#[tauri::command]` layer.

## Tauri commands
| Command | Args | Returns | Notes |
|---|---|---|---|
| `get_config` | — | `Config` | Fails if `config.json` could not be loaded. |
| `update_settings` | `settings: Settings` | `Config` | Rejects changing `storage_root` while games exist. |
| `switch_profile` | `gameId`, `profileId`, `options?: { allow_empty_live }` | `Config` | Async (off the main thread). Errors: `live_saves_empty` (ask, then retry with `allow_empty_live`), `invalid_save_path`, `operation_in_progress`, `rollback_failed`, … |
| `list_backups` | `gameId` | `BackupInfo[]` | Complete backups, newest first. |
| `restore_backup` | `gameId`, `backupId` | `Config` | Async. Only backups of the active profile. |
| `take_recovery_reports` | — | `RecoveryReport[]` | Startup recovery results; returned once. |

## Data layout
App dir `%APPDATA%\SaveFilesManager\` (fixed) and storage root (`settings.storage_root`, defaults to the app dir):
```
config.json, config.json.bak                          (app dir)
manifest\manifest.yaml, manifest.meta.json, index.json (app dir)
icons\<game-id>.png                                   (app dir)
games\<game-id>\                                      (storage root)
  profiles\<profile-id>\data\<path-index>\...
  profiles\<profile-id>\staging\, trash\            (only while storing)
  backups\<timestamp>_<profile-id>\<path-index>\..., backup.json
  swap.journal.json   (only while an operation is in progress, or after a failed rollback)
```
See `docs/SPEC.md` for the config schema and the swap algorithm.
