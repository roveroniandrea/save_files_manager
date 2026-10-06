//! Safety checks on save paths before a swap empties them.

use std::path::{Path, PathBuf};

use crate::config::{Game, GameSource};
use crate::error::{AppError, AppResult};

/// Folders a save path may never be (or contain), because emptying it would
/// destroy unrelated data: user/known folders, the app's own folders, the game
/// install folder.
#[derive(Debug, Clone, Default)]
pub struct Guards {
    /// A save path must not equal or contain these (it may be inside them).
    pub protected: Vec<PathBuf>,
    /// A save path must not equal, contain or be inside these (app data).
    pub reserved: Vec<PathBuf>,
}

impl Guards {
    /// Windows known folders of the current user.
    pub fn system_protected() -> Vec<PathBuf> {
        [
            dirs::home_dir(),
            dirs::config_dir(),
            dirs::data_local_dir(),
            dirs::document_dir(),
            dirs::desktop_dir(),
            dirs::download_dir(),
            dirs::picture_dir(),
            dirs::video_dir(),
            dirs::audio_dir(),
            dirs::public_dir(),
            std::env::var_os("ProgramData").map(PathBuf::from),
            std::env::var_os("ProgramFiles").map(PathBuf::from),
            std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
            std::env::var_os("SystemRoot").map(PathBuf::from),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Case-insensitive, separator-normalized form for comparisons (Windows paths).
fn norm(path: &Path) -> PathBuf {
    let s = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    PathBuf::from(s.trim_end_matches('\\'))
}

fn is_same_or_inside(path: &Path, base: &Path) -> bool {
    norm(path).starts_with(norm(base))
}

/// Returns the live save folders of `game` after checking they are safe to swap.
pub fn live_paths(game: &Game, guards: &Guards) -> AppResult<Vec<PathBuf>> {
    if game.save_paths.is_empty() {
        return Err(AppError::Other(format!("game '{}' has no save folders", game.name)));
    }
    let invalid = |path: &Path, reason: &str| {
        Err(AppError::InvalidSavePath {
            path: path.to_path_buf(),
            reason: reason.to_string(),
        })
    };

    let mut protected = guards.protected.clone();
    if let GameSource::Steam { install_dir, .. } = &game.source {
        protected.push(install_dir.clone());
    }
    if let Some(exe_dir) = game.exe_path.as_deref().and_then(Path::parent) {
        protected.push(exe_dir.to_path_buf());
    }

    let live: Vec<PathBuf> = game.save_paths.iter().map(|p| p.resolved.clone()).collect();
    for (i, path) in live.iter().enumerate() {
        if !path.is_absolute() {
            return invalid(path, "the path is not absolute");
        }
        if path.parent().is_none() || norm(path).parent().is_none_or(|p| p.as_os_str().is_empty()) {
            return invalid(path, "a drive root cannot be a save folder");
        }
        if let Some(p) = protected.iter().find(|p| is_same_or_inside(p, path)) {
            return invalid(path, &format!("it is or contains the protected folder {}", p.display()));
        }
        if let Some(r) = guards
            .reserved
            .iter()
            .find(|r| is_same_or_inside(r, path) || is_same_or_inside(path, r))
        {
            return invalid(path, &format!("it overlaps the app's data folder {}", r.display()));
        }
        if let Some(other) = live[..i]
            .iter()
            .find(|o| is_same_or_inside(o, path) || is_same_or_inside(path, o))
        {
            return invalid(path, &format!("it overlaps another save folder ({})", other.display()));
        }
    }
    Ok(live)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SavePath;

    fn game(paths: &[&str]) -> Game {
        let mut g = Game::new(
            "G",
            GameSource::Steam {
                appid: 1,
                install_dir: PathBuf::from(r"D:\Steam\steamapps\common\G"),
            },
        );
        g.save_paths = paths
            .iter()
            .map(|p| SavePath {
                template: p.to_string(),
                resolved: PathBuf::from(p),
            })
            .collect();
        g
    }

    fn guards() -> Guards {
        Guards {
            protected: vec![PathBuf::from(r"C:\Users\me"), PathBuf::from(r"C:\Users\me\Documents")],
            reserved: vec![PathBuf::from(r"C:\Users\me\AppData\Roaming\SaveFilesManager")],
        }
    }

    #[test]
    fn accepts_normal_save_folders() {
        let g = game(&[
            r"C:\Users\me\Documents\My Games\G",
            r"C:\Users\me\AppData\LocalLow\Studio\G",
            r"D:\Steam\steamapps\common\G\saves",
        ]);
        assert_eq!(live_paths(&g, &guards()).unwrap().len(), 3);
    }

    #[test]
    fn rejects_dangerous_paths() {
        let bad = [
            r"C:\",
            r"relative\path",
            r"C:\Users\me",
            r"c:/users/ME/",
            r"C:\Users",
            r"C:\Users\me\Documents",
            r"D:\Steam\steamapps\common\G",
            r"D:\Steam\steamapps\common",
            r"C:\Users\me\AppData\Roaming\SaveFilesManager\games",
            r"C:\Users\me\AppData\Roaming",
        ];
        for p in bad {
            let r = live_paths(&game(&[p]), &guards());
            assert!(matches!(r, Err(AppError::InvalidSavePath { .. })), "{p} should be rejected");
        }
    }

    #[test]
    fn rejects_exe_folder_overlaps_and_empty() {
        let mut g = game(&[r"E:\Games\Foo"]);
        g.exe_path = Some(PathBuf::from(r"E:\Games\Foo\foo.exe"));
        assert!(live_paths(&g, &guards()).is_err());

        let g = game(&[r"E:\Saves\A", r"E:\Saves\A\Sub"]);
        assert!(live_paths(&g, &guards()).is_err());
        let g = game(&[r"E:\Saves\A", r"E:\Saves\AB"]);
        assert!(live_paths(&g, &guards()).is_ok(), "prefix of a name is not nesting");

        assert!(live_paths(&game(&[]), &guards()).is_err());
    }
}
