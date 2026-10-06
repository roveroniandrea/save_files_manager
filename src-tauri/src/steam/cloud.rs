//! Steam Cloud detection (heuristic).
//!
//! An account uses Steam Cloud for an app when `userdata/<id>/<appid>/` has a
//! `remotecache.vdf` or a non-empty `remote` folder, unless cloud sync was
//! turned off for the app (`CloudEnabled "0"` under `apps/<appid>` in
//! `7/remote/sharedconfig.vdf` or `config/localconfig.vdf`) or globally
//! (`CloudEnabled "0"` directly under `Software/Valve/Steam` in those files).

use std::fs;
use std::path::Path;

use serde::Serialize;

use super::{read_vdf, Steam};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloudAccount {
    pub account_id: u32,
    pub persona_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CloudStatus {
    /// Accounts on this PC for which Steam Cloud syncs this app's saves.
    pub accounts: Vec<CloudAccount>,
}

impl CloudStatus {
    pub fn enabled(&self) -> bool {
        !self.accounts.is_empty()
    }
}

fn has_entries(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}

/// `Some(false)` if a config file turns cloud off for the app (or globally).
fn cloud_setting(user_dir: &Path, appid: u32) -> Option<bool> {
    let files = [
        (user_dir.join("7").join("remote").join("sharedconfig.vdf"), "UserRoamingConfigStore"),
        (user_dir.join("config").join("localconfig.vdf"), "UserLocalConfigStore"),
    ];
    let mut setting = None;
    for (file, root) in files {
        let Some(doc) = read_vdf(&file) else { continue };
        let Some(steam) = doc.path(&[root, "Software", "Valve", "Steam"]) else { continue };
        let app = steam.path(&["apps", &appid.to_string()]);
        for value in [steam.str("CloudEnabled"), app.and_then(|a| a.str("CloudEnabled"))]
            .into_iter()
            .flatten()
        {
            if value == "0" {
                return Some(false);
            }
            setting = Some(true);
        }
    }
    setting
}

impl Steam {
    pub fn cloud_status(&self, appid: u32) -> CloudStatus {
        let users = self.users();
        let accounts = self
            .userdata_ids()
            .into_iter()
            .filter(|&id| {
                let user_dir = self.root.join("userdata").join(id.to_string());
                let app_dir = user_dir.join(appid.to_string());
                let used = app_dir.join("remotecache.vdf").is_file() || has_entries(&app_dir.join("remote"));
                used && cloud_setting(&user_dir, appid) != Some(false)
            })
            .map(|account_id| CloudAccount {
                account_id,
                persona_name: users
                    .iter()
                    .find(|u| u.account_id == account_id)
                    .and_then(|u| u.persona_name.clone()),
            })
            .collect();
        CloudStatus { accounts }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam::fixture::write;
    use tempfile::tempdir;

    fn shared(apps: &str) -> String {
        format!(
            "\"UserRoamingConfigStore\" {{ \"Software\" {{ \"Valve\" {{ \"Steam\" {{ \"apps\" {{ {apps} }} }} }} }} }}"
        )
    }

    #[test]
    fn detects_cloud_per_account() {
        let tmp = tempdir().unwrap();
        let steam = Steam::at(tmp.path());
        let ud = tmp.path().join("userdata");
        assert!(!steam.cloud_status(10).enabled());

        // Account 1: remotecache.vdf → cloud on.
        write(&ud.join("1/10/remotecache.vdf"), "\"10\" { }");
        // Account 2: non-empty remote folder, but cloud disabled for the app.
        write(&ud.join("2/10/remote/save.dat"), "x");
        write(&ud.join("2/7/remote/sharedconfig.vdf"), &shared("\"10\" { \"CloudEnabled\" \"0\" }"));
        // Account 3: empty remote folder only → not in use.
        fs::create_dir_all(ud.join("3/10/remote")).unwrap();
        write(
            &tmp.path().join("config/loginusers.vdf"),
            "users { 76561197960265729 { PersonaName Andrea } }",
        );

        let status = steam.cloud_status(10);
        assert_eq!(
            status.accounts,
            vec![CloudAccount {
                account_id: 1,
                persona_name: Some("Andrea".into())
            }]
        );
        assert!(!steam.cloud_status(11).enabled());
    }

    #[test]
    fn global_cloud_off_wins() {
        let tmp = tempdir().unwrap();
        let ud = tmp.path().join("userdata");
        write(&ud.join("5/10/remotecache.vdf"), "\"10\" { }");
        write(
            &ud.join("5/config/localconfig.vdf"),
            "UserLocalConfigStore { Software { Valve { Steam { CloudEnabled 0 } } } }",
        );
        assert!(!Steam::at(tmp.path()).cloud_status(10).enabled());
    }
}
