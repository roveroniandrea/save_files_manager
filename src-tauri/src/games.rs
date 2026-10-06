//! Adding and editing games (config + icon). File operations on profile data
//! live in `swap::manage`.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use uuid::Uuid;

use crate::config::{clean_name, ConfigStore, Game, GameSource, Profile, SavePath};
use crate::error::{AppError, AppResult};
use crate::icons::{self, IconChoice, IconSources};
use crate::manifest::placeholders::{resolve, to_template, GameContext, Places};
use crate::steam::SteamApp;
use crate::swap::{op_lock, validate, SwapContext};

/// Editable fields of a game, as sent by the UI.
#[derive(Debug, Clone, Deserialize)]
pub struct GameInput {
    pub name: String,
    #[serde(default)]
    pub exe_path: Option<PathBuf>,
    pub save_paths: Vec<SavePathInput>,
    /// Per-game override of the backup limit (`None` = use the setting).
    #[serde(default)]
    pub backup_limit: Option<u32>,
    #[serde(default)]
    pub icon: IconChoice,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SavePathInput {
    /// Existing save path being edited; `None` = new save path.
    #[serde(default)]
    pub id: Option<Uuid>,
    pub path: PathBuf,
}

fn check_exe(exe: &Option<PathBuf>) -> AppResult<()> {
    match exe {
        Some(exe) if !exe.is_file() => Err(AppError::Other(format!("{} is not a file", exe.display()))),
        _ => Ok(()),
    }
}

/// Adds a game (manual or Steam). Its current saves become the first profile
/// (`first_profile`), which is active. `steam_icon` is Steam's cached library
/// icon, used by [`IconChoice::Steam`].
pub fn add_game(
    store: &ConfigStore,
    ctx: &SwapContext,
    icons_dir: &Path,
    input: GameInput,
    first_profile: &str,
    source: GameSource,
    steam_icon: Option<&Path>,
) -> AppResult<Game> {
    if let GameSource::Steam { appid, .. } = source {
        if let Some(existing) = store.get()?.games.iter().find(|g| steam_appid(g) == Some(appid)) {
            return Err(AppError::GameAlreadyAdded(existing.name.clone()));
        }
    }
    let mut game = Game::new(clean_name(&input.name)?, source);
    check_exe(&input.exe_path)?;
    game.exe_path = input.exe_path;
    game.backup_limit = input.backup_limit;
    if input.save_paths.iter().any(|p| p.id.is_some()) {
        return Err(AppError::Other("a new game cannot reference existing save paths".into()));
    }
    let tctx = GameContext::for_game(&game, &ctx.places);
    game.save_paths = input
        .save_paths
        .into_iter()
        .map(|p| SavePath::new(to_template(&p.path, &ctx.places, &tctx), p.path))
        .collect();
    let profile = Profile::new(clean_name(first_profile)?);
    game.active_profile_id = Some(profile.id);
    game.profiles.push(profile);
    validate::live_paths(&game, &ctx.guards)?;

    // Default for a new game: Steam's icon for Steam games, else the exe's icon.
    let choice = match input.icon {
        IconChoice::Keep if steam_appid(&game).is_some() => IconChoice::Steam,
        IconChoice::Keep => IconChoice::Exe,
        other => other,
    };
    let sources = IconSources {
        exe: game.exe_path.as_deref(),
        steam: steam_icon,
    };
    game.icon = icons::apply(icons_dir, game.id, &choice, sources, None)?;

    let added = game.clone();
    if let Err(e) = store.update(|c| {
        if let Some(appid) = steam_appid(&added) {
            if let Some(existing) = c.games.iter().find(|g| steam_appid(g) == Some(appid)) {
                return Err(AppError::GameAlreadyAdded(existing.name.clone()));
            }
        }
        c.games.push(added);
        Ok(())
    }) {
        icons::remove(game.icon.as_deref());
        return Err(e);
    }
    Ok(game)
}

/// Updates a game's name, exe, icon, backup limit and save paths. Save paths
/// keep their id (and so their stored profile data) when edited; removed save
/// paths' stored copies are kept in the profiles. The source (Steam app or
/// manual) never changes.
pub fn update_game(
    store: &ConfigStore,
    ctx: &SwapContext,
    icons_dir: &Path,
    game_id: Uuid,
    input: GameInput,
    steam_icon: Option<&Path>,
) -> AppResult<Game> {
    let _lock = op_lock();
    let journal = ctx.layout.journal_file(game_id);
    if journal.exists() {
        return Err(AppError::OperationInProgress(journal));
    }
    let config = store.get()?;
    let current = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    let mut game = current.clone();
    game.name = clean_name(&input.name)?;
    check_exe(&input.exe_path)?;
    game.exe_path = input.exe_path;
    game.backup_limit = input.backup_limit;
    let tctx = GameContext::for_game(&game, &ctx.places);
    let template = |path: &Path| to_template(path, &ctx.places, &tctx);
    game.save_paths = input
        .save_paths
        .into_iter()
        .map(|p| match p.id {
            None => Ok(SavePath::new(template(&p.path), p.path)),
            Some(id) => {
                let old = current
                    .save_paths
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| AppError::Other(format!("save path {id} not found")))?;
                if old.resolved == p.path {
                    Ok(old.clone())
                } else {
                    Ok(SavePath {
                        id,
                        template: template(&p.path),
                        resolved: p.path,
                    })
                }
            }
        })
        .collect::<AppResult<_>>()?;
    validate::live_paths(&game, &ctx.guards)?;

    let previous_icon = current.icon.clone();
    let sources = IconSources {
        exe: game.exe_path.as_deref(),
        steam: steam_icon,
    };
    game.icon = icons::apply(icons_dir, game_id, &input.icon, sources, previous_icon.as_deref())?;
    let updated = game.clone();
    store.update(|c| {
        let g = c.game_mut(game_id).ok_or(AppError::GameNotFound(game_id))?;
        *g = updated;
        Ok(())
    })?;
    Ok(game)
}

/// Startup maintenance of the games' folders:
/// - Steam games moved to another library get the new install folder (games
///   Steam no longer lists are left as they are);
/// - save paths without a placeholder template (older configs) get one;
/// - a save path whose template now resolves elsewhere (user folder, Steam
///   library moved) follows it, but only if the old folder is gone and the
///   new one exists, so no folder is ever swapped by surprise.
///
/// Returns the number of games updated.
pub fn refresh_paths(store: &ConfigStore, apps: &[SteamApp], places: &Places) -> AppResult<usize> {
    let _lock = op_lock();
    let refreshed = |game: &Game| -> Option<Game> {
        let mut g = game.clone();
        if let GameSource::Steam { appid, install_dir } = &mut g.source {
            if let Some(app) = apps.iter().find(|a| a.appid == *appid) {
                install_dir.clone_from(&app.install_dir);
            }
        }
        let tctx = GameContext::for_game(&g, places);
        for sp in &mut g.save_paths {
            if !sp.template.contains('<') {
                sp.template = to_template(&sp.resolved, places, &tctx);
            } else if let Some(new) = resolve(&sp.template, places, &tctx) {
                if new != sp.resolved && !sp.resolved.is_dir() && new.is_dir() {
                    sp.resolved = new;
                }
            }
        }
        (g != *game).then_some(g)
    };
    if !store.get()?.games.iter().any(|g| refreshed(g).is_some()) {
        return Ok(0);
    }
    store.update(|c| {
        let mut changed = 0;
        for game in &mut c.games {
            if let Some(new) = refreshed(game) {
                *game = new;
                changed += 1;
            }
        }
        Ok(changed)
    })
}

pub fn steam_appid(game: &Game) -> Option<u32> {
    match game.source {
        GameSource::Steam { appid, .. } => Some(appid),
        GameSource::Manual => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::StorageLayout;
    use crate::swap::test_util::write_file;
    use crate::swap::{switch_profile, Guards, SwapOptions};
    use crate::swap::manage::{create_profile, NewProfileSource};
    use tempfile::{tempdir, TempDir};

    fn setup() -> (TempDir, ConfigStore, SwapContext, PathBuf) {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("storage");
        let store = ConfigStore::open(tmp.path().join("config.json"));
        let ctx = SwapContext {
            layout: StorageLayout::new(&root),
            guards: Guards {
                protected: vec![],
                reserved: vec![root],
            },
            places: Default::default(),
            running: Default::default(),
        };
        let icons = tmp.path().join("icons");
        (tmp, store, ctx, icons)
    }

    fn input(name: &str, paths: Vec<SavePathInput>) -> GameInput {
        GameInput {
            name: name.into(),
            exe_path: None,
            save_paths: paths,
            backup_limit: None,
            icon: IconChoice::Keep,
        }
    }

    fn new_path(p: PathBuf) -> SavePathInput {
        SavePathInput { id: None, path: p }
    }

    fn add_manual_game(
        store: &ConfigStore,
        ctx: &SwapContext,
        icons: &Path,
        input: GameInput,
        first: &str,
    ) -> AppResult<Game> {
        add_game(store, ctx, icons, input, first, GameSource::Manual, None)
    }

    #[test]
    fn add_game_creates_active_first_profile() {
        let (tmp, store, ctx, icons) = setup();
        let live = tmp.path().join("live");
        let game = add_manual_game(&store, &ctx, &icons, input(" My Game ", vec![new_path(live.clone())]), "Andrea")
            .unwrap();
        assert_eq!(game.name, "My Game");
        assert_eq!(game.active_profile().unwrap().name, "Andrea");
        assert_eq!(game.save_paths[0].resolved, live);
        assert_eq!(store.get().unwrap().games, vec![game]);
    }

    #[test]
    fn add_game_rejects_unsafe_or_missing_paths() {
        let (tmp, store, ctx, icons) = setup();
        let root = ctx.layout.root().to_path_buf();
        assert!(add_manual_game(&store, &ctx, &icons, input("G", vec![]), "A").is_err());
        assert!(matches!(
            add_manual_game(&store, &ctx, &icons, input("G", vec![new_path(root.join("x"))]), "A"),
            Err(AppError::InvalidSavePath { .. })
        ));
        let mut i = input("G", vec![new_path(tmp.path().join("live"))]);
        i.exe_path = Some(tmp.path().join("missing.exe"));
        assert!(add_manual_game(&store, &ctx, &icons, i, "A").is_err());
        assert!(store.get().unwrap().games.is_empty());
    }

    #[test]
    fn editing_save_paths_keeps_profile_data_by_id() {
        let (tmp, store, ctx, icons) = setup();
        let (p1, p2) = (tmp.path().join("one"), tmp.path().join("two"));
        write_file(&p1.join("1.sav"), b"andrea one", 1);
        write_file(&p2.join("2.sav"), b"andrea two", 2);
        let game = add_manual_game(
            &store,
            &ctx,
            &icons,
            input("G", vec![new_path(p1.clone()), new_path(p2.clone())]),
            "Andrea",
        )
        .unwrap();
        let andrea = game.active_profile_id.unwrap();
        let silvia = create_profile(&store, &ctx, game.id, "Silvia", NewProfileSource::Fresh).unwrap();
        switch_profile(&store, &ctx, game.id, silvia.id, SwapOptions::default()).unwrap();

        // Remove the first save path: the second must keep its data.
        let keep_id = game.save_paths[1].id;
        let edited = update_game(
            &store,
            &ctx,
            &icons,
            game.id,
            input("G", vec![SavePathInput { id: Some(keep_id), path: p2.clone() }]),
            None,
        )
        .unwrap();
        assert_eq!(edited.save_paths.len(), 1);
        assert_eq!(edited.save_paths[0].id, keep_id);

        switch_profile(&store, &ctx, game.id, andrea, SwapOptions::default()).unwrap();
        assert_eq!(std::fs::read(p2.join("2.sav")).unwrap(), b"andrea two");
        // The removed path's stored copy is still kept in the profile.
        let removed = ctx.layout.profile_data_dir(game.id, andrea, game.save_paths[0].id);
        assert!(removed.join("1.sav").exists());

        assert!(update_game(
            &store,
            &ctx,
            &icons,
            game.id,
            input("G", vec![SavePathInput { id: Some(Uuid::new_v4()), path: p2 }]),
            None,
        )
        .is_err());
    }

    #[test]
    fn steam_game_uses_steam_icon_and_is_added_once() {
        let (tmp, store, ctx, icons) = setup();
        let install = tmp.path().join("steamapps/common/NMS");
        let cached = tmp.path().join("cache/fccb20f309ac5ae05b3cb4576a8a3357edf86f2e.jpg");
        std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
        std::fs::write(&cached, b"jpeg").unwrap();
        let source = GameSource::Steam {
            appid: 275850,
            install_dir: install.clone(),
        };
        let live = tmp.path().join("saves");
        let game = add_game(
            &store,
            &ctx,
            &icons,
            input("No Man's Sky", vec![new_path(live.clone())]),
            "Andrea",
            source.clone(),
            Some(&cached),
        )
        .unwrap();
        assert_eq!(game.source, source);
        assert_eq!(std::fs::read(game.icon.as_ref().unwrap()).unwrap(), b"jpeg");

        assert!(matches!(
            add_game(&store, &ctx, &icons, input("Again", vec![new_path(live)]), "A", source.clone(), None),
            Err(AppError::GameAlreadyAdded(name)) if name == "No Man's Sky"
        ));
        // A save folder must not contain the install folder.
        let mut other = source;
        if let GameSource::Steam { appid, .. } = &mut other {
            *appid = 1;
        }
        assert!(matches!(
            add_game(&store, &ctx, &icons, input("G", vec![new_path(tmp.path().join("steamapps"))]), "A", other, None),
            Err(AppError::InvalidSavePath { .. })
        ));
        assert_eq!(store.get().unwrap().games.len(), 1);
    }

    #[test]
    fn refresh_paths_moves_install_dirs_templates_and_relocated_saves() {
        let (tmp, store, mut ctx, icons) = setup();
        ctx.places = crate::manifest::placeholders::test_places(tmp.path());
        let roaming = ctx.places.app_data.clone().unwrap();
        let old_install = tmp.path().join("LibA/common/G");
        let source = |dir: &Path| GameSource::Steam {
            appid: 10,
            install_dir: dir.to_path_buf(),
        };
        let saves = roaming.join("Studio/G");
        let in_install = old_install.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        let game = add_game(
            &store,
            &ctx,
            &icons,
            input("G", vec![new_path(saves.clone()), new_path(in_install.clone())]),
            "A",
            source(&old_install),
            None,
        )
        .unwrap();
        assert_eq!(game.save_paths[0].template, "<winAppData>/Studio/G");
        assert_eq!(game.save_paths[1].template, "<base>/saves");

        let app = |dir: &Path| SteamApp {
            appid: 10,
            name: "G".into(),
            install_dir: dir.to_path_buf(),
        };
        let places = &ctx.places;
        assert_eq!(refresh_paths(&store, &[app(&old_install)], places).unwrap(), 0);
        assert_eq!(refresh_paths(&store, &[], places).unwrap(), 0);

        // Moved library: the install dir follows; the save folder inside it
        // only follows once it exists at the new place (the old one is gone).
        let new_install = tmp.path().join("LibB/common/G");
        assert_eq!(refresh_paths(&store, &[app(&new_install)], places).unwrap(), 1);
        let g = store.get().unwrap().games[0].clone();
        assert_eq!(g.source, source(&new_install));
        assert_eq!(g.save_paths[1].resolved, in_install);
        std::fs::create_dir_all(new_install.join("saves")).unwrap();
        assert_eq!(refresh_paths(&store, &[app(&new_install)], places).unwrap(), 1);
        let g = store.get().unwrap().games[0].clone();
        assert_eq!(g.save_paths[1].resolved, new_install.join("saves"));
        assert_eq!(g.save_paths[0].resolved, saves, "existing folders never move");

        // Old configs: templates were plain paths.
        store
            .update(|c| {
                c.games[0].save_paths[0].template = saves.display().to_string();
                Ok(())
            })
            .unwrap();
        assert_eq!(refresh_paths(&store, &[], places).unwrap(), 1);
        assert_eq!(store.get().unwrap().games[0].save_paths[0].template, "<winAppData>/Studio/G");
    }
}
