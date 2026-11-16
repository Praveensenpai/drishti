//! Application state and the message plumbing between the network tasks and
//! the user interface.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use crate::ai::{CacheEntry, Gemini, LangCache, cache_key, heuristic_original};
use crate::config::Config;
use crate::download::{DownloadEvent, DownloadStatus, DownloadTask};
use crate::forums::{Forum, all_web_hd};
use crate::lang::Lang;
use crate::model::{OriginalLang, Thread};
use crate::quality::analyse;
use crate::quality::Quality;
use crate::site::Client;

/// Which pane is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Browser,
    Detail,
    Downloads,
    Help,
    /// Choose the language / forum to scan.
    Language,
}

/// The active filter set.
#[derive(Clone, Debug)]
pub struct Filter {
    /// Quality buckets that pass.
    pub qualities: BTreeSet<Quality>,
    /// Original languages that pass. Empty means "any".
    pub languages: BTreeSet<Lang>,
    /// Only keep threads whose releases carry the original language.
    pub original_only: bool,
    /// Free text search over the title.
    pub search: String,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            qualities: BTreeSet::from([Quality::WebDl]),
            languages: BTreeSet::new(),
            original_only: false,
            search: String::new(),
        }
    }
}

impl Filter {
    /// Whether a quality passes.
    pub fn allows_quality(&self, quality: Quality) -> bool {
        self.qualities.is_empty() || self.qualities.contains(&quality)
    }

    /// Whether an original language passes.
    pub fn allows_language(&self, lang: Option<Lang>) -> bool {
        if self.languages.is_empty() {
            return true;
        }
        match lang {
            Some(l) => self.languages.contains(&l),
            None => false,
        }
    }

}

/// Messages sent from background tasks to the UI.
#[derive(Debug)]
pub enum Msg {
    /// A forum page arrived.
    PageLoaded { threads: Vec<Thread>, page: u32, total_pages: u32 },
    /// A forum page failed.
    PageFailed(String),
    /// A thread's releases were fetched.
    ThreadEnriched { index: usize, thread: Thread },
    /// A thread could not be fetched.
    ThreadFailed { index: usize, error: String },
    /// The model resolved a batch of movies.
    AiResolved(HashMap<String, CacheEntry>),
    /// The model call failed.
    AiFailed(String),
}

/// The whole application state.
pub struct App {
    pub config: Config,
    pub client: Client,
    pub gemini: Option<Gemini>,
    pub cache: LangCache,
    pub cache_path: PathBuf,

    pub screen: Screen,
    pub forums: Vec<&'static Forum>,
    pub forum_index: usize,
    pub page: u32,
    /// The page the scan starts at, when the user narrowed it down.
    pub page_from: u32,
    /// The last page that may be scanned, if the user narrowed it down.
    pub page_to: Option<u32>,
    /// Whether the current scan should auto-load pages until the last page.
    /// Turned off when the user pages manually with ←/→.
    pub scan_to_end: bool,
    pub total_pages: u32,
    pub threads: Vec<Thread>,
    pub selected: usize,
    pub loading: bool,
    pub status: String,
    pub error: Option<String>,

    pub filter: Filter,
    pub filter_open: bool,
    pub filter_focus: usize,

    /// Index of the thread the detail pane is showing.
    pub detail_index: Option<usize>,
    pub detail_selected: usize,
    pub detail_only_original: bool,

    pub downloads: Vec<DownloadTask>,
    pub download_selected: usize,

    /// Movies queued for model resolution.
    pub ai_pending: Vec<(usize, String, Option<u16>)>,
    pub ai_in_flight: bool,
    /// Set when the user asked to quit.
    pub should_quit: bool,
}

impl App {
    /// Build the state from a configuration.
    pub fn new(config: Config, cache_path: PathBuf) -> anyhow::Result<App> {
        let client = Client::new(
            &config.base_url,
            crate::site::default_cache_dir(),
            std::time::Duration::from_secs(config.cache_ttl_minutes as u64 * 60),
        )?;
        let gemini = {
            let key = config.api_key();
            if key.is_empty() {
                None
            } else {
                Some(Gemini::new(&key, config.gemini_models.clone())?)
            }
        };
        let cache = LangCache::load(&cache_path);
        let forums = all_web_hd();
        let forum_index = forums
            .iter()
            .position(|f| f.lang == Lang::Kannada)
            .unwrap_or(0);
        let filter = Filter {
            qualities: config.allowed_qualities.iter().copied().collect(),
            ..Filter::default()
        };
        Ok(App {
            config,
            client,
            gemini,
            cache,
            cache_path,
            screen: Screen::Language,
            forums,
            forum_index,
            page: 1,
            page_from: 1,
            page_to: None,
            scan_to_end: true,
            total_pages: 1,
            threads: Vec::new(),
            selected: 0,
            loading: false,
            status: String::new(),
            error: None,
            filter,
            filter_open: false,
            filter_focus: 0,
            detail_index: None,
            detail_selected: 0,
            detail_only_original: false,
            downloads: Vec::new(),
            download_selected: 0,
            ai_pending: Vec::new(),
            ai_in_flight: false,
            should_quit: false,
        })
    }

    /// The forum currently being browsed.
    pub fn forum(&self) -> &'static Forum {
        self.forums[self.forum_index.min(self.forums.len() - 1)]
    }

    /// The threads that pass the current filter.
    pub fn visible_threads(&self) -> Vec<usize> {
        self.threads
            .iter()
            .enumerate()
            .filter(|(_, t)| self.thread_passes(t))
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether a thread passes the filter.
    pub fn thread_passes(&self, thread: &Thread) -> bool {
        if !self.thread_quality_ok(thread) {
            return false;
        }
        if !self.filter.search.is_empty() {
            let needle = self.filter.search.to_ascii_lowercase();
            let hay = thread.title.to_ascii_lowercase();
            if !hay.contains(&needle) {
                return false;
            }
        }
        if !self.filter.languages.is_empty() {
            let original = self.effective_original(thread);
            if !self.filter.allows_language(original.map(|o| o.lang)) {
                return false;
            }
        }
        if self.filter.original_only {
            let Some(original) = self.effective_original(thread) else {
                return false;
            };
            // In the browser, "original only" keeps the pure originals of this
            // forum's language: films whose original language differs (i.e. dubs
            // of another language) are hidden. (In the detail pane, original-only
            // instead keeps releases that carry the film's original language.)
            if original.lang != thread.forum.lang {
                return false;
            }
        }
        true
    }

    /// The original language of a thread, from the model or the heuristic.
    pub fn effective_original(&self, thread: &Thread) -> Option<OriginalLang> {
        if let Some(original) = thread.original {
            return Some(original);
        }
        let key = cache_key(&thread.movie, thread.year);
        if let Some(entry) = self.cache.get(&key) {
            return Some(OriginalLang {
                lang: entry.lang,
                confidence: entry.confidence,
                source: entry.source,
            });
        }
        Some(heuristic_original(thread))
    }

    /// The thread shown in the detail pane.
    pub fn detail_thread(&self) -> Option<&Thread> {
        self.detail_index
            .and_then(|i| self.threads.get(i))
    }

    /// The releases shown in the detail pane, after filtering.
    pub fn visible_releases(&self) -> Vec<usize> {
        let Some(thread) = self.detail_thread() else {
            return Vec::new();
        };
        let original = self.effective_original(thread).map(|o| o.lang);
        thread
            .releases
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                if !self.filter.allows_quality(r.info.quality) {
                    return false;
                }
                if self.detail_only_original
                    && let Some(original) = original {
                        return r.has_original(original);
                    }
                true
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Move the selection, clamped to the visible rows.
    pub fn select_visible(&mut self, delta: isize) {
        let visible = self.visible_threads();
        if visible.is_empty() {
            self.selected = 0;
            return;
        }
        let position = visible
            .iter()
            .position(|i| *i == self.selected)
            .unwrap_or(0) as isize;
        let next = (position + delta).clamp(0, visible.len() as isize - 1) as usize;
        self.selected = visible[next];
    }

    /// Move the selection to an absolute visible row.
    pub fn select_row(&mut self, row: usize) {
        let visible = self.visible_threads();
        if row < visible.len() {
            self.selected = visible[row];
        }
    }

    /// Whether the thread passes the current quality filter, using the
    /// releases when they are known and the title otherwise.
    pub fn thread_quality_ok(&self, thread: &Thread) -> bool {
        let quality = analyse(&thread.title).quality;
        if !self.filter.allows_quality(quality) {
            return false;
        }
        if !thread.releases.is_empty() {
            let has_web_dl = thread.releases.iter().any(|r| r.info.quality.is_web_dl());
            if !has_web_dl {
                return false;
            }
        }
        true
    }

    /// The visible row the selection is on.
    pub fn selected_row(&self) -> usize {
        let visible = self.visible_threads();
        visible
            .iter()
            .position(|i| *i == self.selected)
            .unwrap_or(0)
    }

    /// Add a task to the download queue.
    pub fn queue_download(&mut self, task: DownloadTask) {
        self.downloads.push(task);
    }

    /// Apply a download event.
    pub fn apply_download_event(&mut self, event: DownloadEvent) {
        let id = match &event {
            DownloadEvent::Started { id } => *id,
            DownloadEvent::Progress { id, .. } => *id,
            DownloadEvent::Finished { id, .. } => *id,
            DownloadEvent::HandedOff { id, .. } => *id,
            DownloadEvent::Failed { id, .. } => *id,
        };
        let Some(task) = self.downloads.iter_mut().find(|t| t.id == id) else {
            return;
        };
        task.status = match event {
            DownloadEvent::Started { .. } => DownloadStatus::Queued,
            DownloadEvent::Progress { downloaded, total, .. } => {
                DownloadStatus::Running { downloaded, total }
            }
            DownloadEvent::Finished { path, bytes, .. } => DownloadStatus::Finished { path, bytes },
            DownloadEvent::HandedOff { client, .. } => DownloadStatus::HandedOff { client },
            DownloadEvent::Failed { error, .. } => DownloadStatus::Failed { error },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_allows_web_dl_only() {
        let filter = Filter::default();
        assert!(filter.allows_quality(Quality::WebDl));
        assert!(!filter.allows_quality(Quality::DvdRip));
        assert!(!filter.allows_quality(Quality::Theater));
        assert!(!filter.allows_quality(Quality::HdRip));
    }

    #[test]
    fn empty_language_filter_allows_everything() {
        let filter = Filter::default();
        assert!(filter.allows_language(Some(Lang::Tamil)));
        assert!(filter.allows_language(None));
    }
}
