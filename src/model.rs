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
    /// Verified against an external film database.
    Verified,
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
            LangSource::Verified => "web",
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
}

/// A thread as stored in the enriched page cache. The forum is kept by id so
/// the cache stays valid across restarts without serialising a static pointer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredThread {
    pub id: u64,
    pub title: String,
    pub movie: String,
    pub year: Option<u16>,
    pub url: String,
    pub forum_id: u32,
    pub author: String,
    pub posted: Option<String>,
    pub replies: u32,
    pub views: u32,
    pub releases: Vec<Release>,
    pub original: Option<OriginalLang>,
}

impl StoredThread {
    /// Convert a live thread into its serialisable form.
    pub fn from_thread(thread: &Thread) -> Self {
        Self {
            id: thread.id,
            title: thread.title.clone(),
            movie: thread.movie.clone(),
            year: thread.year,
            url: thread.url.clone(),
            forum_id: thread.forum.id,
            author: thread.author.clone(),
            posted: thread.posted.clone(),
            replies: thread.replies,
            views: thread.views,
            releases: thread.releases.clone(),
            original: thread.original,
        }
    }

    /// Convert back, dropping threads whose forum no longer exists.
    pub fn into_thread(self) -> Option<Thread> {
        let forum = crate::forums::by_id(self.forum_id)?;
        Some(Thread {
            id: self.id,
            title: self.title,
            movie: self.movie,
            year: self.year,
            url: self.url,
            forum,
            author: self.author,
            posted: self.posted,
            replies: self.replies,
            views: self.views,
            releases: self.releases,
            original: self.original,
        })
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
        && let Some(close) = trimmed[open..].find(')')
    {
        let inner = &trimmed[open + 1..open + close];
        if inner.len() == 4
            && inner.chars().all(|c| c.is_ascii_digit())
            && let Ok(y) = inner.parse::<u16>()
        {
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
        let (name, year, rest) =
            split_title("Peddi (2026) Kannada TRUE WEB-DL - [4K, 1080p & 720p - AVC / HEVC]");
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

    #[test]
    fn stored_thread_round_trips() {
        let forum = crate::forums::by_id(69).expect("kannada forum");
        let thread = Thread {
            id: 7,
            title: "Test (2026) Kannada WEB-DL".to_string(),
            movie: "Test".to_string(),
            year: Some(2026),
            url: "https://example.invalid/topic/7".to_string(),
            forum,
            author: "someone".to_string(),
            posted: Some("2026-01-01".to_string()),
            replies: 3,
            views: 40,
            releases: Vec::new(),
            original: Some(OriginalLang {
                lang: Lang::Kannada,
                confidence: 0.9,
                source: LangSource::Ai,
            }),
        };
        let stored = StoredThread::from_thread(&thread);
        let restored = stored.into_thread().expect("forum still exists");
        assert_eq!(restored.id, thread.id);
        assert_eq!(restored.movie, thread.movie);
        assert_eq!(restored.forum.id, thread.forum.id);
        assert_eq!(restored.original, thread.original);
    }
}
