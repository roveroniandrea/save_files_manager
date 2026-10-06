# Architecture

> Keep this file in sync with the code (see `CLAUDE.md`). Items marked *(planned)* are not implemented yet.

## Overview
```
Svelte UI (src/)  ──invoke()──▶  Tauri commands (src-tauri/src/commands.rs)
                                   │
                                   ├─ config     load/save config.json (atomic, versioned)   (planned)
                                   ├─ swap       profile swap engine, backups, journal         (planned)
                                   ├─ steam      registry, libraryfolders.vdf, appmanifests    (planned)
                                   ├─ manifest   Ludusavi manifest fetch/index/resolve         (planned)
                                   ├─ process    running-game detection                        (planned)
                                   ├─ icons      exe icon extraction                           (planned)
                                   └─ launch     steam:// URL or exe spawn                     (planned)
```

## Frontend
- SvelteKit with `adapter-static` (SPA, `ssr = false` in `src/routes/+layout.ts`).
- `src/lib/api.ts` — typed wrappers over `invoke()`; `src/lib/types.ts` — shared types mirroring Rust structs. *(planned)*

## Data layout
Stored under `%APPDATA%\SaveFilesManager\` (configurable):
```
config.json
manifest\manifest.yaml, manifest.meta.json, index.json
icons\<game-id>.png
games\<game-id>\
  profiles\<profile-id>\data\<path-index>\...
  backups\<timestamp>_<profile-id>\<path-index>\...
  swap.journal.json   (only while a swap is in progress)
```
See `docs/SPEC.md` for the config schema and the swap algorithm.
