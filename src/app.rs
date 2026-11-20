//! Application state and the message plumbing between the network tasks and
//! the user interface.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use crate::ai::{CacheEntry, Gemini, LangCache, cache_key, heuristic_original, plan_batches};
use crate::config::Config;
use crate::forums::{Forum, all_web_hd};
use crate::lang::Lang;
use crate::model::{OriginalLang, Thread};
use crate::quality::Quality;
use crate::quality::analyse;
use crate::site::{Client, PageLoad};
use crate::search::{match_thread, MatchResult};

/// Which pane is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Browser,
    Detail,
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

/// One fully loaded forum page.
#[derive(Clone, Debug)]
pub struct PageState {
    pub threads: Vec<Thread>,
    pub total_pages: u32,
    pub from_cache: bool,
}

/// Progress of the background scan.
#[derive(Clone, Debug, Default)]
pub struct ScanProgress {
    pub active: bool,
    pub pages_done: u32,
    pub pages_total: u32,
    pub threads_done: u32,
}

impl ScanProgress {
    /// Fraction complete, clamped to 0.0 - 1.0.
    pub fn ratio(&self) -> f64 {
        if self.pages_total == 0 {
            return 0.0;
        }
        (self.pages_done as f64 / self.pages_total as f64).clamp(0.0, 1.0)
    }
}

/// Messages sent from background tasks to the UI.
#[derive(Debug)]
pub enum Msg {
    /// A forum page and all of its releases arrived.
    PageReady {
        forum_id: u32,
        page: u32,
        total_pages: u32,
        threads: Vec<Thread>,
        from_cache: bool,
        scan_id: u64,
    },
    /// A forum page failed.
    PageFailed {
        forum_id: u32,
        page: u32,
        error: String,
        scan_id: u64,
    },
    /// Every page of the scan has been delivered.
    ScanFinished { forum_id: u32, scan_id: u64 },
    /// The model resolved a batch of movies.
    AiResolved(HashMap<String, CacheEntry>),
    /// The model call failed.
    AiFailed(String),
    /// Result of posting a magnet to the configured endpoint.
    MagnetPosted { ok: bool, message: String },
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
    pub total_pages: u32,
    /// The current page's threads.
    pub threads: Vec<Thread>,
    /// Every page that has arrived during this scan or visit.
    pub pages: HashMap<u32, PageState>,
    pub scan: ScanProgress,
    /// Pages already counted towards scan progress.
    pub scan_done: BTreeSet<u32>,
    /// Identifier of the current scan; stale messages are ignored.
    pub scan_id: u64,
    /// Manual page requests that are still in flight.
    pub pages_in_flight: usize,
    pub selected: usize,
    pub loading: bool,
    pub status: String,
    pub error: Option<String>,
    /// Tick counter used by the spinner.
    pub tick: u64,

    pub filter: Filter,
    pub filter_open: bool,
    pub filter_focus: usize,

    /// Fuzzy movie search overlay state.
    pub search_open: bool,
    pub search_draft: String,
    pub saved_query: String,
    pub search_results: Vec<MatchResult>,
    pub result_selected: usize,
    pub search_dirty: bool,

    /// Index of the thread the detail pane is showing.
    pub detail_index: Option<usize>,
    pub detail_selected: usize,
    pub detail_only_original: bool,

    /// Movie cache keys waiting for model resolution.
    pub ai_queue: Vec<String>,
    /// The batch currently in flight, restored to the queue if the app exits.
    pub ai_in_flight_keys: Vec<String>,
    /// Consecutive batch failures, bounded to avoid infinite retry loops.
    pub ai_failures: u32,
    /// Batches already sent to the model.
    pub ai_batches_done: u32,
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
            total_pages: 1,
            threads: Vec::new(),
            pages: HashMap::new(),
            scan: ScanProgress::default(),
            scan_done: BTreeSet::new(),
            scan_id: 0,
            pages_in_flight: 0,
            selected: 0,
            loading: false,
            status: String::new(),
            error: None,
            tick: 0,
            filter,
            filter_open: false,
            filter_focus: 0,
            search_open: false,
            search_draft: String::new(),
            saved_query: String::new(),
            search_results: Vec::new(),
            result_selected: 0,
            search_dirty: false,
            detail_index: None,
            detail_selected: 0,
            detail_only_original: false,
            ai_queue: Vec::new(),
            ai_in_flight_keys: Vec::new(),
            ai_failures: 0,
            ai_batches_done: 0,
            ai_in_flight: false,
            should_quit: false,
        })
    }

    /// The forum currently being browsed.
    pub fn forum(&self) -> &'static Forum {
        self.forums[self.forum_index.min(self.forums.len() - 1)]
    }

    /// How long a model answer stays fresh.
    pub fn ai_ttl_secs(&self) -> u64 {
        self.config.ai_cache_hours.saturating_mul(3600)
    }

    /// Show a page from the in-memory store, or show a loading state.
    pub fn show_page(&mut self, page: u32) {
        self.page = page;
        self.detail_index = None;
        self.selected = 0;
        if let Some(state) = self.pages.get(&page) {
            self.threads = state.threads.clone();
            self.total_pages = state.total_pages;
            self.loading = false;
        } else {
            self.threads.clear();
            self.loading = true;
        }
    }

    /// Store a loaded page and update the view when it is the current page.
    pub fn store_page(&mut self, load: PageLoad) {
        let page = load.page;
        let state = PageState {
            threads: load.threads,
            total_pages: load.total_pages,
            from_cache: load.from_cache,
        };
        self.total_pages = state.total_pages;
        if page == self.page {
            self.threads = state.threads.clone();
            self.loading = false;
            self.error = None;
            self.status = format!(
                "{} threads loaded{}",
                self.threads.len(),
                if state.from_cache { " (cached)" } else { "" }
            );
            self.selected = self.selected.min(self.threads.len().saturating_sub(1));
        }
        self.pages.insert(page, state);
    }

    /// Clear everything before a fresh scan.
    pub fn reset_scan(&mut self) {
        self.pages.clear();
        self.threads.clear();
        self.selected = 0;
        self.detail_index = None;
        self.ai_queue.clear();
        self.ai_in_flight_keys.clear();
        self.ai_failures = 0;
        self.ai_batches_done = 0;
        self.ai_in_flight = false;
        self.loading = true;
        self.error = None;
        self.pages_in_flight = 0;
        self.scan_done.clear();
    }

    /// Whether a thread needs a model answer.
    fn ai_key(&self, thread: &Thread) -> Option<String> {
        self.gemini.as_ref()?;
        if !self.config.resolve_on_load {
            return None;
        }
        let key = cache_key(&thread.movie, thread.year);
        if self.cache.get_with_ttl(&key, self.ai_ttl_secs()).is_some() {
            return None;
        }
        Some(key)
    }

    /// Queue every unresolved thread on a loaded page.
    pub fn queue_ai_threads(&mut self, threads: &[Thread]) {
        let keys: Vec<String> = threads.iter().filter_map(|t| self.ai_key(t)).collect();
        for key in keys {
            if !self.ai_queue.contains(&key) {
                self.ai_queue.push(key);
            }
        }
    }

    /// Copy resolved answers onto the current page and every cached page.
    pub fn apply_ai(&mut self, map: &HashMap<String, CacheEntry>) {
        apply_to_threads(&mut self.threads, map);
        for state in self.pages.values_mut() {
            apply_to_threads(&mut state.threads, map);
        }
    }

    /// Batches still waiting to be sent.
    pub fn ai_batches_left(&self) -> usize {
        plan_batches(
            &self.ai_queue,
            self.config.ai_batch_size.max(1),
            self.config.ai_batch_chars.max(64),
        )
        .len()
    }

    /// Total batches for the current queue, including completed ones.
    pub fn ai_batches_total(&self) -> u32 {
        self.ai_batches_done + self.ai_batches_left() as u32
    }

    /// Spinner frame for progress indicators.
    pub fn spinner(&self) -> &'static str {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[(self.tick as usize / 2) % FRAMES.len()]
    }

    /// A compact progress label for the header.
    pub fn progress_text(&self) -> Option<String> {
        if self.scan.active {
            let done = self.scan.pages_done;
            let total = self.scan.pages_total.max(done).max(1);
            let pct = done.saturating_mul(100) / total;
            return Some(format!("{} scan {done}/{total} {pct}%", self.spinner()));
        }
        if self.ai_in_flight || !self.ai_queue.is_empty() {
            let done = self.ai_batches_done;
            let total = self.ai_batches_total().max(done).max(1);
            return Some(format!("{} AI {done}/{total}", self.spinner()));
        }
        None
    }

    /// The threads that pass the current filter.
    ///
    /// When `search_open` is true the browser draws from `search_results`
    /// instead, so this method returns an empty vec (the caller should use
    /// `app.search_results` directly).
    pub fn visible_threads(&self) -> Vec<usize> {
        if self.search_open {
            // Search overlay is active; caller should use app.search_results.
            return Vec::new();
        }
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
        let key = cache_key(&thread.movie, thread.year);
        if let Some(entry) = self.cache.get_with_ttl(&key, self.ai_ttl_secs()) {
            return Some(OriginalLang {
                lang: entry.lang,
                confidence: entry.confidence,
                source: entry.source,
            });
        }
        if let Some(original) = thread.original {
            return Some(original);
        }
        Some(heuristic_original(thread))
    }

    /// The thread shown in the detail pane.
    pub fn detail_thread(&self) -> Option<&Thread> {
        self.detail_index.and_then(|i| self.threads.get(i))
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
                    && let Some(original) = original
                {
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

    /// Set the search draft query and mark results as needing rebuild.
    pub fn set_query(&mut self, query: String) {
        self.search_draft = query;
        self.search_dirty = true;
    }

    /// Rebuild the ranked search results from every loaded page.
    ///
    /// Called lazily when the UI needs the list; it iterates all threads in
    /// `self.pages`, matches each against `self.search_draft` using
    /// `match_thread` (movie name then title), sorts by score descending,
    /// deduplicates by cache key, and caps at a reasonable limit.
    pub fn rebuild_search_results(&mut self) {
        if !self.search_dirty {
            return;
        }
        self.search_dirty = false;
        self.search_results.clear();

        let query = &self.search_draft;
        if query.is_empty() {
            return;
        }

        let mut scored: Vec<MatchResult> = Vec::new();

        // Iterate all loaded pages.
        for (&page_id, state) in &self.pages {
            for (thread_index, thread) in state.threads.iter().enumerate() {
                if let Some(mut m) = match_thread(query, &thread.movie, &thread.title) {
                    m.page_id = page_id;
                    m.thread_index = thread_index;
                    scored.push(m);
                }
            }
        }

        // Sort by score descending (higher score = better/more specific match).
        scored.sort_by_key(|m| std::cmp::Reverse(m.score));

        self.search_results = scored;
        if self.search_results.is_empty() {
            self.result_selected = 0;
        } else if self.result_selected >= self.search_results.len() {
            self.result_selected = self.search_results.len() - 1;
        }

        // Cap at 200 results to keep the UI responsive.
        self.search_results.truncate(200);
    }

    /// Jump to the search result at the given index.
    ///
    /// Shows the result's page, selects the matching thread, closes search,
    /// and opens the detail view so the user can interact with the post.
    pub fn jump_to_result(&mut self, idx: usize) {
        let Some(result) = self.search_results.get(idx).cloned() else {
            return;
        };
        self.show_page(result.page_id);
        if result.thread_index < self.threads.len() {
            self.selected = result.thread_index;
        } else {
            self.selected = 0;
        }
        self.search_open = false;
        self.search_draft.clear();
        self.search_results.clear();
        self.result_selected = 0;
        self.search_dirty = false;
        // Open detail view for the selected thread.
        self.detail_index = Some(self.selected);
        self.detail_selected = 0;
        self.screen = Screen::Detail;
        self.detail_only_original = false;
    }

    /// The visible row the selection is on.
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
}

/// Copy resolved languages onto a thread list.
fn apply_to_threads(threads: &mut [Thread], map: &HashMap<String, CacheEntry>) {
    for thread in threads {
        let key = cache_key(&thread.movie, thread.year);
        if let Some(entry) = map.get(&key) {
            thread.original = Some(OriginalLang {
                lang: entry.lang,
                confidence: entry.confidence,
                source: entry.source,
            });
        }
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
