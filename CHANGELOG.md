# Changelog

All notable changes to this project are documented here.

## Unreleased
- Swap engine (`src-tauri/src/swap/`): profile switch with pre-swap backup, store via staging/trash, journal-based rollback on any failure, backup pruning, restore-from-backup (active profile only, itself backed up first), automatic startup recovery of interrupted operations. Save path safety checks refuse folders that are/contain user or system folders, the game's install/exe folder, or overlap the app's data. Switching asks for confirmation (`live_saves_empty`) when the live save folders are empty but the active profile has stored data.
- Tauri commands `switch_profile`, `list_backups`, `restore_backup`, `take_recovery_reports`; new error kinds.
- Internationalization groundwork: message catalog `src/lib/i18n/` (English), `t()`/`tn()`, error messages localized by `kind`. Italian is planned as milestone 9.
- Project scaffold: Tauri 2 + SvelteKit (Svelte 5, TypeScript).
- Config model and persistence (`config.json` schema v1): versioned load with migration hook, validation, atomic write keeping `config.json.bak`; a config that fails to load is never overwritten (app runs read-only).
- On-disk layout helpers (`paths.rs`); `AppError` serialized to the UI as `{ kind, message }`.
- Tauri commands `get_config`, `update_settings`; typed frontend wrappers (`src/lib/api.ts`, `src/lib/types.ts`). Replaced the template `greet` demo with a placeholder page.
- Added specification (`docs/SPEC.md`), architecture notes, `CLAUDE.md`, `KNOWN_BUGS.md`, progress tracker (`docs/PROGRESS.md`).
