# CODEBASE.md: drushti Semantic Digest

> **Notice**: This file is an AI-optimized semantic index. Do not write narrative prose. Keep token density high.

## 1. System Topology & Data Flow

```text
main.rs (CLI + event loop)
  ├─ ansi.rs ──> coloured CLI output (TTY + NO_COLOR aware)
  ├─ config.rs ──> Config (TOML, first-run wizard, CLI overrides)
  ├─ discover.rs ──> site::forum_page_url (probe) ──> Bing HTML ──> parse.rs ──> config.save()
  └─ run_tui ──> App (app.rs)
                  ├─ site.rs::Client ──> forums.rs / model.rs / lang.rs / quality.rs
                  ├─ ai.rs::Gemini + LangCache ──> config.toml / langcache.json
                  ├─ search.rs (fuzzy_match, match_thread) ──> ui/browser.rs search overlay
                  └─ ui/mod.rs::draw ──> browser.rs / detail.rs / help.rs
```

Startup order: parse CLI -> load Config -> apply CLI overrides -> first-run wizard -> domain discovery (probe, optional Bing search, persist) -> command dispatch or TUI.

## 2. Global Constraints & Architecture Patterns

- **Primary Language & Edition**: Rust 2024, tokio multi-thread, ratatui 0.30, crossterm 0.29.
- **Architectural Paradigm**: Flat module tree under `src/`. No `domain/ / infra/ / api/` folders exist despite older docs claiming so. `src/discover.rs` plus `src/discover/` follows modern file-module style.
- **Hard Constraints**: <400 lines/file (soft 300), <60 lines/fn (soft 40), max 4 params, nesting <=3, zero production `unwrap()`/`expect()`, zero compiler/clippy warnings. Several pre-existing files exceed the size limits (`main.rs` 1259, `site.rs` 898, etc.).
- **Target Distribution**: Linux x86_64 standalone binary via GitHub Releases (`.github/workflows/release.yml`).

## 3. Module & Interface Skeleton

### `src/main.rs` (Role: cli, Lines: 1326)
- **Responsibility**: CLI definitions, startup wiring, event loop, key dispatch, background task spawning.
- **Imports**: `clap`, `crossterm`, `tokio::sync::mpsc`, all `crate::*` modules.
- **Types & Enums**: `Cli`, `Command::{Probe,Resolve,List,ConfigPath,Config,Init,Update}`.
- **Public Functions & Signatures**:
  ```rust
  #[tokio::main] async fn main() -> Result<()>
  async fn run_tui(config: Config, config_path: PathBuf, page_from: u32, page_to: Option<u32>) -> Result<()>
  async fn handle_key(app: &mut App, key: KeyEvent, tx: &mpsc::UnboundedSender<Msg>)
  async fn handle_message(app: &mut App, msg: Msg, tx: &mpsc::UnboundedSender<Msg>)
  fn start_scan(app: &mut App, tx: &mpsc::UnboundedSender<Msg>)
  fn request_page(app: &mut App, page: u32, tx: &mpsc::UnboundedSender<Msg>)
  fn dispatch_ai(app: &mut App, tx: &mpsc::UnboundedSender<Msg>)
  async fn maybe_rediscover(config: &mut Config, config_path: &std::path::Path)
  fn cache_dir() -> PathBuf
  ```
- **Consumers**: entrypoint.
- **Side Effects**: writes config.toml via discovery, spawns tokio tasks, OSC 52 clipboard, browser launch, self-update via HTTP.

### `src/discover.rs` (Role: infra, Lines: 164)
- **Responsibility**: Startup board-domain probe and Bing fallback.
- **Imports**: `reqwest`, `anyhow`, `crate::forums`, `crate::site`.
- **Types & Enums**: `pub struct Discovery { http: reqwest::Client, cooldown: cooldown::Cooldown }`.
- **Public Functions & Signatures**:
  ```rust
  pub fn Discovery::new(cache_dir: &Path) -> Result<Self>
  pub async fn Discovery::resolve(&self, current: &str, query: &str, search_endpoint: &str) -> Result<Option<String>>
  fn probe_url(base: &str) -> String
  async fn Discovery::probe(&self, url: &str) -> bool
  async fn Discovery::find_origins(&self, query: &str, endpoint: &str) -> Vec<parse::Candidate>
  async fn Discovery::search(&self, query: &str, endpoint: &str) -> Result<String>
  ```
- **Consumers**: `main.rs`.
- **Side Effects**: HTTP GET to board listing and Bing, writes failure timestamp to cache dir.

### `src/discover/parse.rs` (Role: domain, Lines: 218)
- **Responsibility**: Pure Bing-result parsing and strict 1tamilmv hostname validation.
- **Imports**: `scraper::{Html, Selector}`, `url::Url`.
- **Types & Enums**: `pub struct Candidate { pub origin: String, pub host: String }`.
- **Public Functions & Signatures**:
  ```rust
  pub fn extract_candidates(html: &str) -> Vec<String>
  pub fn board_origin(raw: &str) -> Option<Candidate>
  pub fn board_origins(candidates: &[String]) -> Vec<Candidate>
  pub fn extract_text_domains(html: &str) -> Vec<Candidate>
  fn is_board_host(host: &str) -> bool
  ```
- **Consumers**: `discover.rs`.
- **Side Effects**: none.

### `src/discover/cooldown.rs` (Role: infra, Lines: 69)
- **Responsibility**: One-hour failure cooldown stamp for discovery.
- **Imports**: `std::path`, `std::time`.
- **Types & Enums**: `pub struct Cooldown { path: PathBuf }`, `const COOLDOWN_SECS: u64 = 3600`.
- **Public Functions & Signatures**:
  ```rust
  pub fn Cooldown::new(cache_dir: &Path) -> Self
  pub fn Cooldown::is_active(&self) -> bool
  pub fn Cooldown::mark(&self)
  ```
- **Consumers**: `discover.rs`.
- **Side Effects**: reads/writes `<cache_dir>/domain_discovery`.

### `src/ansi.rs` (Role: cli, Lines: 128)
- **Responsibility**: ANSI colour helpers and a stderr spinner for non-interactive commands.
- **Imports**: `std::io::{IsTerminal, Write}`, `std::sync::OnceLock`, `std::thread`.
- **Types & Enums**: `pub struct Spinner { stop: Arc<AtomicBool>, thread: Option<JoinHandle<()>> }`.
- **Public Functions & Signatures**:
  ```rust
  pub fn enabled() -> bool
  pub fn accent(text: &str) -> String
  pub fn success(text: &str) -> String
  pub fn warn(text: &str) -> String
  pub fn error(text: &str) -> String
  pub fn muted(text: &str) -> String
  pub fn good(text: &str) -> String
  pub fn check() -> &'static str
  pub fn cross() -> &'static str
  pub fn Spinner::start(label: impl Into<String>) -> Self
  pub fn Spinner::finish(self, message: &str)
  ```
- **Consumers**: `main.rs`, `discover.rs`.
- **Side Effects**: writes ANSI escape sequences and spinner frames to stdout/stderr only when a TTY and `NO_COLOR` is unset.

### `src/config.rs` (Role: domain, Lines: 315)
- **Responsibility**: TOML config, defaults, first-run wizard.
- **Imports**: `serde`, `toml`, `anyhow`, `crate::quality`.
- **Types & Enums**: `pub struct Config` with fields: `base_url`, `gemini_api_key`, `gemini_models`, `allowed_qualities`, `original_only`, `resolve_on_load`, `cache_ttl_minutes`, `auto_update`, `ai_batch_size`, `ai_batch_chars`, `ai_batch_pause_ms`, `ai_cache_hours`, `magnet_post_url`, `auto_discover_domain` (default true), `domain_search_query` (default "1tamilmv"), `domain_search_endpoint` (default "https://www.bing.com/search").
- **Public Functions & Signatures**:
  ```rust
  pub fn Config::interactive() -> bool
  pub fn Config::configure(config: &mut Config) -> Result<()>
  pub fn Config::load(path: &Path) -> Result<Config>
  pub fn Config::save(&self, path: &Path) -> Result<()>
  pub fn Config::api_key(&self) -> String
  pub fn default_config_path() -> PathBuf
  ```
- **Consumers**: `main.rs`, `app.rs`.
- **Side Effects**: reads/writes config.toml, reads `GEMINI_API_KEY`.

### `src/site.rs` (Role: api, Lines: 898)
- **Responsibility**: HTTP client, three on-disk caches, HTML scraping of forums and releases.
- **Imports**: `reqwest`, `scraper`, `regex`, `futures_util`.
- **Types & Enums**: `Client`, `ForumPage`, `PageLoad`, `AnchorKind`, `Anchor`, `HttpCacheEntry`, `PageCacheEntry`, `ThreadCacheEntry`, `PAGE_CACHE_VERSION`.
- **Public Functions & Signatures**:
  ```rust
  pub fn forum_page_url(base_url: &str, forum: &Forum, page: u32) -> String
  pub fn Client::new(base_url: &str, cache_dir: PathBuf, cache_ttl: Duration) -> Result<Self>
  pub async fn Client::get(&self, url: &str) -> Result<String>
  pub async fn Client::fetch_forum(&self, forum: &'static Forum, page: u32) -> Result<ForumPage>
  pub async fn Client::fetch_page(&self, forum: &'static Forum, page: u32) -> Result<PageLoad>
  pub async fn Client::fetch_thread(&self, thread: &Thread) -> Result<Thread>
  pub fn parse_forum(body: &str, forum: &'static Forum, page: u32) -> Result<ForumPage>
  pub fn parse_releases(body: &str) -> Result<Vec<Release>>
  pub fn clean_release_name(raw: &str) -> String
  pub fn default_cache_dir() -> PathBuf
  pub(crate) const USER_AGENT: &str
  ```
- **Consumers**: `main.rs`, `app.rs`, `discover.rs`.
- **Side Effects**: HTTP GET, JSON cache files under `<cache>/drushti/http|pages|threads`.

### `src/app.rs` (Role: domain, Lines: 650)
- **Responsibility**: Application state, filters, search result assembly, AI queue plumbing.
- **Imports**: `crate::ai`, `config`, `forums`, `lang`, `model`, `quality`, `site`, `search`.
- **Types & Enums**: `Screen::{Browser,Detail,Help,Language}`, `Filter`, `PageState`, `ScanProgress`, `Msg::{PageReady,PageFailed,ScanFinished,AiResolved,AiFailed,MagnetPosted}`, `App` (all state fields incl. search overlay fields).
- **Public Functions & Signatures**:
  ```rust
  pub fn App::new(config: Config, cache_path: PathBuf) -> anyhow::Result<App>
  pub fn App::show_page(&mut self, page: u32)
  pub fn App::store_page(&mut self, load: PageLoad)
  pub fn App::reset_scan(&mut self)
  pub fn App::queue_ai_threads(&mut self, threads: &[Thread])
  pub fn App::apply_ai(&mut self, map: &HashMap<String, CacheEntry>)
  pub fn App::visible_threads(&self) -> Vec<usize>
  pub fn App::thread_passes(&self, thread: &Thread) -> bool
  pub fn App::effective_original(&self, thread: &Thread) -> Option<OriginalLang>
  pub fn App::visible_releases(&self) -> Vec<usize>
  pub fn App::set_query(&mut self, query: String)
  pub fn App::rebuild_search_results(&mut self)
  pub fn App::jump_to_result(&mut self, idx: usize)
  ```
- **Consumers**: `main.rs`, `ui/*`.
- **Side Effects**: none (pure state).

### `src/ai.rs` (Role: api, Lines: 589)
- **Responsibility**: Gemini client, original-language resolution, on-disk language cache, batching.
- **Imports**: `reqwest`, `serde`, `anyhow`, `crate::lang`, `crate::model`.
- **Types & Enums**: `CacheEntry`, `LangCache`, `Gemini`, `MODEL_CHAIN`.
- **Public Functions & Signatures**:
  ```rust
  pub fn Gemini::new(api_key: &str, models: Vec<String>) -> Result<Self>
  pub async fn Gemini::resolve_batch(&self, keys: &[String]) -> Result<HashMap<String, CacheEntry>>
  pub async fn Gemini::probe(&self) -> Result<Vec<String>>
  pub fn LangCache::load(path: &Path) -> LangCache
  pub fn LangCache::save(&self, path: &Path) -> Result<()>
  pub fn LangCache::get_with_ttl(&self, key: &str, ttl_secs: u64) -> Option<&CacheEntry>
  pub fn heuristic_original(thread: &Thread) -> OriginalLang
  pub fn plan_batches(keys: &[String], max_items: usize, max_chars: usize) -> Vec<Vec<String>>
  pub fn cache_key(title: &str, year: Option<u16>) -> String
  pub fn effective_original(thread: &Thread, cache: &LangCache, key: &str, ttl_secs: u64) -> OriginalLang
  pub fn default_cache_path() -> PathBuf
  ```
- **Consumers**: `main.rs`, `app.rs`.
- **Side Effects**: POST to Gemini API, reads/writes `langcache.json`.

### `src/model.rs` (Role: domain, Lines: 225)
- **Responsibility**: Core data types and title splitting.
- **Types & Enums**: `Thread`, `Release`, `OriginalLang`, `LangSource::{Ai,MultiAudio,TitleTag,Forum,Verified,Unknown}`, `StoredThread`.
- **Public Functions & Signatures**:
  ```rust
  pub fn split_title(title: &str) -> (String, Option<u16>, String)
  pub fn StoredThread::from_thread(thread: &Thread) -> Self
  pub fn StoredThread::into_thread(self) -> Option<Thread>
  pub fn Release::has_original(&self, original: Lang) -> bool
  ```
- **Consumers**: `site.rs`, `app.rs`, `ai.rs`, `ui/*`.

### `src/search.rs` (Role: domain, Lines: 229)
- **Responsibility**: Fuzzy subsequence movie search.
- **Types & Enums**: `MatchResult { score, text, indices, page_id, thread_index }`.
- **Public Functions & Signatures**:
  ```rust
  pub fn fuzzy_match(query: &str, text: &str) -> Option<MatchResult>
  pub fn match_thread(query: &str, movie: &str, title: &str) -> Option<MatchResult>
  ```
- **Consumers**: `app.rs`. Note: `MatchResult.indices` are computed but not rendered by the UI.

### `src/quality.rs` (Role: domain, Lines: 408)
- **Responsibility**: Lexical classification of quality, source, resolution, size.
- **Types & Enums**: `Quality`, `Source`, `Resolution`, `ReleaseInfo`.
- **Public Functions & Signatures**:
  ```rust
  pub fn analyse(name: &str) -> ReleaseInfo
  pub fn contains_token(haystack: &str, needle: &str) -> bool
  pub fn human_size(bytes: Option<u64>) -> String
  ```

### `src/lang.rs` (Role: domain, Lines: 555)
- **Responsibility**: Language enum, scene aliases, release-name language parsing.
- **Types & Enums**: `Lang` (27 variants), `LangSet { audio, text, dubbed, subtitled }`.
- **Public Functions & Signatures**:
  ```rust
  pub fn parse_release_langs(name: &str) -> LangSet
  pub fn scan_text(text: &str) -> Vec<Lang>
  pub fn lang_from_token(token: &str) -> Option<Lang>
  pub fn lang_from_list_token(token: &str) -> Option<Lang>
  pub fn Lang::code(self) -> &'static str
  pub fn Lang::label(self) -> &'static str
  pub fn Lang::from_code(code: &str) -> Lang
  ```

### `src/forums.rs` (Role: domain, Lines: 424)
- **Responsibility**: Static forum catalogue (id, slug, title, language, kind).
- **Types & Enums**: `Forum`, `ForumKind`.
- **Public Functions & Signatures**:
  ```rust
  pub fn by_id(id: u32) -> Option<&'static Forum>
  pub fn all_web_hd() -> Vec<&'static Forum>
  pub fn f_lang_root(lang: Lang) -> u32
  ```

### `src/ui/mod.rs` (Role: tui, Lines: 88)
- **Responsibility**: Theme constants, screen router, truncate helper.
- **Public Functions & Signatures**: `pub fn draw(frame: &mut ratatui::Frame, app: &mut App)`, `pub fn truncate(text: &str, width: usize) -> String`, `Theme::{block,popup,good,bad,muted,focus}`.

### `src/ui/browser.rs` (Role: tui, Lines: 599)
- **Responsibility**: Header, filter bar, thread table, filter popup, language picker, search bar and results.

### `src/ui/detail.rs` (Role: tui, Lines: 232)
- **Responsibility**: Release header, release table, key hints.

### `src/ui/help.rs` (Role: tui, Lines: 86)
- **Responsibility**: Key binding reference screen.

## 4. Execution Lifecycle Trace

1. **Startup**: `main()` parses CLI, loads `Config`, applies overrides, runs first-run wizard if needed.
2. **Discovery**: if `auto_discover_domain`, probe configured board using `site::forum_page_url`; on failure check cooldown, search Bing, parse link targets and result text for board domains, probe each candidate, persist the first working `base_url` to config.toml.
3. **Dispatch**: subcommands run directly (`list`, `resolve`, `probe`, `update`, config helpers); no subcommand enters `run_tui`.
4. **TUI**: language picker -> `start_scan` spawns page fetches -> `Msg::PageReady` stores pages -> AI batches resolve original languages -> tables render.
5. **Exit**: save language cache, `ratatui::restore()`.

## 5. Verification Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --target x86_64-unknown-linux-gnu
```

## 6. Recent Iteration Changes

- **2026-11-21**: Added `src/ansi.rs` for TTY-aware coloured CLI output and a stderr spinner, applied to `list`, `resolve`, `probe`, `update` and discovery messages; `list --json` and `NO_COLOR` stay plain. Added `src/discover.rs` + `src/discover/{parse,cooldown}.rs` for startup board-domain discovery via Bing with a 1-hour failure cooldown. Candidates come from Bing redirect links and from domains mentioned in result text; each is probed until one works. Added `auto_discover_domain`, `domain_search_query` and `domain_search_endpoint` config keys. Extracted `site::forum_page_url` and made `USER_AGENT` crate-visible. Removed unused `chrono` dependency and unused imports in `ui/mod.rs`. Rewrote this index from the stale v0.4.0-only stub.
