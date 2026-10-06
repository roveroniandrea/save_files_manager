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
   - Scan Steam installs: read `SteamPath` from registry `HKCU\Software\Valve\Steam` (fallback `%ProgramFiles(x86)%\Steam`), parse `steamapps/libraryfolders.vdf` → each library's `appmanifest_*.acf` (appid, name, installdir). Hide tools/redistributables (Steamworks Common Redistributables, Proton, Steam Linux Runtime, SteamVR).
   - User chooses which discovered games to **add** (not all installed games are auto-tracked): "Add game" opens a searchable list of installed Steam games (already added ones are disabled) with an "Other game…" button for manual games. A Steam game is added through the same editor (name prefilled, appid and install folder shown read-only); its install folder is read from Steam by the backend, never taken from the UI. A Steam appid can be added only once.
   - Steam game icon: by default Steam's cached library icon (`appcache/librarycache/<appid>/<sha1>.jpg`, or the older `<appid>_icon.jpg`; 32×32), copied to `icons/`. The exe icon or a custom image can be chosen instead.
   - At startup, Steam games whose install folder moved to another library get the new folder (games no longer installed keep the old one).
   - **Add manual game**: pick exe (optional, file dialog; the name defaults to the exe name), set name, icon (default = extracted from exe as a 256px PNG; or a custom png/jpg/gif/webp/bmp/ico ≤ 5 MB; or none), one or more save folders, name of the first profile. Save folders pass the safety checks (see swap algorithm) when the game is saved, not only at swap time.
   - **Edit game**: rename, change icon, change exe, add/remove/relocate save paths, backup limit. Each save path has a stable id, and profile data is stored per save path id: relocating a folder keeps its data, adding one gives the other profiles an empty copy of it (they start fresh for that folder), removing one keeps its stored copies in the profiles (never deleted implicitly). Blocked while an operation is in progress.
   - **Remove game** from app: option to keep or delete stored profile data (default keep → moved to `games/_orphaned/<game-id>_<timestamp>`). The live save folders are never touched. The icon file is deleted.
2. **Save location discovery**
   - Download Ludusavi manifest (`https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml`), cache in app data, refresh at most once per 7 days using ETag/If-None-Match; manual "Refresh" button. App works offline with cached/no manifest.
   - Match by `steam.id == appid` (fallback: name). Keep only `files` entries with `when.os` = windows (or unspecified) and tag `save` (optionally `config`, user toggle).
   - Expand placeholders: `<base>`/`<root>`/`<game>` (install dir), `<storeUserId>` (Steam user id3 from `config/loginusers.vdf`, most recent user; enumerate all `userdata/*` if ambiguous), `<winAppData>`, `<winLocalAppData>`, `<winLocalAppDataLow>`, `<winDocuments>`, `<winPublic>`, `<winProgramData>`, `<winDir>`, `<home>`, `<osUserName>`, `<storeGameId>`.
   - Resolve globs; collapse file globs to their **containing directory** (swap unit = folder). Show results as **suggestions** the user confirms/edits; the user can always add a path manually.
   - Paths stored as templates (with placeholders) + resolved absolute path, so they survive user-profile/drive changes.
3. **Profiles (per game, independent)**
   - When a game is first configured, the existing live saves become the first profile (user names it, default "Default"), and it's marked **active**.
   - **Create profile**: "Fresh" (no stored data → the game starts a new save) or "Clone from <profile>" (the active profile is cloned from the live folders, others from their stored copy).
   - **Switch profile**: see swap algorithm. Active profile persisted per game.
   - **Rename** any profile; **delete** non-active profiles (confirm dialog; deletes its stored saves). Profile names are unique per game (case-insensitive) and non-empty.
   - Show per-profile: name, active badge, last used, size and newest file modification time (live folders for the active profile, stored copy for the others).
   - All profile/game file operations are serialized by one app-wide lock and refused (`operation_in_progress`) while a journal exists for the game.
4. **Play**: button launches the game (Steam: `steam://rungameid/<appid>` via opener; manual: spawn exe with working dir = exe dir). Profile selector next to Play does "switch then launch".
5. **Safety**
   - Block swap if the game process is running (match by exe path via `sysinfo`; for Steam games, any process whose exe lives under the install dir).
   - Warn if Steam Cloud is enabled for the game. Heuristic, per Steam account (`userdata/<account-id>`): `<appid>/remotecache.vdf` exists or `<appid>/remote` is non-empty, unless `CloudEnabled "0"` is set for the app (`apps/<appid>`) or globally (directly under `Software/Valve/Steam`) in `7/remote/sharedconfig.vdf` or `config/localconfig.vdf`. The game page shows a banner naming the accounts, explaining Steam may overwrite swapped saves and how to disable cloud saves for the game.
   - Pre-swap backup snapshot; restore-from-backup UI (list backups with timestamp, restore into live = itself a guarded swap).
   - Detect **drift**: if the live folder was changed outside the app since last swap, that's normal (game played); but if live content is missing/empty unexpectedly, warn before overwriting the active profile's store.
6. **Settings**: storage root (default `%APPDATA%\SaveFilesManager`), default backup limit, manifest refresh, include `config` tagged paths, theme (system/light/dark).
7. **Languages (i18n)**: the UI is multilingual. Every user-visible string lives in a message catalog (`src/lib/i18n/`), never hard-coded in components. English is the only language until the final milestone, which adds **Italian** plus a language setting (system/English/Italian; "system" follows the OS locale, falling back to English). Backend errors carry a stable `kind` that the UI translates; the backend `message` is English technical detail.

## Swap algorithm (core, Rust, unit-tested)
For game G, switching active profile A → B, with save paths P1..Pn (implemented in `src-tauri/src/swap/`):
1. **Preconditions** (nothing is written if they fail): B ≠ A (same profile = no-op), B exists, game not running *(M7)*, and every Pi passes the **save path safety checks**:
   - absolute, not a drive root, no Pi nested in another;
   - not equal to and not containing a protected folder (user home, Documents, Desktop, Downloads, Pictures, Videos, Music, Public, `%APPDATA%`, `%LOCALAPPDATA%`, ProgramData, Program Files, Windows, the Steam install dir, `Steam\userdata` and each account folder in it, every Steam library with its `steamapps` and `steamapps\common` folders, the game's install dir, the exe's folder); being *inside* one is fine;
   - no overlap at all with the app dir or the storage root.
2. **Empty-live check**: if a live Pi is empty/missing while A's stored copy of Pi has data → error `live_saves_empty`; the UI asks for confirmation and retries with `allow_empty_live`.
3. Create the **journal** `games/<id>/swap.journal.json` (exclusive create → acts as a per-game lock; an existing journal → `operation_in_progress`). It holds operation, from, to, backup name, the live paths and the current step, so recovery never depends on the config.
4. **Backup** (step `backing_up`): copy every live Pi → `backups/<yyyymmddThhmmssmmmZ>_<A>/<save-path-id>/`, then write `backup.json` (marks it complete).
5. **Store A** (step `storing`): copy the backup → `profiles/<A>/staging/<save-path-id>`, plus any stored folder of a save path no longer configured (removed save paths' data is carried over, never dropped), then rename `data`→`trash`, `staging`→`data`, delete `trash`.
6. **Load B** (step `loading`): for each Pi, delete its contents (the folder itself is kept, created if missing) and copy `profiles/<B>/data/<save-path-id>` in (B fresh → Pi stays empty).
7. Step `loaded`; set `active_profile_id = B` and `last_used_at` of A and B in config (atomic save; aborts if the active profile changed meanwhile).
8. Delete the journal, prune complete backups to the game's limit (default 5). Failures here are not errors (see recovery of `loaded`).
- **On any error in 4–7**: undo according to the step — `backing_up`: delete the partial backup; `storing`: repair A's store (`trash`→`data` if `data` is missing, delete `staging`/`trash`); `loading`/`loaded`: restore every Pi from the backup (journal reset to `loading` first). Then delete the journal and surface the original error; A stays active. If the undo itself fails → error `rollback_failed`, the journal and backup are kept.
- **Startup recovery** (automatic, before the UI can start operations; reported once to the UI): every `games/*/swap.journal.json` is undone as above and `from` is made active — except step `loaded`, which is **completed** instead (all files were copied; the user may already have played on them): `to` is made active (restore: unchanged). Then the journal is deleted. On failure the journal stays and recovery retries at the next start.

**Restore backup** (guarded the same way): only backups of the currently *active* profile can be restored; only save paths present both in the game and in the backup are restored (others are left untouched). Live Pi are first backed up (reason `restore`, so a restore can be undone), then replaced with the backup content; the active profile does not change.

**Copy rules**: `walkdir`, file contents + modification/access times (set via a `FILE_WRITE_ATTRIBUTES` handle so read-only files and folders work) + read-only attribute are preserved; read-only files are made writable before overwrite/delete. Symlinks/junctions inside save folders are refused (the backup step fails before anything is modified). Long paths are handled by Rust's std (automatic `\\?\` prefix).

## Data layout
```
%APPDATA%\SaveFilesManager\     # app dir (fixed)
  config.json                 # settings + games list (versioned schema)
  config.json.bak             # previous config.json, kept on every save
  manifest\manifest.yaml, manifest.meta.json (etag, fetched_at)
  icons\<game-id>.png
<storage root>\               # settings.storage_root, defaults to the app dir
  games\<game-id>\
    profiles\<profile-id>\data\<save-path-id>\...
    profiles\<profile-id>\profile.json
    profiles\<profile-id>\staging\, trash\   # only during a store
    backups\<timestamp>_<profile-id>\<save-path-id>\...
    backups\<timestamp>_<profile-id>\backup.json   # {profile_id, created_at, reason, save_path_ids}; marks the backup complete
    swap.journal.json         # only during a swap
```
`config.json` (schema v1): `{ version, settings{storage_root, backup_limit, include_config, theme}, games:[{ id(uuid), name, source:{kind:"steam",appid,install_dir}|{kind:"manual"}, exe_path, icon, save_paths:[{id(uuid), template, resolved}], profiles:[{id, name, created_at, last_used_at}], active_profile_id, backup_limit? }] }`. Profile folders use ids (not names) so renames are free.

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
9. Italian translation: `it` catalog for every message key, language setting (system/en/it), localized dates/sizes, installer languages.

## Verification
- `cargo test`: swap engine on temp dirs (A→B→A round trip preserves bytes & mtimes; fresh profile → empty live; failure mid-copy triggers rollback; journal recovery; backup pruning), VDF/ACF parsing fixtures, placeholder expansion fixtures, manifest indexing on a small sample.
- `npm run check` (svelte-check / tsc).
- Manual E2E with `npm run tauri dev`: scan Steam, add a real installed game, confirm suggested save path matches PCGamingWiki, create "Andrea" (existing) + "Silvia" (fresh), switch, launch via Play, verify game starts a new save for Silvia, switch back, verify Andrea's save intact; verify swap is blocked while game is running; verify backup restore.
- `npm run tauri build` produces an installer; check binary size stays small (target < 15 MB).
