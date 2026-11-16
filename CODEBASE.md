# CODEBASE.md: drishti Semantic Digest

> **Notice**: AI-optimized semantic index. Do not write narrative prose. Keep token density high.

## 1. System Topology & Data Flow
```text
Entrypoint ──> CLI/Parser ──> Domain Logic ──> Infra/IO
```

## 2. Global Constraints & Architecture Patterns
- **Primary Language**: Rust 2021 edition
- **Architectural Paradigm**: Role-based (domain/, infra/, api/cli/, tui/)
- **Hard Constraints**: <400 lines/file, <60 lines/fn, zero production unwrap(), 0 warnings.
- **Target Distribution**: Linux x86_64 standalone binary

## 3. Module & Interface Skeleton

### `src/ai.rs` (Role: general, Lines: 396)
- **Responsibility**: Core general logic in src/ai.rs
- **Imports**: use std :: collections :: HashMap , use std :: path :: { Path , PathBuf } , use anyhow :: { Context , Result , bail } , use serde :: { Deserialize , Serialize } , use crate :: lang :: Lang , use crate :: model :: { LangSource , OriginalLang , Thread } 
- **Types & Enums**:
  ```rust
  pub struct CacheEntry
  pub struct LangCache
  pub struct Gemini
  ```
- **Public Functions & Signatures**:
  ```rust
  fn load (path : & Path) -> LangCache
  fn save (& self , path : & Path) -> Result < () >
  fn get (& self , key : & str) -> Option < & CacheEntry >
  fn put (& mut self , key : String , entry : CacheEntry)
  fn new (api_key : & str , models : Vec < String >) -> Result < Self >
  async fn resolve_batch (& self , movies : & [(String , String)] ,) -> Result < HashMap < String , CacheEntry > >
  async fn probe (& self) -> Result < Vec < String > >
  fn cache_key (title : & str , year : Option < u16 >) -> String
  fn heuristic_original (thread : & Thread) -> OriginalLang
  fn default_cache_path () -> PathBuf
  fn effective_original (thread : & Thread , cache : & LangCache , key : & str) -> OriginalLang
  ```

### `src/app.rs` (Role: general, Lines: 350)
- **Responsibility**: Core general logic in src/app.rs
- **Imports**: use std :: collections :: { BTreeSet , HashMap } , use std :: path :: PathBuf , use crate :: ai :: { CacheEntry , Gemini , LangCache , cache_key , heuristic_original } , use crate :: config :: Config , use crate :: forums :: { Forum , all_web_hd } , use crate :: lang :: Lang , use crate :: model :: { OriginalLang , Thread } , use crate :: quality :: analyse , use crate :: quality :: Quality , use crate :: site :: Client 
- **Types & Enums**:
  ```rust
  pub enum Screen
  pub struct Filter
  pub enum Msg
  pub struct App
  ```
- **Public Functions & Signatures**:
  ```rust
  fn allows_quality (& self , quality : Quality) -> bool
  fn allows_language (& self , lang : Option < Lang >) -> bool
  fn new (config : Config , cache_path : PathBuf) -> anyhow :: Result < App >
  fn forum (& self) -> & 'static Forum
  fn visible_threads (& self) -> Vec < usize >
  fn thread_passes (& self , thread : & Thread) -> bool
  fn effective_original (& self , thread : & Thread) -> Option < OriginalLang >
  fn detail_thread (& self) -> Option < & Thread >
  fn visible_releases (& self) -> Vec < usize >
  fn select_visible (& mut self , delta : isize)
  fn select_row (& mut self , row : usize)
  fn thread_quality_ok (& self , thread : & Thread) -> bool
  fn selected_row (& self) -> usize
  ```

### `src/config.rs` (Role: general, Lines: 131)
- **Responsibility**: Core general logic in src/config.rs
- **Imports**: use std :: path :: { Path , PathBuf } , use anyhow :: { Context , Result } , use serde :: { Deserialize , Serialize } , use crate :: quality :: Quality 
- **Types & Enums**:
  ```rust
  pub struct Config
  ```
- **Public Functions & Signatures**:
  ```rust
  fn load (path : & Path) -> Result < Config >
  fn save (& self , path : & Path) -> Result < () >
  fn api_key (& self) -> String
  fn default_config_path () -> PathBuf
  ```

### `src/forums.rs` (Role: general, Lines: 184)
- **Responsibility**: Core general logic in src/forums.rs
- **Imports**: use crate :: lang :: Lang 
- **Types & Enums**:
  ```rust
  pub struct Forum
  pub enum ForumKind
  ```
- **Public Functions & Signatures**:
  ```rust
  fn label (self) -> & 'static str
  fn by_id (id : u32) -> Option < & 'static Forum >
  fn web_hd_forum (lang : Lang) -> Option < & 'static Forum >
  fn all_web_hd () -> Vec < & 'static Forum >
  ```

### `src/lang.rs` (Role: general, Lines: 551)
- **Responsibility**: Core general logic in src/lang.rs
- **Imports**: use std :: fmt , use serde :: { Deserialize , Serialize } 
- **Types & Enums**:
  ```rust
  pub enum Lang
  pub struct LangSet
  ```
- **Public Functions & Signatures**:
  ```rust
  fn code (self) -> & 'static str
  fn label (self) -> & 'static str
  fn from_code (code : & str) -> Lang
  fn lang_from_token (token : & str) -> Option < Lang >
  fn lang_from_list_token (token : & str) -> Option < Lang >
  fn parse_list_group (group : & str) -> Vec < Lang >
  fn scan_text (text : & str) -> Vec < Lang >
  fn all (& self) -> Vec < Lang >
  fn is_multi_audio (& self) -> bool
  fn compact (& self) -> String
  fn parse_release_langs (name : & str) -> LangSet
  ```

### `src/main.rs` (Role: general, Lines: 1081)
- **Responsibility**: Core general logic in src/main.rs
- **Imports**: use std :: collections :: HashMap , use std :: path :: PathBuf , use std :: sync :: Arc , use anyhow :: { Context , Result } , use clap :: { Parser , Subcommand } , use crossterm :: event :: { Event , EventStream , KeyCode , KeyEvent , KeyEventKind , KeyModifiers } , use futures_util :: StreamExt , use tokio :: sync :: { Semaphore , mpsc } , use crate :: ai :: { CacheEntry , Gemini , LangCache , cache_key , effective_original , heuristic_original } , use crate :: app :: { App , Msg , Screen } , use crate :: config :: Config , use crate :: forums :: { Forum , all_web_hd } , use crate :: lang :: Lang , use crate :: model :: Thread , use crate :: quality :: { Quality , analyse , human_size } 

### `src/model.rs` (Role: general, Lines: 135)
- **Responsibility**: Core general logic in src/model.rs
- **Imports**: use serde :: { Deserialize , Serialize } , use crate :: forums :: Forum , use crate :: lang :: { Lang , LangSet } , use crate :: quality :: ReleaseInfo 
- **Types & Enums**:
  ```rust
  pub struct Thread
  pub enum LangSource
  pub struct OriginalLang
  pub struct Release
  ```
- **Public Functions & Signatures**:
  ```rust
  fn label (self) -> & 'static str
  fn has_original (& self , original : Lang) -> bool
  fn split_title (title : & str) -> (String , Option < u16 > , String)
  ```

### `src/quality.rs` (Role: general, Lines: 406)
- **Responsibility**: Core general logic in src/quality.rs
- **Imports**: use std :: fmt , use serde :: { Deserialize , Serialize } 
- **Types & Enums**:
  ```rust
  pub enum Quality
  pub enum Source
  pub enum Resolution
  pub struct ReleaseInfo
  ```
- **Public Functions & Signatures**:
  ```rust
  fn label (self) -> & 'static str
  fn is_web_dl (self) -> bool
  fn label (self) -> & 'static str
  fn label (self) -> & 'static str
  fn analyse (name : & str) -> ReleaseInfo
  fn contains_token (haystack : & str , needle : & str) -> bool
  fn human_size (bytes : Option < u64 >) -> String
  ```

### `src/site.rs` (Role: general, Lines: 784)
- **Responsibility**: Core general logic in src/site.rs
- **Imports**: use std :: time :: Duration , use anyhow :: { Context , Result , bail } , use regex :: Regex , use scraper :: { ElementRef , Html , Selector } , use std :: path :: PathBuf , use std :: sync :: LazyLock , use crate :: forums :: { Forum } , use crate :: lang :: { LangSet , parse_release_langs } , use crate :: model :: { Release , Thread , split_title } , use crate :: quality :: analyse 
- **Types & Enums**:
  ```rust
  pub struct Client
  pub struct ForumPage
  ```
- **Public Functions & Signatures**:
  ```rust
  fn new (base_url : & str , cache_dir : PathBuf , cache_ttl : Duration) -> Result < Self >
  async fn get (& self , url : & str) -> Result < String >
  fn forum_url (& self , forum : & Forum , page : u32) -> String
  async fn fetch_forum (& self , forum : & 'static Forum , page : u32) -> Result < ForumPage >
  async fn fetch_thread (& self , thread : & Thread) -> Result < Thread >
  fn parse_forum (body : & str , forum : & 'static Forum , page : u32) -> Result < ForumPage >
  fn parse_releases (body : & str) -> Result < Vec < Release > >
  fn clean_release_name (raw : & str) -> String
  fn default_cache_dir () -> PathBuf
  ```

### `src/ui/browser.rs` (Role: tui, Lines: 451)
- **Responsibility**: Core tui logic in src/ui/browser.rs
- **Imports**: use ratatui :: layout :: { Constraint , Direction , Layout , Rect } , use ratatui :: style :: { Modifier , Style } , use ratatui :: text :: { Line , Span , Text } , use ratatui :: widgets :: { Cell , Paragraph , Row , Table , TableState , Wrap } , use crate :: app :: App , use crate :: lang :: Lang , use crate :: model :: LangSource , use crate :: quality :: { Quality , analyse , human_size } , use crate :: ui :: Theme 
- **Public Functions & Signatures**:
  ```rust
  fn draw (frame : & mut ratatui :: Frame , app : & mut App)
  fn draw_language (frame : & mut ratatui :: Frame , app : & mut App)
  ```

### `src/ui/detail.rs` (Role: tui, Lines: 203)
- **Responsibility**: Core tui logic in src/ui/detail.rs
- **Imports**: use ratatui :: layout :: { Constraint , Direction , Layout , Rect } , use ratatui :: style :: { Modifier , Style } , use ratatui :: text :: { Line , Span } , use ratatui :: widgets :: { Cell , Paragraph , Row , Table , TableState } , use crate :: app :: App , use crate :: model :: LangSource , use crate :: quality :: { Quality , human_size } , use crate :: ui :: { Theme , truncate } 
- **Public Functions & Signatures**:
  ```rust
  fn draw (frame : & mut ratatui :: Frame , app : & mut App)
  ```

### `src/ui/help.rs` (Role: tui, Lines: 79)
- **Responsibility**: Core tui logic in src/ui/help.rs
- **Imports**: use ratatui :: layout :: { Constraint , Direction , Layout } , use ratatui :: style :: Style , use ratatui :: text :: { Line , Span } , use ratatui :: widgets :: { Paragraph , Wrap } , use crate :: app :: App , use crate :: ui :: Theme 
- **Public Functions & Signatures**:
  ```rust
  fn draw (frame : & mut ratatui :: Frame , _app : & mut App)
  ```

### `src/ui/mod.rs` (Role: tui, Lines: 86)
- **Responsibility**: Core tui logic in src/ui/mod.rs
- **Imports**: use ratatui :: style :: { Color , Modifier , Style } , use ratatui :: widgets :: { Block , BorderType , Borders } , use crate :: app :: { App , Screen } 
- **Types & Enums**:
  ```rust
  pub struct Theme
  ```
- **Public Functions & Signatures**:
  ```rust
  fn block (title : & str) -> Block < 'static >
  fn popup (title : & str) -> Block < 'static >
  fn good () -> Style
  fn bad () -> Style
  fn muted () -> Style
  fn focus () -> Style
  fn draw (frame : & mut ratatui :: Frame , app : & mut App)
  fn truncate (text : & str , width : usize) -> String
  ```

## 4. Execution Lifecycle Trace
1. **Startup**: Entrypoint parses CLI flags & dispatches command.
2. **Execution**: Core domain logic processes inputs and evaluates rules.
3. **Persistence / I/O**: Domain logic calls infra for disk/terminal I/O.
4. **Exit**: Graceful termination with standard exit codes.

## 5. Verification Commands
```bash
cargo build --release --target x86_64-unknown-linux-gnu
cargo test --all-targets
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```
