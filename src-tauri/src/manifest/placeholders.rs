//! Ludusavi-style path placeholders (`<winAppData>`, `<base>`, …).
//!
//! - [`resolve`] turns a stored template into an absolute path (literal).
//! - [`glob_pattern`] turns a manifest path into a glob pattern (substituted
//!   values are escaped; `<storeUserId>` becomes `*`, as Ludusavi does).
//! - [`to_template`] maps an absolute folder back to a template using the
//!   most specific known folder, so saved paths survive user-folder or Steam
//!   library changes.

use std::path::{Component, Path, PathBuf};

use crate::config::{Game, GameSource};

/// Known folders of the current Windows user.
#[derive(Debug, Clone, Default)]
pub struct Places {
    pub home: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub local_app_data_low: Option<PathBuf>,
    pub documents: Option<PathBuf>,
    pub public: Option<PathBuf>,
    pub program_data: Option<PathBuf>,
    pub win_dir: Option<PathBuf>,
    pub user_name: Option<String>,
    /// Steam install folder (`<root>` of Steam games).
    pub steam_root: Option<PathBuf>,
}

impl Places {
    pub fn from_system(steam_root: Option<PathBuf>) -> Self {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let home = dirs::home_dir();
        Places {
            local_app_data_low: home.as_ref().map(|h| h.join("AppData").join("LocalLow")),
            home,
            app_data: dirs::config_dir(),
            local_app_data: dirs::data_local_dir(),
            documents: dirs::document_dir(),
            public: dirs::public_dir(),
            program_data: env("ProgramData"),
            win_dir: env("SystemRoot"),
            user_name: std::env::var("USERNAME").ok(),
            steam_root,
        }
    }
}

/// Game-specific placeholders.
#[derive(Debug, Clone, Default)]
pub struct GameContext {
    /// `<base>`: the game's install folder.
    pub base: Option<PathBuf>,
    /// `<root>`: the store root (Steam install folder for Steam games).
    pub root: Option<PathBuf>,
    /// `<game>`: name of the install folder.
    pub game: Option<String>,
    /// `<storeGameId>`: Steam appid.
    pub store_game_id: Option<u32>,
}

impl GameContext {
    pub fn steam(install_dir: &Path, appid: u32, steam_root: Option<&Path>) -> Self {
        GameContext {
            base: Some(install_dir.to_path_buf()),
            root: steam_root.map(Path::to_path_buf),
            game: install_dir.file_name().map(|n| n.to_string_lossy().into_owned()),
            store_game_id: Some(appid),
        }
    }

    /// Context used for a game's stored templates: Steam games get
    /// `<base>`/`<root>`; manual games none (their folder is not reliable).
    pub fn for_game(game: &Game, places: &Places) -> Self {
        match &game.source {
            GameSource::Steam { appid, install_dir } => {
                Self::steam(install_dir, *appid, places.steam_root.as_deref())
            }
            GameSource::Manual => GameContext::default(),
        }
    }

    /// Context for suggestions of a manual game: `<base>` = the exe's folder.
    pub fn manual(exe: Option<&Path>) -> Self {
        let base = exe.and_then(Path::parent).map(Path::to_path_buf);
        GameContext {
            root: base.as_deref().and_then(Path::parent).map(Path::to_path_buf),
            game: base
                .as_deref()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned()),
            base,
            store_game_id: None,
        }
    }
}

enum Value {
    Path(PathBuf),
    Text(String),
    Wildcard,
}

fn value(name: &str, places: &Places, game: &GameContext) -> Option<Value> {
    let path = |p: &Option<PathBuf>| p.clone().map(Value::Path);
    match name {
        "base" => path(&game.base),
        "root" => path(&game.root),
        "game" => game.game.clone().map(Value::Text),
        "storeGameId" => game.store_game_id.map(|id| Value::Text(id.to_string())),
        "storeUserId" => Some(Value::Wildcard),
        "home" => path(&places.home),
        "winAppData" => path(&places.app_data),
        "winLocalAppData" => path(&places.local_app_data),
        "winLocalAppDataLow" => path(&places.local_app_data_low),
        "winDocuments" => path(&places.documents),
        "winPublic" => path(&places.public),
        "winProgramData" => path(&places.program_data),
        "winDir" => path(&places.win_dir),
        "osUserName" => places.user_name.clone().map(Value::Text),
        _ => None,
    }
}

/// Substitutes placeholders; `None` if one is unknown or unavailable.
fn substitute(template: &str, places: &Places, game: &GameContext, glob: bool) -> Option<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('<') {
        let end = start + rest[start..].find('>')?;
        out.push_str(&rest[..start]);
        let text = match value(&rest[start + 1..end], places, game)? {
            Value::Path(p) => p.to_string_lossy().into_owned(),
            Value::Text(t) => t,
            Value::Wildcard if glob => {
                out.push('*');
                rest = &rest[end + 1..];
                continue;
            }
            Value::Wildcard => return None,
        };
        if glob {
            out.push_str(&glob::Pattern::escape(&text));
        } else {
            out.push_str(&text);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out.replace('/', "\\"))
}

/// Absolute path of a stored template (no wildcards allowed).
pub fn resolve(template: &str, places: &Places, game: &GameContext) -> Option<PathBuf> {
    let path = PathBuf::from(substitute(template, places, game, false)?);
    path.is_absolute().then_some(path)
}

/// Glob pattern for a manifest path; `None` if it cannot be resolved here.
pub fn glob_pattern(template: &str, places: &Places, game: &GameContext) -> Option<String> {
    let pattern = substitute(template, places, game, true)?;
    Path::new(&pattern).is_absolute().then_some(pattern)
}

fn lower_components(path: &Path) -> Vec<String> {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// Remaining components of `path` below `base` (case-insensitive).
fn rest_below(path: &Path, base: &Path) -> Option<Vec<String>> {
    let base = lower_components(base);
    let comps: Vec<_> = path.components().filter(|c| !matches!(c, Component::CurDir)).collect();
    if base.is_empty() || comps.len() < base.len() {
        return None;
    }
    let matches = comps
        .iter()
        .zip(&base)
        .all(|(c, b)| c.as_os_str().to_string_lossy().to_lowercase() == *b);
    matches.then(|| {
        comps[base.len()..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect()
    })
}

/// Template form of an absolute folder: the most specific known folder that
/// contains it becomes a placeholder; otherwise the path itself.
pub fn to_template(path: &Path, places: &Places, game: &GameContext) -> String {
    let candidates = [
        ("<base>", &game.base),
        ("<root>", &game.root),
        ("<winLocalAppDataLow>", &places.local_app_data_low),
        ("<winLocalAppData>", &places.local_app_data),
        ("<winAppData>", &places.app_data),
        ("<winDocuments>", &places.documents),
        ("<winPublic>", &places.public),
        ("<winProgramData>", &places.program_data),
        ("<winDir>", &places.win_dir),
        ("<home>", &places.home),
    ];
    let best = candidates
        .iter()
        .filter_map(|(name, base)| {
            let base = base.as_ref()?;
            let rest = rest_below(path, base)?;
            Some((lower_components(base).len(), *name, rest))
        })
        .max_by_key(|(depth, _, _)| *depth);
    match best {
        Some((_, name, rest)) if rest.is_empty() => name.to_string(),
        Some((_, name, rest)) => format!("{name}/{}", rest.join("/")),
        None => path.to_string_lossy().replace('\\', "/"),
    }
}

#[cfg(test)]
pub(crate) fn test_places(root: &Path) -> Places {
    let home = root.join("Users").join("me");
    Places {
        app_data: Some(home.join("AppData").join("Roaming")),
        local_app_data: Some(home.join("AppData").join("Local")),
        local_app_data_low: Some(home.join("AppData").join("LocalLow")),
        documents: Some(home.join("Documents")),
        public: Some(root.join("Users").join("Public")),
        program_data: Some(root.join("ProgramData")),
        win_dir: Some(root.join("Windows")),
        user_name: Some("me".into()),
        steam_root: Some(root.join("Steam")),
        home: Some(home),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Places, GameContext) {
        let places = test_places(Path::new(r"C:\"));
        let game = GameContext::steam(
            Path::new(r"D:\Lib\steamapps\common\My [Game]"),
            42,
            places.steam_root.as_deref(),
        );
        (places, game)
    }

    #[test]
    fn resolves_templates() {
        let (places, game) = setup();
        assert_eq!(
            resolve("<winAppData>/Studio/Game", &places, &game),
            Some(PathBuf::from(r"C:\Users\me\AppData\Roaming\Studio\Game"))
        );
        assert_eq!(
            resolve("<base>/saves/<storeGameId>", &places, &game),
            Some(PathBuf::from(r"D:\Lib\steamapps\common\My [Game]\saves\42"))
        );
        assert_eq!(resolve("<root>/userdata/<storeUserId>", &places, &game), None);
        assert_eq!(resolve("<xdgData>/x", &places, &game), None);
        assert_eq!(resolve("<base>/x", &places, &GameContext::default()), None);
        assert_eq!(resolve("relative/x", &places, &game), None);
        assert_eq!(resolve(r"E:\Saves", &places, &game), Some(PathBuf::from(r"E:\Saves")));
    }

    #[test]
    fn glob_patterns_escape_values() {
        let (places, game) = setup();
        assert_eq!(
            glob_pattern("<base>/*.sav", &places, &game).unwrap(),
            r"D:\Lib\steamapps\common\My [[]Game[]]\*.sav"
        );
        assert_eq!(
            glob_pattern("<root>/userdata/<storeUserId>/42/remote", &places, &game).unwrap(),
            r"C:\Steam\userdata\*\42\remote"
        );
    }

    #[test]
    fn maps_folders_back_to_templates() {
        let (places, game) = setup();
        let cases = [
            (r"C:\Users\me\AppData\LocalLow\Team Cherry\HK", "<winLocalAppDataLow>/Team Cherry/HK"),
            (r"c:\users\ME\appdata\roaming\Studio", "<winAppData>/Studio"),
            (r"C:\Users\me\Documents\My Games\X", "<winDocuments>/My Games/X"),
            (r"C:\Users\me\Saved Games\X", "<home>/Saved Games/X"),
            (r"D:\Lib\steamapps\common\My [Game]\saves", "<base>/saves"),
            (r"C:\Steam\userdata\1\42\remote", "<root>/userdata/1/42/remote"),
            (r"C:\Users\meX\a", "C:/Users/meX/a"),
            (r"E:\Saves\X", "E:/Saves/X"),
        ];
        for (path, template) in cases {
            assert_eq!(to_template(Path::new(path), &places, &game), template, "{path}");
            let back = resolve(template, &places, &game).unwrap();
            assert!(back.to_string_lossy().eq_ignore_ascii_case(path), "{path} -> {}", back.display());
        }
        // Manual games never use <base>.
        let manual = GameContext::default();
        assert_eq!(
            to_template(Path::new(r"D:\Lib\steamapps\common\My [Game]\saves"), &places, &manual),
            "D:/Lib/steamapps/common/My [Game]/saves"
        );
    }

    #[test]
    fn manual_context_uses_exe_folder() {
        let ctx = GameContext::manual(Some(Path::new(r"E:\Games\Foo\bin\foo.exe")));
        assert_eq!(ctx.base, Some(PathBuf::from(r"E:\Games\Foo\bin")));
        assert_eq!(ctx.root, Some(PathBuf::from(r"E:\Games\Foo")));
        assert_eq!(ctx.game.as_deref(), Some("bin"));
    }
}
