//! Core data types.

use serde::{Deserialize, Serialize};

use crate::forums::Forum;
use crate::lang::{Lang, LangSet};
use crate::quality::ReleaseInfo;

/// A thread on the board.
#[derive(Clone, Debug)]
pub struct Thread {
    pub id: u64,
    /// The full title as posted.
    pub title: String,
    /// The cleaned movie name.
    pub movie: String,
    /// The year, when the title carries one.
    pub year: Option<u16>,
    pub url: String,
    pub forum: &'static Forum,
    pub author: String,
    pub posted: Option<String>,
    pub replies: u32,
    pub views: u32,
    /// Releases discovered inside the thread. Empty until the thread is opened.
    pub releases: Vec<Release>,
    /// The original production language, once it has been resolved.
    pub original: Option<OriginalLang>,
}

/// Where a release's language information came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LangSource {
    /// Resolved by the language model.
    Ai,
    /// Inferred from a multi-audio release in the thread.
    MultiAudio,
    /// Inferred from the language word in the thread title.
    TitleTag,
    /// Assumed to be the forum's language.
    Forum,
    /// Could not be determined.
    Unknown,
}

impl LangSource {
    pub fn label(self) -> &'static str {
        match self {
            LangSource::Ai => "AI",
            LangSource::MultiAudio => "multi",
            LangSource::TitleTag => "tag",
            LangSource::Forum => "forum",
            LangSource::Unknown => "?",
        }
    }
}

/// The original production language of a film.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OriginalLang {
    pub lang: Lang,
    /// 0.0 - 1.0.
    pub confidence: f32,
    pub source: LangSource,
}

/// A single downloadable release inside a thread.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    /// The cleaned release name.
    pub name: String,
    pub info: ReleaseInfo,
    pub langs: LangSet,
    pub magnet: Option<String>,
    pub direct: Option<String>,
    pub torrent: Option<String>,
}

impl Release {
    /// True when this release carries the film's original language.
    pub fn has_original(&self, original: Lang) -> bool {
        self.langs.all().contains(&original)
    }

    /// Best available download link, preferring a magnet over a direct link.
    pub fn preferred_link(&self) -> Option<(&'static str, String)> {
        if let Some(m) = &self.magnet {
            return Some(("magnet", m.clone()));
        }
        if let Some(d) = &self.direct {
            return Some(("direct", d.clone()));
        }
        if let Some(t) = &self.torrent {
            return Some(("torrent", t.clone()));
        }
        None
    }
}

/// Split a thread title into a movie name, a year and the remaining tags.
pub fn split_title(title: &str) -> (String, Option<u16>, String) {
    let trimmed = title.trim();
    // Look for the first "(YYYY)" group.
    let mut name = trimmed.to_string();
    let mut year = None;
    let mut rest = trimmed.to_string();

    if let Some(open) = trimmed.find('(')
        && let Some(close) = trimmed[open..].find(')') {
            let inner = &trimmed[open + 1..open + close];
            if inner.len() == 4 && inner.chars().all(|c| c.is_ascii_digit())
                && let Ok(y) = inner.parse::<u16>() {
                    year = Some(y);
                    name = trimmed[..open].trim().to_string();
                    rest = trimmed[open + close + 1..].trim().to_string();
                }
        }

    // Strip trailing separators from the movie name.
    while name.ends_with('-') || name.ends_with('–') || name.ends_with('—') {
        name.pop();
        name = name.trim_end().to_string();
    }

    (name, year, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_peddi_title() {
        let (name, year, rest) = split_title(
            "Peddi (2026) Kannada TRUE WEB-DL - [4K, 1080p & 720p - AVC / HEVC]",
        );
        assert_eq!(name, "Peddi");
        assert_eq!(year, Some(2026));
        assert!(rest.starts_with("Kannada"));
    }

    #[test]
    fn handles_title_without_year() {
        let (name, year, _) = split_title("Some Movie WEB-DL 1080p");
        assert_eq!(name, "Some Movie WEB-DL 1080p");
        assert_eq!(year, None);
    }
}
