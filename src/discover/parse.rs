//! Pure parsing for board-domain discovery.
//!
//! Turns a search results page into candidate URLs, then accepts only origins
//! whose hostname really is a 1tamilmv host. No network or filesystem access
//! lives here, so the rules can be tested without hitting the internet.

use std::sync::LazyLock;

use base64::Engine;
use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

/// A validated board origin and its hostname.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub origin: String,
    pub host: String,
}

/// Pull every plausible result URL out of a search results page, in order.
pub fn extract_candidates(html: &str) -> Vec<String> {
    let Ok(selector) = Selector::parse("a[href]") else {
        return Vec::new();
    };
    let doc = Html::parse_document(html);
    let mut out = Vec::new();
    for anchor in doc.select(&selector) {
        let Some(href) = anchor.attr("href") else {
            continue;
        };
        if let Some(target) = bing_redirect_target(href) {
            push_unique(&mut out, target);
        } else if href.starts_with("http://") || href.starts_with("https://") {
            push_unique(&mut out, href.to_string());
        }
    }
    out
}

fn push_unique(out: &mut Vec<String>, value: String) {
    if !out.contains(&value) {
        out.push(value);
    }
}

/// Resolve Bing's `/ck/a?...&u=a1<base64url>` wrapper to the target URL.
fn bing_redirect_target(href: &str) -> Option<String> {
    let parsed = Url::parse(href).ok()?;
    if !matches!(parsed.host_str(), Some("bing.com" | "www.bing.com")) {
        return None;
    }
    if parsed.path() != "/ck/a" {
        return None;
    }
    let (_, value) = parsed.query_pairs().find(|(key, _)| key == "u")?;
    let encoded = value.strip_prefix("a1")?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(encoded))
        .ok()?;
    String::from_utf8(bytes).ok()
}

/// Accept a URL only when its hostname identifies a 1tamilmv site.
pub fn board_origin(raw: &str) -> Option<Candidate> {
    let parsed = Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    let bare = host.strip_prefix("www.").unwrap_or(&host);
    if !is_board_host(bare) {
        return None;
    }
    Some(Candidate {
        origin: format!("{}://{}", parsed.scheme(), host),
        host,
    })
}

/// A host is a board host when its first label is exactly `1tamilmv` and the
/// remaining labels look like a TLD, optionally with a country prefix.
fn is_board_host(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|label| label.is_empty()) {
        return false;
    }
    if labels[0] != "1tamilmv" {
        return false;
    }
    match labels.len() {
        2 => true,
        3 => matches!(
            labels[1],
            "co" | "com" | "net" | "org" | "gov" | "edu" | "ac"
        ),
        _ => false,
    }
}

/// Every valid board origin in a list of candidates.
pub fn board_origins(candidates: &[String]) -> Vec<Candidate> {
    candidates
        .iter()
        .filter_map(|candidate| board_origin(candidate))
        .collect()
}

/// Collect board domains mentioned in result titles and snippets, not just in
/// link targets. Search engines often show the real domain as plain text while
/// the clickable link points somewhere else.
pub fn extract_text_domains(html: &str) -> Vec<Candidate> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:www\.)?1tamilmv[a-z0-9-]*\.[a-z]{2,}(?:\.[a-z]{2,})?")
            .expect("board domain regex")
    });
    let doc = Html::parse_document(html);
    let text: String = doc.root_element().text().collect();
    let mut out = Vec::new();
    for found in RE.find_iter(&text) {
        let raw = found.as_str().trim_start_matches("www.");
        let Some(origin) = board_origin(&format!("https://{raw}")) else {
            continue;
        };
        if !out.iter().any(|c: &Candidate| c.origin == origin.origin) {
            out.push(origin);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bing_redirect_links() {
        let html = r#"<h2><a href="https://www.bing.com/ck/a?p=x&amp;u=a1aHR0cHM6Ly8xdGFtaWxtdi5hYy8&amp;ntb=1">x</a></h2>"#;
        assert_eq!(
            extract_candidates(html),
            vec!["https://1tamilmv.ac/".to_string()]
        );
    }

    #[test]
    fn parses_direct_links_and_skips_noise() {
        let html = r#"<a href="https://1tamilmv.io/">a</a><a href="https://google.com/">b</a><a href="/search?q=x">c</a>"#;
        assert_eq!(
            extract_candidates(html),
            vec![
                "https://1tamilmv.io/".to_string(),
                "https://google.com/".to_string()
            ]
        );
    }

    #[test]
    fn deduplicates_candidates() {
        let html = r#"<a href="https://1tamilmv.io/">a</a><a href="https://1tamilmv.io/">b</a>"#;
        assert_eq!(
            extract_candidates(html),
            vec!["https://1tamilmv.io/".to_string()]
        );
    }

    #[test]
    fn accepts_real_board_origins() {
        assert_eq!(
            board_origin("https://www.1tamilmv.ceo/").map(|c| c.origin),
            Some("https://www.1tamilmv.ceo".to_string())
        );
        assert!(board_origin("http://1tamilmv.to").is_some());
        assert!(board_origin("https://1tamilmv.co.in/").is_some());
    }

    #[test]
    fn extracts_domains_from_result_text() {
        let html = r#"<h2><strong>1Tamilmv</strong>.ceo | Official</h2><p>New domain www.1tamilmv.date and 1tamilmv.plus</p>"#;
        let origins: Vec<String> = extract_text_domains(html)
            .into_iter()
            .map(|c| c.origin)
            .collect();
        assert_eq!(
            origins,
            vec![
                "https://1tamilmv.ceo".to_string(),
                "https://1tamilmv.date".to_string(),
                "https://1tamilmv.plus".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_lookalike_hosts() {
        assert!(board_origin("https://not1tamilmv.com/").is_none());
        assert!(board_origin("https://1tamilmv.evil.com/").is_none());
        assert!(board_origin("https://1tamilmv.ceo.evil.com/").is_none());
        assert!(board_origin("ftp://1tamilmv.ceo/").is_none());
        assert!(board_origin("https://www.1tamilmv.ceo@evil.com/").is_none());
        assert!(board_origin("not a url").is_none());
    }

    #[test]
    fn picks_first_valid_candidate() {
        let candidates = vec![
            "https://google.com/".to_string(),
            "https://1tamilmv.evil.com/".to_string(),
            "https://www.1tamilmv.ceo/".to_string(),
            "https://1tamilmv.io/".to_string(),
        ];
        let picked = board_origins(&candidates)
            .into_iter()
            .next()
            .expect("board origin");
        assert_eq!(picked.origin, "https://www.1tamilmv.ceo");
    }
}
