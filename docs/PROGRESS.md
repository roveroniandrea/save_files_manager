# Implementation progress

Read this first at the start of every session; update it at the end of every work session / commit (see `CLAUDE.md`).
Milestones come from `docs/SPEC.md`.

## Status
| # | Milestone | Status |
|---|---|---|
| 1 | Scaffold Tauri 2 + Svelte 5 + TS; docs (CLAUDE.md, README, KNOWN_BUGS, CHANGELOG, docs/) | ✅ Done |
| 2 | Config model + persistence (atomic write, schema version) | ✅ Done |
| 3 | Swap engine + backups + journal recovery (with tests) | ✅ Done |
| 4 | Manual games: add/edit, exe icon extraction, save path editor, profiles CRUD, switch | ✅ Done (tested by the user) |
| 5 | Steam: library scan, users, appmanifests, icons, Steam Cloud detection | ✅ Done (manual UI test by the user pending) |
| 6 | Ludusavi manifest fetch/cache/index + placeholder resolution | ⏳ Next |
| 7 | Process detection, Play/launch, backup restore UI, settings | ⬜ Todo |
| 8 | Polish, installer build, docs pass | ⬜ Todo |
| 9 | Italian translation + language setting (i18n infra exists since M3; English only until then) | ⬜ Todo |

## Environment notes
- Node 22 / npm 10 installed. `npm run check` passes.
- Rust stable (x86_64-pc-windows-msvc) via rustup + VS Build Tools 2019 installed (2026-10-06).
- `cargo` lives in `%USERPROFILE%\.cargo\bin` and is on `PATH` in new terminals (since 2026-10-06).
- This PC's Steam: `C:\Program Files (x86)\Steam`, one library; games No Man's Sky (275850, Steam Cloud on) and Wallpaper Engine (431960). The user's app config has one manual test game: never make disruptive changes to its save or game files.
- Git remote: private GitHub repo `roveroniandrea/save_files_manager` (branch `main`).

## Log
- 2026-10-06 — Milestone 5: `src-tauri/src/steam/` (`vdf` own KeyValues parser, `library` libraries + appmanifests + tool filter, `users` loginusers/userdata, `cloud` Steam Cloud heuristic, `mod` locate via registry `RegGetValueW`, library icons, protected folders, scan). `add_game` takes `steamAppid` (install dir resolved by the backend, appid unique), `IconChoice::Steam`, commands `steam_scan` / `steam_cloud_status`, Steam folders added to the swap guards, moved install dirs refreshed at startup. UI: `SteamPicker` (searchable, already-added disabled, "Other game…"), editor Steam row + "From Steam" icon, Steam Cloud banner and install folder on the game page. 67 Rust tests (+1 ignored real-Steam smoke test, run on this PC: 2 games found, redistributables hidden, cloud detected), clippy and `npm run check` clean, `npm run tauri dev` builds and starts. UI flows not clicked through by Claude → user to test.
- 2026-10-06 — Milestone 4: save paths got a stable `id` (profile data/backups keyed by it; removed paths' stored copies are carried over, never dropped). `swap/manage.rs` (create fresh/clone, rename, delete profile, remove game → `_orphaned` or delete, profile stats), `games.rs` (add manual game, update game), `icons.rs` (exe icon via `SHDefExtractIconW` + GDI → PNG, custom image, data URLs), app-wide `op_lock`. UI: sidebar + game detail (profile cards with switch/rename/delete, new profile, `live_saves_empty` confirm), game editor modal, remove dialog, toasts, recovery notices, light/dark theme; all strings in `en.ts`. `tauri-plugin-dialog` added. 51 Rust tests, clippy and `npm run check` clean. `npm run tauri dev` builds and the window starts (process stays up); the UI flows were not clicked through by Claude (no screen access) → user to test.
- 2026-10-06 — Milestone 3: swap engine `src-tauri/src/swap/` (`mod`, `copy`, `journal`, `backup`, `validate`, tests). Switch = backup → store active profile (staging/trash) → load target → config commit; journal-driven rollback on any failure; startup `recover_all` (undo, or complete if step `loaded`); restore-from-backup (active profile only); backup pruning; save-path safety guards; `live_saves_empty` confirmation. Commands `switch_profile`, `list_backups`, `restore_backup`, `take_recovery_reports`. i18n groundwork (`src/lib/i18n/`, English) and milestone 9 (Italian) added at the user's request. 39 Rust tests pass (5 repeated runs), clippy and `npm run check` clean. Decisions approved by the user: empty live → confirm; missing save folder = empty, created on load; clear contents but keep the folder; backups kept on rollback; automatic startup recovery with a notice. One refinement: a crash after all files were copied (`loaded`) is completed instead of undone, so play progress is never discarded.
- Test note: NTFS updates folder timestamps lazily, so tests compare file bytes/mtimes/read-only but not folder mtimes.
- 2026-10-06 — Milestone 2: `config.rs` (schema v1 model, load/migrate/validate, atomic save + `.bak`, `ConfigStore` read-only on load error), `paths.rs` (fixed app dir + storage layout), `error.rs`, `commands.rs` (`get_config`, `update_settings`), `src/lib/{types,api}.ts`, placeholder page. 13 Rust tests pass; `npm run check` and clippy clean.
- 2026-10-06 — Milestone 1: scaffolded project via `create-tauri-app` (svelte-ts template, SvelteKit + adapter-static), wrote spec and project docs.

## Next steps
- User: test M5 — Add game → Steam list (No Man's Sky / Wallpaper Engine shown, redistributables hidden, search, "Other game…"); add a Steam game with a scratch save folder (Steam icon, appid/install folder shown, Steam Cloud banner for NMS); edit it (icon options); check an added game is disabled in the list. Fix any reported issues first.
- Milestone 6 (Ludusavi): `src-tauri/src/manifest/{mod,fetch,resolve}.rs` — download `manifest.yaml` (ETag, at most once per 7 days, manual refresh; needs `reqwest` with rustls + a YAML parser), build a compact `index.json` (steam id → Windows `save` (+`config`) file entries) in a background task, placeholder expansion (`<base>`, `<storeUserId>` from `Steam::users()`/`userdata_ids()`, `<winAppData>`, …), glob → containing folder; the editor shows suggestions for Steam games (fixes KB-012) and stores templates (KB-008). Idea: Steam's `userdata/<id>/<appid>/remotecache.vdf` also lists auto-cloud save files with a `root` code — a possible extra suggestion source.
