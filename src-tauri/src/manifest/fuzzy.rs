//! Fuzzy name matching over the manifest's game names ("did you mean" and
//! the manual database search in the editor).
//!
//! A score in `0.0..=1.0` is the best of three measures:
//! - **tokens**: words (lowercase, apostrophes dropped, small stop words
//!   removed, Roman numerals II–X as digits) compared one by one, a word
//!   counting as found when equal, a prefix (≥ 3 letters) or within a small
//!   edit distance; weighted towards the query's words being found;
//! - **whole name**: edit distance between the normalized names;
//! - **containment**: one normalized name contains the other.

/// Minimum score for an automatic "did you mean" suggestion.
pub const SIMILAR_MIN: f64 = 0.55;
/// Minimum score for a result of the manual search.
pub const SEARCH_MIN: f64 = 0.45;

const STOP_WORDS: [&str; 5] = ["the", "a", "an", "of", "and"];
const ROMAN: [(&str, &str); 9] = [
    ("ii", "2"),
    ("iii", "3"),
    ("iv", "4"),
    ("v", "5"),
    ("vi", "6"),
    ("vii", "7"),
    ("viii", "8"),
    ("ix", "9"),
    ("x", "10"),
];

/// Pre-computed form of a name.
#[derive(Debug, Clone)]
pub struct Key {
    /// Normalized name (see [`super::index::normalize_name`]) as chars.
    norm: Vec<char>,
    tokens: Vec<Vec<char>>,
}

impl Key {
    pub fn new(name: &str) -> Self {
        let lower: String = name
            .chars()
            .filter(|c| !matches!(c, '\'' | '’' | '`'))
            .flat_map(char::to_lowercase)
            .collect();
        let norm = lower.chars().filter(|c| c.is_alphanumeric()).collect();
        let tokens = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty() && !STOP_WORDS.contains(t))
            .map(|t| {
                let t = ROMAN.iter().find(|(r, _)| *r == t).map_or(t, |(_, d)| d);
                t.chars().collect()
            })
            .collect();
        Key { norm, tokens }
    }

    pub fn is_empty(&self) -> bool {
        self.norm.is_empty()
    }
}

/// Edit distance (insert/delete/substitute/swap of two neighbours; optimal
/// string alignment), three-row version.
fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut before: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            let mut d = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
            if i > 0 && j > 0 && *ca == b[j - 1] && a[i - 1] == *cb {
                d = d.min(before[j - 1] + 1);
            }
            cur[j + 1] = d;
        }
        std::mem::swap(&mut before, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// `1 - distance / longer length`, or 0 when the lengths alone rule out
/// reaching `min` (saves the distance computation).
fn edit_similarity(a: &[char], b: &[char], min: f64) -> f64 {
    let max = a.len().max(b.len());
    if max == 0 {
        return 1.0;
    }
    let len_diff = a.len().abs_diff(b.len());
    if 1.0 - (len_diff as f64) / (max as f64) < min {
        return 0.0;
    }
    let sim = 1.0 - levenshtein(a, b) as f64 / max as f64;
    if sim < min {
        0.0
    } else {
        sim
    }
}

fn is_number(t: &[char]) -> bool {
    t.iter().all(char::is_ascii_digit)
}

fn starts_with(long: &[char], short: &[char]) -> bool {
    long.len() > short.len() && long[..short.len()] == *short
}

/// Similarity of a query word to a candidate word. A query word that starts
/// a candidate word (typing in progress) counts more than the reverse.
fn token_similarity(q: &[char], c: &[char]) -> f64 {
    if q == c {
        return 1.0;
    }
    if is_number(q) || is_number(c) {
        return 0.0; // "2" vs "3" is a different game
    }
    if q.len() >= 3 && starts_with(c, q) {
        return 0.8;
    }
    if c.len() >= 3 && starts_with(q, c) {
        return 0.4;
    }
    if q.len().max(c.len()) >= 4 {
        return 0.9 * edit_similarity(q, c, 0.75);
    }
    0.0
}

/// Share of the query's words found in the candidate (`recall`) and of the
/// candidate's words found in the query (`precision`).
fn coverage(query: &[Vec<char>], candidate: &[Vec<char>]) -> (f64, f64) {
    if query.is_empty() || candidate.is_empty() {
        return (0.0, 0.0);
    }
    let recall: f64 = query
        .iter()
        .map(|q| candidate.iter().map(|c| token_similarity(q, c)).fold(0.0, f64::max))
        .sum();
    let precision: f64 = candidate
        .iter()
        .map(|c| query.iter().map(|q| token_similarity(q, c)).fold(0.0, f64::max))
        .sum();
    (recall / query.len() as f64, precision / candidate.len() as f64)
}

/// One normalized name inside the other; a candidate that is only part of
/// the query ("W.I.T.C.H." for "Witcher 3") counts less than the reverse.
fn containment(q: &[char], c: &[char]) -> f64 {
    let (short, long, factor) = if q.len() <= c.len() { (q, c, 1.0) } else { (c, q, 0.8) };
    if short.len() < 2 || short.len() == long.len() {
        return 0.0;
    }
    let ratio = short.len() as f64 / long.len() as f64;
    if starts_with(long, short) {
        return factor * (0.6 + 0.4 * ratio);
    }
    if long.windows(short.len()).any(|w| w == short) {
        return factor * (0.55 + 0.4 * ratio);
    }
    0.0
}

/// How well `candidate` matches `query` (1.0 = same normalized name).
pub fn score(query: &Key, candidate: &Key) -> f64 {
    if query.is_empty() || candidate.is_empty() {
        return 0.0;
    }
    if query.norm == candidate.norm {
        return 1.0;
    }
    let (recall, precision) = coverage(&query.tokens, &candidate.tokens);
    let tokens = 0.7 * recall + 0.3 * precision;
    // "Portal 2" vs "Portal 3" is one edit apart but a different game.
    let numbers = |k: &Key| k.tokens.iter().filter(|t| is_number(t)).cloned().collect::<Vec<_>>();
    let (qn, cn) = (numbers(query), numbers(candidate));
    let numbers_differ = !qn.is_empty() && !cn.is_empty() && qn != cn;
    let whole = if !numbers_differ && query.norm.len().min(candidate.norm.len()) >= 3 {
        edit_similarity(&query.norm, &candidate.norm, 0.5)
    } else {
        0.0
    };
    // Below 1.0: only an equal normalized name is a perfect match.
    tokens.max(whole).max(containment(&query.norm, &candidate.norm)).min(0.99)
}

/// Indices into `candidates` (deduplicated by their `usize` target) with a
/// score ≥ `min`, best first: score, then shorter name, then name.
pub fn rank<'a>(
    query: &str,
    candidates: impl Iterator<Item = (&'a Key, usize, &'a str)>,
    min: f64,
    limit: usize,
) -> Vec<usize> {
    let q = Key::new(query);
    if q.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut best: std::collections::HashMap<usize, (f64, &str)> = std::collections::HashMap::new();
    for (key, target, name) in candidates {
        let s = score(&q, key);
        if s < min {
            continue;
        }
        let e = best.entry(target).or_insert((s, name));
        if s > e.0 {
            *e = (s, name);
        }
    }
    let mut hits: Vec<(usize, f64, &str)> = best.into_iter().map(|(t, (s, n))| (t, s, n)).collect();
    hits.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then(a.2.len().cmp(&b.2.len()))
            .then(a.2.cmp(b.2))
            .then(a.0.cmp(&b.0))
    });
    hits.into_iter().take(limit).map(|(t, _, _)| t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(q: &str, c: &str) -> f64 {
        score(&Key::new(q), &Key::new(c))
    }

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn levenshtein_distances() {
        assert_eq!(levenshtein(&chars(""), &chars("abc")), 3);
        assert_eq!(levenshtein(&chars("kitten"), &chars("sitting")), 3);
        assert_eq!(levenshtein(&chars("skyrim"), &chars("skyrm")), 1);
        assert_eq!(levenshtein(&chars("same"), &chars("same")), 0);
        assert_eq!(levenshtein(&chars("gaem"), &chars("game")), 1); // swap
        assert_eq!(levenshtein(&chars("abc"), &chars("")), 3);
    }

    #[test]
    fn keys_tokenize_and_normalize() {
        let k = Key::new("The Elder Scrolls V: Skyrim");
        let tokens: Vec<String> = k.tokens.iter().map(|t| t.iter().collect()).collect();
        assert_eq!(tokens, ["elder", "scrolls", "5", "skyrim"]);
        assert_eq!(k.norm.iter().collect::<String>(), "theelderscrollsvskyrim");
        let nms = Key::new("No Man’s Sky");
        let tokens: Vec<String> = nms.tokens.iter().map(|t| t.iter().collect()).collect();
        assert_eq!(tokens, ["no", "mans", "sky"]);
        assert!(Key::new(" :: ").is_empty());
    }

    #[test]
    fn scores_typical_variants_high() {
        assert_eq!(s("no mans sky", "No Man's Sky"), 1.0);
        assert!(s("Skyrm", "Skyrim") >= SIMILAR_MIN);
        assert!(s("Witcher 3", "The Witcher 3: Wild Hunt") >= SIMILAR_MIN);
        assert!(s("Skyrim", "The Elder Scrolls V: Skyrim") >= SIMILAR_MIN);
        assert!(s("Hollow Knight Silksong", "Hollow Knight: Silksong") == 1.0);
        assert!(s("Final Fantasy 7", "Final Fantasy VII") >= 0.9);
        assert!(s("Stardew Valey", "Stardew Valley") >= 0.9);
        // Partial input for the manual search.
        assert!(s("witch", "The Witcher 3: Wild Hunt") >= SEARCH_MIN);
        assert!(s("stard", "Stardew Valley") >= SEARCH_MIN);
    }

    #[test]
    fn scores_unrelated_names_low() {
        assert!(s("Hollow Knight", "Knight Online") < SIMILAR_MIN);
        assert!(s("Celeste", "Factorio") < SEARCH_MIN);
        assert!(s("Doom", "Cities: Skylines") < SEARCH_MIN);
        assert_eq!(s("", "Game"), 0.0);
        assert_eq!(s("Game", "!!"), 0.0);
        // Only an equal normalized name scores 1.
        assert!(s("Doom", "Doom II") < 1.0);
    }

    #[test]
    fn partial_candidates_rank_below_longer_ones() {
        // A candidate that is only part of the query ranks below one that
        // contains all of it.
        assert!(s("Witcher 3", "W.I.T.C.H.") < s("Witcher 3", "The Witcher 3: Wild Hunt"));
        assert!(s("Skyrim", "Dark Sky") < SIMILAR_MIN);
        assert_eq!(token_similarity(&chars("sky"), &chars("skyrim")), 0.8);
        assert_eq!(token_similarity(&chars("skyrim"), &chars("sky")), 0.4);
    }

    #[test]
    fn numbers_must_match() {
        assert!(s("Portal 2", "Portal 3") < SIMILAR_MIN);
        assert!(s("Portal 2", "Portal") >= SIMILAR_MIN);
        assert_eq!(token_similarity(&chars("2"), &chars("3")), 0.0);
        assert_eq!(token_similarity(&chars("ii"), &chars("iii")), 0.0); // too short for edits
    }

    #[test]
    fn rank_orders_dedupes_and_limits() {
        let names = [
            ("Stardew Valley", 0),
            ("Stardew Valley Alias", 0),
            ("Star Wars", 1),
            ("Stardew", 2),
            ("Factorio", 3),
        ];
        let keys: Vec<(Key, usize, &str)> = names.iter().map(|(n, t)| (Key::new(n), *t, *n)).collect();
        let iter = || keys.iter().map(|(k, t, n)| (k, *t, *n));
        let got = rank("Stardew Valey", iter(), SIMILAR_MIN, 5);
        assert_eq!(got[0], 0);
        assert!(got.contains(&2) && !got.contains(&3));
        assert_eq!(got.iter().filter(|t| **t == 0).count(), 1);
        assert_eq!(rank("Stardew Valey", iter(), SIMILAR_MIN, 1), vec![0]);
        assert!(rank("", iter(), 0.0, 5).is_empty());
        assert!(rank("zzzzqqq", iter(), SIMILAR_MIN, 5).is_empty());
        // Ties: the shorter name first.
        let got = rank("stard", iter(), SEARCH_MIN, 5);
        assert_eq!(&got[..2], &[2, 0]);
    }
}
