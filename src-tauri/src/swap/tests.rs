//! Swap engine tests. Everything happens inside a `tempfile` dir.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use uuid::Uuid;

use super::journal::{Journal, Operation, Step};
use super::test_util::{set_readonly, snapshot, write_file};
use super::*;
use crate::config::{ConfigStore, Game, GameSource, Profile, SavePath};

struct Fx {
    _tmp: TempDir,
    store: ConfigStore,
    ctx: SwapContext,
    game: Uuid,
    a: Uuid,
    b: Uuid,
    live: Vec<PathBuf>,
    /// Save path ids, parallel to `live`.
    ids: Vec<Uuid>,
}

impl Fx {
    /// Game with two save folders and profiles A (active, owns the live saves) and B (fresh).
    fn new() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("storage");
        let live = vec![tmp.path().join("live0"), tmp.path().join("live1")];
        write_file(&live[0].join("slot1.sav"), b"andrea slot 1", 1_600_000_000);
        write_file(&live[0].join("sub/deep/slot2.sav"), b"andrea slot 2", 1_600_000_100);
        write_file(&live[1].join("settings.ini"), b"volume=3", 1_600_000_200);
        set_readonly(&live[1].join("settings.ini"));

        let mut game = Game::new("Test Game", GameSource::Manual);
        game.save_paths = live.iter().map(|p| SavePath::new(p.display().to_string(), p)).collect();
        let ids = game.save_paths.iter().map(|p| p.id).collect();
        let (a, b) = (Profile::new("Andrea"), Profile::new("Silvia"));
        let (a_id, b_id) = (a.id, b.id);
        game.active_profile_id = Some(a_id);
        game.profiles = vec![a, b];
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
            _tmp: tmp,
            store,
            ctx,
            game: game_id,
            a: a_id,
            b: b_id,
            live,
            ids,
        }
    }

    fn live_paths(&self) -> Vec<LivePath> {
        self.ids
            .iter()
            .zip(&self.live)
            .map(|(id, path)| LivePath { id: *id, path: path.clone() })
            .collect()
    }

    fn switch(&self, to: Uuid) -> AppResult<()> {
        switch_profile(&self.store, &self.ctx, self.game, to, SwapOptions::default())
    }

    fn active(&self) -> Uuid {
        self.store.get().unwrap().game(self.game).unwrap().active_profile_id.unwrap()
    }

    fn live_snapshot(&self) -> Vec<BTreeMapSnap> {
        self.live.iter().map(|p| snapshot(p)).collect()
    }

    fn stored(&self, profile: Uuid) -> Vec<BTreeMapSnap> {
        self.ids
            .iter()
            .map(|id| snapshot(&self.ctx.layout.profile_data_dir(self.game, profile, *id)))
            .collect()
    }

    fn journal(&self) -> PathBuf {
        self.ctx.layout.journal_file(self.game)
    }

    fn backups(&self) -> Vec<backup::BackupInfo> {
        list_backups(&self.ctx, self.game).unwrap()
    }

    /// No journal, no staging/trash leftovers.
    fn assert_clean(&self) {
        assert!(!self.journal().exists(), "journal left behind");
        for p in [self.a, self.b] {
            let dir = self.ctx.layout.profile_dir(self.game, p);
            assert!(!dir.join("staging").exists() && !dir.join("trash").exists());
        }
    }
}

type BTreeMapSnap = std::collections::BTreeMap<String, test_util::Entry>;

#[test]
fn round_trip_preserves_bytes_times_and_attributes() {
    let fx = Fx::new();
    let andrea = fx.live_snapshot();

    // A -> B: B is fresh, so live folders become empty (but still exist).
    fx.switch(fx.b).unwrap();
    assert_eq!(fx.active(), fx.b);
    for p in &fx.live {
        assert!(p.is_dir());
        assert!(snapshot(p).is_empty());
    }
    assert_eq!(fx.stored(fx.a), andrea);
    fx.assert_clean();

    // Silvia plays.
    write_file(&fx.live[0].join("slot1.sav"), b"silvia slot 1", 1_700_000_000);
    let silvia = fx.live_snapshot();

    fx.switch(fx.a).unwrap();
    assert_eq!(fx.active(), fx.a);
    assert_eq!(fx.live_snapshot(), andrea);
    assert_eq!(fx.stored(fx.b), silvia);

    fx.switch(fx.b).unwrap();
    assert_eq!(fx.live_snapshot(), silvia);
    fx.assert_clean();

    let config = fx.store.get().unwrap();
    let game = config.game(fx.game).unwrap();
    assert!(game.profiles.iter().all(|p| p.last_used_at.is_some()));
    assert_eq!(fx.backups().len(), 3);
}

#[test]
fn switching_to_active_profile_is_a_no_op_and_unknown_profile_fails() {
    let fx = Fx::new();
    fx.switch(fx.a).unwrap();
    assert!(fx.backups().is_empty());
    assert!(matches!(fx.switch(Uuid::new_v4()), Err(AppError::ProfileNotFound(_))));
    assert!(matches!(
        switch_profile(&fx.store, &fx.ctx, Uuid::new_v4(), fx.a, SwapOptions::default()),
        Err(AppError::GameNotFound(_))
    ));
}

#[test]
fn backups_are_pruned_to_the_game_limit() {
    let fx = Fx::new();
    fx.store
        .update(|c| {
            c.game_mut(fx.game).unwrap().backup_limit = Some(2);
            Ok(())
        })
        .unwrap();
    for to in [fx.b, fx.a, fx.b, fx.a] {
        fx.switch(to).unwrap();
    }
    let backups = fx.backups();
    assert_eq!(backups.len(), 2);
    // Newest backup is of B's live saves (taken when switching B -> A).
    assert_eq!(backups[0].meta.profile_id, fx.b);
}

#[test]
fn empty_live_folders_require_confirmation() {
    let fx = Fx::new();
    fx.switch(fx.b).unwrap();
    // B has no stored data, so empty live folders are fine.
    fx.switch(fx.a).unwrap();

    // Andrea's live saves vanish (e.g. deleted outside the app).
    for p in &fx.live {
        copy::clear_dir(p).unwrap();
    }
    let stored_a = fx.stored(fx.a);
    let backups_before = fx.backups().len();
    match fx.switch(fx.b) {
        Err(AppError::LiveSavesEmpty(paths)) => assert_eq!(paths, fx.live),
        other => panic!("expected LiveSavesEmpty, got {other:?}"),
    }
    assert_eq!(fx.active(), fx.a);
    assert_eq!(fx.stored(fx.a), stored_a, "stored data untouched");
    assert_eq!(fx.backups().len(), backups_before);
    fx.assert_clean();

    let opts = SwapOptions {
        allow_empty_live: true,
    };
    switch_profile(&fx.store, &fx.ctx, fx.game, fx.b, opts).unwrap();
    assert_eq!(fx.active(), fx.b);
}

#[test]
fn failure_at_any_step_rolls_back() {
    // (fail point, nth hit)
    let points = [
        ("after_backup", 1),
        ("store_swap", 1),
        ("load_path", 1),
        ("load_path", 2), // first live folder already replaced
        ("before_commit", 1),
        ("copy_file", 1), // during the backup
        ("copy_file", 5), // during the store (3 live files)
    ];
    for (point, nth) in points {
        let fx = Fx::new();
        // Give both profiles stored data so every step has work to do.
        fx.switch(fx.b).unwrap();
        write_file(&fx.live[0].join("silvia.sav"), b"silvia", 1_700_000_000);
        write_file(&fx.live[1].join("silvia.ini"), b"s", 1_700_000_000);
        fx.switch(fx.a).unwrap();
        let live = fx.live_snapshot();
        let stored_a = fx.stored(fx.a);
        let stored_b = fx.stored(fx.b);
        let backups = fx.backups().len();

        set_fail_point(point, nth);
        let result = fx.switch(fx.b);
        clear_fail_point();

        assert!(result.is_err(), "{point}#{nth}: expected failure");
        assert!(!matches!(result, Err(AppError::RollbackFailed { .. })), "{point}#{nth}");
        assert_eq!(fx.active(), fx.a, "{point}#{nth}");
        assert_eq!(fx.live_snapshot(), live, "{point}#{nth}: live restored");
        assert_eq!(fx.stored(fx.a), stored_a, "{point}#{nth}");
        assert_eq!(fx.stored(fx.b), stored_b, "{point}#{nth}");
        fx.assert_clean();
        // The pre-swap backup is kept unless the backup itself failed.
        assert!(fx.backups().len() >= backups, "{point}#{nth}");

        // The game still swaps normally afterwards.
        fx.switch(fx.b).unwrap();
        assert_eq!(fx.live_snapshot(), stored_b);
    }
}

#[test]
fn failure_mid_load_copy_rolls_back() {
    let fx = Fx::new();
    fx.switch(fx.b).unwrap();
    write_file(&fx.live[0].join("s1.sav"), b"1", 1_700_000_000);
    write_file(&fx.live[0].join("s2.sav"), b"2", 1_700_000_000);
    fx.switch(fx.a).unwrap();
    let live = fx.live_snapshot();

    // 3 files backed up + 3 stored, then the 2nd file copied into live fails.
    set_fail_point("copy_file", 3 + 3 + 2);
    let result = fx.switch(fx.b);
    clear_fail_point();
    assert!(result.is_err());
    assert_eq!(fx.live_snapshot(), live);
    assert_eq!(fx.active(), fx.a);
    fx.assert_clean();
}

#[cfg(windows)]
fn open_locked(path: &std::path::Path, share: u32) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new().read(true).share_mode(share).open(path).unwrap()
}

/// A file in the target profile that cannot be read makes the load fail; the
/// live folders are restored.
#[cfg(windows)]
#[test]
fn unreadable_target_file_rolls_back() {
    let fx = Fx::new();
    fx.switch(fx.b).unwrap();
    write_file(&fx.live[0].join("silvia.sav"), b"silvia", 1_700_000_000);
    fx.switch(fx.a).unwrap();
    let live = fx.live_snapshot();

    let stored = fx.ctx.layout.profile_data_dir(fx.game, fx.b, fx.ids[0]).join("silvia.sav");
    let lock = open_locked(&stored, 0);
    let result = fx.switch(fx.b);
    drop(lock);

    assert!(matches!(result, Err(AppError::Io { .. })), "{result:?}");
    assert_eq!(fx.live_snapshot(), live);
    assert_eq!(fx.active(), fx.a);
    fx.assert_clean();
}

/// A live file that cannot be deleted (game still holding it) makes both the
/// load and the rollback fail: the journal is kept and startup recovery
/// restores everything once the file is released.
#[cfg(windows)]
#[test]
fn failed_rollback_is_recovered_at_startup() {
    const FILE_SHARE_READ: u32 = 1;
    let fx = Fx::new();
    let live = fx.live_snapshot();

    let lock = open_locked(&fx.live[0].join("slot1.sav"), FILE_SHARE_READ);
    let result = fx.switch(fx.b);
    assert!(matches!(result, Err(AppError::RollbackFailed { .. })), "{result:?}");
    assert!(fx.journal().exists(), "journal kept for recovery");
    assert_eq!(fx.active(), fx.a);
    // New operations are refused until recovery ran.
    assert!(matches!(fx.switch(fx.b), Err(AppError::OperationInProgress(_))));
    drop(lock);

    let reports = recover_all(&fx.store, &fx.ctx.layout);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].outcome, RecoveryOutcome::RolledBack);
    assert_eq!(reports[0].game_name.as_deref(), Some("Test Game"));
    assert_eq!(fx.live_snapshot(), live);
    assert_eq!(fx.active(), fx.a);
    fx.assert_clean();

    fx.switch(fx.b).unwrap();
    assert_eq!(fx.stored(fx.a), live);
}

fn crash_journal(fx: &Fx, step: Step, backup: &str) -> Journal {
    let mut journal = Journal {
        operation: Operation::Switch,
        game_id: fx.game,
        from: fx.a,
        to: Some(fx.b),
        backup: backup.to_string(),
        save_paths: fx.live_paths(),
        step: Step::BackingUp,
        started_at: chrono::Utc::now(),
    };
    journal.create(&fx.journal()).unwrap();
    journal.set_step(&fx.journal(), step).unwrap();
    journal
}

#[test]
fn recovery_of_crash_during_backup_removes_partial_backup() {
    let fx = Fx::new();
    let live = fx.live_snapshot();
    let partial = fx.ctx.layout.backups_dir(fx.game).join("partial_backup");
    write_file(&partial.join("0/slot1.sav"), b"andrea slot 1", 1);
    crash_journal(&fx, Step::BackingUp, "partial_backup");

    let reports = recover_all(&fx.store, &fx.ctx.layout);
    assert_eq!(reports[0].outcome, RecoveryOutcome::RolledBack);
    assert!(!partial.exists());
    assert_eq!(fx.live_snapshot(), live);
    assert_eq!(fx.active(), fx.a);
    fx.assert_clean();
}

#[test]
fn recovery_of_crash_during_store_repairs_profile() {
    let fx = Fx::new();
    // Give A stored data, then simulate a crash between the two renames of a store.
    fx.switch(fx.b).unwrap();
    fx.switch(fx.a).unwrap();
    let stored_a = fx.stored(fx.a);
    let dir = fx.ctx.layout.profile_dir(fx.game, fx.a);
    fs::rename(dir.join("data"), dir.join("trash")).unwrap();
    write_file(&dir.join("staging/0/half.sav"), b"half", 1);
    let backup = fx.backups()[0].id.clone();
    crash_journal(&fx, Step::Storing, &backup);
    let live = fx.live_snapshot();

    let reports = recover_all(&fx.store, &fx.ctx.layout);
    assert_eq!(reports[0].outcome, RecoveryOutcome::RolledBack);
    assert_eq!(fx.stored(fx.a), stored_a);
    assert_eq!(fx.live_snapshot(), live);
    fx.assert_clean();
}

#[test]
fn recovery_of_crash_during_load_restores_live() {
    let fx = Fx::new();
    let live = fx.live_snapshot();
    // Simulate: backup taken, then the crash happened while live was half cleared.
    let name = backup::backup_name(chrono::Utc::now(), fx.a);
    let meta = backup::BackupMeta {
        profile_id: fx.a,
        created_at: chrono::Utc::now(),
        reason: backup::BackupReason::Switch,
        save_path_ids: fx.ids.clone(),
    };
    backup::create(&fx.ctx.layout.backups_dir(fx.game).join(&name), &fx.live_paths(), &meta).unwrap();
    crash_journal(&fx, Step::Loading, &name);
    copy::clear_dir(&fx.live[0]).unwrap();

    let reports = recover_all(&fx.store, &fx.ctx.layout);
    assert_eq!(reports[0].outcome, RecoveryOutcome::RolledBack);
    assert_eq!(fx.live_snapshot(), live);
    assert_eq!(fx.active(), fx.a);
    fx.assert_clean();
}

#[test]
fn recovery_after_full_copy_completes_the_switch() {
    let fx = Fx::new();
    fx.switch(fx.b).unwrap();
    write_file(&fx.live[0].join("silvia.sav"), b"silvia", 1_700_000_000);
    let live = fx.live_snapshot();
    // Simulate: live already holds B's data but the config update was lost.
    fx.store
        .update(|c| {
            c.game_mut(fx.game).unwrap().active_profile_id = Some(fx.a);
            Ok(())
        })
        .unwrap();
    let backup = fx.backups()[0].id.clone();
    crash_journal(&fx, Step::Loaded, &backup);

    let reports = recover_all(&fx.store, &fx.ctx.layout);
    assert_eq!(reports[0].outcome, RecoveryOutcome::Completed);
    assert_eq!(fx.active(), fx.b);
    assert_eq!(fx.live_snapshot(), live, "progress made after the switch is kept");
    fx.assert_clean();
}

#[test]
fn recovery_with_no_journals_does_nothing() {
    let fx = Fx::new();
    assert!(recover_all(&fx.store, &fx.ctx.layout).is_empty());
    fx.switch(fx.b).unwrap();
    assert!(recover_all(&fx.store, &fx.ctx.layout).is_empty());
}

#[test]
fn restore_backup_of_active_profile() {
    let fx = Fx::new();
    let andrea = fx.live_snapshot();
    fx.switch(fx.b).unwrap(); // backup of A's saves
    fx.switch(fx.a).unwrap(); // backup of B's (empty) saves
    // Andrea's save gets corrupted.
    write_file(&fx.live[0].join("slot1.sav"), b"corrupted", 1_800_000_000);
    let corrupted = fx.live_snapshot();

    let backups = fx.backups();
    let of_a = backups.iter().find(|b| b.meta.profile_id == fx.a).unwrap();
    let of_b = backups.iter().find(|b| b.meta.profile_id == fx.b).unwrap();

    assert!(restore_backup(&fx.store, &fx.ctx, fx.game, &of_b.id).is_err());
    assert_eq!(fx.live_snapshot(), corrupted);
    assert!(matches!(
        restore_backup(&fx.store, &fx.ctx, fx.game, "nope"),
        Err(AppError::BackupNotFound(_))
    ));

    restore_backup(&fx.store, &fx.ctx, fx.game, &of_a.id).unwrap();
    assert_eq!(fx.live_snapshot(), andrea);
    assert_eq!(fx.active(), fx.a);
    fx.assert_clean();

    // The corrupted state was backed up first, so the restore can be undone.
    let newest = &fx.backups()[0];
    assert_eq!(newest.meta.reason, backup::BackupReason::Restore);
    restore_backup(&fx.store, &fx.ctx, fx.game, &newest.id.clone()).unwrap();
    assert_eq!(fx.live_snapshot(), corrupted);
}

#[test]
fn restore_failure_rolls_back() {
    let fx = Fx::new();
    fx.switch(fx.b).unwrap();
    fx.switch(fx.a).unwrap();
    write_file(&fx.live[0].join("slot1.sav"), b"newer", 1_800_000_000);
    let live = fx.live_snapshot();
    let of_a = fx
        .backups()
        .into_iter()
        .find(|b| b.meta.profile_id == fx.a)
        .unwrap();

    set_fail_point("load_path", 2);
    let result = restore_backup(&fx.store, &fx.ctx, fx.game, &of_a.id);
    clear_fail_point();
    assert!(result.is_err());
    assert_eq!(fx.live_snapshot(), live);
    fx.assert_clean();
}

#[test]
fn unsafe_save_path_is_refused_before_touching_anything() {
    let fx = Fx::new();
    let root = fx.ctx.layout.root().to_path_buf();
    fx.store
        .update(|c| {
            c.game_mut(fx.game).unwrap().save_paths[1].resolved = root.join("games");
            Ok(())
        })
        .unwrap();
    let live = fx.live_snapshot();
    assert!(matches!(fx.switch(fx.b), Err(AppError::InvalidSavePath { .. })));
    assert_eq!(fx.live_snapshot(), live);
    assert!(fx.backups().is_empty());
}
