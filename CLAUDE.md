# CLAUDE.md

Save Files Manager: a lightweight Windows desktop app that keeps **per-game save profiles** (e.g. "Andrea", "Silvia") and swaps the game's live save folder(s) to the selected profile. Full specification: `docs/SPEC.md`. Architecture: `docs/ARCHITECTURE.md`.

## Stack
- Tauri 2 (Rust backend, `src-tauri/`) + SvelteKit/Svelte 5 + TypeScript frontend (`src/`, static adapter, SPA).
- Windows only for now; Steam games are auto-discovered, other games are added manually.

## Commands
- `npm install` — install frontend deps
- `npm run tauri dev` — run the app in development
- `npm run tauri build` — build the installer
- `npm run check` — Svelte/TypeScript type check
- `cargo test --manifest-path src-tauri/Cargo.toml` — Rust unit tests

## Rules
- **Track implementation progress in `docs/PROGRESS.md`.** Read it at the start of every session to know where work stopped; update it (milestone status, log entry with date, next steps, environment notes) with every commit.
- **Commit and push** completed units of work to `origin` (private GitHub repo) so progress is never only local.
- **Always keep documentation up to date.** Any change to behavior, data layout, Tauri commands, settings or build steps must update `README.md`, `docs/SPEC.md` and/or `docs/ARCHITECTURE.md` in the same change, and add an entry under `Unreleased` in `CHANGELOG.md`.
- **Write down known bugs in `KNOWN_BUGS.md`.** Every bug or limitation that is discovered but not fixed gets an entry (ID, description, repro, workaround, status). When a bug is fixed, mark it fixed (or remove it) and mention the fix in `CHANGELOG.md`.
- Keep Windows/Steam-specific code isolated (`src-tauri/src/steam/`, `icons.rs`, `process.rs`) so other platforms/stores can be added later.
- The swap engine (`src-tauri/src/swap/`) must never lose data: every change needs unit tests, and tests must only touch temp dirs (`tempfile`), never real save folders.
- Tauri commands stay thin (`commands.rs`); logic lives in testable modules.
- Profiles are stored by id, never by name (renames must not move data).
