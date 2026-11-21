//! Startup board-domain discovery.
//!
//! Probes the configured board with the same listing request the scraper uses.
//! When that probe fails, a Bing search finds the current domain, which the
//! caller can persist to the configuration file.

mod cooldown;
mod parse;

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::ansi;
use crate::forums;
use crate::site;

/// Probe timeout. Short enough that a dead domain does not stall startup.
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const BOARD_MARKER: &str = "ipsdataitem";

/// Network helper plus the failure cooldown.
pub struct Discovery {
    http: reqwest::Client,
    cooldown: cooldown::Cooldown,
}

impl Discovery {
    pub fn new(cache_dir: &Path) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(site::USER_AGENT)
            .timeout(PROBE_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .context("building discovery HTTP client")?;
        Ok(Self {
            http,
            cooldown: cooldown::Cooldown::new(cache_dir),
        })
    }

    /// Return a replacement base URL when the configured one is unreachable.
    pub async fn resolve(
        &self,
        current: &str,
        query: &str,
        search_endpoint: &str,
    ) -> Result<Option<String>> {
        if self.probe(&probe_url(current)).await {
            return Ok(None);
        }
        eprintln!(
            "{} {}",
            ansi::cross(),
            ansi::warn(&format!("configured board {current} is unreachable"))
        );
        if self.cooldown.is_active() {
            return Ok(None);
        }
        let mut origins = self.find_origins(query, search_endpoint).await;
        origins.truncate(10);
        for candidate in origins {
            if self.probe(&probe_url(&candidate.origin)).await {
                println!(
                    "{} {}",
                    ansi::success(ansi::check()),
                    ansi::accent(&format!("discovered board domain {}", candidate.origin))
                );
                return Ok(Some(candidate.origin));
            }
            eprintln!(
                "{} {}",
                ansi::cross(),
                ansi::muted(&format!(
                    "discovered domain {} failed its probe",
                    candidate.origin
                ))
            );
        }
        eprintln!(
            "{} {}",
            ansi::cross(),
            ansi::warn("no working 1tamilmv domain found in the search results")
        );
        self.cooldown.mark();
        Ok(None)
    }

    /// Search one endpoint for board domains in both links and result text.
    async fn find_origins(&self, query: &str, endpoint: &str) -> Vec<parse::Candidate> {
        let Ok(html) = self.search(query, endpoint).await else {
            return Vec::new();
        };
        let candidates = parse::extract_candidates(&html);
        let mut origins = parse::board_origins(&candidates);
        for text in parse::extract_text_domains(&html) {
            if !origins.iter().any(|c| c.origin == text.origin) {
                origins.push(text);
            }
        }
        origins
    }

    /// A listing request is considered healthy only when it returns the marker
    /// the scraper itself expects. This rejects parked and challenge pages.
    async fn probe(&self, url: &str) -> bool {
        let response = match self.http.get(url).send().await {
            Ok(response) => response,
            Err(error) => {
                eprintln!(
                    "{} {}",
                    ansi::muted("probe failed"),
                    ansi::muted(&format!("{url}: {error}"))
                );
                return false;
            }
        };
        if !response.status().is_success() {
            eprintln!(
                "{} {}",
                ansi::muted("probe failed"),
                ansi::muted(&format!("{url} returned HTTP {}", response.status()))
            );
            return false;
        }
        match response.text().await {
            Ok(body) => body.to_ascii_lowercase().contains(BOARD_MARKER),
            Err(error) => {
                eprintln!(
                    "{} {}",
                    ansi::muted("probe failed"),
                    ansi::muted(&format!("{url}: {error}"))
                );
                false
            }
        }
    }

    /// Fetch the search results page for the configured query.
    async fn search(&self, query: &str, endpoint: &str) -> Result<String> {
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let url = format!("{endpoint}?q={encoded}&num=12&hl=en&start=0&safe=active");
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .context("searching for the board domain")?;
        if !response.status().is_success() {
            anyhow::bail!("search returned HTTP {}", response.status());
        }
        response.text().await.context("reading search results")
    }
}

/// Build the same forum URL the application itself fetches.
fn probe_url(base: &str) -> String {
    let Some(forum) = forums::all_web_hd().first().copied() else {
        return base.trim_end_matches('/').to_string();
    };
    site::forum_page_url(base, forum, 1)
}
