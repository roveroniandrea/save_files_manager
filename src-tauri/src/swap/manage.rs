//! Profile and game lifecycle operations that touch profile folders.
//!
//! Ordering rule: data is written before the config references it, and the
//! config stops referencing data before it is deleted, so a failure never
//! leaves the config pointing at missing data.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::{clean_name, ConfigStore, Game, Profile};
use crate::error::{AppError, AppResult};

use super::{copy, op_lock, store_profile, validate, LivePath, SwapContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewProfileSource {
    /// No saves: the game starts a new save when this profile is loaded.
    Fresh,
    /// Copy of another profile's saves.
    Clone { profile_id: Uuid },
}

fn ensure_unique_name(game: &Game, name: &str, except: Option<Uuid>) -> AppResult<()> {
    let taken = game
        .profiles
        .iter()
        .any(|p| Some(p.id) != except && p.name.to_lowercase() == name.to_lowercase());
    if taken {
        Err(AppError::NameTaken(name.to_string()))
    } else {
        Ok(())
    }
}

fn ensure_idle(ctx: &SwapContext, game_id: Uuid) -> AppResult<()> {
    let journal = ctx.layout.journal_file(game_id);
    if journal.exists() {
        Err(AppError::OperationInProgress(journal))
    } else {
        Ok(())
    }
}

/// Creates a profile. A clone of the *active* profile copies the live save
/// folders (they hold its current state); a clone of another profile copies
/// its stored data.
pub fn create_profile(
    store: &ConfigStore,
    ctx: &SwapContext,
    game_id: Uuid,
    name: &str,
    source: NewProfileSource,
) -> AppResult<Profile> {
    let _lock = op_lock();
    let name = clean_name(name)?;
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    ensure_unique_name(game, &name, None)?;
    ensure_idle(ctx, game_id)?;
    let profile = Profile::new(name);
    let profile_dir = ctx.layout.profile_dir(game_id, profile.id);

    if let NewProfileSource::Clone { profile_id } = source {
        if game.profile(profile_id).is_none() {
            return Err(AppError::ProfileNotFound(profile_id));
        }
        let sources: Vec<LivePath> = if game.active_profile_id == Some(profile_id) {
            validate::live_paths(game, &ctx.guards)?
        } else {
            stored_paths(&ctx.layout.profile_dir(game_id, profile_id).join("data"))?
        };
        if let Err(e) = store_profile(&ctx.layout, game_id, profile.id, &sources) {
            let _ = copy::remove_path(&profile_dir);
            return Err(e);
        }
    }

    let added = profile.clone();
    let result = store.update(|c| {
        let g = c.game_mut(game_id).ok_or(AppError::GameNotFound(game_id))?;
        g.profiles.push(added);
        Ok(())
    });
    if let Err(e) = result {
        let _ = copy::remove_path(&profile_dir);
        return Err(e);
    }
    Ok(profile)
}

/// Every stored save folder of a profile (`data/<save-path-id>`).
fn stored_paths(data_dir: &Path) -> AppResult<Vec<LivePath>> {
    let entries = match fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(AppError::io(data_dir, e)),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| AppError::io(data_dir, e))?;
        if let Ok(id) = Uuid::parse_str(&entry.file_name().to_string_lossy()) {
            out.push(LivePath { id, path: entry.path() });
        }
    }
    Ok(out)
}

/// Renames a profile (config only: folders are named by id).
pub fn rename_profile(store: &ConfigStore, game_id: Uuid, profile_id: Uuid, name: &str) -> AppResult<()> {
    let name = clean_name(name)?;
    store.update(|c| {
        let g = c.game_mut(game_id).ok_or(AppError::GameNotFound(game_id))?;
        ensure_unique_name(g, &name, Some(profile_id))?;
        let p = g.profile_mut(profile_id).ok_or(AppError::ProfileNotFound(profile_id))?;
        p.name = name.clone();
        Ok(())
    })
}

/// Deletes a non-active profile and its stored saves.
pub fn delete_profile(store: &ConfigStore, ctx: &SwapContext, game_id: Uuid, profile_id: Uuid) -> AppResult<()> {
    let _lock = op_lock();
    ensure_idle(ctx, game_id)?;
    store.update(|c| {
        let g = c.game_mut(game_id).ok_or(AppError::GameNotFound(game_id))?;
        if g.profile(profile_id).is_none() {
            return Err(AppError::ProfileNotFound(profile_id));
        }
        if g.active_profile_id == Some(profile_id) {
            return Err(AppError::ProfileActive);
        }
        g.profiles.retain(|p| p.id != profile_id);
        Ok(())
    })?;
    // The config no longer references the folder; a failure here only leaves
    // unused files behind.
    if let Err(e) = copy::remove_path(&ctx.layout.profile_dir(game_id, profile_id)) {
        eprintln!("could not delete profile folder: {e}");
    }
    Ok(())
}

/// Removes a game from the app. Its live save folders are never touched.
/// `keep_data`: move the stored profiles/backups to `games/_orphaned/<id>`
/// instead of deleting them.
pub fn remove_game(store: &ConfigStore, ctx: &SwapContext, game_id: Uuid, keep_data: bool) -> AppResult<()> {
    let _lock = op_lock();
    ensure_idle(ctx, game_id)?;
    store.update(|c| {
        let before = c.games.len();
        c.games.retain(|g| g.id != game_id);
        if c.games.len() == before {
            return Err(AppError::GameNotFound(game_id));
        }
        Ok(())
    })?;
    let dir = ctx.layout.game_dir(game_id);
    if !dir.exists() {
        return Ok(());
    }
    let result = if keep_data {
        let orphaned = ctx.layout.orphaned_dir();
        let target = orphaned.join(format!("{}_{}", game_id, Utc::now().format("%Y%m%dT%H%M%SZ")));
        fs::create_dir_all(&orphaned)
            .and_then(|()| fs::rename(&dir, &target))
            .map_err(|e| AppError::io(&dir, e))
    } else {
        copy::remove_path(&dir)
    };
    if let Err(e) = result {
        eprintln!("could not clean up game folder: {e}");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileStats {
    pub profile_id: Uuid,
    /// Total size of the profile's saves in bytes.
    pub size_bytes: u64,
    /// Newest file modification time among the profile's saves.
    pub last_modified: Option<DateTime<Utc>>,
}

/// Size and last modification of each profile's saves: the live folders for
/// the active profile, the stored copy for the others.
pub fn profile_stats(store: &ConfigStore, ctx: &SwapContext, game_id: Uuid) -> AppResult<Vec<ProfileStats>> {
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    Ok(game
        .profiles
        .iter()
        .map(|p| {
            let dirs: Vec<_> = if game.active_profile_id == Some(p.id) {
                game.save_paths.iter().map(|s| s.resolved.clone()).collect()
            } else {
                game.save_paths
                    .iter()
                    .map(|s| ctx.layout.profile_data_dir(game_id, p.id, s.id))
                    .collect()
            };
            let (size_bytes, last_modified) = dirs.iter().map(|d| dir_stats(d)).fold(
                (0, None),
                |(size, newest): (u64, Option<DateTime<Utc>>), (s, n)| (size + s, newest.max(n)),
            );
            ProfileStats {
                profile_id: p.id,
                size_bytes,
                last_modified,
            }
        })
        .collect())
}

fn dir_stats(dir: &Path) -> (u64, Option<DateTime<Utc>>) {
    let mut size = 0;
    let mut newest = None;
    for entry in WalkDir::new(dir).into_iter().flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                size += meta.len();
                if let Ok(modified) = meta.modified() {
                    newest = newest.max(Some(DateTime::<Utc>::from(modified)));
                }
            }
        }
    }
    (size, newest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GameSource, SavePath};
    use crate::paths::StorageLayout;
    use crate::swap::test_util::{snapshot, write_file};
    use crate::swap::{switch_profile, Guards, SwapOptions};
    use tempfile::TempDir;

    struct Fx {
        tmp: TempDir,
        store: ConfigStore,
        ctx: SwapContext,
        game: Uuid,
        a: Uuid,
        live: std::path::PathBuf,
    }

    fn fx() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("storage");
        let live = tmp.path().join("live");
        write_file(&live.join("a.sav"), b"andrea", 1_600_000_000);
        let mut game = Game::new("G", GameSource::Manual);
        game.save_paths = vec![SavePath::new("x", &live)];
        let a = Profile::new("Andrea");
        game.active_profile_id = Some(a.id);
        let a_id = a.id;
        game.profiles.push(a);
        let game_id = game.id;
        let store = ConfigStore::open(tmp.path().join("config.json"));
        store
            .update(|c| {
                c.games.push(game);
                Ok(())
            })
            .unwrap();
        let ctx = SwapContext {
            layout: StorageLayout::new(&root),
            guards: Guards {
                protected: vec![],
                reserved: vec![root],
            },
        };
        Fx {
            tmp,
            store,
            ctx,
            game: game_id,
            a: a_id,
            live,
        }
    }

    fn switch(fx: &Fx, to: Uuid) {
        switch_profile(&fx.store, &fx.ctx, fx.game, to, SwapOptions::default()).unwrap();
    }

    #[test]
    fn fresh_profile_then_clone_of_active_and_inactive() {
        let fx = fx();
        let andrea = snapshot(&fx.live);
        let silvia = create_profile(&fx.store, &fx.ctx, fx.game, " Silvia ", NewProfileSource::Fresh).unwrap();
        assert_eq!(silvia.name, "Silvia");
        assert!(!fx.ctx.layout.profile_dir(fx.game, silvia.id).exists());

        // Clone of the active profile copies the live saves.
        let copy = create_profile(
            &fx.store,
            &fx.ctx,
            fx.game,
            "Andrea copy",
            NewProfileSource::Clone { profile_id: fx.a },
        )
        .unwrap();
        switch(&fx, copy.id);
        assert_eq!(snapshot(&fx.live), andrea);

        // Clone of an inactive profile copies its stored data.
        write_file(&fx.live.join("a.sav"), b"copy progressed", 1_700_000_000);
        switch(&fx, silvia.id);
        let second = create_profile(
            &fx.store,
            &fx.ctx,
            fx.game,
            "Second copy",
            NewProfileSource::Clone { profile_id: copy.id },
        )
        .unwrap();
        switch(&fx, second.id);
        assert_eq!(fs::read(fx.live.join("a.sav")).unwrap(), b"copy progressed");
    }

    #[test]
    fn names_must_be_unique_and_non_empty() {
        let fx = fx();
        assert!(matches!(
            create_profile(&fx.store, &fx.ctx, fx.game, "andrea", NewProfileSource::Fresh),
            Err(AppError::NameTaken(_))
        ));
        assert!(matches!(
            create_profile(&fx.store, &fx.ctx, fx.game, "  ", NewProfileSource::Fresh),
            Err(AppError::EmptyName)
        ));
        let s = create_profile(&fx.store, &fx.ctx, fx.game, "Silvia", NewProfileSource::Fresh).unwrap();
        assert!(matches!(
            rename_profile(&fx.store, fx.game, s.id, "ANDREA"),
            Err(AppError::NameTaken(_))
        ));
        rename_profile(&fx.store, fx.game, s.id, "Silvia 2").unwrap();
        rename_profile(&fx.store, fx.game, s.id, "silvia 2").unwrap(); // own name, other case
        let config = fx.store.get().unwrap();
        assert_eq!(config.game(fx.game).unwrap().profile(s.id).unwrap().name, "silvia 2");
    }

    #[test]
    fn delete_profile_removes_data_but_never_the_active_one() {
        let fx = fx();
        let s = create_profile(&fx.store, &fx.ctx, fx.game, "Silvia", NewProfileSource::Fresh).unwrap();
        switch(&fx, s.id);
        assert!(matches!(
            delete_profile(&fx.store, &fx.ctx, fx.game, s.id),
            Err(AppError::ProfileActive)
        ));
        let a_dir = fx.ctx.layout.profile_dir(fx.game, fx.a);
        assert!(a_dir.exists());
        delete_profile(&fx.store, &fx.ctx, fx.game, fx.a).unwrap();
        assert!(!a_dir.exists());
        assert!(fx.store.get().unwrap().game(fx.game).unwrap().profile(fx.a).is_none());
    }

    #[test]
    fn remove_game_keeps_or_deletes_data_and_never_touches_live() {
        for keep in [true, false] {
            let fx = fx();
            let s = create_profile(&fx.store, &fx.ctx, fx.game, "Silvia", NewProfileSource::Fresh).unwrap();
            switch(&fx, s.id);
            write_file(&fx.live.join("s.sav"), b"silvia", 1_700_000_000);
            let live = snapshot(&fx.live);

            remove_game(&fx.store, &fx.ctx, fx.game, keep).unwrap();
            assert!(fx.store.get().unwrap().games.is_empty());
            assert!(!fx.ctx.layout.game_dir(fx.game).exists());
            assert_eq!(snapshot(&fx.live), live);
            let orphaned: Vec<_> = fs::read_dir(fx.ctx.layout.orphaned_dir())
                .map(|d| d.flatten().collect())
                .unwrap_or_default();
            assert_eq!(orphaned.len(), usize::from(keep));
            if keep {
                let data = orphaned[0].path().join("profiles").join(fx.a.to_string()).join("data");
                assert!(data.exists());
            }
            drop(fx.tmp);
        }
    }

    #[test]
    fn stats_use_live_for_active_and_store_for_others() {
        let fx = fx();
        let s = create_profile(&fx.store, &fx.ctx, fx.game, "Silvia", NewProfileSource::Fresh).unwrap();
        let stats = profile_stats(&fx.store, &fx.ctx, fx.game).unwrap();
        assert_eq!(stats[0].size_bytes, 6);
        assert_eq!(stats[0].last_modified.unwrap().timestamp(), 1_600_000_000);
        assert_eq!(stats[1].size_bytes, 0);
        assert_eq!(stats[1].last_modified, None);

        switch(&fx, s.id);
        let stats = profile_stats(&fx.store, &fx.ctx, fx.game).unwrap();
        assert_eq!(stats[0].size_bytes, 6, "now from the stored copy");
        assert_eq!(stats[1].size_bytes, 0);
    }

    #[test]
    fn operations_are_refused_while_a_journal_exists() {
        let fx = fx();
        let journal = fx.ctx.layout.journal_file(fx.game);
        fs::create_dir_all(journal.parent().unwrap()).unwrap();
        fs::write(&journal, "{}").unwrap();
        assert!(matches!(
            create_profile(&fx.store, &fx.ctx, fx.game, "S", NewProfileSource::Fresh),
            Err(AppError::OperationInProgress(_))
        ));
        assert!(matches!(
            remove_game(&fx.store, &fx.ctx, fx.game, true),
            Err(AppError::OperationInProgress(_))
        ));
    }
}
