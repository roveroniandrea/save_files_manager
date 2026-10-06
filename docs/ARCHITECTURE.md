# Architecture

> Keep this file in sync with the code (see `CLAUDE.md`). Items marked *(planned)* are not implemented yet.

## Overview
```
Svelte UI (src/)  ──invoke()──▶  Tauri commands (src-tauri/src/commands.rs)
                                   │
                                   ├─ config     config.json model, load/migrate/validate, atomic save, ConfigStore
                                   ├─ paths      app dir + storage layout (all on-disk paths built here)
                                   ├─ error      AppError, serialized to the UI as { kind, message }
                                   ├─ swap       profile swap engine, backups, journal         (planned)
                                   ├─ steam      registry, libraryfolders.vdf, appmanifests    (planned)
                                   ├─ manifest   Ludusavi manifest fetch/index/resolve         (planned)
                                   ├─ process    running-game detection                        (planned)
                                   ├─ icons      exe icon extraction                           (planned)
                                   └─ launch     steam:// URL or exe spawn                     (planned)
```

## Frontend
- SvelteKit with `adapter-static` (SPA, `ssr = false` in `src/routes/+layout.ts`).
- `src/lib/api.ts` — typed wrappers over `invoke()`; `src/lib/types.ts` — shared types mirroring Rust structs (keep in sync with `config.rs` / `error.rs`).

## Backend modules
- `config.rs` — `Config`/`Settings`/`Game`/`GameSource`/`SavePath`/`Profile` (serde, snake_case JSON). `load` (missing file → default, migrate, validate), `save` (validate, tmp + fsync, `.bak`, rename). `ConfigStore` is the Tauri-managed state: `get()` returns a snapshot, `update(f)` applies `f` to a copy, saves, then commits; if the file could not be loaded the store is read-only.
- `paths.rs` — `AppPaths` (fixed app dir: `config.json`, `manifest\`, `icons\`) and `StorageLayout` (storage root: `games\<id>\profiles\<id>\data\<i>`, `backups\`, `swap.journal.json`, `_orphaned\`).
- `error.rs` — `AppError` (thiserror); every command returns `AppResult<T>`.
- `commands.rs` — thin `#[tauri::command]` layer.

## Tauri commands
| Command | Args | Returns | Notes |
|---|---|---|---|
| `get_config` | — | `Config` | Fails if `config.json` could not be loaded. |
| `update_settings` | `settings: Settings` | `Config` | Rejects changing `storage_root` while games exist. |

## Data layout
App dir `%APPDATA%\SaveFilesManager\` (fixed) and storage root (`settings.storage_root`, defaults to the app dir):
```
config.json, config.json.bak                          (app dir)
manifest\manifest.yaml, manifest.meta.json, index.json (app dir)
icons\<game-id>.png                                   (app dir)
games\<game-id>\                                      (storage root)
  profiles\<profile-id>\data\<path-index>\...
  backups\<timestamp>_<profile-id>\<path-index>\...
  swap.journal.json   (only while a swap is in progress)
```
See `docs/SPEC.md` for the config schema and the swap algorithm.
