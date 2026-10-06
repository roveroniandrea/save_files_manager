# Implementation progress

Read this first at the start of every session; update it at the end of every work session / commit (see `CLAUDE.md`).
Milestones come from `docs/SPEC.md`.

## Status
| # | Milestone | Status |
|---|---|---|
| 1 | Scaffold Tauri 2 + Svelte 5 + TS; docs (CLAUDE.md, README, KNOWN_BUGS, CHANGELOG, docs/) | ✅ Done |
| 2 | Config model + persistence (atomic write, schema version) | ⏳ Next |
| 3 | Swap engine + backups + journal recovery (with tests) | ⬜ Todo |
| 4 | Manual games: add/edit, exe icon extraction, save path editor, profiles CRUD, switch | ⬜ Todo |
| 5 | Steam: library scan, users, appmanifests, icons, Steam Cloud detection | ⬜ Todo |
| 6 | Ludusavi manifest fetch/cache/index + placeholder resolution | ⬜ Todo |
| 7 | Process detection, Play/launch, backup restore UI, settings | ⬜ Todo |
| 8 | Polish, installer build, docs pass | ⬜ Todo |

## Environment notes
- Node 22 / npm 10 installed. `npm run check` passes.
- Rust toolchain **not yet installed** on the dev machine (2026-10-06) — required before milestone 2 (`winget install Rustlang.Rustup` + MSVC Build Tools).
- Git remote: private GitHub repo `roveroniandrea/save-files-manager` (branch `main`).

## Log
- 2026-10-06 — Milestone 1: scaffolded project via `create-tauri-app` (svelte-ts template, SvelteKit + adapter-static), wrote spec and project docs.

## Next steps
- Install Rust, verify `npm run tauri dev` opens the window.
- Milestone 2: `src-tauri/src/config.rs` + `error.rs`, `Config`/`Game`/`Profile`/`SavePath` structs, atomic save, unit tests.
