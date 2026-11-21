//! Original-language resolution.
//!
//! Release names only describe the audio tracks on offer. This board always
//! lists Tamil first, even when the film was made in another language, so the
//! bracketed audio order is not a valid original-language signal.
//!
//! The heuristic below is a low-confidence placeholder. The model resolves the
//! real production language once per film and the answer is cached for a day.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::lang::Lang;
use crate::model::{LangSource, OriginalLang, Thread};

/// Model chain tried in order; the first one that answers is used.
pub const MODEL_CHAIN: &[&str] = &[
    "gemini-3.5-flash",
    "gemini-3.6-flash",
    "gemini-3.1-flash-lite",
    "gemini-3.5-flash-lite",
    "gemini-flash-latest",
];

const API_ROOT: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A cached answer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub lang: Lang,
    pub confidence: f32,
    pub source: LangSource,
    /// When the answer was resolved, in seconds since the Unix epoch.
    #[serde(default)]
    pub resolved_at: u64,
}

/// On-disk cache of resolved languages.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LangCache {
    #[serde(default)]
    entries: HashMap<String, CacheEntry>,
    #[serde(default)]
    pub hits: u32,
    #[serde(default)]
    pub misses: u32,
}

impl LangCache {
    /// Load the cache from disk.
    pub fn load(path: &Path) -> LangCache {
        let mut cache: LangCache = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // Older caches have no timestamp. Treat them as resolved now so they
        // stay usable for one day instead of being discarded in one go.
        let now = now_secs();
        for entry in cache.entries.values_mut() {
            if entry.resolved_at == 0 {
                entry.resolved_at = now;
            }
        }
        cache
    }

    /// Save the cache to disk.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Look a movie up, treating answers older than `ttl_secs` as missing.
    /// A `ttl_secs` of zero disables expiry.
    pub fn get_with_ttl(&self, key: &str, ttl_secs: u64) -> Option<&CacheEntry> {
        let entry = self.entries.get(key)?;
        // Answers confirmed against an external database never expire.
        if entry.source == LangSource::Verified {
            return Some(entry);
        }
        if ttl_secs == 0 || now_secs().saturating_sub(entry.resolved_at) < ttl_secs {
            Some(entry)
        } else {
            None
        }
    }

    /// Store a resolved language.
    pub fn put(&mut self, key: String, mut entry: CacheEntry) {
        if entry.resolved_at == 0 {
            entry.resolved_at = now_secs();
        }
        self.entries.insert(key, entry);
    }

    /// How many answers are stored, fresh or stale.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// The Gemini client.
#[derive(Clone)]
pub struct Gemini {
    http: reqwest::Client,
    api_key: String,
    models: Vec<String>,
}

impl Gemini {
    /// Build a client. `models` is tried in order.
    pub fn new(api_key: &str, models: Vec<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .context("building Gemini HTTP client")?;
        Ok(Self {
            http,
            api_key: api_key.to_string(),
            models,
        })
    }

    /// Resolve the original language of every given movie.
    ///
    /// `keys` are `Movie|Year` cache keys. Each key is echoed back by the model
    /// so the answer can be matched to the right movie.
    pub async fn resolve_batch(&self, keys: &[String]) -> Result<HashMap<String, CacheEntry>> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let prompt = build_prompt(keys);
        let body = build_body(&prompt, keys.len());

        let mut attempt = 0u32;
        let outcome: Result<HashMap<String, CacheEntry>> = loop {
            let mut attempt_error: Option<anyhow::Error> = None;
            let mut settled = false;
            let mut index = 0usize;
            while index < self.models.len() {
                let model = self.models[index].clone();
                index += 1;
                let result = self.call(&model, &body).await;
                match result {
                    Ok(text) => {
                        let parsed = parse_reply(&text);
                        if !parsed.is_empty() {
                            return Ok(parsed);
                        }
                        attempt_error = Some(anyhow::anyhow!("{model} returned no usable rows"));
                        settled = true;
                        break;
                    }
                    Err(e) => {
                        attempt_error = Some(e.context(format!("model {model}")));
                    }
                }
            }
            if !settled || attempt_error.is_none() || attempt >= 2 {
                break Err(attempt_error.unwrap_or_else(|| anyhow::anyhow!("no models configured")));
            }
            tokio::time::sleep(std::time::Duration::from_millis(
                2000 * (attempt as u64 + 1),
            ))
            .await;
            attempt += 1;
        };
        outcome
    }

    /// Call one model and return the raw text of the first candidate.
    async fn call(&self, model: &str, body: &serde_json::Value) -> Result<String> {
        let url = format!("{API_ROOT}/{model}:generateContent?key={}", self.api_key);
        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .with_context(|| format!("requesting {model}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("{model} returned HTTP {status}: {}", truncate(&text, 300));
        }
        let value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("decoding {model} response"))?;
        if let Some(err) = value.get("error") {
            bail!("{model} error: {}", truncate(&err.to_string(), 300));
        }
        value
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .context("response contained no text")
    }

    /// Verify the key works by listing the models available to it.
    pub async fn probe(&self) -> Result<Vec<String>> {
        let url = format!("{API_ROOT}?key={}&pageSize=200", self.api_key);
        let resp = self.http.get(&url).send().await.context("listing models")?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!(
                "model list returned HTTP {status}: {}",
                truncate(&text, 200)
            );
        }
        let value: serde_json::Value =
            serde_json::from_str(&text).context("decoding model list")?;
        let mut names = Vec::new();
        if let Some(models) = value.get("models").and_then(|m| m.as_array()) {
            for m in models {
                let methods = m
                    .get("supportedGenerationMethods")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().any(|x| x.as_str() == Some("generateContent")))
                    .unwrap_or(false);
                if !methods {
                    continue;
                }
                if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                    names.push(name.trim_start_matches("models/").to_string());
                }
            }
        }
        Ok(names)
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect()
    }
}

/// Build the prompt asking for the original production language.
fn build_prompt(keys: &[String]) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are a film database assistant. For each movie listed, reply with the \
         ORIGINAL PRODUCTION LANGUAGE of the film.\n\n\
         Rules:\n\
         - The key is \"Movie Title|Year\"; the year disambiguates the film.\n\
         - The original language is the language the film was first shot in, not any dub, \
           dubbing language or subtitle language.\n\
         - Do not infer the language from the forum or from the audio tracks offered.\n\
         - For Indian films shot simultaneously in two languages, answer the dominant one.\n\
         - For Hollywood and other English-language films answer \"eng\".\n\
         - For Japanese films answer \"jpn\", for Korean films \"kor\".\n\
         - Answer with a three letter ISO 639-2/639-3 code \
           (tam, tel, hin, mal, kan, eng, ...).\n\
         - Never answer with a language the film was only dubbed into.\n\
         - If you do not know the film, give your single best guess anyway.\n\
         - Copy the \"key\" field back exactly as given, unchanged.\n\n\
         MOVIES:\n",
    );
    for key in keys {
        prompt.push_str(&format!("- key: {key}\n"));
    }
    prompt
}

/// Build the request body with a strict JSON schema.
fn build_body(prompt: &str, count: usize) -> serde_json::Value {
    serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": {
            "temperature": 0,
            "maxOutputTokens": 8192,
            "responseMimeType": "application/json",
            "responseSchema": {
                "type": "array",
                "minItems": 0,
                "maxItems": count.max(1),
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" },
                        "language": { "type": "string" },
                        "confidence": { "type": "string" }
                    },
                    "required": ["key", "language"]
                }
            }
        }
    })
}

/// Parse the model's JSON reply into cache entries keyed by movie key.
fn parse_reply(text: &str) -> HashMap<String, CacheEntry> {
    let mut out = HashMap::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
        return out;
    };
    let Some(rows) = value.as_array() else {
        return out;
    };
    for row in rows {
        let Some(key) = row.get("key").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(code) = row.get("language").and_then(|v| v.as_str()) else {
            continue;
        };
        let parsed = Lang::from_code(code);
        let lang = if parsed == Lang::Other {
            crate::lang::lang_from_token(code).unwrap_or(Lang::Other)
        } else {
            parsed
        };
        // Cache the answer even when the model could not name a language.
        // Otherwise that movie is treated as a cache miss and re-asked on
        // every single launch. An unknown answer is a real answer: "und".
        let (lang, confidence, source) = if lang == Lang::Other {
            (Lang::Other, 0.0, LangSource::Unknown)
        } else {
            let confidence = match row
                .get("confidence")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "high" => 0.9,
                "medium" => 0.65,
                "low" => 0.4,
                _ => 0.6,
            };
            (lang, confidence, LangSource::Ai)
        };
        out.insert(
            key.trim().to_string(),
            CacheEntry {
                lang,
                confidence,
                source,
                resolved_at: now_secs(),
            },
        );
    }
    out
}

/// The cache key for a movie.
pub fn cache_key(title: &str, year: Option<u16>) -> String {
    match year {
        Some(y) => format!("{title}|{y}"),
        None => title.to_string(),
    }
}

/// Resolve a thread's original language without asking the model.
///
/// The position of a language in a release name says nothing about the
/// production language: this board lists Tamil first even for films made in
/// another language. A title tag is therefore only a weak hint, and a
/// language forum contributes an even weaker one. The model resolves the
/// real answer and caches it for a day.
pub fn heuristic_original(thread: &Thread) -> OriginalLang {
    if !thread.releases.is_empty()
        && thread
            .releases
            .iter()
            .all(|r| r.langs.dubbed || r.langs.subtitled)
    {
        return OriginalLang {
            lang: Lang::Other,
            confidence: 0.0,
            source: LangSource::Unknown,
        };
    }
    if let Some(lang) = thread
        .releases
        .iter()
        .find_map(|r| r.langs.text.first().copied())
    {
        return OriginalLang {
            lang,
            confidence: 0.35,
            source: LangSource::TitleTag,
        };
    }
    OriginalLang {
        lang: thread.forum.lang,
        confidence: 0.25,
        source: LangSource::Forum,
    }
}

/// Split cache keys into batches that respect both an item and a character cap.
///
/// The character cap keeps each Gemini request well below the model's input
/// limit even when film titles are unusually long.
pub fn plan_batches(keys: &[String], max_items: usize, max_chars: usize) -> Vec<Vec<String>> {
    let max_items = max_items.max(1);
    let max_chars = max_chars.max(64);
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut chars = 0usize;
    for key in keys {
        let cost = key.len() + 8;
        if !current.is_empty() && (current.len() >= max_items || chars + cost > max_chars) {
            batches.push(std::mem::take(&mut current));
            chars = 0;
        }
        current.push(key.clone());
        chars += cost;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// The default cache location.
pub fn default_cache_path() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("drushti").join("langcache.json")
}

/// The original language of a thread, from the model, the cache or the heuristic.
pub fn effective_original(
    thread: &Thread,
    cache: &LangCache,
    key: &str,
    ttl_secs: u64,
) -> OriginalLang {
    if let Some(entry) = cache.get_with_ttl(key, ttl_secs) {
        return OriginalLang {
            lang: entry.lang,
            confidence: entry.confidence,
            source: entry.source,
        };
    }
    if let Some(original) = thread.original {
        return original;
    }
    heuristic_original(thread)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::LangSet;
    use crate::model::Release;
    use crate::quality::ReleaseInfo;

    fn release(audio: &[Lang], text: &[Lang]) -> Release {
        Release {
            name: String::new(),
            info: ReleaseInfo::default(),
            langs: LangSet {
                audio: audio.to_vec(),
                text: text.to_vec(),
                dubbed: false,
                subtitled: false,
            },
            magnet: None,
            direct: None,
            torrent: None,
        }
    }

    fn thread_with(releases: Vec<Release>) -> Thread {
        Thread {
            id: 1,
            title: "Test (2026)".to_string(),
            movie: "Test".to_string(),
            year: Some(2026),
            url: String::new(),
            forum: crate::forums::by_id(69).expect("kannada forum"),
            author: String::new(),
            posted: None,
            replies: 0,
            views: 0,
            releases,
            original: None,
        }
    }

    #[test]
    fn parses_a_reply() {
        let reply = r#"[{"key":"Leo|2023","language":"tam","confidence":"high"}]"#;
        let parsed = parse_reply(reply);
        let entry = parsed.get("Leo|2023").expect("entry");
        assert_eq!(entry.lang, Lang::Tamil);
        assert_eq!(entry.confidence, 0.9);
        assert!(entry.resolved_at > 0);
    }

    #[test]
    fn caches_unknown_codes_so_they_are_not_reasked() {
        let reply = r#"[{"key":"X","language":"zzz"}]"#;
        let parsed = parse_reply(reply);
        let entry = parsed.get("X").expect("unknown answer must be cached");
        assert_eq!(entry.lang, Lang::Other);
        assert_eq!(entry.source, LangSource::Unknown);
        assert_eq!(entry.confidence, 0.0);
    }

    #[test]
    fn cache_key_includes_year() {
        assert_eq!(cache_key("Leo", Some(2023)), "Leo|2023");
        assert_eq!(cache_key("Leo", None), "Leo");
    }

    #[test]
    fn multi_audio_order_is_not_the_original_language() {
        // Tamil first, like the board, but the title tag names Kannada.
        let thread = thread_with(vec![release(
            &[Lang::Tamil, Lang::Telugu, Lang::Kannada],
            &[Lang::Kannada],
        )]);
        let original = heuristic_original(&thread);
        assert_eq!(original.lang, Lang::Kannada);
        assert_eq!(original.source, LangSource::TitleTag);
        assert!(original.confidence < 0.5);
    }

    #[test]
    fn all_dubbed_is_unknown() {
        let mut release = release(&[Lang::Tamil, Lang::Telugu], &[]);
        release.langs.dubbed = true;
        let thread = thread_with(vec![release]);
        let original = heuristic_original(&thread);
        assert_eq!(original.source, LangSource::Unknown);
        assert_eq!(original.confidence, 0.0);
    }

    #[test]
    fn cache_entries_expire_after_seven_days() {
        let mut cache = LangCache::default();
        let stale = now_secs().saturating_sub(8 * 24 * 60 * 60);
        let fresh = now_secs().saturating_sub(6 * 24 * 60 * 60);
        cache.put(
            "old|2020".to_string(),
            CacheEntry {
                lang: Lang::Kannada,
                confidence: 0.9,
                source: LangSource::Ai,
                resolved_at: stale,
            },
        );
        cache.put(
            "new|2020".to_string(),
            CacheEntry {
                lang: Lang::Telugu,
                confidence: 0.9,
                source: LangSource::Ai,
                resolved_at: fresh,
            },
        );
        let ttl_7_days = 7 * 24 * 3600;
        assert!(cache.get_with_ttl("old|2020", ttl_7_days).is_none());
        assert!(cache.get_with_ttl("new|2020", ttl_7_days).is_some());
    }

    #[test]
    fn batches_respect_item_and_character_caps() {
        let keys: Vec<String> = (0..50).map(|i| format!("Movie {i}|2026")).collect();
        let batches = plan_batches(&keys, 10, 200);
        let total: usize = batches.iter().map(Vec::len).sum();
        assert_eq!(total, keys.len());
        for batch in &batches {
            assert!(batch.len() <= 10);
            let chars: usize = batch.iter().map(|k| k.len() + 8).sum();
            assert!(chars <= 200 || batch.len() == 1);
        }
    }
}
