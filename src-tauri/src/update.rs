//! App self-update through `tauri-plugin-updater` (milestone 12).
//!
//! `check` asks the release feed (`plugins.updater.endpoints` in
//! `tauri.conf.json`) for a newer version and remembers it; `install`
//! downloads it (the plugin verifies the signature against
//! `plugins.updater.pubkey`) and runs the installer **only while holding the
//! app-wide operation lock**, so an update never interrupts a switch, restore
//! or other operation on save/profile folders. On Windows the plugin exits the
//! process right after starting the installer (passive mode, which restarts
//! the app), so the lock is never released and no new operation can start.

use std::sync::Mutex;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::error::{AppError, AppResult};
use crate::swap::op_lock;

/// The update found by the last `check`, waiting for the user's confirmation.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// What the UI shows in the update dialog.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// Release notes (the changelog section of the release).
    pub notes: Option<String>,
    /// Publication date (RFC 3339), if the feed has one.
    pub date: Option<String>,
}

/// Progress of `install`, streamed to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DownloadEvent {
    /// A chunk arrived; `downloaded` is the running total in bytes.
    Progress { downloaded: u64, total: Option<u64> },
    /// Download done and signature verified; waiting for the operation lock.
    Downloaded,
    /// The lock is held and the installer is being started.
    Installing,
}

fn update_error(e: impl std::fmt::Display) -> AppError {
    AppError::Update(e.to_string())
}

fn info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone().filter(|n| !n.trim().is_empty()),
        date: update
            .date
            .and_then(|d| chrono::DateTime::from_timestamp(d.unix_timestamp(), 0))
            .map(|d| d.to_rfc3339()),
    }
}

/// Checks for a newer version. `None` = up to date.
pub async fn check(app: &AppHandle, pending: &PendingUpdate) -> AppResult<Option<UpdateInfo>> {
    let update = app.updater().map_err(update_error)?.check().await.map_err(update_error)?;
    let result = update.as_ref().map(info);
    *pending.0.lock().unwrap_or_else(|e| e.into_inner()) = update;
    Ok(result)
}

/// Downloads, verifies and installs the pending update. Returns only if the
/// installer did not exit the app (not on Windows) — the UI then restarts it.
pub async fn install(pending: &PendingUpdate, on_event: impl Fn(DownloadEvent) + Send + 'static) -> AppResult<()> {
    let update = pending
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| AppError::Update("no update is pending; check for updates first".into()))?;

    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                on_event(DownloadEvent::Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(update_error)?;
    on_event(DownloadEvent::Downloaded);

    // Waiting for the lock blocks, so do it off the async runtime's workers.
    tauri::async_runtime::spawn_blocking(move || {
        with_op_lock(|| {
            on_event(DownloadEvent::Installing);
            update.install(bytes).map_err(update_error)
        })
    })
    .await
    .map_err(update_error)?
}

/// Runs `f` while holding the app-wide operation lock (waits for a running
/// swap/restore/profile operation to finish first).
pub(crate) fn with_op_lock<T>(f: impl FnOnce() -> T) -> T {
    let _lock = op_lock();
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    #[test]
    fn install_waits_for_a_running_operation() {
        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        // A "swap" in progress holds the operation lock.
        let operation = std::thread::spawn(move || {
            let _lock = op_lock();
            locked_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        locked_rx.recv().unwrap();

        let installed = Arc::new(AtomicBool::new(false));
        let flag = installed.clone();
        let installer = std::thread::spawn(move || with_op_lock(|| flag.store(true, Ordering::SeqCst)));

        std::thread::sleep(Duration::from_millis(200));
        assert!(!installed.load(Ordering::SeqCst), "installed while an operation held the lock");

        release_tx.send(()).unwrap();
        operation.join().unwrap();
        installer.join().unwrap();
        assert!(installed.load(Ordering::SeqCst));
    }

    #[test]
    fn download_events_are_tagged() {
        let value = serde_json::to_value(DownloadEvent::Progress { downloaded: 5, total: Some(10) }).unwrap();
        assert_eq!(value["event"], "progress");
        assert_eq!(value["downloaded"], 5);
        assert_eq!(serde_json::to_value(DownloadEvent::Installing).unwrap()["event"], "installing");
    }

    #[test]
    fn update_errors_have_their_own_kind() {
        assert_eq!(update_error("offline").kind(), "update");
    }
}
