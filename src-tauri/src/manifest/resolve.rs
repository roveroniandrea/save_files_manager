//! Turns a game's manifest entries into save folder suggestions.
//!
//! Each Windows `save` (and optionally `config`) entry is expanded; globs are
//! matched on disk; a matched file stands for its containing folder (the swap
//! unit is a folder). Folders nested in another suggestion are dropped, and
//! each suggestion is checked with the same safety rules as a swap.

use std::path::{Path, PathBuf};

use glob::MatchOptions;
use serde::Serialize;

use super::index::IndexGame;
use super::placeholders::{glob_pattern, to_template, GameContext, Places};
use crate::swap::validate::{check_folder, is_same_or_inside};

/// Matches considered per manifest path (guards against huge globs).
const MAX_MATCHES: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestion {
    /// Placeholder form, e.g. `<winAppData>/HelloGames/NMS`.
    pub template: String,
    pub path: PathBuf,
    /// The folder exists on this PC.
    pub exists: bool,
    /// Tagged `save` / `config` in the manifest.
    pub save: bool,
    pub config: bool,
    /// Why it cannot be used as a save folder (safety checks), if so.
    pub problem: Option<String>,
}

pub struct SuggestOptions<'a> {
    pub places: &'a Places,
    /// Placeholders for expanding manifest paths.
    pub expand: &'a GameContext,
    /// Placeholders for the stored templates (see `GameContext::for_game`).
    pub template: &'a GameContext,
    /// Steam game: entries limited to other stores are skipped. Otherwise
    /// entries limited to Steam are skipped.
    pub steam: bool,
    pub include_config: bool,
    pub protected: &'a [PathBuf],
    pub reserved: &'a [PathBuf],
}

fn has_glob(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// "save.dat", "profile.sav" look like files; "HelloGames.NMS_x1y2z3" does not.
fn looks_like_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| (1..=4).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Folders on this PC (or expected) for one expanded pattern: `(folder, exists)`.
fn folders_for(pattern: &str) -> Vec<(PathBuf, bool)> {
    let folder_of = |p: PathBuf| -> Option<(PathBuf, bool)> {
        if p.is_dir() {
            Some((p, true))
        } else {
            p.parent().map(|d| (d.to_path_buf(), true))
        }
    };
    if has_glob(pattern) {
        let options = MatchOptions {
            case_sensitive: false,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        };
        return match glob::glob_with(pattern, options) {
            Ok(paths) => paths.flatten().take(MAX_MATCHES).filter_map(folder_of).collect(),
            Err(_) => Vec::new(),
        };
    }
    let path = PathBuf::from(pattern);
    if path.exists() {
        folder_of(path).into_iter().collect()
    } else if looks_like_file(&path) {
        path.parent().map(|d| (d.to_path_buf(), false)).into_iter().collect()
    } else {
        vec![(path, false)]
    }
}

fn same(a: &Path, b: &Path) -> bool {
    is_same_or_inside(a, b) && is_same_or_inside(b, a)
}

pub fn suggest(game: &IndexGame, opts: &SuggestOptions) -> Vec<Suggestion> {
    let mut found: Vec<Suggestion> = Vec::new();
    for file in &game.files {
        if !(file.save || (file.config && opts.include_config)) {
            continue;
        }
        let store_ok = file.stores.is_empty()
            || if opts.steam {
                file.stores.iter().any(|s| s == "steam")
            } else {
                file.stores.iter().any(|s| s != "steam")
            };
        if !store_ok {
            continue;
        }
        let Some(pattern) = glob_pattern(&file.path, opts.places, opts.expand) else {
            continue;
        };
        for (folder, exists) in folders_for(&pattern) {
            if let Some(s) = found.iter_mut().find(|s| same(&s.path, &folder)) {
                s.save |= file.save;
                s.config |= file.config;
                s.exists |= exists;
                continue;
            }
            found.push(Suggestion {
                template: to_template(&folder, opts.places, opts.template),
                path: folder,
                exists,
                save: file.save,
                config: file.config,
                problem: None,
            });
        }
    }

    // A folder inside another suggestion is covered by it.
    let all = found.clone();
    found.retain(|s| {
        !all.iter()
            .any(|outer| !same(&outer.path, &s.path) && is_same_or_inside(&s.path, &outer.path))
    });
    for s in &mut found {
        s.problem = check_folder(&s.path, opts.protected, opts.reserved).err();
    }
    found.sort_by_key(|s| {
        (
            s.problem.is_some(),
            !s.exists,
            !s.save,
            s.path.to_string_lossy().to_lowercase(),
        )
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::index::IndexFile;
    use crate::manifest::placeholders::test_places;
    use std::fs;
    use tempfile::tempdir;

    fn file(path: &str, save: bool, stores: &[&str]) -> IndexFile {
        IndexFile {
            path: path.into(),
            save,
            config: !save,
            stores: stores.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn mkfile(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn expands_globs_collapses_and_checks() {
        let tmp = tempdir().unwrap();
        let places = test_places(tmp.path());
        let install = tmp.path().join("Lib/steamapps/common/G");
        let ctx = GameContext::steam(&install, 42, places.steam_root.as_deref());
        let roaming = places.app_data.clone().unwrap();
        let docs = places.documents.clone().unwrap();

        mkfile(&roaming.join("Studio/G/slot1.sav"));
        mkfile(&roaming.join("Studio/G/sub/deep.sav"));
        mkfile(&docs.join("My Games/G/Profile1/save.dat"));
        mkfile(&docs.join("My Games/G/Profile2/save.dat"));
        mkfile(&install.join("save0.sav"));
        mkfile(&places.steam_root.clone().unwrap().join("userdata/77/42/remote/s.dat"));

        let game = IndexGame {
            name: "G".into(),
            steam_ids: vec![42],
            files: vec![
                file("<winAppData>/Studio/G", true, &[]),
                file("<winAppData>/Studio/G/sub/*.sav", true, &[]),
                file("<winDocuments>/My Games/G/*/save.dat", true, &[]),
                file("<winLocalAppData>/G/settings.ini", false, &[]),
                file("<winLocalAppDataLow>/G Studio/G", true, &[]),
                file("<base>/*.sav", true, &[]),
                file("<root>/userdata/<storeUserId>/42/remote", true, &["steam"]),
                file("<winLocalAppData>/Packages/G/wgs", true, &["microsoft"]),
                file("<xdgData>/G", true, &[]),
            ],
        };
        let protected = vec![install.clone()];
        let opts = SuggestOptions {
            places: &places,
            expand: &ctx,
            template: &ctx,
            steam: true,
            include_config: false,
            protected: &protected,
            reserved: &[],
        };
        let got = suggest(&game, &opts);
        let summary: Vec<_> = got
            .iter()
            .map(|s| (s.template.as_str(), s.exists, s.problem.is_some()))
            .collect();
        assert_eq!(
            summary,
            [
                ("<root>/userdata/77/42/remote", true, false),
                ("<winAppData>/Studio/G", true, false),
                ("<winDocuments>/My Games/G/Profile1", true, false),
                ("<winDocuments>/My Games/G/Profile2", true, false),
                ("<winLocalAppDataLow>/G Studio/G", false, false),
                ("<base>", true, true),
            ]
        );

        // Config entries on request; manual games skip Steam-only entries.
        let manual = GameContext::default();
        let opts = SuggestOptions {
            include_config: true,
            steam: false,
            expand: &manual,
            template: &manual,
            ..opts
        };
        let templates: Vec<_> = suggest(&game, &opts).into_iter().map(|s| s.template).collect();
        assert!(templates.contains(&"<winLocalAppData>/G".to_string()));
        assert!(templates.contains(&"<winLocalAppData>/Packages/G/wgs".to_string()));
        assert!(!templates.iter().any(|t| t.contains("userdata")));
    }

    #[test]
    fn file_like_names() {
        assert!(looks_like_file(Path::new(r"C:\a\save.dat")));
        assert!(!looks_like_file(Path::new(r"C:\a\HelloGames.NoMansSky_bs190hzg1sesy")));
        assert!(!looks_like_file(Path::new(r"C:\a\Saves")));
    }
}
