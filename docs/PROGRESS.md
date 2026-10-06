# Implementation progress

Read this first at the start of every session; update it at the end of every work session / commit (see `CLAUDE.md`).
Milestones come from `docs/SPEC.md`.

## Status
| # | Milestone | Status |
|---|---|---|
| 1 | Scaffold Tauri 2 + Svelte 5 + TS; docs (CLAUDE.md, README, KNOWN_BUGS, CHANGELOG, docs/) | ✅ Done |
| 2 | Config model + persistence (atomic write, schema version) | ✅ Done |
| 3 | Swap engine + backups + journal recovery (with tests) | ⏳ Next |
| 4 | Manual games: add/edit, exe icon extraction, save path editor, profiles CRUD, switch | ⬜ Todo |
| 5 | Steam: library scan, users, appmanifests, icons, Steam Cloud detection | ⬜ Todo |
| 6 | Ludusavi manifest fetch/cache/index + placeholder resolution | ⬜ Todo |
| 7 | Process detection, Play/launch, backup restore UI, settings | ⬜ Todo |
| 8 | Polish, installer build, docs pass | ⬜ Todo |

## Environment notes
- Node 22 / npm 10 installed. `npm run check` passes.
- Rust stable (x86_64-pc-windows-msvc) via rustup + VS Build Tools 2019 installed (2026-10-06).
- `cargo` lives in `%USERPROFILE%\.cargo\bin`; if a tool shell can't find it, prepend that to `PATH` (PowerShell: `$env:Path += ";$env:USERPROFILE\.cargo\bin"`).
- Git remote: private GitHub repo `roveroniandrea/save_files_manager` (branch `main`).

## Log
- 2026-10-06 — Milestone 2: `config.rs` (schema v1 model, load/migrate/validate, atomic save + `.bak`, `ConfigStore` read-only on load error), `paths.rs` (fixed app dir + storage layout), `error.rs`, `commands.rs` (`get_config`, `update_settings`), `src/lib/{types,api}.ts`, placeholder page. 13 Rust tests pass; `npm run check` and clippy clean.
- 2026-10-06 — Milestone 1: scaffolded project via `create-tauri-app` (svelte-ts template, SvelteKit + adapter-static), wrote spec and project docs.

## Next steps
- Verify `npm run tauri dev` opens the window (not checked manually yet).
- Milestone 3: `src-tauri/src/swap/{mod,copy,journal,backup}.rs` per the swap algorithm in `docs/SPEC.md`, using `StorageLayout` (`paths.rs`) for every path. Add `walkdir`, `filetime`. Tests on `tempfile` dirs only: A→B→A round trip (bytes + mtimes), fresh profile → empty live, mid-copy failure → rollback, journal recovery, backup pruning, read-only files.
