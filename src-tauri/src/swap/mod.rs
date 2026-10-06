//! Profile swap engine. See "Swap algorithm" in `docs/SPEC.md`.
//!
//! Invariant: live save folders are only modified after a complete backup of
//! them exists and the journal says `Loading`. Any failure from then on
//! restores the live folders from that backup; if even that fails, the journal
//! is kept and [`recover_all`] retries at the next startup.

pub mod backup;
pub mod copy;
pub mod journal;
pub mod manage;
pub mod validate;

#[cfg(test)]
pub mod test_util;

use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::{ConfigStore, Game};
use crate::error::{AppError, AppResult};
use crate::manifest::placeholders::Places;
use crate::paths::{AppPaths, StorageLayout};

use backup::{BackupMeta, BackupReason};
pub use journal::LivePath;
use journal::{Journal, Operation, Step};
pub use validate::Guards;

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct SwapOptions {
    /// Proceed even if live folders are empty while the active profile has
    /// stored data (the user confirmed `LiveSavesEmpty`).
    #[serde(default)]
    pub allow_empty_live: bool,
}

/// Everything the engine needs besides the config.
#[derive(Debug, Clone)]
pub struct SwapContext {
    pub layout: StorageLayout,
    pub guards: Guards,
    /// Known folders, for save path templates.
    pub places: Places,
}

impl SwapContext {
    pub fn new(paths: &AppPaths, store: &ConfigStore) -> AppResult<Self> {
        let layout = paths.storage(&store.get()?.settings);
        let steam = crate::steam::Steam::locate();
        let mut protected = Guards::system_protected();
        if let Some(steam) = &steam {
            protected.extend(steam.protected_folders());
        }
        let guards = Guards {
            protected,
            reserved: vec![paths.app_dir().to_path_buf(), layout.root().to_path_buf()],
        };
        let places = Places::from_system(steam.map(|s| s.root().to_path_buf()));
        Ok(SwapContext { layout, guards, places })
    }
}

/// Switches the active profile of `game_id` to `to`.
pub fn switch_profile(
    store: &ConfigStore,
    ctx: &SwapContext,
    game_id: Uuid,
    to: Uuid,
    opts: SwapOptions,
) -> AppResult<()> {
    let _lock = op_lock();
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    let from = active_profile(game)?;
    if game.profile(to).is_none() {
        return Err(AppError::ProfileNotFound(to));
    }
    if to == from {
        return Ok(());
    }
    let live = validate::live_paths(game, &ctx.guards)?;
    check_live_not_empty(&ctx.layout, game_id, from, &live, opts)?;

    let journal_path = ctx.layout.journal_file(game_id);
    let mut journal = new_journal(Operation::Switch, game_id, from, Some(to), live);
    journal.create(&journal_path)?;

    let result = (|| -> AppResult<()> {
        let backup_dir = ctx.layout.backups_dir(game_id).join(&journal.backup);
        let meta = BackupMeta {
            profile_id: from,
            created_at: journal.started_at,
            reason: BackupReason::Switch,
            save_path_ids: journal.save_paths.iter().map(|l| l.id).collect(),
        };
        backup::create(&backup_dir, &journal.save_paths, &meta)?;
        fail_point("after_backup")?;

        journal.set_step(&journal_path, Step::Storing)?;
        // The active profile's stored copy is refreshed from the backup just taken
        // (identical to live, and avoids reading the live folders twice).
        let sources: Vec<LivePath> = journal
            .save_paths
            .iter()
            .map(|l| LivePath {
                id: l.id,
                path: backup_dir.join(l.id.to_string()),
            })
            .collect();
        store_profile(&ctx.layout, game_id, from, &sources)?;

        journal.set_step(&journal_path, Step::Loading)?;
        for live in &journal.save_paths {
            fail_point("load_path")?;
            copy::replace_dir_contents(&ctx.layout.profile_data_dir(game_id, to, live.id), &live.path)?;
        }
        journal.set_step(&journal_path, Step::Loaded)?;
        fail_point("before_commit")?;

        store.update(|c| {
            let g = c.game_mut(game_id).ok_or(AppError::GameNotFound(game_id))?;
            if g.active_profile_id != Some(from) {
                return Err(AppError::Other("the active profile changed during the swap".into()));
            }
            g.active_profile_id = Some(to);
            let now = Utc::now();
            for id in [from, to] {
                if let Some(p) = g.profile_mut(id) {
                    p.last_used_at = Some(now);
                }
            }
            Ok(())
        })
    })();
    if let Err(e) = result {
        return Err(abort(&ctx.layout, &journal_path, &mut journal, e));
    }
    finish(store, ctx, game_id, &journal_path);
    Ok(())
}

/// Restores backup `backup_id` into the live folders of `game_id`. Only
/// backups of the currently active profile can be restored; the live content
/// is itself backed up first, so a restore can be undone. Save folders that are
/// not in the backup (added later) are left untouched.
pub fn restore_backup(store: &ConfigStore, ctx: &SwapContext, game_id: Uuid, backup_id: &str) -> AppResult<()> {
    let _lock = op_lock();
    let config = store.get()?;
    let game = config.game(game_id).ok_or(AppError::GameNotFound(game_id))?;
    let active = active_profile(game)?;
    let live = validate::live_paths(game, &ctx.guards)?;
    let backups_dir = ctx.layout.backups_dir(game_id);
    let (source, meta) = backup::find(&backups_dir, backup_id)?;
    if meta.profile_id != active {
        return Err(AppError::Other(
            "this backup belongs to a different profile than the active one; switch to that profile first".into(),
        ));
    }
    let live: Vec<LivePath> = live
        .into_iter()
        .filter(|l| meta.save_path_ids.contains(&l.id))
        .collect();
    if live.is_empty() {
        return Err(AppError::Other(
            "this backup contains none of the game's current save folders".into(),
        ));
    }

    let journal_path = ctx.layout.journal_file(game_id);
    let mut journal = new_journal(Operation::Restore, game_id, active, None, live);
    journal.create(&journal_path)?;

    let result = (|| -> AppResult<()> {
        let meta = BackupMeta {
            profile_id: active,
            created_at: journal.started_at,
            reason: BackupReason::Restore,
            save_path_ids: journal.save_paths.iter().map(|l| l.id).collect(),
        };
        backup::create(&backups_dir.join(&journal.backup), &journal.save_paths, &meta)?;
        journal.set_step(&journal_path, Step::Loading)?;
        for live in &journal.save_paths {
            fail_point("load_path")?;
            copy::replace_dir_contents(&source.join(live.id.to_string()), &live.path)?;
        }
        journal.set_step(&journal_path, Step::Loaded)?;
        Ok(())
    })();
    if let Err(e) = result {
        return Err(abort(&ctx.layout, &journal_path, &mut journal, e));
    }
    finish(store, ctx, game_id, &journal_path);
    Ok(())
}

/// Complete backups of a game, newest first.
pub fn list_backups(ctx: &SwapContext, game_id: Uuid) -> AppResult<Vec<backup::BackupInfo>> {
    backup::list(&ctx.layout.backups_dir(game_id))
}

static OP_LOCK: Mutex<()> = Mutex::new(());

/// Serializes every operation that touches save or profile folders, so two
/// commands (running on different threads) never interleave.
pub(crate) fn op_lock() -> MutexGuard<'static, ()> {
    OP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn active_profile(game: &Game) -> AppResult<Uuid> {
    game.active_profile_id
        .ok_or_else(|| AppError::Other(format!("game '{}' has no active profile", game.name)))
}

fn new_journal(operation: Operation, game_id: Uuid, from: Uuid, to: Option<Uuid>, live: Vec<LivePath>) -> Journal {
    let now = Utc::now();
    Journal {
        operation,
        game_id,
        from,
        to,
        backup: backup::backup_name(now, from),
        save_paths: live,
        step: Step::BackingUp,
        started_at: now,
    }
}

/// Refuses to overwrite the active profile's stored data with empty live folders.
fn check_live_not_empty(
    layout: &StorageLayout,
    game_id: Uuid,
    active: Uuid,
    live: &[LivePath],
    opts: SwapOptions,
) -> AppResult<()> {
    if opts.allow_empty_live {
        return Ok(());
    }
    let mut empty = Vec::new();
    for live in live {
        if copy::is_empty_or_missing(&live.path)?
            && !copy::is_empty_or_missing(&layout.profile_data_dir(game_id, active, live.id))?
        {
            empty.push(live.path.clone());
        }
    }
    if empty.is_empty() {
        Ok(())
    } else {
        Err(AppError::LiveSavesEmpty(empty))
    }
}

/// Replaces `profiles/<profile>/data/<id>` with copies of `sources` (keyed by
/// save path id), via `staging` and `trash` folders so the old data is only
/// discarded once the new copy is complete. Stored folders of save paths not
/// in `sources` (removed from the game) are carried over, never dropped.
fn store_profile(layout: &StorageLayout, game_id: Uuid, profile: Uuid, sources: &[LivePath]) -> AppResult<()> {
    let dir = layout.profile_dir(game_id, profile);
    let (data, staging, trash) = (dir.join("data"), dir.join("staging"), dir.join("trash"));
    repair_profile_store(layout, game_id, profile)?;
    for src in sources {
        copy::copy_dir_contents(&src.path, &staging.join(src.id.to_string()))?;
    }
    if let Ok(entries) = fs::read_dir(&data) {
        for entry in entries.flatten() {
            let target = staging.join(entry.file_name());
            if !target.exists() {
                copy::copy_dir_contents(&entry.path(), &target)?;
            }
        }
    }
    fail_point("store_swap")?;
    if data.exists() {
        fs::rename(&data, &trash).map_err(|e| AppError::io(&data, e))?;
    }
    fs::rename(&staging, &data).map_err(|e| AppError::io(&staging, e))?;
    // A leftover trash folder is harmless and removed on the next store.
    let _ = copy::remove_path(&trash);
    Ok(())
}

/// Brings a profile's store back to a consistent `data` folder after an
/// interrupted [`store_profile`].
fn repair_profile_store(layout: &StorageLayout, game_id: Uuid, profile: Uuid) -> AppResult<()> {
    let dir = layout.profile_dir(game_id, profile);
    let (data, staging, trash) = (dir.join("data"), dir.join("staging"), dir.join("trash"));
    if !data.exists() && trash.exists() {
        fs::rename(&trash, &data).map_err(|e| AppError::io(&trash, e))?;
    }
    copy::remove_path(&staging)?;
    copy::remove_path(&trash)
}

/// Undoes the file changes of an interrupted operation: live folders are
/// restored from the pre-operation backup if they may have been touched, the
/// `from` profile's store is repaired, a partial backup is removed. Does not
/// remove the journal.
fn roll_back_files(layout: &StorageLayout, journal_path: &Path, journal: &mut Journal) -> AppResult<()> {
    let backup_dir = layout.backups_dir(journal.game_id).join(&journal.backup);
    match journal.step {
        Step::BackingUp => copy::remove_path(&backup_dir)?,
        Step::Storing => repair_profile_store(layout, journal.game_id, journal.from)?,
        Step::Loading | Step::Loaded => {
            if journal.step == Step::Loaded {
                // Live folders are about to change again: a crash during the
                // rollback must not be mistaken for a finished operation.
                journal.set_step(journal_path, Step::Loading)?;
            }
            if backup::find(&layout.backups_dir(journal.game_id), &journal.backup).is_err() {
                return Err(AppError::BackupNotFound(journal.backup.clone()));
            }
            for live in &journal.save_paths {
                fail_point("rollback_path")?;
                copy::replace_dir_contents(&backup_dir.join(live.id.to_string()), &live.path)?;
            }
            if journal.operation == Operation::Switch {
                repair_profile_store(layout, journal.game_id, journal.from)?;
            }
        }
    }
    Ok(())
}

/// Rolls back after `cause`; returns the error to surface.
fn abort(layout: &StorageLayout, journal_path: &Path, journal: &mut Journal, cause: AppError) -> AppError {
    match roll_back_files(layout, journal_path, journal).and_then(|()| Journal::remove(journal_path)) {
        Ok(()) => cause,
        Err(rollback) => AppError::RollbackFailed {
            cause: cause.to_string(),
            rollback: rollback.to_string(),
        },
    }
}

/// Post-success cleanup. Failures here never affect the saves, so they are
/// not reported as errors: a leftover `Loaded` journal is completed by
/// recovery, and pruning is retried after the next operation.
fn finish(store: &ConfigStore, ctx: &SwapContext, game_id: Uuid, journal_path: &Path) {
    if let Err(e) = Journal::remove(journal_path) {
        eprintln!("could not remove journal: {e}");
    }
    let keep = store
        .get()
        .ok()
        .and_then(|c| c.game(game_id).map(|g| c.backup_limit_for(g)))
        .unwrap_or(crate::config::DEFAULT_BACKUP_LIMIT);
    if let Err(e) = backup::prune(&ctx.layout.backups_dir(game_id), keep as usize) {
        eprintln!("could not prune backups: {e}");
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    /// Live folders were restored from the pre-operation backup; `from` is active.
    RolledBack,
    /// The operation had finished copying; it was completed.
    Completed,
    /// Recovery failed; it will be retried at the next startup.
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoveryReport {
    pub game_id: Uuid,
    /// Game name from the config, if the game still exists.
    pub game_name: Option<String>,
    pub outcome: RecoveryOutcome,
    pub error: Option<String>,
}

/// Recovers every interrupted operation found under the storage root.
/// Called once at startup, before the UI can start new operations.
pub fn recover_all(store: &ConfigStore, layout: &StorageLayout) -> Vec<RecoveryReport> {
    let _lock = op_lock();
    let games_dir = layout.root().join("games");
    let Ok(entries) = fs::read_dir(&games_dir) else {
        return Vec::new();
    };
    let mut reports = Vec::new();
    for entry in entries.flatten() {
        let journal_path = entry.path().join("swap.journal.json");
        if !journal_path.is_file() {
            continue;
        }
        let game_id = Uuid::parse_str(&entry.file_name().to_string_lossy()).unwrap_or_default();
        let (outcome, error) = match recover_one(store, layout, &journal_path) {
            Ok(outcome) => (outcome, None),
            Err(e) => (RecoveryOutcome::Failed, Some(e.to_string())),
        };
        let game_name = store
            .get()
            .ok()
            .and_then(|c| c.game(game_id).map(|g| g.name.clone()));
        reports.push(RecoveryReport {
            game_id,
            game_name,
            outcome,
            error,
        });
    }
    reports
}

fn recover_one(store: &ConfigStore, layout: &StorageLayout, journal_path: &Path) -> AppResult<RecoveryOutcome> {
    let Some(mut journal) = Journal::read(journal_path)? else {
        return Ok(RecoveryOutcome::Completed);
    };
    // Once everything is copied, finishing is safer than undoing: the user may
    // already have played on the new content.
    let (outcome, active) = if journal.step == Step::Loaded {
        let active = match journal.operation {
            Operation::Switch => journal.to.unwrap_or(journal.from),
            Operation::Restore => journal.from,
        };
        (RecoveryOutcome::Completed, active)
    } else {
        roll_back_files(layout, journal_path, &mut journal)?;
        (RecoveryOutcome::RolledBack, journal.from)
    };
    store.update(|c| {
        if let Some(g) = c.game_mut(journal.game_id) {
            if g.profile(active).is_some() && g.active_profile_id != Some(active) {
                g.active_profile_id = Some(active);
                if let Some(p) = g.profile_mut(active) {
                    p.last_used_at = Some(Utc::now());
                }
            }
        }
        Ok(())
    })?;
    Journal::remove(journal_path)?;
    Ok(outcome)
}

// --- test fault injection -------------------------------------------------

#[cfg(test)]
thread_local! {
    static FAIL_AT: std::cell::RefCell<Option<(&'static str, usize)>> = const { std::cell::RefCell::new(None) };
}

/// In tests, fails the `nth` (1-based) time the named point is reached.
#[cfg(test)]
pub fn set_fail_point(name: &'static str, nth: usize) {
    FAIL_AT.with(|f| *f.borrow_mut() = Some((name, nth)));
}

#[cfg(test)]
pub fn clear_fail_point() {
    FAIL_AT.with(|f| *f.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn fail_point(name: &'static str) -> AppResult<()> {
    FAIL_AT.with(|f| {
        let mut f = f.borrow_mut();
        if let Some((point, remaining)) = f.as_mut() {
            if *point == name {
                *remaining -= 1;
                if *remaining == 0 {
                    *f = None;
                    return Err(AppError::Other(format!("injected failure at {name}")));
                }
            }
        }
        Ok(())
    })
}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn fail_point(_name: &'static str) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests;
