//! Configuration, persisted as TOML under the user's config directory.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::quality::Quality;

/// The user's configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Board address.
    pub base_url: String,
    /// Gemini API key. Falls back to `GEMINI_API_KEY` from the environment.
    pub gemini_api_key: String,
    /// Models tried in order when resolving original languages.
    pub gemini_models: Vec<String>,
    /// Quality buckets that are shown. Digital/HD rips allowed by default;
    /// DVDRip, CAM and theater prints are excluded.
    pub allowed_qualities: Vec<Quality>,
    /// Hide releases that do not carry the film's original language.
    pub original_only: bool,
    /// Resolve original languages with the model as soon as a page is loaded.
    pub resolve_on_load: bool,
    /// How long (in minutes) fetched forum/thread pages are cached on disk.
    /// The board rarely updates, so 30 minutes is a sensible default; set 0 to
    /// disable caching.
    pub cache_ttl_minutes: u32,
    /// Check for a newer source build once a day (on first launch) and update
    /// in the background. Requires an install that recorded its source dir.
    pub auto_update: bool,
    /// Number of movies sent to the model in one request.
    pub ai_batch_size: usize,
    /// Rough character cap for one model request. Titles are short, but this
    /// keeps pathological batches far below the model's input limit.
    pub ai_batch_chars: usize,
    /// Pause between model batches, in milliseconds, to avoid rate limits.
    pub ai_batch_pause_ms: u64,
    /// How long (in hours) a model-resolved original language stays fresh.
    /// Set 0 to keep answers forever.
    pub ai_cache_hours: u64,
    /// Optional HTTP endpoint to POST a selected release's magnet link to.
    /// Leave empty to disable (not everyone runs ryoiki). When set, the
    /// selected release's magnet is POSTed as JSON `{"magnet": "..."}`.
    pub magnet_post_url: String,
    /// Probe the board at startup and rediscover its domain when it is down.
    #[serde(default = "default_auto_discover_domain")]
    pub auto_discover_domain: bool,
    /// Query sent to the search engine when the board domain is unreachable.
    #[serde(default = "default_domain_search_query")]
    pub domain_search_query: String,
    /// Search endpoint used for domain discovery.
    #[serde(default = "default_domain_search_endpoint")]
    pub domain_search_endpoint: String,
}

fn default_domain_search_endpoint() -> String {
    "https://www.bing.com/search".to_string()
}

fn default_auto_discover_domain() -> bool {
    true
}

fn default_domain_search_query() -> String {
    "1tamilmv".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: crate::site::DEFAULT_BASE_URL.to_string(),
            gemini_api_key: String::new(),
            gemini_models: crate::ai::MODEL_CHAIN
                .iter()
                .map(|s| s.to_string())
                .collect(),
            allowed_qualities: vec![
                Quality::WebDl,
                Quality::HdRip,
                Quality::WebRip,
                Quality::Bluray,
                Quality::TvRip,
                Quality::Unknown,
            ],
            original_only: false,
            resolve_on_load: true,
            cache_ttl_minutes: 30,
            auto_update: true,
            ai_batch_size: 24,
            ai_batch_chars: 6000,
            ai_batch_pause_ms: 1500,
            ai_cache_hours: 24,
            magnet_post_url: String::new(),
            auto_discover_domain: default_auto_discover_domain(),
            domain_search_query: default_domain_search_query(),
            domain_search_endpoint: default_domain_search_endpoint(),
        }
    }
}

/// Read a line from stdin, using `default` when the user just presses Enter.
fn prompt_line(label: &str, default: &str) -> Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let entered = line.trim();
    Ok(if entered.is_empty() {
        default.to_string()
    } else {
        entered.to_string()
    })
}

/// Ask a yes/no question, returning `default` on an empty answer.
fn prompt_bool(label: &str, default: bool) -> Result<bool> {
    let hint = if default { "Y/n" } else { "y/N" };
    let answer = prompt_line(
        &format!("{label} ({hint})"),
        if default { "y" } else { "n" },
    )?;
    Ok(matches!(
        answer.to_ascii_lowercase().as_str(),
        "y" | "yes" | "true" | "1"
    ))
}

/// Ask for a number, falling back to `default` on empty/invalid input.
fn prompt_u64(label: &str, default: u64) -> Result<u64> {
    let answer = prompt_line(label, &default.to_string())?;
    Ok(answer.parse::<u64>().unwrap_or(default))
}

/// Parse a comma-separated list of quality names.
fn parse_qualities(input: &str) -> Vec<Quality> {
    let all = [
        Quality::WebDl,
        Quality::HdRip,
        Quality::WebRip,
        Quality::Bluray,
        Quality::TvRip,
        Quality::DvdRip,
        Quality::HdRip,
        Quality::Theater,
        Quality::Cam,
        Quality::Unknown,
    ];
    let wanted: Vec<String> = input.split(',').map(|s| s.trim().to_string()).collect();
    all.into_iter()
        .filter(|q| wanted.iter().any(|w| w.eq_ignore_ascii_case(q.label())))
        .collect()
}

impl Config {
    /// True when stdin is interactive, so a wizard can safely ask questions.
    pub fn interactive() -> bool {
        io::stdin().is_terminal()
    }

    /// Run the first-run / reconfigure wizard, updating `config` in place.
    pub fn configure(config: &mut Config) -> Result<()> {
        println!("\ndrushti setup — press Enter to accept the value in [brackets]\n");

        config.base_url = prompt_line("Board URL", &config.base_url)?;

        let key_default = if config.gemini_api_key.trim().is_empty() {
            "(none)"
        } else {
            "(already set — enter to keep)"
        };
        let key = prompt_line("Gemini API key", key_default)?;
        if key != key_default && !key.trim().is_empty() {
            config.gemini_api_key = key.trim().to_string();
        }

        let models = config.gemini_models.join(", ");
        let models = prompt_line("Gemini models (comma separated)", &models)?;
        config.gemini_models = models
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let qualities = config
            .allowed_qualities
            .iter()
            .map(|q| q.label())
            .collect::<Vec<_>>()
            .join(", ");
        let qualities = prompt_line("Allowed qualities (comma separated)", &qualities)?;
        let parsed = parse_qualities(&qualities);
        if !parsed.is_empty() {
            config.allowed_qualities = parsed;
        }

        config.original_only = prompt_bool(
            "Only show original-language dubs filtered",
            config.original_only,
        )?;
        config.resolve_on_load =
            prompt_bool("Resolve original languages on load", config.resolve_on_load)?;
        config.auto_update = prompt_bool("Check for updates once a day", config.auto_update)?;
        config.cache_ttl_minutes =
            prompt_u64("Page cache TTL (minutes)", config.cache_ttl_minutes as u64)? as u32;
        config.ai_cache_hours = prompt_u64("AI answer cache TTL (hours)", config.ai_cache_hours)?;
        config.ai_batch_size = prompt_u64("AI batch size", config.ai_batch_size as u64)? as usize;

        let magnet_default = if config.magnet_post_url.trim().is_empty() {
            "(none)".to_string()
        } else {
            config.magnet_post_url.clone()
        };
        let magnet = prompt_line("ryoiki magnet endpoint (blank to disable)", &magnet_default)?;
        config.magnet_post_url = if magnet == "(none)" {
            String::new()
        } else {
            magnet.trim().to_string()
        };
        Self::configure_discovery(config)?;

        println!("\nsetup complete\n");
        Ok(())
    }

    /// Ask about the domain discovery settings.
    fn configure_discovery(config: &mut Config) -> Result<()> {
        config.auto_discover_domain = prompt_bool(
            "Discover the board domain at startup",
            config.auto_discover_domain,
        )?;
        let query = prompt_line("Domain search query", &config.domain_search_query)?;
        if !query.trim().is_empty() {
            config.domain_search_query = query.trim().to_string();
        }
        Ok(())
    }

    /// Load the config, falling back to defaults when the file is missing.
    pub fn load(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut config: Config =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if config.gemini_api_key.trim().is_empty() {
            config.gemini_api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        }
        Ok(config)
    }

    /// Save the config.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serialising config")?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// The API key, taking the environment into account.
    pub fn api_key(&self) -> String {
        if !self.gemini_api_key.trim().is_empty() {
            return self.gemini_api_key.trim().to_string();
        }
        std::env::var("GEMINI_API_KEY")
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

/// The default config file location.
pub fn default_config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("drushti").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_allows_digital_but_not_dvd_cam_theater() {
        let config = Config::default();
        assert!(config.allowed_qualities.contains(&Quality::WebDl));
        assert!(config.allowed_qualities.contains(&Quality::HdRip));
        assert!(!config.allowed_qualities.contains(&Quality::DvdRip));
        assert!(!config.allowed_qualities.contains(&Quality::Theater));
        assert!(!config.allowed_qualities.contains(&Quality::Cam));
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.base_url, config.base_url);
        assert_eq!(parsed.allowed_qualities, config.allowed_qualities);
    }

    #[test]
    fn discovery_settings_default_when_absent() {
        let config: Config = toml::from_str("base_url = \"https://example.invalid\"").unwrap();
        assert!(config.auto_discover_domain);
        assert_eq!(config.domain_search_query, "1tamilmv");
    }
}
