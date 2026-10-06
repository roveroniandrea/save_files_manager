//! Steam integration (Windows): locating the Steam install, its libraries and
//! installed games, users, library icons and Steam Cloud detection.
//!
//! Everything here only *reads* Steam's files. [`Steam::at`] works on any
//! folder, so tests run on fixtures; [`Steam::locate`] finds the real install.

pub mod cloud;
pub mod library;
pub mod users;
pub mod vdf;

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub use cloud::CloudStatus;
pub use library::SteamApp;
pub use users::SteamUser;

/// A Steam installation (the folder containing `steam.exe`).
#[derive(Debug, Clone)]
pub struct Steam {
    root: PathBuf,
}

impl Steam {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Steam { root: root.into() }
    }

    /// The Steam install of the current user (`HKCU\Software\Valve\Steam\SteamPath`,
    /// falling back to the default folder), if it exists.
    pub fn locate() -> Option<Self> {
        let candidates = [
            registry_steam_path(),
            std::env::var_os("ProgramFiles(x86)").map(|p| PathBuf::from(p).join("Steam")),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|p| p.join("steamapps").is_dir())
            .map(|p| Steam::at(pretty_path(&p)))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Folders a save folder must never be or contain: the Steam install,
    /// `userdata` and each account folder, every library and its `steamapps`
    /// and `steamapps\common` folders.
    pub fn protected_folders(&self) -> Vec<PathBuf> {
        let userdata = self.root.join("userdata");
        let mut folders = vec![self.root.clone(), userdata.clone()];
        folders.extend(self.userdata_ids().into_iter().map(|id| userdata.join(id.to_string())));
        for lib in self.libraries() {
            let steamapps = lib.join("steamapps");
            folders.push(steamapps.join("common"));
            folders.push(steamapps);
            folders.push(lib);
        }
        folders
    }

    /// Installed games, for the "Add Steam game" picker.
    pub fn scan(&self) -> SteamScan {
        let games = self
            .apps()
            .into_iter()
            .map(|app| {
                let icon = self
                    .library_icon(app.appid)
                    .and_then(|p| crate::icons::data_url(&p).ok());
                ScannedGame { app, icon }
            })
            .collect();
        SteamScan {
            root: Some(self.root.clone()),
            games,
        }
    }

    /// Small square icon of an app from Steam's library cache, if cached.
    pub fn library_icon(&self, appid: u32) -> Option<PathBuf> {
        let cache = self.root.join("appcache").join("librarycache");
        // Older clients: flat `<appid>_icon.jpg`.
        let flat = cache.join(format!("{appid}_icon.jpg"));
        if flat.is_file() {
            return Some(flat);
        }
        // Current clients: `<appid>/<sha1>.jpg` (the community icon, named by its hash).
        fs::read_dir(cache.join(appid.to_string()))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.is_file()
                    && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
                    && p.file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
            })
    }
}

/// Result of a library scan. `root` is `None` when Steam is not installed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SteamScan {
    pub root: Option<PathBuf>,
    pub games: Vec<ScannedGame>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScannedGame {
    #[serde(flatten)]
    pub app: SteamApp,
    /// Library icon as a `data:` URL.
    pub icon: Option<String>,
}

/// Reads and parses a VDF file; `None` if it is missing or malformed.
fn read_vdf(path: &Path) -> Option<vdf::Obj> {
    let text = fs::read_to_string(path).ok()?;
    match vdf::parse(&text) {
        Ok(doc) => Some(doc),
        Err(e) => {
            eprintln!("cannot parse {}: {e}", path.display());
            None
        }
    }
}

/// Real-case, backslash form of a path (the registry stores
/// `c:/program files (x86)/steam`).
fn pretty_path(path: &Path) -> PathBuf {
    let Ok(canonical) = fs::canonicalize(path) else {
        return PathBuf::from(path.to_string_lossy().replace('/', "\\"));
    };
    let s = canonical.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        // Keep the verbatim prefix for UNC paths (`\\?\UNC\...`).
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => canonical,
    }
}

#[cfg(windows)]
fn registry_steam_path() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    let mut buf = [0u16; 1024];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf` and `size` are valid for the call; `size` is its size in bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Valve\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16(&buf[..len]).ok()?;
    (!path.is_empty()).then(|| PathBuf::from(path.replace('/', "\\")))
}

#[cfg(not(windows))]
fn registry_steam_path() -> Option<PathBuf> {
    None
}

#[cfg(test)]
pub(crate) mod fixture {
    //! A fake Steam install on a temp dir.
    use std::fs;
    use std::path::Path;

    pub fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Steam root + a second library on `lib2`, with three apps.
    pub fn steam(root: &Path, lib2: &Path) {
        let esc = |p: &Path| p.display().to_string().replace('\\', "\\\\");
        write(
            &root.join("steamapps/libraryfolders.vdf"),
            &format!(
                "\"libraryfolders\"\n{{\n \"0\" {{ \"path\" \"{}\" }}\n \"1\" {{ \"path\" \"{}\" }}\n \"2\" {{ \"path\" \"Z:\\\\gone\" }}\n}}\n",
                esc(root),
                esc(lib2)
            ),
        );
        let acf = |appid: u32, name: &str, dir: &str| {
            format!("\"AppState\"\n{{\n \"appid\" \"{appid}\"\n \"name\" \"{name}\"\n \"installdir\" \"{dir}\"\n \"StateFlags\" \"4\"\n}}\n")
        };
        write(&root.join("steamapps/appmanifest_275850.acf"), &acf(275850, "No Man's Sky", "No Man's Sky"));
        write(
            &root.join("steamapps/appmanifest_228980.acf"),
            &acf(228980, "Steamworks Common Redistributables", "Steamworks Shared"),
        );
        write(&lib2.join("steamapps/appmanifest_367520.acf"), &acf(367520, "Hollow Knight", "Hollow Knight"));
        write(&lib2.join("steamapps/appmanifest_broken.acf"), "\"AppState\" {");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn finds_library_icon_in_both_layouts() {
        let tmp = tempdir().unwrap();
        let steam = Steam::at(tmp.path());
        let cache = tmp.path().join("appcache/librarycache");
        assert_eq!(steam.library_icon(1), None);

        let hashed = cache.join("2").join("fccb20f309ac5ae05b3cb4576a8a3357edf86f2e.jpg");
        fixture::write(&cache.join("2/header.jpg"), "x");
        fixture::write(&cache.join("2/0123/library_capsule.jpg"), "x");
        fixture::write(&hashed, "x");
        assert_eq!(steam.library_icon(2), Some(hashed));

        let flat = cache.join("3_icon.jpg");
        fixture::write(&flat, "x");
        assert_eq!(steam.library_icon(3), Some(flat));
    }

    #[test]
    fn protected_folders_cover_steam_and_libraries() {
        let tmp = tempdir().unwrap();
        let (root, lib2) = (tmp.path().join("Steam"), tmp.path().join("Lib2"));
        fixture::steam(&root, &lib2);
        std::fs::create_dir_all(root.join("userdata/42")).unwrap();
        let folders = Steam::at(&root).protected_folders();
        for f in [
            root.clone(),
            root.join("userdata"),
            root.join("userdata/42"),
            lib2.join("steamapps/common"),
            lib2.clone(),
        ] {
            assert!(folders.contains(&f), "{} missing", f.display());
        }
        let scan = Steam::at(&root).scan();
        assert_eq!(scan.games.len(), 2);
        assert_eq!(scan.root, Some(root));
    }

    /// Read-only check against the Steam install of this PC:
    /// `cargo test real_steam -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_steam_smoke() {
        let steam = Steam::locate().expect("Steam not found");
        println!("root: {}", steam.root().display());
        println!("libraries: {:?}", steam.libraries());
        for app in steam.apps() {
            println!(
                "{} {} {} icon={:?} cloud={:?}",
                app.appid,
                app.name,
                app.install_dir.display(),
                steam.library_icon(app.appid),
                steam.cloud_status(app.appid).accounts.iter().map(|a| a.account_id).collect::<Vec<_>>()
            );
        }
        println!("users: {:?}", steam.users().iter().map(|u| (u.account_id, u.most_recent)).collect::<Vec<_>>());
    }

    #[test]
    fn pretty_path_normalizes_separators() {
        let tmp = tempdir().unwrap();
        let forward = tmp.path().display().to_string().replace('\\', "/");
        let pretty = pretty_path(Path::new(&forward));
        assert!(!pretty.to_string_lossy().contains('/'));
        assert!(!pretty.to_string_lossy().starts_with(r"\\?\"));
        assert!(pretty.is_dir());
    }
}
