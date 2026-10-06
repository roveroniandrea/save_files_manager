//! Steam accounts that logged in on this PC (`config/loginusers.vdf`) and the
//! per-account `userdata/<account-id>` folders.

use std::fs;

use serde::Serialize;

use super::{read_vdf, Steam};

/// Offset between a SteamID64 and the 32-bit account id (`userdata` folder name).
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SteamUser {
    pub steam_id64: u64,
    /// Account id ("id3"), the name of the `userdata` folder.
    pub account_id: u32,
    pub persona_name: Option<String>,
    pub most_recent: bool,
    /// Last login (Unix seconds).
    pub timestamp: u64,
}

impl Steam {
    /// Known accounts, most recent first.
    pub fn users(&self) -> Vec<SteamUser> {
        let doc = read_vdf(&self.root.join("config").join("loginusers.vdf"));
        let Some(users) = doc.as_ref().and_then(|d| d.obj("users")) else {
            return Vec::new();
        };
        let mut list: Vec<SteamUser> = users
            .objects()
            .filter_map(|(id, u)| {
                let steam_id64: u64 = id.parse().ok()?;
                let account_id = u32::try_from(steam_id64.checked_sub(STEAM_ID64_BASE)?).ok()?;
                Some(SteamUser {
                    steam_id64,
                    account_id,
                    persona_name: u.str("PersonaName").map(str::to_string),
                    most_recent: u.str("MostRecent") == Some("1"),
                    timestamp: u.str("Timestamp").and_then(|t| t.parse().ok()).unwrap_or(0),
                })
            })
            .collect();
        list.sort_by_key(|u| (!u.most_recent, std::cmp::Reverse(u.timestamp)));
        list
    }

    /// Account ids that have a `userdata` folder.
    pub fn userdata_ids(&self) -> Vec<u32> {
        let Ok(entries) = fs::read_dir(self.root.join("userdata")) else {
            return Vec::new();
        };
        let mut ids: Vec<u32> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str()?.parse().ok())
            .filter(|&id| id != 0)
            .collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam::fixture::write;
    use tempfile::tempdir;

    #[test]
    fn reads_users_most_recent_first() {
        let tmp = tempdir().unwrap();
        write(
            &tmp.path().join("config/loginusers.vdf"),
            r#""users"
{
	"76561198000000001" { "PersonaName" "Old" "MostRecent" "0" "Timestamp" "100" }
	"76561198271609977" { "PersonaName" "Andrea" "MostRecent" "1" "Timestamp" "50" }
	"76561198000000002" { "Timestamp" "200" }
	"garbage" { }
}"#,
        );
        let users = Steam::at(tmp.path()).users();
        assert_eq!(users.len(), 3);
        assert_eq!(users[0].account_id, 311_344_249);
        assert_eq!(users[0].persona_name.as_deref(), Some("Andrea"));
        assert_eq!(users[1].timestamp, 200);
        assert_eq!(users[2].persona_name.as_deref(), Some("Old"));
    }

    #[test]
    fn lists_userdata_ids() {
        let tmp = tempdir().unwrap();
        for d in ["userdata/311344249", "userdata/12", "userdata/0", "userdata/anonymous"] {
            std::fs::create_dir_all(tmp.path().join(d)).unwrap();
        }
        assert_eq!(Steam::at(tmp.path()).userdata_ids(), vec![12, 311_344_249]);
        assert!(Steam::at(tmp.path().join("x")).users().is_empty());
    }
}
