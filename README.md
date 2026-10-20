# Save Files Manager

A lightweight Windows app to keep **separate save profiles per game** for games that only support a single save.
Pick a game, pick a profile ("Andrea", "Silvia", …) and the app swaps the game's save folder(s) for you, remembering which profile is active.

> Status: feature-complete for v1 (English and Italian UI). See `docs/PROGRESS.md`, and `KNOWN_BUGS.md` for known issues and limitations.

## Features
- **Steam games** are discovered automatically (all libraries); any other game can be added by hand (exe, name, icon, save folders).
- **Suggested save folders** from the [Ludusavi manifest](https://github.com/mtkennerly/ludusavi-manifest) (downloaded at most once a week; the app works offline).
- **Independent profiles per game**: start empty or as a copy, rename, delete, switch.
- **Safe swaps**: a backup before every switch, automatic rollback on failure, crash recovery at the next start, and switches are refused while the game is running.
- **Play** button: switch to the chosen profile and start the game (Steam games through Steam).
- **Backups** list per game with one-click restore.
- **Steam Cloud warning** for games whose saves Steam Cloud syncs (it could overwrite swapped saves).
- Light/dark theme; English or Italian UI (follows Windows by default, changeable in Settings).

## Install
Download `Save Files Manager_<version>_x64-setup.exe` from the [latest GitHub release](https://github.com/roveroniandrea/save_files_manager/releases/latest) (or build it yourself, see below) and run it. The installer is not Authenticode-signed, so Windows SmartScreen may warn about an unknown publisher (More info → Run anyway; KB-019). It installs for the current user only (no administrator rights needed) and requires the Microsoft Edge WebView2 runtime, which Windows 10/11 already include (the installer downloads it if missing). Setup asks for its language (English/Italiano) on the first install only; later installs reuse that choice. The app's language is set separately in Settings.

Only one copy of the app runs at a time; starting it again brings the open window to the front.

## Using it
1. **Add game** → pick an installed Steam game from the list, or **Other game…** for anything else.
2. In the editor, add the game's **save folders**: use the suggestions (for Steam games they appear automatically; for others press **Find**) or **Add folder…**. If the game is not found under its name, pick one of the **Did you mean** names, or search the database yourself (**Not the right game? Search the database**). Check them: the whole folder is swapped.
3. Name the **first profile**: the saves currently in those folders become this profile.
4. **New profile** creates another one: *Empty* (the game starts a new save) or a copy of an existing profile.
5. **Switch to this profile** swaps the folders (a backup is taken first). Or pick the profile next to **Play** and press Play: the app switches, then starts the game.
6. Close the game before switching: the app refuses while the game runs.
7. If something goes wrong, **Backups** on the game page lists the snapshots taken before each switch; restore one of the active profile.

Turn off Steam Cloud for games you manage here (Steam → right-click the game → Properties → General), otherwise Steam may bring back the other profile's saves. The app shows a warning when it detects cloud sync.

### Where data lives
- `%APPDATA%\SaveFilesManager\` — `config.json` (+ `config.json.bak`), game icons, the save location database cache.
- Profiles and backups: `games\<game-id>\` in the same folder, unless another **storage folder** is chosen in Settings (possible only while no games are added). **Settings → Open folder** opens it.
- The game's live save folders are only touched by switches and restores. Removing a game never touches them; its stored profiles are kept in `games\_orphaned\` unless you choose to delete them.

## Development
Prerequisites: [Node.js](https://nodejs.org/) 20+, [Rust](https://www.rust-lang.org/tools/install) (stable, MSVC toolchain) and the [Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/) (WebView2, MSVC Build Tools).

```sh
npm install
npm run tauri dev      # run in development (uses the real %APPDATA%\SaveFilesManager)
npm run build:unsigned # release build + NSIS installer in src-tauri/target/release/bundle/nsis/ (no updater signature)
npm run tauri build    # same, plus signed updater artifacts: needs TAURI_SIGNING_PRIVATE_KEY(_PASSWORD), see Releases
npm run check          # type check the frontend
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
```

The app icon is generated from `src-tauri/icons/app-icon.svg`: `npx tauri icon src-tauri/icons/app-icon.svg` (then delete the generated `android/` and `ios/` folders).

CI (`.github/workflows/ci.yml`) runs `npm run check`, clippy (warnings are errors) and the Rust tests on a Windows runner for every push to `main` and every pull request.

## Releases
Releases are built and published by GitHub Actions (`.github/workflows/release.yml`) when a `vX.Y.Z` tag is pushed:

1. Add the changes under `## Unreleased` in `CHANGELOG.md` and commit them on `main`.
2. `npm run release X.Y.Z` — checks the tree is clean and on `main`, sets the version in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`, moves the `Unreleased` entries into `## X.Y.Z — <date>`, commits `Release vX.Y.Z` and creates the tag `vX.Y.Z`. Nothing is pushed (undo with `git tag -d vX.Y.Z && git reset --hard HEAD~1`).
3. `git push origin main && git push origin vX.Y.Z` — the workflow checks that the tag matches every version file, runs the tests, builds the NSIS installer and **publishes** the GitHub Release right away (no draft) with the installer, the signed updater bundle (`.sig`) and `latest.json`; the release notes are that version's changelog section.

One-time setup: the repository must be public (the in-app updater downloads `latest.json` and the installer without authentication), and the updater signing key must be stored as repository secrets (Settings → Secrets and variables → Actions): `TAURI_SIGNING_PRIVATE_KEY` (contents of `%USERPROFILE%\.tauri\save_files_manager.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The matching public key is in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). Keep a backup of the private key and its password: without them, installed copies can no longer verify (and install) updates. To build a signed installer locally, set the same two variables in the shell before `npm run tauri build`.

## Documentation
- `docs/SPEC.md` — functional specification
- `docs/ARCHITECTURE.md` — code structure, commands and data layout
- `docs/PROGRESS.md` — implementation progress and next steps
- `KNOWN_BUGS.md` — known bugs and limitations
- `CHANGELOG.md` — changes
