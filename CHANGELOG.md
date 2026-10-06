# Changelog

All notable changes to this project are documented here.

## Unreleased
- Project scaffold: Tauri 2 + SvelteKit (Svelte 5, TypeScript).
- Config model and persistence (`config.json` schema v1): versioned load with migration hook, validation, atomic write keeping `config.json.bak`; a config that fails to load is never overwritten (app runs read-only).
- On-disk layout helpers (`paths.rs`); `AppError` serialized to the UI as `{ kind, message }`.
- Tauri commands `get_config`, `update_settings`; typed frontend wrappers (`src/lib/api.ts`, `src/lib/types.ts`). Replaced the template `greet` demo with a placeholder page.
- Added specification (`docs/SPEC.md`), architecture notes, `CLAUDE.md`, `KNOWN_BUGS.md`, progress tracker (`docs/PROGRESS.md`).
