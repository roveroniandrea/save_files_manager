//! Steam libraries (`steamapps/libraryfolders.vdf`) and installed apps
//! (`appmanifest_<appid>.acf`).

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use super::{read_vdf, Steam};

/// An installed Steam app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SteamApp {
    pub appid: u32,
    pub name: String,
    /// `<library>\steamapps\common\<installdir>`.
    pub install_dir: PathBuf,
}

/// Apps that are not games: redistributables, compatibility tools, runtimes.
const TOOL_APPIDS: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime 1.0 (scout)
    1391110, // Steam Linux Runtime 2.0 (soldier)
    1628350, // Steam Linux Runtime 3.0 (sniper)
    1493710, // Proton Experimental
    1826330, // Proton EasyAntiCheat Runtime
    1161040, // Proton BattlEye Runtime
    250820,  // SteamVR
];

const TOOL_NAME_PREFIXES: &[&str] = &["Proton ", "Steam Linux Runtime", "Steamworks Common Redistributables"];

pub fn is_tool(appid: u32, name: &str) -> bool {
    TOOL_APPIDS.contains(&appid) || TOOL_NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

impl Steam {
    /// Library folders (the Steam root first), existing ones only.
    pub fn libraries(&self) -> Vec<PathBuf> {
        let mut libs = vec![self.root.clone()];
        let vdf = read_vdf(&self.root.join("steamapps").join("libraryfolders.vdf"));
        if let Some(folders) = vdf.as_ref().and_then(|d| d.obj("libraryfolders")) {
            for (_, folder) in folders.objects() {
                let Some(path) = folder.str("path") else { continue };
                let path = PathBuf::from(path);
                let known = libs
                    .iter()
                    .any(|l| l.to_string_lossy().eq_ignore_ascii_case(&path.to_string_lossy()));
                if !known && path.join("steamapps").is_dir() {
                    libs.push(path);
                }
            }
        }
        libs
    }

    /// Installed games (tools filtered out), sorted by name.
    pub fn apps(&self) -> Vec<SteamApp> {
        let mut apps: Vec<SteamApp> = Vec::new();
        for lib in self.libraries() {
            let steamapps = lib.join("steamapps");
            let Ok(entries) = fs::read_dir(&steamapps) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                    continue;
                }
                let Some(app) = read_vdf(&entry.path()).and_then(|d| {
                    let state = d.obj("AppState")?;
                    Some(SteamApp {
                        appid: state.str("appid")?.parse().ok()?,
                        name: state.str("name")?.trim().to_string(),
                        install_dir: steamapps.join("common").join(state.str("installdir")?),
                    })
                }) else {
                    continue;
                };
                // The same app can appear twice while it is being moved between libraries.
                if !is_tool(app.appid, &app.name) && !app.name.is_empty() && !apps.iter().any(|a| a.appid == app.appid)
                {
                    apps.push(app);
                }
            }
        }
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps
    }

    pub fn app(&self, appid: u32) -> Option<SteamApp> {
        self.apps().into_iter().find(|a| a.appid == appid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam::fixture;
    use tempfile::tempdir;

    #[test]
    fn scans_all_libraries_and_skips_tools() {
        let tmp = tempdir().unwrap();
        let (root, lib2) = (tmp.path().join("Steam"), tmp.path().join("Lib2"));
        fixture::steam(&root, &lib2);
        let steam = Steam::at(&root);
        assert_eq!(steam.libraries(), vec![root.clone(), lib2.clone()]);

        let apps = steam.apps();
        assert_eq!(
            apps,
            vec![
                SteamApp {
                    appid: 367520,
                    name: "Hollow Knight".into(),
                    install_dir: lib2.join("steamapps/common/Hollow Knight"),
                },
                SteamApp {
                    appid: 275850,
                    name: "No Man's Sky".into(),
                    install_dir: root.join("steamapps/common/No Man's Sky"),
                },
            ]
        );
        assert_eq!(steam.app(275850).unwrap().name, "No Man's Sky");
        assert_eq!(steam.app(228980), None);
    }

    #[test]
    fn missing_library_file_still_scans_root() {
        let tmp = tempdir().unwrap();
        fixture::write(
            &tmp.path().join("steamapps/appmanifest_10.acf"),
            "AppState { appid 10 name \"Counter-Strike\" installdir \"Half-Life\" }",
        );
        assert_eq!(Steam::at(tmp.path()).apps().len(), 1);
        assert!(Steam::at(tmp.path().join("nope")).apps().is_empty());
    }

    #[test]
    fn tool_filter() {
        assert!(is_tool(228980, "Steamworks Common Redistributables"));
        assert!(is_tool(1, "Proton 9.0"));
        assert!(!is_tool(275850, "No Man's Sky"));
    }
}
