# Save Files Manager

A lightweight Windows app to keep **separate save profiles per game** for games that only support a single save.
Pick a game, pick a profile ("Andrea", "Silvia", …) and the app swaps the game's save folder(s) for you, remembering which profile is active.

> Status: early development. See `docs/SPEC.md` for the full specification and `KNOWN_BUGS.md` for known issues.

## Features (planned for v1)
- Auto-discovery of installed Steam games and suggested save folders (via the [Ludusavi manifest](https://github.com/mtkennerly/ludusavi-manifest), downloaded at most once a week; the app works offline).
- Steam Cloud detection: a warning on games whose saves Steam Cloud syncs (it could overwrite swapped saves).
- Manually add any game: exe, custom name and icon, one or more save folders.
- Independent profiles per game: create fresh or cloned, rename, delete, switch.
- Safe swaps: automatic backups before each swap, rollback on failure, blocked while the game is running.
- Play button: switch to the chosen profile and launch the game (Steam games through Steam).
- Backups list per game with one-click restore; settings page (theme, backup limit, storage folder, save location database).
- Multilingual UI: English first, Italian planned.

## Development
Prerequisites: [Node.js](https://nodejs.org/) 20+, [Rust](https://www.rust-lang.org/tools/install) (stable, MSVC toolchain) and the [Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/) (WebView2, MSVC Build Tools).

```sh
npm install
npm run tauri dev      # run in development
npm run tauri build    # build installer
npm run check          # type check frontend
cargo test --manifest-path src-tauri/Cargo.toml
```

## Documentation
- `docs/SPEC.md` — functional specification
- `docs/ARCHITECTURE.md` — code structure and data layout
- `docs/PROGRESS.md` — implementation progress and next steps
- `KNOWN_BUGS.md` — known bugs and limitations
- `CHANGELOG.md` — changes
