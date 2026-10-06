# Save Files Manager — Specification & Implementation Plan

## Context
Many games (especially on Steam) have a single save slot/folder and no notion of "who is playing". When two people (e.g. "Andrea" and "Silvia") share a PC, each must manually back up / overwrite the save folder, risking destroying the other's progress. This app manages **per-game profiles**: each profile owns a full copy of the game's save data, and the app swaps the live save folder(s) to the selected profile, remembering which profile is currently active per game.

Scope v1: **Windows only, Steam games** (auto-discovered) + manually added games (exe, name, icon, save paths). Repo is empty (`C:\Users\Utente\Documents\repositories\save_files_manager`), so this is a greenfield project.

## Decisions (confirmed with user)
| Topic | Decision |
|---|---|
| Stack | **Tauri 2** (Rust backend) + **Svelte 5 + TypeScript** (Vite) frontend |
| Save discovery | **Ludusavi manifest** (downloaded + cached), manual path fallback |
| Launch | Swap + optional **Play** (`steam://rungameid/<appid>` or exe); swaps blocked while game runs |
| Steam Cloud | Detect & **warn**; document as known limitation |
| Storage | `%APPDATA%\SaveFilesManager\`, **copy-based** swap with rollback |
| Backups | Snapshot before every swap, keep last **N (default 5)** per game, configurable |
| Save paths | **Multiple paths per game**, swapped together as one unit |

## Functional requirements
1. **Library**
   - Scan Steam installs: read `SteamPath` from registry `HKCU\Software\Valve\Steam`, parse `steamapps/libraryfolders.vdf` → each library's `appmanifest_*.acf` (appid, name, installdir). Hide tools/redistributables (Steamworks Common Redistributables, Proton, etc.).
   - User chooses which discovered games to **add** (not all installed games are auto-tracked).
   - **Add manual game**: pick exe (file dialog), set name, icon (default = extracted from exe; or custom image file), one or more save folders.
   - **Edit game**: rename, change icon, change exe, add/remove/edit save paths, backup limit.
   - **Remove game** from app: option to keep or delete stored profile data (default keep → moved to an `_orphaned` folder).
2. **Save location discovery**
   - Download Ludusavi manifest (`https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml`), cache in app data, refresh at most once per 7 days using ETag/If-None-Match; manual "Refresh" button. App works offline with cached/no manifest.
   - Match by `steam.id == appid` (fallback: name). Keep only `files` entries with `when.os` = windows (or unspecified) and tag `save` (optionally `config`, user toggle).
   - Expand placeholders: `<base>`/`<root>`/`<game>` (install dir), `<storeUserId>` (Steam user id3 from `config/loginusers.vdf`, most recent user; enumerate all `userdata/*` if ambiguous), `<winAppData>`, `<winLocalAppData>`, `<winLocalAppDataLow>`, `<winDocuments>`, `<winPublic>`, `<winProgramData>`, `<winDir>`, `<home>`, `<osUserName>`, `<storeGameId>`.
   - Resolve globs; collapse file globs to their **containing directory** (swap unit = folder). Show results as **suggestions** the user confirms/edits; the user can always add a path manually.
   - Paths stored as templates (with placeholders) + resolved absolute path, so they survive user-profile/drive changes.
3. **Profiles (per game, independent)**
   - When a game is first configured, the existing live saves become the first profile (user names it, default "Default"), and it's marked **active**.
   - **Create profile**: "Fresh" (empty save folders → the game starts a new save) or "Clone from <profile>".
   - **Switch profile**: see swap algorithm. Active profile persisted per game.
   - **Rename** any profile; **delete** non-active profiles (confirm dialog).
   - Show per-profile: name, last used, size on disk, last modified time.
4. **Play**: button launches the game (Steam: `steam://rungameid/<appid>` via opener; manual: spawn exe with working dir = exe dir). Profile selector next to Play does "switch then launch".
5. **Safety**
   - Block swap if the game process is running (match by exe path via `sysinfo`; for Steam games, any process whose exe lives under the install dir).
   - Warn if Steam Cloud is enabled for the game (heuristic: `userdata/<id>/<appid>/remote` exists and/or `remotecache.vdf` present; per-app `CloudEnabled` in `localconfig.vdf` if present). Warning text explains Steam may overwrite swapped saves and how to disable cloud per game.
   - Pre-swap backup snapshot; restore-from-backup UI (list backups with timestamp, restore into live = itself a guarded swap).
   - Detect **drift**: if the live folder was changed outside the app since last swap, that's normal (game played); but if live content is missing/empty unexpectedly, warn before overwriting the active profile's store.
6. **Settings**: storage root (default `%APPDATA%\SaveFilesManager`), default backup limit, manifest refresh, include `config` tagged paths, theme (system/light/dark).

## Swap algorithm (core, Rust, unit-tested)
For game G, switching active profile A → B, with save paths P1..Pn:
1. Preconditions: B ≠ A, game not running, all Pi parent dirs accessible.
2. Write a **journal** `games/<id>/swap.journal.json` {from A, to B, step, timestamp}.
3. **Backup**: copy every live Pi → `games/<id>/backups/<timestamp>_<A>/<i>/`. Prune to N.
4. **Store A**: copy live Pi → `games/<id>/profiles/<A>/staging/<i>`, then atomically replace `profiles/<A>/data/<i>` (rename staging→data, old→trash, delete trash).
5. **Load B**: clear live Pi contents, copy `profiles/<B>/data/<i>` → Pi (if B is fresh/has no data for i, leave Pi empty but existing).
6. Update `active_profile = B` in config, save config atomically (write tmp + rename), delete journal.
7. On any error in 4–5: restore live Pi from the step-3 backup, keep A active, surface error.
8. On app startup, if a journal exists → offer "recover" (restore backup, keep `from` active).

Copy uses `walkdir` with preserved mtimes (`filetime`); handles read-only attributes and long paths (`\\?\` prefix).

## Data layout
```
%APPDATA%\SaveFilesManager\     # app dir (fixed)
  config.json                 # settings + games list (versioned schema)
  config.json.bak             # previous config.json, kept on every save
  manifest\manifest.yaml, manifest.meta.json (etag, fetched_at)
  icons\<game-id>.png
<storage root>\               # settings.storage_root, defaults to the app dir
  games\<game-id>\
    profiles\<profile-id>\data\<path-index>\...
    profiles\<profile-id>\profile.json
    backups\<timestamp>_<profile-id>\<path-index>\...
    swap.journal.json         # only during a swap
```
`config.json` (schema v1): `{ version, settings{storage_root, backup_limit, include_config, theme}, games:[{ id(uuid), name, source:{kind:"steam",appid,install_dir}|{kind:"manual"}, exe_path, icon, save_paths:[{template, resolved}], profiles:[{id, name, created_at, last_used_at}], active_profile_id, backup_limit? }] }`. Profile folders use ids (not names) so renames are free.

Config persistence rules:
- Missing `config.json` → default config (nothing is written until the first change).
- On load the document is migrated to the current schema version. A **newer** version than the app supports, invalid JSON, or broken invariants (duplicate ids, active profile not among the game's profiles, a game with profiles but no active one, empty names, backup limit 0) are load errors: the app then runs read-only and never overwrites the file.
- Every change is applied to a copy, validated, written to `config.json.tmp` (fsync), the old file is copied to `config.json.bak`, then the tmp file is renamed over `config.json`. The in-memory config changes only after a successful write.
- Optional fields may be omitted from the file and take their defaults, so additive changes don't need a schema bump.
- The storage root cannot be changed while games are configured (data is not moved yet, see `KNOWN_BUGS.md` KB-004).

## Architecture
```
save_files_manager/
  CLAUDE.md, README.md, KNOWN_BUGS.md, CHANGELOG.md
  docs/SPEC.md (this spec), docs/ARCHITECTURE.md
  package.json, vite.config.ts, svelte.config.js
  src/                         # Svelte 5 UI
    lib/api.ts                 # typed wrappers over Tauri invoke()
    lib/types.ts
    routes|components: GameList, GameDetail, ProfileList, AddGameWizard, SavePathEditor, Settings, BackupList
  src-tauri/
    Cargo.toml, tauri.conf.json, capabilities/
    src/main.rs, lib.rs        # tauri builder, command registration
    src/commands.rs            # #[tauri::command] thin layer
    src/config.rs              # load/save/migrate config.json
    src/steam/{mod,vdf,library,users,cloud}.rs
    src/manifest/{mod,fetch,resolve}.rs  # Ludusavi parse + placeholder expansion
    src/swap/{mod,copy,journal,backup}.rs
    src/process.rs             # running-game detection (sysinfo)
    src/icons.rs               # exe icon extraction (windows crate → PNG via image)
    src/launch.rs
    src/error.rs               # thiserror AppError, serialized to UI
```
Key crates: `tauri 2`, `tauri-plugin-dialog`, `tauri-plugin-opener`, `serde`/`serde_json`, `serde_yml` (manifest), `keyvalues-parser` (VDF), `winreg`, `sysinfo`, `walkdir`, `filetime`, `glob`, `reqwest` (rustls), `dirs`, `chrono`, `uuid`, `thiserror`, `windows` (icon extraction), `image`. Manifest is large (tens of MB YAML) → parse once in a background task, index only `steam.id → files` into a compact cached JSON to keep startup fast and memory low.

## UI (single window)
- Left: game list (icon + name + active profile badge), "+ Add game" (Steam scan / manual).
- Right: game detail → header (icon, name, Play ▸ with profile dropdown, Edit), Steam Cloud warning banner, profile cards (active highlighted; Switch / Rename / Delete / Clone), "New profile" (Fresh / Clone), save paths list, backups list with Restore.
- Add-game wizard: pick game → review suggested save paths → name first profile.
- Swaps show progress + toast result; all destructive actions confirm.

## Project instructions (to go in `CLAUDE.md`)
- **Always keep documentation up to date**: any change to behavior, data layout, commands, or settings must update `README.md`, `docs/SPEC.md` and/or `docs/ARCHITECTURE.md` in the same change; add an entry to `CHANGELOG.md`.
- **Record known bugs in `KNOWN_BUGS.md`**: every discovered-but-unfixed bug or limitation gets an entry (ID, description, repro, workaround, status); remove/mark fixed when resolved. Seed it with: Steam Cloud may overwrite swapped saves; saves in registry are not supported; games storing saves inside the install dir may be reset by Steam "verify files".
- Windows-only for now; keep platform-specific code isolated (`steam/`, `icons.rs`, `process.rs`).
- Swap engine changes require unit tests; never write to real save dirs in tests (use `tempfile`).
- Commands: `npm run tauri dev`, `npm run tauri build`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run check`.

## Milestones
1. Scaffold Tauri 2 + Svelte 5 + TS; add CLAUDE.md, README, KNOWN_BUGS, CHANGELOG, docs/.
2. Config model + persistence (atomic write, schema version).
3. Swap engine + backups + journal recovery (with tests) — the core.
4. Manual games: add/edit, exe icon extraction, save path editor, profiles CRUD, switch.
5. Steam: library scan, users, appmanifest parsing, Steam icons (`appcache/librarycache`), Steam Cloud detection.
6. Ludusavi manifest fetch/cache/index + placeholder resolution → suggestions.
7. Process detection, Play/launch, backup restore UI, settings.
8. Polish, installer build (NSIS/MSI via Tauri bundler), docs pass.

## Verification
- `cargo test`: swap engine on temp dirs (A→B→A round trip preserves bytes & mtimes; fresh profile → empty live; failure mid-copy triggers rollback; journal recovery; backup pruning), VDF/ACF parsing fixtures, placeholder expansion fixtures, manifest indexing on a small sample.
- `npm run check` (svelte-check / tsc).
- Manual E2E with `npm run tauri dev`: scan Steam, add a real installed game, confirm suggested save path matches PCGamingWiki, create "Andrea" (existing) + "Silvia" (fresh), switch, launch via Play, verify game starts a new save for Silvia, switch back, verify Andrea's save intact; verify swap is blocked while game is running; verify backup restore.
- `npm run tauri build` produces an installer; check binary size stays small (target < 15 MB).
