//! Starting a game: Steam games through `steam://rungameid/<appid>`, manual
//! games by running their exe with its folder as working directory. Both go
//! through the Windows shell (`ShellExecuteW`), so exes that need elevation
//! show the usual UAC prompt instead of failing.

use std::path::PathBuf;

use crate::config::{Game, GameSource};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    Url(String),
    Exe { exe: PathBuf, dir: Option<PathBuf> },
}

/// What launching `game` runs.
pub fn target(game: &Game) -> AppResult<LaunchTarget> {
    match (&game.source, &game.exe_path) {
        (GameSource::Steam { appid, .. }, _) => Ok(LaunchTarget::Url(format!("steam://rungameid/{appid}"))),
        (GameSource::Manual, Some(exe)) if exe.is_file() => Ok(LaunchTarget::Exe {
            exe: exe.clone(),
            dir: exe.parent().map(PathBuf::from),
        }),
        (GameSource::Manual, Some(exe)) => Err(AppError::LaunchFailed(format!("{} does not exist", exe.display()))),
        (GameSource::Manual, None) => Err(AppError::NoExecutable),
    }
}

/// Opens a folder in Explorer.
pub fn open_folder(dir: &std::path::Path) -> AppResult<()> {
    std::fs::create_dir_all(dir).map_err(|e| AppError::io(dir, e))?;
    shell_open(dir.as_os_str(), None)
}

pub fn launch(game: &Game) -> AppResult<()> {
    match target(game)? {
        LaunchTarget::Url(url) => shell_open(url.as_ref(), None),
        LaunchTarget::Exe { exe, dir } => shell_open(exe.as_os_str(), dir.as_deref().map(|d| d.as_os_str())),
    }
}

#[cfg(windows)]
fn shell_open(file: &std::ffi::OsStr, dir: Option<&std::ffi::OsStr>) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let file_w = wide(file);
    let dir_w = dir.map(wide);
    // SAFETY: all strings are NUL-terminated and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(file_w.as_ptr()),
            PCWSTR::null(),
            dir_w.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(AppError::LaunchFailed(format!(
            "could not start {} (shell error {code})",
            file.to_string_lossy()
        )))
    }
}

#[cfg(not(windows))]
fn shell_open(file: &std::ffi::OsStr, _dir: Option<&std::ffi::OsStr>) -> AppResult<()> {
    Err(AppError::LaunchFailed(format!(
        "launching is not supported on this platform ({})",
        file.to_string_lossy()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn launch_targets() {
        let steam = Game::new(
            "S",
            GameSource::Steam {
                appid: 275850,
                install_dir: PathBuf::from(r"D:\x"),
            },
        );
        assert_eq!(target(&steam).unwrap(), LaunchTarget::Url("steam://rungameid/275850".into()));

        let tmp = tempdir().unwrap();
        let exe = tmp.path().join("game.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let mut manual = Game::new("M", GameSource::Manual);
        assert!(matches!(target(&manual), Err(AppError::NoExecutable)));
        manual.exe_path = Some(tmp.path().join("missing.exe"));
        assert!(matches!(target(&manual), Err(AppError::LaunchFailed(_))));
        manual.exe_path = Some(exe.clone());
        assert_eq!(
            target(&manual).unwrap(),
            LaunchTarget::Exe {
                exe,
                dir: Some(tmp.path().to_path_buf())
            }
        );
    }
}
