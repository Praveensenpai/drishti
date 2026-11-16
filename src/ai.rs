//! Original-language resolution.
//!
//! Two sources of truth are combined:
//!
//! 1. **Heuristic** — a thread that offers a multi-audio release names the
//!    original language first, so it can be read straight off the release name.
//! 2. **Gemini** — for a thread that only offers single-language releases we
//!    cannot tell an original from a dub by reading the title, so the model is
//!    asked what language the film was made in.
//!
//! Answers are cached on disk, keyed by movie name and year.

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

/// A cached answer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub lang: Lang,
    pub confidence: f32,
    pub source: LangSource,
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
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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

    /// Look a movie up.
    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.entries.get(key)
    }

    /// Store a resolved language.
    pub fn put(&mut self, key: String, entry: CacheEntry) {
        self.entries.insert(key, entry);
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
    /// `movies` are `(key, display)` pairs. The key is echoed back by the
    /// model so the answer can be matched to the right movie.
    pub async fn resolve_batch(
        &self,
        movies: &[(String, String)],
    ) -> Result<HashMap<String, CacheEntry>> {
        if movies.is_empty() {
            return Ok(HashMap::new());
        }
        let prompt = build_prompt(movies);
        let body = build_body(&prompt, movies.len());

        let mut last_error: Option<anyhow::Error> = None;
        for model in &self.models {
            match self.call(model, &body).await {
                Ok(text) => {
                    let parsed = parse_reply(&text);
                    if parsed.is_empty() {
                        last_error = Some(anyhow::anyhow!("{model} returned no usable rows"));
                        continue;
                    }
                    return Ok(parsed);
                }
                Err(e) => {
                    last_error = Some(e.context(format!("model {model}")));
                }
            }
        }
        match last_error {
            Some(e) => Err(e),
            None => bail!("no models configured"),
        }
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
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}...", &text[..max])
    }
}

/// Build the prompt asking for the original production language.
fn build_prompt(movies: &[(String, String)]) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are a film database assistant. For each movie listed, reply with the \
         ORIGINAL PRODUCTION LANGUAGE of the film.\n\n\
         Rules:\n\
         - The original language is the language the film was first shot in, not any dub, \
           dubbing language or subtitle language.\n\
         - For Indian films shot simultaneously in two languages, answer the dominant one.\n\
         - For Hollywood and other English-language films answer \"eng\".\n\
         - For Japanese films answer \"jpn\", for Korean films \"kor\".\n\
         - Answer using a two letter ISO 639-1 code (tam, tel, hin, mal, kan, eng, ...).\n\
         - Never answer with a language the film was only dubbed into.\n\
         - If you do not know the film, give your single best guess anyway.\n\
         - Copy the \"key\" field back exactly as given, unchanged.\n\n\
         MOVIES:\n",
    );
    for (key, display) in movies {
        prompt.push_str(&format!("- key: {key}  movie: {display}\n"));
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
        let lang = Lang::from_code(code);
        if lang == Lang::Other {
            continue;
        }
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
        out.insert(
            key.trim().to_string(),
            CacheEntry {
                lang,
                confidence,
                source: LangSource::Ai,
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
/// A thread that offers a multi-audio release names the original language
/// first, so the answer can be read off the release name. A single-language
/// release in a language forum is assumed to be in that forum's language.
pub fn heuristic_original(thread: &Thread) -> OriginalLang {
    for release in &thread.releases {
        if release.langs.is_multi_audio() {
            return OriginalLang {
                lang: release.langs.audio[0],
                confidence: 0.8,
                source: LangSource::MultiAudio,
            };
        }
    }
    if !thread.releases.is_empty() {
        let all_dubbed = thread
            .releases
            .iter()
            .all(|r| r.langs.dubbed || r.langs.subtitled);
        if all_dubbed {
            return OriginalLang {
                lang: thread.forum.lang,
                confidence: 0.0,
                source: LangSource::Unknown,
            };
        }
        if let Some(lang) = thread.releases[0].langs.text.first() {
            return OriginalLang {
                lang: *lang,
                confidence: 0.45,
                source: LangSource::TitleTag,
            };
        }
    }
    OriginalLang {
        lang: thread.forum.lang,
        confidence: 0.25,
        source: LangSource::Forum,
    }
}

/// The default cache location.
pub fn default_cache_path() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("drishti").join("langcache.json")
}

/// The original language of a thread, from the model, the cache or the heuristic.
pub fn effective_original(thread: &Thread, cache: &LangCache, key: &str) -> OriginalLang {
    if let Some(original) = thread.original {
        return original;
    }
    if let Some(entry) = cache.get(key) {
        return OriginalLang {
            lang: entry.lang,
            confidence: entry.confidence,
            source: entry.source,
        };
    }
    heuristic_original(thread)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_reply() {
        let reply = r#"[{"key":"Leo|2023","language":"ta","confidence":"high"}]"#;
        let parsed = parse_reply(reply);
        let entry = parsed.get("Leo|2023").expect("entry");
        assert_eq!(entry.lang, Lang::Tamil);
        assert_eq!(entry.confidence, 0.9);
    }

    #[test]
    fn ignores_unknown_codes() {
        let reply = r#"[{"key":"X","language":"zzz"}]"#;
        assert!(parse_reply(reply).is_empty());
    }

    #[test]
    fn cache_key_includes_year() {
        assert_eq!(cache_key("Leo", Some(2023)), "Leo|2023");
        assert_eq!(cache_key("Leo", None), "Leo");
    }
}
