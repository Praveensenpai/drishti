//! Scraping layer for 1TamilMV.
//!
//! The board runs Invision Community. Thread rows live in `li.ipsDataItem`
//! elements, and each thread post lists its releases as a torrent attachment
//! followed by a magnet and a direct link.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::forums::Forum;
use crate::lang::{LangSet, parse_release_langs};
use crate::model::{Release, StoredThread, Thread, split_title};
use crate::quality::analyse;

/// Default board address.
pub const DEFAULT_BASE_URL: &str = "https://www.1tamilmv.ceo";

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// HTTP client for the board.
#[derive(Clone)]
pub struct Client {
    pub(crate) http: reqwest::Client,
    base_url: String,
    cache_dir: PathBuf,
    cache_ttl: Duration,
    thread_cache_dir: PathBuf,
    thread_cache_ttl: Duration,
    page_cache_dir: PathBuf,
}

impl Client {
    /// Build a client for the given board address.
    pub fn new(base_url: &str, cache_dir: PathBuf, cache_ttl: Duration) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(45))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(8))
            .build()
            .context("failed to build HTTP client")?;
        let base = cache_dir.parent().unwrap_or(&cache_dir);
        let thread_cache_dir = base.join("threads");
        let page_cache_dir = base.join("pages");
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            cache_dir,
            cache_ttl,
            thread_cache_dir,
            thread_cache_ttl: cache_ttl,
            page_cache_dir,
        })
    }

    /// Fetch a URL and return the body, using the on-disk cache when fresh.
    pub async fn get(&self, url: &str) -> Result<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Try the cache first.
        if self.cache_ttl.as_secs() > 0 {
            let path = self.cache_path_for(url);
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(entry) = serde_json::from_str::<HttpCacheEntry>(&text)
                && now.saturating_sub(entry.saved) < self.cache_ttl.as_secs()
            {
                return Ok(entry.body);
            }
        }

        let resp = self
            .http
            .get(url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", format!("{}/index.php?/forums/", self.base_url))
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;
        let status = resp.status();
        let body = resp.text().await.context("failed to read response body")?;
        if !status.is_success() {
            bail!("GET {url} returned HTTP {status}");
        }

        // Store in cache.
        if self.cache_ttl.as_secs() > 0 {
            let path = self.cache_path_for(url);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let entry = HttpCacheEntry {
                saved: now,
                body: body.clone(),
            };
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = std::fs::write(&path, json);
            }
        }

        Ok(body)
    }

    fn cache_path_for(&self, url: &str) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        self.cache_dir.join(format!("{:x}.json", h.finish()))
    }

    /// The URL of a forum page.
    pub fn forum_url(&self, forum: &Forum, page: u32) -> String {
        if page <= 1 {
            format!(
                "{}/index.php?/forums/forum/{}-{}/",
                self.base_url, forum.id, forum.slug
            )
        } else {
            format!(
                "{}/index.php?/forums/forum/{}-{}/page/{}/",
                self.base_url, forum.id, forum.slug, page
            )
        }
    }

    /// Fetch one page of a forum.
    pub async fn fetch_forum(&self, forum: &'static Forum, page: u32) -> Result<ForumPage> {
        let url = self.forum_url(forum, page);
        let body = self.get(&url).await?;
        parse_forum(&body, forum, page)
    }

    /// Fetch a forum page and enrich every thread with its releases.
    ///
    /// The fully enriched page is cached as one unit, so reopening a page is
    /// instant even when it carries twenty-five threads.
    pub async fn fetch_page(&self, forum: &'static Forum, page: u32) -> Result<PageLoad> {
        let now = now_secs();
        if self.cache_ttl.as_secs() > 0 {
            let path = self.page_cache_path(forum, page);
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(entry) = serde_json::from_str::<PageCacheEntry>(&text)
                && entry.version == PAGE_CACHE_VERSION
                && now.saturating_sub(entry.saved) < self.cache_ttl.as_secs()
            {
                let threads: Vec<Thread> = entry
                    .threads
                    .into_iter()
                    .filter_map(StoredThread::into_thread)
                    .collect();
                return Ok(PageLoad {
                    threads,
                    page,
                    total_pages: entry.total_pages,
                    from_cache: true,
                });
            }
        }

        let listing = self.fetch_forum(forum, page).await?;
        let threads = self.enrich_threads(&listing.threads).await;

        if self.cache_ttl.as_secs() > 0 {
            let path = self.page_cache_path(forum, page);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let entry = PageCacheEntry {
                version: PAGE_CACHE_VERSION,
                saved: now,
                total_pages: listing.total_pages,
                threads: threads.iter().map(StoredThread::from_thread).collect(),
            };
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = std::fs::write(&path, json);
            }
        }

        Ok(PageLoad {
            threads,
            page,
            total_pages: listing.total_pages,
            from_cache: false,
        })
    }

    /// Fetch every thread's releases with a bounded number of requests.
    async fn enrich_threads(&self, threads: &[Thread]) -> Vec<Thread> {
        let jobs = threads.iter().cloned().enumerate().map(|(index, thread)| {
            let client = self.clone();
            async move {
                client
                    .fetch_thread(&thread)
                    .await
                    .ok()
                    .map(|updated| (index, updated))
            }
        });
        let results: Vec<Option<(usize, Thread)>> = futures_util::stream::iter(jobs)
            .buffer_unordered(6)
            .collect()
            .await;
        let mut out = threads.to_vec();
        for (index, updated) in results.into_iter().flatten() {
            if let Some(slot) = out.get_mut(index) {
                *slot = updated;
            }
        }
        out
    }

    fn page_cache_path(&self, forum: &Forum, page: u32) -> PathBuf {
        self.page_cache_dir
            .join(format!("{}-{page}.json", forum.id))
    }

    /// Fetch a thread and its releases, using the parsed-thread cache when fresh.
    pub async fn fetch_thread(&self, thread: &Thread) -> Result<Thread> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Try the parsed-thread cache first: no HTTP, no parsing.
        if self.thread_cache_ttl.as_secs() > 0 {
            let path = self.thread_cache_path(&thread.url);
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(entry) = serde_json::from_str::<ThreadCacheEntry>(&text)
                && now.saturating_sub(entry.saved) < self.thread_cache_ttl.as_secs()
            {
                let mut updated = thread.clone();
                updated.releases = entry.releases;
                if entry.original.is_some() {
                    updated.original = entry.original;
                }
                return Ok(updated);
            }
        }

        let body = self.get(&thread.url).await?;
        let mut updated = thread.clone();
        updated.releases = parse_releases(&body)?;

        // Store the parsed releases so the next visit is instant.
        if self.thread_cache_ttl.as_secs() > 0 {
            let path = self.thread_cache_path(&thread.url);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let entry = ThreadCacheEntry {
                saved: now,
                releases: updated.releases.clone(),
                original: updated.original,
            };
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = std::fs::write(&path, json);
            }
        }

        Ok(updated)
    }

    fn thread_cache_path(&self, url: &str) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        self.thread_cache_dir.join(format!("{:x}.json", h.finish()))
    }
}

/// A page of thread listings.
#[derive(Debug)]
pub struct ForumPage {
    pub threads: Vec<Thread>,
    pub total_pages: u32,
}

/// A fully enriched forum page.
#[derive(Debug)]
pub struct PageLoad {
    pub threads: Vec<Thread>,
    pub page: u32,
    pub total_pages: u32,
    pub from_cache: bool,
}

// ---------------------------------------------------------------------------
// Forum listing
// ---------------------------------------------------------------------------

/// Parse a forum page into thread rows.
pub fn parse_forum(body: &str, forum: &'static Forum, page: u32) -> Result<ForumPage> {
    let doc = Html::parse_document(body);
    let item_sel = Selector::parse("li.ipsDataItem").context("bad selector")?;
    let topic_sel = Selector::parse("a[href*='/forums/topic/']").context("bad selector")?;
    let time_sel = Selector::parse("time").context("bad selector")?;
    let author_sel =
        Selector::parse(".ipsDataItem_meta a, .ipsDataItem_meta").context("bad selector")?;
    let reply_sel =
        Selector::parse("li[data-statType='forums_comments'] .ipsDataItem_stats_number")
            .context("bad selector")?;
    let view_sel = Selector::parse("li[data-statType='num_views'] .ipsDataItem_stats_number")
        .context("bad selector")?;

    let mut threads = Vec::new();
    for item in doc.select(&item_sel) {
        let Some(anchor) = item.select(&topic_sel).next() else {
            continue;
        };
        let Some(href) = anchor.attr("href") else {
            continue;
        };
        let title = anchor
            .attr("title")
            .map(|t| t.trim().to_string())
            .unwrap_or_else(|| text_of(&anchor));
        if title.is_empty() {
            continue;
        }

        let id = extract_topic_id(href).unwrap_or(0);
        let (movie, year, _rest) = split_title(&title);

        let posted = item
            .select(&time_sel)
            .next()
            .and_then(|t| t.attr("datetime"))
            .map(|s| s.to_string());
        let replies = item
            .select(&reply_sel)
            .next()
            .map(|e| parse_count(&text_of(&e)))
            .unwrap_or(0);
        let views = item
            .select(&view_sel)
            .next()
            .map(|e| parse_count(&text_of(&e)))
            .unwrap_or(0);
        let author = item
            .select(&author_sel)
            .next()
            .map(|e| {
                text_of(&e)
                    .trim()
                    .trim_start_matches("By")
                    .trim()
                    .trim_end_matches(',')
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        threads.push(Thread {
            id,
            title,
            movie,
            year,
            url: href.to_string(),
            forum,
            author,
            posted,
            replies,
            views,
            releases: Vec::new(),
            original: None,
        });
    }

    let total_pages = parse_total_pages(body).unwrap_or(page.max(1));
    Ok(ForumPage {
        threads,
        total_pages,
    })
}

/// The concatenated text of an element.
fn text_of(element: &ElementRef) -> String {
    element.text().collect::<String>()
}

/// Pull the topic id out of a topic URL.
fn extract_topic_id(href: &str) -> Option<u64> {
    let idx = href.find("/forums/topic/")? + "/forums/topic/".len();
    let rest = &href[idx..];
    let end = rest.find('-')?;
    rest[..end].parse().ok()
}

/// Parse counts that may be abbreviated, e.g. `82k`.
fn parse_count(text: &str) -> u32 {
    let t = text.trim();
    let (num, mult) = match t.chars().last() {
        Some('k') | Some('K') => (&t[..t.len() - 1], 1_000u32),
        Some('m') | Some('M') => (&t[..t.len() - 1], 1_000_000u32),
        _ => (t, 1u32),
    };
    num.trim()
        .parse::<f64>()
        .map(|f| (f * mult as f64) as u32)
        .unwrap_or(0)
}

/// Find the "Page X of Y" text to learn the page count.
fn parse_total_pages(body: &str) -> Option<u32> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Page\s+\d+\s+of\s+(\d+)").unwrap());
    RE.captures(body)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

// ---------------------------------------------------------------------------
// Releases inside a thread
// ---------------------------------------------------------------------------

/// Matches a single anchor element.
static ANCHOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<a\b([^>]*)>(.*?)</a>"#).unwrap());

/// Matches the href attribute of an anchor.
static HREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\bhref\s*=\s*"([^"]*)""#).unwrap());

/// What an anchor element is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnchorKind {
    Torrent,
    Magnet,
    Direct,
    Other,
}

/// An anchor found in the post body.
struct Anchor {
    start: usize,
    end: usize,
    kind: AnchorKind,
    url: String,
    text: String,
}

/// Parse the releases out of a thread page.
pub fn parse_releases(body: &str) -> Result<Vec<Release>> {
    let doc = Html::parse_document(body);
    let content_sel = Selector::parse("[data-role='commentContent']").context("bad selector")?;
    let Some(content) = doc.select(&content_sel).next() else {
        return Ok(Vec::new());
    };
    let html = content.inner_html();

    let anchors = collect_anchors(&html);
    let relevant: Vec<usize> = anchors
        .iter()
        .enumerate()
        .filter(|(_, a)| a.kind != AnchorKind::Other)
        .map(|(i, _)| i)
        .collect();

    // A release always starts with its torrent attachment, followed by the
    // magnet and the direct link. A release with no torrent attachment starts
    // at its magnet instead.
    let mut starts: Vec<usize> = Vec::new();
    let mut seen_magnet = false;
    for position in &relevant {
        let anchor = &anchors[*position];
        match anchor.kind {
            AnchorKind::Torrent => {
                starts.push(*position);
                seen_magnet = false;
            }
            AnchorKind::Magnet => {
                if seen_magnet {
                    starts.push(*position);
                }
                seen_magnet = true;
            }
            _ => {}
        }
    }
    if starts.is_empty() {
        return Ok(Vec::new());
    }

    let mut releases = Vec::new();
    for (group, start_anchor) in starts.iter().enumerate() {
        let end_anchor = starts.get(group + 1).copied().unwrap_or(usize::MAX);

        let mut magnet = None;
        let mut direct = None;
        let mut torrent = None;
        let mut torrent_name = None;
        for position in &relevant {
            if *position < *start_anchor {
                continue;
            }
            if *position >= end_anchor {
                break;
            }
            let anchor = &anchors[*position];
            match anchor.kind {
                AnchorKind::Magnet => {
                    magnet.get_or_insert_with(|| anchor.url.clone());
                }
                AnchorKind::Direct => {
                    direct.get_or_insert_with(|| anchor.url.clone());
                }
                AnchorKind::Torrent => {
                    torrent = Some(anchor.url.clone());
                    torrent_name = Some(anchor.text.clone());
                }
                AnchorKind::Other => {}
            }
        }

        // The header of the group is the prose between the previous group and
        // this one's first link.
        let group_anchor = &anchors[*start_anchor];
        let header_start = if group == 0 {
            0
        } else {
            let mut previous_end = 0usize;
            for position in &relevant {
                if *position < *start_anchor {
                    previous_end = anchors[*position].end;
                }
            }
            previous_end
        };
        let header = strip_tags(&html[header_start..group_anchor.start]);

        if let Some(release) = build_release(header, torrent_name, magnet, direct, torrent) {
            releases.push(release);
        }
    }

    Ok(releases)
}

/// Find every anchor in a fragment and classify it.
fn collect_anchors(html: &str) -> Vec<Anchor> {
    let mut anchors = Vec::new();
    for capture in ANCHOR_RE.captures_iter(html) {
        let whole = capture.get(0).expect("whole match");
        let attrs = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
        let inner = capture.get(2).map(|m| m.as_str()).unwrap_or_default();
        let url = HREF_RE
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| decode_entities(m.as_str()))
            .unwrap_or_default();

        let kind = if url.starts_with("magnet:") {
            // Magnets on this board are sometimes plain <a href="magnet:…">
            // with no class, so the URL itself is the reliable signal.
            AnchorKind::Magnet
        } else if attrs.contains("data-fileext=\"torrent\"")
            || attrs.contains("data-fileext=torrent")
        {
            AnchorKind::Torrent
        } else if attrs.contains("download-button") {
            AnchorKind::Direct
        } else {
            AnchorKind::Other
        };

        anchors.push(Anchor {
            start: whole.start(),
            end: whole.end(),
            kind,
            url,
            text: strip_tags(inner),
        });
    }
    anchors
}

/// Assemble a release from the parts of a group.
fn build_release(
    header: String,
    torrent_name: Option<String>,
    magnet: Option<String>,
    direct: Option<String>,
    torrent: Option<String>,
) -> Option<Release> {
    if magnet.is_none() && direct.is_none() && torrent.is_none() {
        return None;
    }
    let torrent_name = torrent_name.unwrap_or_default();

    // The magnet's `dn` parameter is the exact release filename.
    let dn = magnet.as_deref().and_then(magnet_display_name);
    let raw_name = [
        dn.as_deref(),
        Some(torrent_name.as_str()),
        Some(header.as_str()),
    ]
    .into_iter()
    .flatten()
    .find(|s| !s.trim().is_empty())
    .unwrap_or("")
    .trim()
    .to_string();

    let name = clean_release_name(&raw_name);

    // The magnet name, the torrent file name and the header all describe the
    // same release but abbreviate it differently, so the quality is read from
    // all three together.
    let combined = [
        dn.as_deref().unwrap_or_default(),
        torrent_name.as_str(),
        header.as_str(),
    ]
    .join(" ~ ");

    let mut langs: LangSet = parse_release_langs(&header);
    let extra = parse_release_langs(&raw_name);
    for l in extra.audio {
        if !langs.audio.contains(&l) {
            langs.audio.push(l);
        }
    }
    for l in extra.text {
        if !langs.text.contains(&l) {
            langs.text.push(l);
        }
    }
    langs.dubbed |= extra.dubbed;
    langs.subtitled |= extra.subtitled;

    Some(Release {
        name,
        info: analyse(&combined),
        langs,
        magnet,
        direct,
        torrent,
    })
}

/// Strip tags and decode entities from a fragment of HTML.
fn strip_tags(html: &str) -> String {
    static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());
    let text = TAG_RE.replace_all(html, " ");
    let text = decode_entities(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decode the HTML entities the board uses.
fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let Some(semi) = rest
            .char_indices()
            .take(12)
            .find(|(_, c)| *c == ';')
            .map(|(i, _)| i)
        else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let replacement = match entity {
            "amp" => Some("&".to_string()),
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" | "#39" => Some("'".to_string()),
            "nbsp" | "#160" => Some(" ".to_string()),
            other => {
                if let Some(number) = other.strip_prefix('#') {
                    number
                        .parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                } else {
                    None
                }
            }
        };
        match replacement {
            Some(text) => {
                out.push_str(&text);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Extract the `dn` value from a magnet URI.
fn magnet_display_name(magnet: &str) -> Option<String> {
    let query = magnet.split_once("magnet:?").map(|(_, q)| q)?;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("dn=") {
            return Some(percent_decode(value));
        }
    }
    None
}

/// Minimal percent decoding for magnet display names.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Strip the site branding and file extension from a release name.
pub fn clean_release_name(raw: &str) -> String {
    let mut name = raw.trim().to_string();
    static BRAND_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^www\.1tamilmv\.[a-z]+(?:\s*-\s*)?").expect("brand regex")
    });
    if let Some(m) = BRAND_RE.find(&name) {
        name = name[m.end()..].trim().to_string();
    }
    for suffix in [
        ".mkv.torrent",
        ".mp4.torrent",
        ".torrent",
        ".mkv",
        ".mp4",
        ".avi",
    ] {
        if name.to_ascii_lowercase().ends_with(suffix) {
            name.truncate(name.len() - suffix.len());
            break;
        }
    }
    name.trim().trim_end_matches('-').trim().to_string()
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_release_names() {
        assert_eq!(
            clean_release_name(
                "www.1TamilMV.report - Peddi (2026) TRUE WEB-DL - 19.5GB - ESub.mkv.torrent"
            ),
            "Peddi (2026) TRUE WEB-DL - 19.5GB - ESub"
        );
    }

    #[test]
    fn parses_magnet_display_name() {
        let m = "magnet:?xt=urn:btih:abc&dn=www.1TamilMV.report%20-%20Peddi%20(2026)%20.mkv&xl=1";
        assert_eq!(
            magnet_display_name(m).as_deref(),
            Some("www.1TamilMV.report - Peddi (2026) .mkv")
        );
    }

    #[test]
    fn parses_counts() {
        assert_eq!(parse_count("82k"), 82_000);
        assert_eq!(parse_count("0"), 0);
        assert_eq!(parse_count("1.2m"), 1_200_000);
    }

    #[test]
    fn extracts_topic_id() {
        assert_eq!(
            extract_topic_id("https://www.1tamilmv.io/index.php?/forums/topic/198763-peddi/"),
            Some(198763)
        );
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(decode_entities("a &amp; b &lt;c&gt;"), "a & b <c>");
        assert_eq!(decode_entities("no entities"), "no entities");
    }

    #[test]
    fn parses_releases_from_a_thread_body() {
        let body = r#"<div data-role="commentContent">
        <span><strong>Peddi (2026) TRUE WEB-DL - 4K SDR - HEVC - [Tamil + Telugu + Malayalam + Kannada] - 19.5GB - ESub</strong></span><br>
        <strong><a data-fileext="torrent" href="https://example.invalid/a.torrent"><span>www.1TamilMV.report - Peddi (2026) TRUE WEB-DL - [Tam + Tel + Mal + Kan] - 19.5GB - ESub.mkv.torrent</span></a></strong><br>
        <a class="skyblue-button" href="magnet:?xt=urn:btih:aaa&amp;dn=www.1TamilMV.report%20-%20Peddi%20(2026)%20TRUE%20WEB-DL.mkv">MAGNET</a><br>
        <a class="download-button" href="https://files.invalid/x.mkv">DIRECT LINK</a><br>
        <span><strong>Peddi (2026) Kannada TRUE WEB-DL - 1080p - AVC - 3.3GB - ESub</strong></span><br>
        <strong><a data-fileext="torrent" href="https://example.invalid/b.torrent"><span>www.1TamilMV.report - Peddi (2026) Kannada TRUE WEB-DL - 3.3GB - ESub.mkv.torrent</span></a></strong><br>
        <a class="skyblue-button" href="magnet:?xt=urn:btih:bbb&amp;dn=www.1TamilMV.report%20-%20Peddi%20(2026)%20Kannada.mkv">MAGNET</a><br>
        </div>"#;

        let releases = parse_releases(body).expect("releases");
        assert_eq!(releases.len(), 2);

        assert_eq!(releases[0].info.quality, crate::quality::Quality::WebDl);
        assert_eq!(
            releases[0].langs.audio,
            vec![
                crate::lang::Lang::Tamil,
                crate::lang::Lang::Telugu,
                crate::lang::Lang::Malayalam,
                crate::lang::Lang::Kannada
            ]
        );
        assert!(releases[0].magnet.is_some());
        assert!(releases[0].direct.is_some());
        assert!(releases[0].torrent.is_some());

        assert_eq!(releases[1].langs.text, vec![crate::lang::Lang::Kannada]);
        assert_eq!(releases[1].info.quality, crate::quality::Quality::WebDl);
    }
}

/// A cached HTTP response.
#[derive(serde::Serialize, serde::Deserialize)]
struct HttpCacheEntry {
    saved: u64,
    body: String,
}

/// Bump when the stored thread shape changes.
const PAGE_CACHE_VERSION: u32 = 1;

/// A cached, fully enriched forum page.
#[derive(serde::Serialize, serde::Deserialize)]
struct PageCacheEntry {
    version: u32,
    saved: u64,
    total_pages: u32,
    threads: Vec<StoredThread>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Default directory for the on-disk HTTP cache.
pub fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("drushti")
        .join("http")
}

/// A cached, parsed thread (releases + resolved original language).
#[derive(serde::Serialize, serde::Deserialize)]
struct ThreadCacheEntry {
    saved: u64,
    releases: Vec<crate::model::Release>,
    original: Option<crate::model::OriginalLang>,
}
