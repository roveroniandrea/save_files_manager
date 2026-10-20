//! Ludusavi manifest: download, compact index cache, save folder suggestions.
//!
//! Files in `<app dir>\manifest\`: `index.v<N>.json` (compact index, see
//! [`index`]) and `manifest.meta.json` (ETag, last check, last update). The
//! YAML itself is not kept. The manifest is checked at most once per
//! [`REFRESH_DAYS`] days (at startup, in the background) or on request; the
//! app works offline with the cached index or without one.

pub mod fetch;
pub mod fuzzy;
pub mod index;
pub mod placeholders;
pub mod resolve;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::swap::validate::{game_protected, Guards};
use fetch::{FetchResult, Fetcher};
use index::{Index, Lookup, INDEX_VERSION};
use placeholders::{GameContext, Places};
use resolve::{SuggestOptions, Suggestion};

pub const REFRESH_DAYS: i64 = 7;
/// "Did you mean" names shown when the game is not found.
pub const SIMILAR_LIMIT: usize = 5;
/// Results of the manual database search.
pub const SEARCH_LIMIT: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub etag: Option<String>,
    /// Last successful check with the server.
    pub checked_at: Option<DateTime<Utc>>,
    /// When the cached index was last rebuilt.
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManifestStatus {
    /// A download/check is running.
    pub updating: bool,
    /// Number of games in the cached index (0 = no index yet).
    pub games: usize,
    pub checked_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    /// Error of the last check, if it failed.
    pub error: Option<String>,
}

/// What the editor knows about the game being added or edited.
#[derive(Debug, Clone, Deserialize)]
pub struct SuggestQuery {
    pub name: String,
    #[serde(default)]
    pub steam_appid: Option<u32>,
    #[serde(default)]
    pub exe_path: Option<PathBuf>,
    /// Database entry picked by the user ("did you mean" / search); used
    /// instead of matching by Steam appid or name.
    #[serde(default)]
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestions {
    pub status: ManifestStatus,
    /// Name of the manifest entry used, if one matched.
    pub matched: Option<String>,
    pub items: Vec<Suggestion>,
    /// Similar entry names when nothing matched ("did you mean"), best first.
    pub similar: Vec<String>,
}

#[derive(Default)]
struct Inner {
    lookup: Option<Arc<Lookup>>,
    /// The index file was read (or found missing/broken) already.
    loaded: bool,
    error: Option<String>,
}

/// Tauri-managed manifest state.
pub struct Manifest {
    dir: PathBuf,
    /// Held during a refresh; lookups wait for it so they see the new index.
    refresh_lock: Mutex<()>,
    updating: AtomicBool,
    inner: Mutex<Inner>,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| AppError::io(dir, e))?;
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| AppError::io(&tmp, e))?;
        f.write_all(bytes).map_err(|e| AppError::io(&tmp, e))?;
        f.sync_all().map_err(|e| AppError::io(&tmp, e))?;
    }
    fs::rename(&tmp, path).map_err(|e| AppError::io(path, e))
}

impl Manifest {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Manifest {
            dir: dir.into(),
            refresh_lock: Mutex::new(()),
            updating: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
        }
    }

    fn index_file(&self) -> PathBuf {
        self.dir.join(format!("index.v{INDEX_VERSION}.json"))
    }

    fn meta_file(&self) -> PathBuf {
        self.dir.join("manifest.meta.json")
    }

    fn read_meta(&self) -> Meta {
        fs::read(self.meta_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write_meta(&self, meta: &Meta) -> AppResult<()> {
        let json = serde_json::to_vec_pretty(meta).map_err(|e| AppError::Other(e.to_string()))?;
        write_atomic(&self.meta_file(), &json)
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Loads the cached index once (no waiting for a running refresh).
    fn cached(&self) -> Option<Arc<Lookup>> {
        let mut inner = self.inner();
        if !inner.loaded {
            inner.loaded = true;
            inner.lookup = fs::read(self.index_file())
                .ok()
                .and_then(|b| serde_json::from_slice::<Index>(&b).ok())
                .filter(|i| !i.games.is_empty())
                .map(|i| Arc::new(Lookup::new(i)));
        }
        inner.lookup.clone()
    }

    pub fn status(&self) -> ManifestStatus {
        let meta = self.read_meta();
        let updating = self.updating.load(Ordering::SeqCst);
        // While a refresh runs, don't block on loading; report what is known.
        let games = if updating {
            self.inner().lookup.as_ref().map_or(0, |l| l.len())
        } else {
            self.cached().map_or(0, |l| l.len())
        };
        ManifestStatus {
            updating,
            games,
            checked_at: meta.checked_at,
            updated_at: meta.updated_at,
            error: self.inner().error.clone(),
        }
    }

    /// The index, waiting for a running refresh first. `None` = no index.
    pub fn lookup(&self) -> Option<Arc<Lookup>> {
        let _wait = self.refresh_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.cached()
    }

    /// Save folder suggestions for a game (waits for a running refresh).
    /// `install_dir` is the Steam install folder of a Steam game.
    pub fn suggest(
        &self,
        query: &SuggestQuery,
        install_dir: Option<&Path>,
        places: &Places,
        guards: &Guards,
        include_config: bool,
    ) -> Suggestions {
        let lookup = self.lookup();
        let entry = query.entry.as_deref().filter(|e| !e.trim().is_empty());
        let game = lookup.as_ref().and_then(|l| match entry {
            Some(entry) => l.entry(entry),
            None => l.find(query.steam_appid, &query.name),
        });
        let similar = match (&lookup, game) {
            (Some(l), None) => l.similar(entry.unwrap_or(&query.name), SIMILAR_LIMIT),
            _ => Vec::new(),
        };
        let items = game.map_or_else(Vec::new, |game| {
            let (expand, template) = match (query.steam_appid, install_dir) {
                (Some(appid), Some(dir)) => {
                    let ctx = GameContext::steam(dir, appid, places.steam_root.as_deref());
                    (ctx.clone(), ctx)
                }
                _ => (GameContext::manual(query.exe_path.as_deref()), GameContext::default()),
            };
            let protected = game_protected(guards, install_dir, query.exe_path.as_deref());
            resolve::suggest(
                game,
                &SuggestOptions {
                    places,
                    expand: &expand,
                    template: &template,
                    steam: query.steam_appid.is_some(),
                    include_config,
                    protected: &protected,
                    reserved: &guards.reserved,
                },
            )
        });
        Suggestions {
            status: self.status(),
            matched: game.map(|g| g.name.clone()),
            items,
            similar,
        }
    }

    /// Database entry names matching `text` (waits for a running refresh).
    pub fn search(&self, text: &str) -> Vec<String> {
        self.lookup().map_or_else(Vec::new, |l| l.search(text, SEARCH_LIMIT))
    }

    /// Checks the server for a newer manifest. Without `force`, does nothing
    /// if the last check is recent and an index is cached.
    pub fn refresh(&self, fetcher: &dyn Fetcher, force: bool) -> AppResult<ManifestStatus> {
        {
            let _guard = self.refresh_lock.lock().unwrap_or_else(|e| e.into_inner());
            self.updating.store(true, Ordering::SeqCst);
            let result = self.refresh_locked(fetcher, force);
            self.updating.store(false, Ordering::SeqCst);
            let mut inner = self.inner();
            inner.error = result.as_ref().err().map(|e| e.to_string());
            result?;
        }
        Ok(self.status())
    }

    fn refresh_locked(&self, fetcher: &dyn Fetcher, force: bool) -> AppResult<()> {
        let mut meta = self.read_meta();
        let have_index = self.cached().is_some();
        let recent = meta
            .checked_at
            .is_some_and(|t| Utc::now() - t < Duration::days(REFRESH_DAYS));
        if have_index && recent && !force {
            return Ok(());
        }
        let etag = if have_index { meta.etag.as_deref() } else { None };
        match fetcher.fetch(etag).map_err(AppError::Manifest)? {
            FetchResult::NotModified => {}
            FetchResult::Updated { body, etag } => {
                let index = index::build(&body).map_err(AppError::Manifest)?;
                drop(body);
                let json = serde_json::to_vec(&index).map_err(|e| AppError::Other(e.to_string()))?;
                write_atomic(&self.index_file(), &json)?;
                let mut inner = self.inner();
                inner.lookup = Some(Arc::new(Lookup::new(index)));
                inner.loaded = true;
                meta.etag = etag;
                meta.updated_at = Some(Utc::now());
            }
        }
        meta.checked_at = Some(Utc::now());
        self.write_meta(&meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use tempfile::tempdir;

    const YAML: &str = "Game:\n  files:\n    <winAppData>/G:\n      tags: [save]\n  steam:\n    id: 5\n";

    /// Replies with queued results and records the ETags it was sent.
    struct Fake {
        replies: RefCell<Vec<Result<FetchResult, String>>>,
        etags: RefCell<Vec<Option<String>>>,
    }

    impl Fake {
        fn new(replies: Vec<Result<FetchResult, String>>) -> Self {
            Fake {
                replies: RefCell::new(replies),
                etags: RefCell::new(Vec::new()),
            }
        }
    }

    impl Fetcher for Fake {
        fn fetch(&self, etag: Option<&str>) -> Result<FetchResult, String> {
            self.etags.borrow_mut().push(etag.map(str::to_string));
            self.replies.borrow_mut().remove(0)
        }
    }

    fn updated(yaml: &str, etag: &str) -> Result<FetchResult, String> {
        Ok(FetchResult::Updated {
            body: yaml.as_bytes().to_vec(),
            etag: Some(etag.into()),
        })
    }

    #[test]
    fn downloads_caches_and_respects_interval() {
        let tmp = tempdir().unwrap();
        let m = Manifest::new(tmp.path());
        assert_eq!(m.status().games, 0);
        assert!(m.lookup().is_none());

        let fake = Fake::new(vec![updated(YAML, "e1"), Ok(FetchResult::NotModified)]);
        let status = m.refresh(&fake, false).unwrap();
        assert_eq!(status.games, 1);
        assert!(status.checked_at.is_some() && status.error.is_none());
        assert_eq!(m.lookup().unwrap().find(Some(5), "").unwrap().name, "Game");

        // Recent check: nothing fetched without force.
        m.refresh(&fake, false).unwrap();
        assert_eq!(fake.etags.borrow().len(), 1);
        // Forced: conditional request with the ETag.
        m.refresh(&fake, true).unwrap();
        assert_eq!(*fake.etags.borrow(), vec![None, Some("e1".to_string())]);

        // A new instance (next app start) reads the cached index.
        let again = Manifest::new(tmp.path());
        assert_eq!(again.status().games, 1);
        assert_eq!(again.read_meta().etag.as_deref(), Some("e1"));
    }

    #[test]
    fn stale_check_refetches_and_errors_keep_the_cache() {
        let tmp = tempdir().unwrap();
        let m = Manifest::new(tmp.path());
        let fake = Fake::new(vec![
            updated(YAML, "e1"),
            Err("offline".into()),
            updated("not: [valid", "e2"),
        ]);
        m.refresh(&fake, false).unwrap();
        let mut meta = m.read_meta();
        meta.checked_at = Some(Utc::now() - Duration::days(REFRESH_DAYS + 1));
        m.write_meta(&meta).unwrap();

        assert!(matches!(m.refresh(&fake, false), Err(AppError::Manifest(_))));
        let status = m.status();
        assert_eq!(status.error.as_deref(), Some("offline"));
        assert_eq!(status.games, 1);

        assert!(m.refresh(&fake, true).is_err());
        assert_eq!(m.status().games, 1);
        assert_eq!(m.read_meta().etag.as_deref(), Some("e1"));
    }

    #[test]
    fn suggests_for_matched_game() {
        let tmp = tempdir().unwrap();
        let m = Manifest::new(tmp.path().join("manifest"));
        let places = placeholders::test_places(tmp.path());
        let query = SuggestQuery {
            name: "game".into(),
            steam_appid: None,
            exe_path: None,
            entry: None,
        };
        let none = m.suggest(&query, None, &places, &Guards::default(), false);
        assert_eq!((none.matched, none.items.len(), none.status.games), (None, 0, 0));

        m.refresh(&Fake::new(vec![updated(YAML, "e")]), false).unwrap();
        let got = m.suggest(&query, None, &places, &Guards::default(), false);
        assert_eq!(got.matched.as_deref(), Some("Game"));
        assert_eq!(got.items.len(), 1);
        assert_eq!(got.items[0].template, "<winAppData>/G");
        assert!(!got.items[0].exists);
    }

    #[test]
    fn did_you_mean_and_explicit_entry() {
        let tmp = tempdir().unwrap();
        let m = Manifest::new(tmp.path().join("manifest"));
        let places = placeholders::test_places(tmp.path());
        let yaml = format!("{YAML}Other:
  files:
    <winDocuments>/O:
      tags: [save]
");
        assert!(m.search("game").is_empty());
        m.refresh(&Fake::new(vec![updated(&yaml, "e")]), false).unwrap();

        let mut query = SuggestQuery {
            name: "Gaem".into(),
            steam_appid: None,
            exe_path: None,
            entry: None,
        };
        let got = m.suggest(&query, None, &places, &Guards::default(), false);
        assert_eq!((got.matched, got.items.len()), (None, 0));
        assert_eq!(got.similar, ["Game"]);

        // A picked entry wins over the name (and the Steam appid).
        query.entry = Some("Other".into());
        query.steam_appid = Some(5);
        let got = m.suggest(&query, None, &places, &Guards::default(), false);
        assert_eq!(got.matched.as_deref(), Some("Other"));
        assert_eq!(got.items[0].template, "<winDocuments>/O");
        assert!(got.similar.is_empty());

        // An entry gone from the database: nothing matched, similar names offered.
        query.entry = Some("Othr".into());
        let got = m.suggest(&query, None, &places, &Guards::default(), false);
        assert_eq!((got.matched, got.similar), (None, vec!["Other".to_string()]));

        assert_eq!(m.search("oth"), ["Other"]);
    }

    /// Read-only: real manifest + this PC's Steam and folders.
    /// `MANIFEST_YAML=<file> cargo test real_suggestions -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_suggestions() {
        let yaml = fs::read(std::env::var("MANIFEST_YAML").expect("set MANIFEST_YAML")).unwrap();
        let tmp = tempdir().unwrap();
        let m = Manifest::new(tmp.path());
        let fake = Fake::new(vec![Ok(FetchResult::Updated { body: yaml, etag: None })]);
        m.refresh(&fake, false).unwrap();
        let steam = crate::steam::Steam::locate().expect("Steam");
        let places = Places::from_system(Some(steam.root().to_path_buf()));
        let mut guards = Guards {
            protected: Guards::system_protected(),
            reserved: vec![],
        };
        guards.protected.extend(steam.protected_folders());
        for app in steam.apps() {
            let q = SuggestQuery { name: app.name.clone(), steam_appid: Some(app.appid), exe_path: None, entry: None };
            let s = m.suggest(&q, Some(&app.install_dir), &places, &guards, true);
            println!("{} -> {:?}", app.name, s.matched);
            for i in s.items {
                println!("  {} | {} | exists={} save={} config={} problem={:?}", i.template, i.path.display(), i.exists, i.save, i.config, i.problem);
            }
        }
    }

    #[test]
    fn missing_index_ignores_etag() {
        let tmp = tempdir().unwrap();
        let m = Manifest::new(tmp.path());
        m.write_meta(&Meta {
            etag: Some("old".into()),
            checked_at: Some(Utc::now()),
            updated_at: None,
        })
        .unwrap();
        let fake = Fake::new(vec![updated(YAML, "e2")]);
        m.refresh(&fake, false).unwrap();
        assert_eq!(*fake.etags.borrow(), vec![None]);
        assert_eq!(m.status().games, 1);
    }
}
