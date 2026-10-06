//! Running-game detection (Windows).
//!
//! A game is running when a process's executable is the game's exe (manual
//! games) or lives under the game's install folder (Steam games). Process
//! paths come from `EnumProcesses` + `QueryFullProcessImageNameW`; processes
//! that cannot be queried (protected/system) are skipped.

use std::path::PathBuf;

use crate::config::{Game, GameSource};
use crate::swap::validate::is_same_or_inside;

/// Whether one of `exes` belongs to `game`. Manual games without an exe
/// cannot be detected.
pub fn game_matches(game: &Game, exes: &[PathBuf]) -> bool {
    match &game.source {
        GameSource::Steam { install_dir, .. } => exes.iter().any(|e| is_same_or_inside(e, install_dir)),
        GameSource::Manual => match &game.exe_path {
            Some(exe) => exes.iter().any(|e| is_same_or_inside(e, exe) && is_same_or_inside(exe, e)),
            None => false,
        },
    }
}

/// Whether `game` is running now.
pub fn is_running(game: &Game) -> bool {
    game_matches(game, &running_exes())
}

/// Executable paths of the processes this user can query.
#[cfg(windows)]
pub fn running_exes() -> Vec<PathBuf> {
    win::running_exes()
}

#[cfg(not(windows))]
pub fn running_exes() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(windows)]
mod win {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    pub fn running_exes() -> Vec<PathBuf> {
        let mut pids = vec![0u32; 4096];
        loop {
            let mut used = 0u32;
            let size = (pids.len() * std::mem::size_of::<u32>()) as u32;
            // SAFETY: `pids` is valid for `size` bytes; `used` is a valid out pointer.
            if unsafe { EnumProcesses(pids.as_mut_ptr(), size, &mut used) }.is_err() {
                return Vec::new();
            }
            let count = used as usize / std::mem::size_of::<u32>();
            if count < pids.len() {
                pids.truncate(count);
                break;
            }
            pids.resize(pids.len() * 2, 0);
        }
        pids.into_iter().filter(|&pid| pid != 0).filter_map(exe_of).collect()
    }

    fn exe_of(pid: u32) -> Option<PathBuf> {
        // SAFETY: plain FFI call; the handle is closed below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` holds `len` u16s; `handle` is valid.
        let ok = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
        // SAFETY: `handle` was opened above.
        let _ = unsafe { CloseHandle(handle) };
        ok.ok()?;
        Some(PathBuf::from(OsString::from_wide(&buf[..len as usize])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_steam_by_install_dir_and_manual_by_exe() {
        let steam = Game::new(
            "S",
            GameSource::Steam {
                appid: 1,
                install_dir: PathBuf::from(r"D:\Steam\steamapps\common\Game"),
            },
        );
        let mut manual = Game::new("M", GameSource::Manual);
        manual.exe_path = Some(PathBuf::from(r"E:\Games\M\m.exe"));
        let exes = |list: &[&str]| list.iter().map(PathBuf::from).collect::<Vec<_>>();

        assert!(game_matches(&steam, &exes(&[r"d:\steam\steamapps\common\game\bin\x64\game.exe"])));
        assert!(!game_matches(&steam, &exes(&[r"D:\Steam\steamapps\common\GameTwo\g.exe"])));
        assert!(game_matches(&manual, &exes(&[r"C:\x.exe", r"e:\games\m\M.EXE"])));
        assert!(!game_matches(&manual, &exes(&[r"E:\Games\M\launcher.exe"])));
        manual.exe_path = None;
        assert!(!game_matches(&manual, &exes(&[r"E:\Games\M\m.exe"])));
    }

    #[cfg(windows)]
    #[test]
    fn sees_the_current_process() {
        let me = std::env::current_exe().unwrap();
        let exes = running_exes();
        assert!(
            exes.iter().any(|e| is_same_or_inside(e, &me) && is_same_or_inside(&me, e)),
            "{} not in {} processes",
            me.display(),
            exes.len()
        );
    }
}
