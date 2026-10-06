# Implementation progress

Read this first at the start of every session; update it at the end of every work session / commit (see `CLAUDE.md`).
Milestones come from `docs/SPEC.md`.

## Status
| # | Milestone | Status |
|---|---|---|
| 1 | Scaffold Tauri 2 + Svelte 5 + TS; docs (CLAUDE.md, README, KNOWN_BUGS, CHANGELOG, docs/) | ✅ Done |
| 2 | Config model + persistence (atomic write, schema version) | ✅ Done |
| 3 | Swap engine + backups + journal recovery (with tests) | ✅ Done |
| 4 | Manual games: add/edit, exe icon extraction, save path editor, profiles CRUD, switch | ⏳ Next |
| 5 | Steam: library scan, users, appmanifests, icons, Steam Cloud detection | ⬜ Todo |
| 6 | Ludusavi manifest fetch/cache/index + placeholder resolution | ⬜ Todo |
| 7 | Process detection, Play/launch, backup restore UI, settings | ⬜ Todo |
| 8 | Polish, installer build, docs pass | ⬜ Todo |
| 9 | Italian translation + language setting (i18n infra exists since M3; English only until then) | ⬜ Todo |

## Environment notes
- Node 22 / npm 10 installed. `npm run check` passes.
- Rust stable (x86_64-pc-windows-msvc) via rustup + VS Build Tools 2019 installed (2026-10-06).
- `cargo` lives in `%USERPROFILE%\.cargo\bin`; if a tool shell can't find it, prepend that to `PATH` (PowerShell: `$env:Path += ";$env:USERPROFILE\.cargo\bin"`).
- Git remote: private GitHub repo `roveroniandrea/save_files_manager` (branch `main`).

## Log
- 2026-10-06 — Milestone 3: swap engine `src-tauri/src/swap/` (`mod`, `copy`, `journal`, `backup`, `validate`, tests). Switch = backup → store active profile (staging/trash) → load target → config commit; journal-driven rollback on any failure; startup `recover_all` (undo, or complete if step `loaded`); restore-from-backup (active profile only); backup pruning; save-path safety guards; `live_saves_empty` confirmation. Commands `switch_profile`, `list_backups`, `restore_backup`, `take_recovery_reports`. i18n groundwork (`src/lib/i18n/`, English) and milestone 9 (Italian) added at the user's request. 39 Rust tests pass (5 repeated runs), clippy and `npm run check` clean. Decisions approved by the user: empty live → confirm; missing save folder = empty, created on load; clear contents but keep the folder; backups kept on rollback; automatic startup recovery with a notice. One refinement: a crash after all files were copied (`loaded`) is completed instead of undone, so play progress is never discarded.
- Test note: NTFS updates folder timestamps lazily, so tests compare file bytes/mtimes/read-only but not folder mtimes.
- 2026-10-06 — Milestone 2: `config.rs` (schema v1 model, load/migrate/validate, atomic save + `.bak`, `ConfigStore` read-only on load error), `paths.rs` (fixed app dir + storage layout), `error.rs`, `commands.rs` (`get_config`, `update_settings`), `src/lib/{types,api}.ts`, placeholder page. 13 Rust tests pass; `npm run check` and clippy clean.
- 2026-10-06 — Milestone 1: scaffolded project via `create-tauri-app` (svelte-ts template, SvelteKit + adapter-static), wrote spec and project docs.

## Next steps
- Verify `npm run tauri dev` opens the window (not checked manually yet).
- Milestone 4 (manual games + UI), all strings via `t()`:
  - Engine additions in `swap/` (with tests): create profile *fresh* (no data dir) or *clone* (from live if the source is the active profile, else from its stored `data`); delete a non-active profile's folder; remove game → move `games/<id>` to `_orphaned/` or delete; run `validate::live_paths` when save paths are edited (not only at swap time).
  - Game add/edit commands, file dialog (`tauri-plugin-dialog`), exe icon extraction (`icons.rs`), save path editor, profile list/switch UI handling `live_saves_empty` with a confirm dialog, recovery notices.
