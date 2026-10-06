//! Compact index of the Ludusavi manifest: per game only the Windows file
//! entries tagged `save` or `config`, plus Steam ids and aliases.
//!
//! The manifest (~18 MB of YAML) is parsed one game at a time, so memory
//! stays bounded and an odd entry is skipped instead of failing the parse.

use std::collections::HashMap;
use std::fmt;

use serde::de::{Deserializer as _, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

/// Bump when the index format changes (the file name includes it).
pub const INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub games: Vec<IndexGame>,
    /// `(alias, target game name)`.
    #[serde(default)]
    pub aliases: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexGame {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steam_ids: Vec<u32>,
    pub files: Vec<IndexFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexFile {
    /// Manifest path with placeholders, e.g. `<winAppData>/Studio/Game`.
    pub path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub save: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub config: bool,
    /// Stores the entry is limited to (`steam`, `gog`, …); empty = any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stores: Vec<String>,
}

fn as_u32(v: &Value) -> Option<u32> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .or_else(|| v.as_str()?.trim().parse().ok())
}

fn key_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Windows-applicable file entry, or `None`.
fn file_entry(path: &str, v: &Value) -> Option<IndexFile> {
    let tags: Vec<&str> = v
        .get("tags")
        .and_then(Value::as_sequence)
        .map(|s| s.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let (save, config) = (tags.contains(&"save"), tags.contains(&"config"));
    if !save && !config {
        return None;
    }
    let whens = v.get("when").and_then(Value::as_sequence).cloned().unwrap_or_default();
    let mut any_store = whens.is_empty();
    let mut stores: Vec<String> = Vec::new();
    for w in &whens {
        let os = w.get("os").and_then(Value::as_str);
        if os.is_some_and(|os| os != "windows") {
            continue;
        }
        match w.get("store").and_then(Value::as_str) {
            Some(store) if !stores.iter().any(|s| s == store) => stores.push(store.to_string()),
            Some(_) => {}
            None => any_store = true,
        }
    }
    if !any_store && stores.is_empty() {
        return None; // only for other operating systems
    }
    Some(IndexFile {
        path: path.to_string(),
        save,
        config,
        stores: if any_store { Vec::new() } else { stores },
    })
}

impl Index {
    fn add(&mut self, name: String, v: &Value) {
        if let Some(target) = v.get("alias").and_then(Value::as_str) {
            self.aliases.push((name, target.to_string()));
            return;
        }
        let files: Vec<IndexFile> = v
            .get("files")
            .and_then(Value::as_mapping)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, fv)| file_entry(k.as_str()?, fv))
                    .collect()
            })
            .unwrap_or_default();
        if files.is_empty() {
            return;
        }
        let mut steam_ids: Vec<u32> = v.get("steam").and_then(|s| s.get("id")).and_then(as_u32).into_iter().collect();
        if let Some(extra) = v.get("id").and_then(|i| i.get("steamExtra")).and_then(Value::as_sequence) {
            steam_ids.extend(extra.iter().filter_map(as_u32));
        }
        self.games.push(IndexGame { name, steam_ids, files });
    }
}

struct ManifestVisitor;

impl<'de> Visitor<'de> for ManifestVisitor {
    type Value = Index;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a map of games")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Index, A::Error> {
        let mut index = Index::default();
        while let Some((key, value)) = map.next_entry::<Value, Value>()? {
            if let Some(name) = key_string(&key) {
                index.add(name, &value);
            }
        }
        Ok(index)
    }
}

/// Builds the index from the manifest YAML.
pub fn build(yaml: &[u8]) -> Result<Index, String> {
    let text = std::str::from_utf8(yaml).map_err(|e| format!("the manifest is not UTF-8: {e}"))?;
    let index = serde_yaml_ng::Deserializer::from_str(text)
        .deserialize_map(ManifestVisitor)
        .map_err(|e| format!("cannot parse the manifest: {e}"))?;
    if index.games.is_empty() {
        return Err("the manifest contains no games".into());
    }
    Ok(index)
}

/// Lowercase letters and digits only: "No Man's Sky" → "nomanssky".
pub fn normalize_name(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The index with lookup tables.
#[derive(Debug)]
pub struct Lookup {
    index: Index,
    by_steam: HashMap<u32, usize>,
    by_name: HashMap<String, usize>,
}

impl Lookup {
    pub fn new(index: Index) -> Self {
        let mut by_steam = HashMap::new();
        let mut by_name = HashMap::new();
        for (i, game) in index.games.iter().enumerate() {
            for id in &game.steam_ids {
                by_steam.entry(*id).or_insert(i);
            }
            by_name.entry(normalize_name(&game.name)).or_insert(i);
        }
        for (alias, target) in &index.aliases {
            if let Some(&i) = by_name.get(&normalize_name(target)) {
                by_name.entry(normalize_name(alias)).or_insert(i);
            }
        }
        Lookup {
            index,
            by_steam,
            by_name,
        }
    }

    pub fn len(&self) -> usize {
        self.index.games.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.games.is_empty()
    }

    /// By Steam appid first, then by (normalized) name.
    pub fn find(&self, steam_appid: Option<u32>, name: &str) -> Option<&IndexGame> {
        let i = steam_appid
            .and_then(|id| self.by_steam.get(&id))
            .or_else(|| self.by_name.get(&normalize_name(name)).filter(|_| !name.trim().is_empty()))?;
        self.index.games.get(*i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SAMPLE: &str = r#"
"No Man's Sky":
  files:
    "<base>/Binaries/SETTINGS":
      tags: [config]
      when:
        - os: windows
    "<home>/Library/Application Support/HelloGames/NMS":
      tags: [config, save]
      when:
        - os: mac
    "<winAppData>/HelloGames/NMS":
      tags: [save]
      when:
        - os: windows
    "<winLocalAppData>/Packages/HelloGames.NoMansSky/wgs":
      tags: [save]
      when:
        - os: windows
          store: microsoft
    "<base>/logs":
      tags: [log]
  steam:
    id: 275850
NMS Alias:
  alias: "No Man's Sky"
1979:
  files:
    "<winDocuments>/1979":
      tags: [save]
  id:
    steamExtra: [1, "2"]
No Files:
  steam:
    id: 9
Odd:
  files: "not a map"
"#;

    #[test]
    fn builds_compact_index() {
        let index = build(SAMPLE.as_bytes()).unwrap();
        assert_eq!(index.games.len(), 2);
        let nms = &index.games[0];
        assert_eq!(nms.steam_ids, vec![275850]);
        let paths: Vec<_> = nms.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "<base>/Binaries/SETTINGS",
                "<winAppData>/HelloGames/NMS",
                "<winLocalAppData>/Packages/HelloGames.NoMansSky/wgs"
            ]
        );
        assert!(nms.files[0].config && !nms.files[0].save);
        assert_eq!(nms.files[2].stores, ["microsoft"]);
        assert_eq!(index.games[1].name, "1979");
        assert_eq!(index.games[1].steam_ids, vec![1, 2]);
        assert_eq!(index.aliases, vec![("NMS Alias".into(), "No Man's Sky".into())]);

        // Round trip through JSON (the cached form).
        let json = serde_json::to_string(&index).unwrap();
        assert_eq!(serde_json::from_str::<Index>(&json).unwrap(), index);
    }

    #[test]
    fn lookup_by_steam_id_name_and_alias() {
        let lookup = Lookup::new(build(SAMPLE.as_bytes()).unwrap());
        assert_eq!(lookup.find(Some(275850), "whatever").unwrap().name, "No Man's Sky");
        assert_eq!(lookup.find(None, "no mans sky").unwrap().name, "No Man's Sky");
        assert_eq!(lookup.find(Some(123), "NMS alias").unwrap().name, "No Man's Sky");
        assert_eq!(lookup.find(Some(2), "").unwrap().name, "1979");
        assert!(lookup.find(None, "").is_none());
        assert!(lookup.find(None, "Unknown").is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(build(b"[1, 2]").is_err());
        assert!(build(b"Game: {}").is_err());
        assert!(build(&[0xff, 0xfe]).is_err());
    }

    /// Parses a real manifest: `MANIFEST_YAML=<file> cargo test real_manifest -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_manifest() {
        let path = std::env::var("MANIFEST_YAML").expect("set MANIFEST_YAML");
        let start = std::time::Instant::now();
        let index = build(&std::fs::read(path).unwrap()).unwrap();
        let json = serde_json::to_vec(&index).unwrap();
        println!(
            "{} games, {} aliases, index {} KB, {:?}",
            index.games.len(),
            index.aliases.len(),
            json.len() / 1024,
            start.elapsed()
        );
        let lookup = Lookup::new(index);
        println!("{:#?}", lookup.find(Some(275850), ""));
    }
}
