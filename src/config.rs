//! Configuration, persisted as TOML under the user's config directory.

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
    /// Where downloads are written.
    pub download_dir: PathBuf,
    /// Gemini API key. Falls back to `GEMINI_API_KEY` from the environment.
    pub gemini_api_key: String,
    /// Models tried in order when resolving original languages.
    pub gemini_models: Vec<String>,
    /// Quality buckets that are shown. WEB-DL only by default.
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
    /// Which external torrent client to hand magnets to.
    pub torrent_client: String,
    /// Number of movies sent to the model in one request.
    pub ai_batch_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: crate::site::DEFAULT_BASE_URL.to_string(),
            download_dir: default_download_dir(),
            gemini_api_key: String::new(),
            gemini_models: crate::ai::MODEL_CHAIN.iter().map(|s| s.to_string()).collect(),
            allowed_qualities: vec![Quality::WebDl],
            original_only: false,
            resolve_on_load: true,
            cache_ttl_minutes: 30,
            auto_update: true,
            torrent_client: String::new(),
            ai_batch_size: 24,
        }
    }
}

impl Config {
    /// Load the config, falling back to defaults when the file is missing.
    pub fn load(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut config: Config = toml::from_str(&text)
            .with_context(|| format!("parsing {}", path.display()))?;
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
        std::env::var("GEMINI_API_KEY").unwrap_or_default().trim().to_string()
    }

}

/// The default config file location.
pub fn default_config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("drishti").join("config.toml")
}

/// The default download directory.
pub fn default_download_dir() -> PathBuf {
    let base = dirs::video_dir()
        .or_else(dirs::download_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("tamilmv")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_web_dl_only() {
        let config = Config::default();
        assert!(config.allowed_qualities.contains(&Quality::WebDl));
        assert!(!config.allowed_qualities.contains(&Quality::DvdRip));
        assert!(!config.allowed_qualities.contains(&Quality::Theater));
        assert!(!config.allowed_qualities.contains(&Quality::HdRip));
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.base_url, config.base_url);
        assert_eq!(parsed.allowed_qualities, config.allowed_qualities);
    }
}
