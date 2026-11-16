//! 1TamilMV browser — a TUI for finding WEB-DL releases and filtering them
//! by the language the film was originally made in.

mod ai;
mod app;
mod config;
mod download;
mod forums;
mod lang;
mod model;
mod quality;
mod site;
mod ui;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use tokio::sync::{Semaphore, mpsc};

use crate::ai::{CacheEntry, Gemini, LangCache, cache_key, effective_original, heuristic_original};
use crate::app::{App, Msg, Screen};
use crate::config::Config;
use crate::download::{DownloadEvent, DownloadKind, DownloadStatus, DownloadTask};
use crate::forums::{Forum, all_web_hd};
use crate::lang::Lang;
use crate::model::Thread;
use crate::quality::{Quality, analyse, human_size};

#[derive(Parser)]
#[command(
    name = "drishti",
    version,
    about = "Browse 1TamilMV for WEB-DL releases, filtered by original language"
)]
struct Cli {
    /// Path to the configuration file.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Board address.
    #[arg(long, global = true)]
    base_url: Option<String>,

    /// Download directory.
    #[arg(long, global = true)]
    download_dir: Option<PathBuf>,

    /// Gemini API key. Falls back to GEMINI_API_KEY.
    #[arg(long, global = true)]
    gemini_key: Option<String>,

    /// Disable the on-disk HTTP cache (always re-fetch).
    #[arg(long, global = true)]
    no_cache: bool,

    /// First forum page to scan (default 1).
    #[arg(long, global = true)]
    from: Option<u32>,

    /// Last forum page to scan (default: every page).
    #[arg(long, global = true)]
    to: Option<u32>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Resolve the original production language of every thread on a forum page.
    Resolve {
        /// Forum name or id, for example kannada or 69.
        #[arg(long, default_value = "kannada")]
        forum: String,
        /// Page number.
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    /// List a forum page without starting the interface.
    List {
        /// Forum name or id, for example kannada or 69.
        #[arg(long, default_value = "kannada")]
        forum: String,
        /// Page number.
        #[arg(long, default_value_t = 1)]
        page: u32,
        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,
        /// Also fetch each thread and list its releases.
        #[arg(long)]
        releases: bool,
        /// Allow every quality instead of only WEB-DL.
        #[arg(long)]
        all_qualities: bool,
    },
    /// Check that the Gemini key works and list the available models.
    Probe,
    /// Print the configuration file path.
    ConfigPath,
    /// Write a default configuration file.
    Init,
    /// Update drishti to the latest source (git pull + reinstall).
    Update,
}

/// The entry point.
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);

    let mut config = Config::load(&config_path)?;
    if let Some(base) = cli.base_url {
        config.base_url = base;
    }
    if let Some(dir) = cli.download_dir.clone() {
        config.download_dir = dir;
    }
    if let Some(key) = cli.gemini_key.clone() {
        config.gemini_api_key = key;
    }

    let from_page = cli.from.unwrap_or(1);
    let to_page = cli.to;
    let no_cache = cli.no_cache;
    if no_cache {
        config.cache_ttl_minutes = 0;
    }
    match cli.command {
        Some(Command::Probe) => probe(&config).await,
        Some(Command::Resolve { forum, page }) => resolve(&config, &forum, page).await,
        Some(Command::ConfigPath) => {
            println!("{}", config_path.display());
            Ok(())
        }
        Some(Command::Init) => {
            config.save(&config_path)?;
            println!("wrote {}", config_path.display());
            Ok(())
        }
        Some(Command::Update) => update().await,
        Some(Command::List {
            forum,
            page,
            json,
            releases,
            all_qualities,
        }) => {
            let mut config = config;
            if all_qualities {
                config.allowed_qualities = vec![
                    Quality::WebDl,
                    Quality::Bluray,
                    Quality::WebRip,
                    Quality::TvRip,
                    Quality::DvdRip,
                    Quality::HdRip,
                    Quality::Theater,
                    Quality::Cam,
                ];
            }
            list(&config, &forum, page, json, releases).await
        }
        None => {
            maybe_auto_update(&config);
            run_tui(config, config_path, from_page, to_page).await
        }
    }
}

/// Verify the Gemini key and print the usable models.
async fn probe(config: &Config) -> Result<()> {
    let key = config.api_key();
    if key.is_empty() {
        anyhow::bail!("no API key set. Use --gemini-key, GEMINI_API_KEY, or the config file.");
    }
    let gemini = Gemini::new(&key, config.gemini_models.clone())?;
    let models = gemini.probe().await?;
    println!("key is valid, {} models available", models.len());
    for name in &config.gemini_models {
        let mark = if models.iter().any(|m| m == name) {
            "available"
        } else {
            "not listed"
        };
        println!("  {name:<28} {mark}");
    }
    Ok(())
}

/// Print a forum page.
async fn list(
    config: &Config,
    forum: &str,
    page: u32,
    json: bool,
    with_releases: bool,
) -> Result<()> {
    let forum = resolve_forum(forum)?;
    let client = site::Client::new(
        &config.base_url,
        site::default_cache_dir(),
        std::time::Duration::from_secs(config.cache_ttl_minutes as u64 * 60),
    )?;
    let mut result = client.fetch_forum(forum, page).await?;

    if with_releases {
        let sem = Arc::new(Semaphore::new(6));
        let mut handles = Vec::new();
        for thread in result.threads.iter_mut() {
            let c = client.clone();
            let s = sem.clone();
            let t = thread.clone();
            handles.push(tokio::spawn(async move {
                let _guard = s.acquire().await.ok()?;
                c.fetch_thread(&t).await.ok()
            }));
        }
        for handle in handles {
            if let Ok(Some(updated)) = handle.await
                && let Some(slot) = result
                    .threads
                    .iter_mut()
                    .find(|x| x.url == updated.url)
                {
                    *slot = updated;
                }
        }
    }

    if json {
        let rows: Vec<serde_json::Value> = result
            .threads
            .iter()
            .map(|thread| {
                let info = analyse(&thread.title);
                let original = thread.original.or_else(|| Some(heuristic_original(thread)));
                serde_json::json!({
                    "id": thread.id,
                    "movie": thread.movie,
                    "year": thread.year,
                    "title": thread.title,
                    "forum": thread.forum.title,
                    "quality": info.quality.label(),
                    "source": info.source.label(),
                    "resolution": info.resolution.label(),
                    "original_language": original.map(|o| o.lang.code()),
                    "original_source": original.map(|o| o.source.label()),
                    "url": thread.url,
                    "releases": thread.releases.iter().map(|r| serde_json::json!({
                        "name": r.name,
                        "quality": r.info.quality.label(),
                        "resolution": r.info.resolution.label(),
                        "audio": r.langs.compact(),
                        "dubbed": r.langs.dubbed,
                        "size": r.info.size_bytes,
                        "size_human": human_size(r.info.size_bytes),
                        "magnet": r.magnet,
                        "direct": r.direct,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    println!(
        "{} — page {}/{}  ({} threads)",
        forum.title,
        result.page,
        result.total_pages,
        result.threads.len()
    );
    for thread in &result.threads {
        let info = analyse(&thread.title);
        let original = thread.original.or_else(|| Some(heuristic_original(thread)));
        let quality = format!("{:<8}", info.quality.label());
        println!(
            "  {:<44} {:<8} {:<12} {}",
            truncate(&thread.title, 44),
            quality,
            original
                .map(|o| o.lang.label().to_string())
                .unwrap_or_else(|| "?".to_string()),
            thread.url
        );
        if with_releases {
            for release in &thread.releases {
                println!(
                    "      {:<8} {:<7} {:<22} {:<8} {}",
                    release.info.quality.label(),
                    release.info.resolution.label(),
                    release.langs.compact(),
                    human_size(release.info.size_bytes),
                    release.name
                );
            }
        }
    }
    Ok(())
}

/// Find a forum by name fragment or id.
fn resolve_forum(name: &str) -> Result<&'static Forum> {
    if let Ok(id) = name.parse::<u32>() {
        return forums::by_id(id).context("unknown forum id");
    }
    let needle = name.to_ascii_lowercase();
    all_web_hd()
        .into_iter()
        .find(|f| {
            f.title.to_ascii_lowercase().contains(&needle)
                || f.lang.label().to_ascii_lowercase() == needle
        })
        .context("no forum matched")
}

/// Resolve the original production language of the threads on a forum page.
async fn resolve(config: &Config, forum_name: &str, page: u32) -> Result<()> {
    let forum = resolve_forum(forum_name)?;
    let client = site::Client::new(
        &config.base_url,
        site::default_cache_dir(),
        std::time::Duration::from_secs(config.cache_ttl_minutes as u64 * 60),
    )?;
    let result = client.fetch_forum(forum, page).await?;
    let mut threads = result.threads;

    let sem = Arc::new(Semaphore::new(6));
    let mut handles = Vec::new();
    for thread in threads.iter_mut() {
        let c = client.clone();
        let s = sem.clone();
        let t = thread.clone();
        handles.push(tokio::spawn(async move {
            let _guard = s.acquire().await.ok();
            c.fetch_thread(&t).await.ok()
        }));
    }
    for handle in handles {
        if let Ok(Some(updated)) = handle.await
            && let Some(slot) = threads.iter_mut().find(|x| x.url == updated.url) {
                *slot = updated;
            }
    }

    let key = config.api_key();
    if key.trim().is_empty() {
        anyhow::bail!("no API key set. Use --gemini-key, GEMINI_API_KEY, or the config file.");
    }
    let gemini = Gemini::new(&key, config.gemini_models.clone())?;
    let mut cache = LangCache::load(&ai::default_cache_path());

    let mut ai_pending: Vec<(usize, String, Option<u16>)> = Vec::new();
    for (index, thread) in threads.iter().enumerate() {
        let cache_key_str = cache_key(&thread.movie, thread.year);
        if cache.get(&cache_key_str).is_none() {
            let original = heuristic_original(thread);
            if original.source != crate::model::LangSource::MultiAudio {
                ai_pending.push((index, cache_key_str, thread.year));
            }
        }
    }

    println!(
        "forum {} page {} — {} threads, {} needing model resolution",
        forum.title,
        page,
        threads.len(),
        ai_pending.len()
    );

    let size = config.ai_batch_size.max(1);
    for batch in ai_pending.chunks(size) {
        let requests: Vec<(String, String)> = batch
            .iter()
            .map(|(_, key, year)| {
                (
                    key.clone(),
                    match year {
                        Some(y) => format!("{key}|{y}"),
                        None => key.clone(),
                    },
                )
            })
            .collect();
        let map = gemini.resolve_batch(&requests).await?;
        for (cache_key_str, entry) in map {
            let entry_lang = entry.lang;
            let entry_confidence = entry.confidence;
            let entry_source = entry.source;
            cache.put(cache_key_str.clone(), entry);
            for thread in &mut threads {
                if cache_key(&thread.movie, thread.year) == cache_key_str {
                    thread.original = Some(crate::model::OriginalLang {
                        lang: entry_lang,
                        confidence: entry_confidence,
                        source: entry_source,
                    });
                }
            }
        }
        let _ = cache.save(&ai::default_cache_path());
    }

    for thread in &threads {
        let key = cache_key(&thread.movie, thread.year);
        let original = effective_original(thread, &cache, &key);
        println!(
            "  {:<44} {:<8} confidence {:.2}",
            truncate(&thread.title, 44),
            original.lang.label(),
            original.confidence
        );
    }
    Ok(())
}

/// The file the installer writes to record where the source lives.
fn source_file() -> PathBuf {
    config::default_config_path()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("source.txt")
}

/// The recorded source checkout, if the installer wrote one.
fn recorded_source() -> Option<PathBuf> {
    std::fs::read_to_string(source_file())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Update drishti: from its source checkout when installed from source,
/// otherwise by downloading the latest prebuilt release binary.
async fn update() -> Result<()> {
    if let Some(src) = recorded_source() {
        return update_from_source(&src);
    }
    update_from_release().await
}

/// `git pull` + `cargo install` from the recorded source checkout.
fn update_from_source(src: &std::path::Path) -> Result<()> {
    if !src.join(".git").exists() {
        anyhow::bail!("{} is not a git checkout", src.display());
    }
    println!("==> pulling latest source in {}", src.display());
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src)
        .args(["pull", "--ff-only"])
        .status()
        .context("running git pull")?;
    if !status.success() {
        anyhow::bail!("git pull failed");
    }
    println!("==> rebuilding & installing drishti");
    let status = std::process::Command::new("cargo")
        .args(["install", "--path"])
        .arg(src)
        .arg("--force")
        .status()
        .context("running cargo install")?;
    if !status.success() {
        anyhow::bail!("cargo install failed");
    }
    println!("==> drishti is up to date");
    Ok(())
}

/// Download the latest prebuilt release binary and replace the running one.
async fn update_from_release() -> Result<()> {
    const URL: &str =
        "https://github.com/Praveensenpai/drishti/releases/latest/download/drishti-linux-x86_64";
    println!("==> downloading latest release");
    let client = reqwest::Client::builder()
        .user_agent("drishti-updater")
        .build()
        .context("building HTTP client")?;
    let resp = client.get(URL).send().await.context("downloading release")?;
    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await.context("reading release bytes")?;
    let exe = std::env::current_exe().context("locating current executable")?;
    let tmp = exe.with_extension("new");
    std::fs::write(&tmp, &bytes).context("writing new binary")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }
    std::fs::rename(&tmp, &exe).context("replacing executable")?;
    println!("==> drishti updated to the latest release");
    Ok(())
}

/// The directory used for cache / update bookkeeping.
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("drishti")
}

/// Spawn `drishti update` in the background once a day (best effort).
fn maybe_auto_update(config: &Config) {
    if !config.auto_update {
        return;
    }
    if recorded_source().is_none() {
        return;
    }
    let stamp = cache_dir().join("update_check");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(text) = std::fs::read_to_string(&stamp)
        && let Ok(last) = text.trim().parse::<u64>()
        && now.saturating_sub(last) < 24 * 60 * 60
    {
        return;
    }
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&stamp, now.to_string());
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::process::Command::new(exe)
        .arg("update")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Run the interactive interface.
async fn run_tui(
    config: Config,
    config_path: PathBuf,
    page_from: u32,
    page_to: Option<u32>,
) -> Result<()> {
    let cache_path = ai::default_cache_path();
    let mut app = App::new(config, cache_path)?;
    app.page = page_from;
    app.page_from = page_from;
    app.page_to = page_to;

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let (download_tx, mut download_rx) = mpsc::unbounded_channel::<DownloadEvent>();

    // The language picker is shown first; the first page is loaded when the
    // user picks a language. Nothing is fetched until then.

    let mut terminal = ratatui::init();
    let mut events = EventStream::new();

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        tokio::select! {
            Some(Ok(event)) = events.next() => {
                if let Event::Key(key) = event
                    && key.kind == KeyEventKind::Press {
                        handle_key(&mut app, key, &tx, &download_tx).await;
                    }
            }
            Some(msg) = rx.recv() => {
                handle_message(&mut app, msg, &tx).await;
            }
            Some(event) = download_rx.recv() => {
                app.apply_download_event(event);
            }
        }

        if app.should_quit {
            break;
        }
    }

    let _ = app.cache.save(&app.cache_path);
    ratatui::restore();
    let _ = config_path;
    Ok(())
}

/// Fetch a forum page in the background and then enrich every thread.
fn spawn_page_load(app: &App, forum: &'static Forum, page: u32, tx: mpsc::UnboundedSender<Msg>) {
    let client = app.client.clone();
    let tx_page = tx.clone();
    tokio::spawn(async move {
        match client.fetch_forum(forum, page).await {
            Ok(result) => {
                let threads = result.threads;
                if tx_page.send(Msg::PageLoaded {
                    threads: threads.clone(),
                    page: result.page,
                    total_pages: result.total_pages,
                }).is_err() {
                    return;
                }
                let sem = Arc::new(Semaphore::new(6));
                let mut handles = Vec::new();
                for (index, thread) in threads.iter().enumerate() {
                    let c = client.clone();
                    let s = sem.clone();
                    let t = tx_page.clone();
                    let thread = thread.clone();
                    handles.push(tokio::spawn(async move {
                        let _guard = s.acquire().await.ok();
                        match c.fetch_thread(&thread).await {
                            Ok(updated) => {
                                let _ = t.send(Msg::ThreadEnriched { index, thread: updated });
                            }
                            Err(e) => {
                                let _ = t.send(Msg::ThreadFailed { index, error: e.to_string() });
                            }
                        }
                    }));
                }
                for handle in handles {
                    let _ = handle.await;
                }
                let _ = tx_page.send(Msg::PageFailed(String::new()));
            }
            Err(e) => {
                let _ = tx_page.send(Msg::PageFailed(e.to_string()));
            }
        }
    });
}

/// Handle a message from a background task.
async fn handle_message(app: &mut App, msg: Msg, tx: &mpsc::UnboundedSender<Msg>) {
    match msg {
        Msg::PageLoaded {
            threads,
            page,
            total_pages,
        } => {
            app.loading = false;
            app.page = page;
            app.total_pages = total_pages;
            app.threads = threads;
            app.selected = 0;
            app.detail_index = None;
            app.error = None;
            app.status = format!("{} threads loaded", app.threads.len());

            // Auto-paginate: when a scan is running and a page arrives with more
            // pages left, load the next one automatically. Manual ←/→ paging
            // disables `scan_to_end`, so the user stays in control.
            let limit = app.page_to.unwrap_or(total_pages);
            if app.scan_to_end {
                if page < limit {
                    let forum = app.forum();
                    spawn_page_load(app, forum, page + 1, tx.clone());
                } else if app.page_from < limit {
                    // The scan reached its end. Come back to the page the scan
                    // started from, so the user lands on real content instead of
                    // the (often nearly empty) final page.
                    let forum = app.forum();
                    let start = app.page_from;
                    app.scan_to_end = false;
                    app.page = start;
                    spawn_page_load(app, forum, start, tx.clone());
                }
            }
        }
        Msg::PageFailed(error) => {
            if error.is_empty() {
                // The enrichment pass finished; ask the model about anything left.
                dispatch_ai(app, tx);
                app.loading = false;
            } else {
                app.loading = false;
                app.error = Some(error);
            }
        }
        Msg::ThreadEnriched { index, thread } => {
            if let Some(slot) = app.threads.get_mut(index) {
                *slot = thread;
            }
            queue_ai_candidate(app, index);
        }
        Msg::ThreadFailed { index, error } => {
            app.status = format!("thread {index} could not be read: {error}");
        }
        Msg::AiResolved(map) => {
            app.ai_in_flight = false;
            for (key, entry) in map {
                app.cache.put(key.clone(), entry);
            }
            let _ = app.cache.save(&app.cache_path);
            apply_ai_to_threads(app);
            dispatch_ai(app, tx);
        }
        Msg::AiFailed(error) => {
            app.ai_in_flight = false;
            app.status = format!("language lookup failed: {error}");
            dispatch_ai(app, tx);
        }
    }
}

/// Decide whether a thread needs the model to resolve its original language.
fn queue_ai_candidate(app: &mut App, index: usize) {
    if app.gemini.is_none() {
        return;
    }
    let Some(thread) = app.threads.get(index) else {
        return;
    };
    let key = cache_key(&thread.movie, thread.year);
    if app.cache.get(&key).is_some() {
        return;
    }
    let original = heuristic_original(thread);
    // Only ask when the heuristic is not already confident.
    if original.source == crate::model::LangSource::MultiAudio {
        return;
    }
    if original.confidence >= 0.8 {
        return;
    }
    if app.ai_pending.iter().any(|(i, _, _)| *i == index) {
        return;
    }
    app.ai_pending
        .push((index, key, thread.year));
}

/// Send the pending movies to the model in batches.
fn dispatch_ai(app: &mut App, tx: &mpsc::UnboundedSender<Msg>) {
    if app.gemini.is_none() || app.ai_in_flight || app.ai_pending.is_empty() {
        return;
    }
    let size = app.config.ai_batch_size.max(1);
    let batch: Vec<_> = app.ai_pending.drain(..size.min(app.ai_pending.len())).collect();
    app.ai_in_flight = true;

    let Some(gemini) = app.gemini.clone() else {
        app.ai_in_flight = false;
        return;
    };
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut requests: Vec<(String, String)> = Vec::new();
        for (index, key, year) in &batch {
            let display = match year {
                Some(y) => format!("{key}|{y}"),
                None => key.clone(),
            };
            requests.push((key.clone(), display));
            let _ = index;
        }
        match gemini.resolve_batch(&requests).await {
            Ok(map) => {
                let _ = tx.send(Msg::AiResolved(map));
            }
            Err(e) => {
                let _ = tx.send(Msg::AiFailed(e.to_string()));
            }
        }
    });
}

/// Copy text to the terminal clipboard using OSC 52.
fn copy_to_clipboard(text: &str) {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    print!("\x1b]52;c;{encoded}\x07");
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

/// Handle a key press.
async fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &mpsc::UnboundedSender<Msg>,
    download_tx: &mpsc::UnboundedSender<DownloadEvent>,
) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char('c') = key.code { app.should_quit = true }
        return;
    }

    if app.screen == Screen::Help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => app.screen = Screen::Browser,
            _ => {}
        }
        return;
    }

    if app.filter_open {
        handle_filter_key(app, key);
        return;
    }

    match app.screen {
        Screen::Browser => handle_browser_key(app, key, tx).await,
        Screen::Detail => handle_detail_key(app, key, download_tx).await,
        Screen::Downloads => handle_downloads_key(app, key),
        Screen::Help => {}
        Screen::Language => handle_language_key(app, key, tx).await,
    }
}

/// Keys for the language picker.
async fn handle_language_key(
    app: &mut App,
    key: KeyEvent,
    tx: &mpsc::UnboundedSender<Msg>,
) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.forum_index = app.forum_index.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !app.forums.is_empty() {
                app.forum_index = (app.forum_index + 1) % app.forums.len();
            }
        }
        KeyCode::Enter => {
            app.screen = Screen::Browser;
            let forum = app.forum();
            let page = app.page;
            app.page_from = page;
            app.scan_to_end = true;
            app.loading = true;
            app.status = format!("scanning {} from page {page}", forum.title);
            spawn_page_load(app, forum, page, tx.clone());
        }
        KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Browser,
        KeyCode::Char('?') => app.screen = Screen::Help,
        _ => {}
    }
}

/// Keys for the thread browser.
async fn handle_browser_key(
    app: &mut App,
    key: KeyEvent,
    tx: &mpsc::UnboundedSender<Msg>,
) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.screen = Screen::Help,
        KeyCode::Char('f') => {
            app.filter_open = true;
            app.filter_focus = 0;
        }
        KeyCode::Char('o') => {
            app.filter.original_only = !app.filter.original_only;
            app.config.original_only = app.filter.original_only;
        }
        KeyCode::Char('c') => {
            app.filter.languages.clear();
        }
        KeyCode::Char('s') => {
            let forum = app.forum();
            let page = app.page;
            app.scan_to_end = true;
            app.loading = true;
            app.status = "rescanning...".to_string();
            spawn_page_load(app, forum, page, tx.clone());
        }
        KeyCode::Char('r') => {
            let forum = app.forum();
            let page = app.page;
            app.scan_to_end = true;
            app.loading = true;
            spawn_page_load(app, forum, page, tx.clone());
        }
        KeyCode::Char('[') => {
            if !app.forums.is_empty() {
                app.forum_index = (app.forum_index + app.forums.len() - 1) % app.forums.len();
                app.page = 1;
                app.page_from = 1;
                app.scan_to_end = true;
                app.loading = true;
                let forum = app.forum();
                spawn_page_load(app, forum, 1, tx.clone());
            }
        }
        KeyCode::Char(']') => {
            if !app.forums.is_empty() {
                app.forum_index = (app.forum_index + 1) % app.forums.len();
                app.page = 1;
                app.page_from = 1;
                app.scan_to_end = true;
                app.loading = true;
                let forum = app.forum();
                spawn_page_load(app, forum, 1, tx.clone());
            }
        }
        KeyCode::Char('d') => app.screen = Screen::Downloads,
        KeyCode::Char('g') => app.select_row(0),
        KeyCode::Char('G') => {
            let last = app.visible_threads().len().saturating_sub(1);
            app.select_row(last);
        }
        KeyCode::Up | KeyCode::Char('k') => app.select_visible(-1),
        KeyCode::Down | KeyCode::Char('j') => app.select_visible(1),
        KeyCode::PageUp => app.select_visible(-10),
        KeyCode::PageDown => app.select_visible(10),
        KeyCode::Left | KeyCode::Char('h') => {
            if app.page > 1 {
                app.page -= 1;
                // Manual paging: stop the auto-scan so we don't jump onward.
                app.scan_to_end = false;
                app.loading = true;
                let forum = app.forum();
                spawn_page_load(app, forum, app.page, tx.clone());
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if app.page < app.total_pages {
                app.page += 1;
                // Manual paging: stop the auto-scan so we don't jump onward.
                app.scan_to_end = false;
                app.loading = true;
                let forum = app.forum();
                spawn_page_load(app, forum, app.page, tx.clone());
            }
        }
        KeyCode::Enter => {
            if !app.threads.is_empty() {
                app.detail_index = Some(app.selected);
                app.detail_selected = 0;
                app.screen = Screen::Detail;
                app.detail_only_original = false;
            }
        }
        KeyCode::Tab => app.screen = Screen::Downloads,
        _ => {}
    }
}

/// Keys for the filter popup.
fn handle_filter_key(app: &mut App, key: KeyEvent) {
    const QUALITIES: [Quality; 8] = [
        Quality::WebDl,
        Quality::Bluray,
        Quality::WebRip,
        Quality::TvRip,
        Quality::DvdRip,
        Quality::HdRip,
        Quality::Theater,
        Quality::Cam,
    ];
    match key.code {
        KeyCode::Esc => app.filter_open = false,
        KeyCode::Char('q') => app.filter_open = false,
        KeyCode::Char('f') | KeyCode::Enter => app.filter_open = false,
        KeyCode::Up | KeyCode::Char('k') => {
            app.filter_focus = app.filter_focus.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.filter_focus = (app.filter_focus + 1).min(QUALITIES.len() - 1);
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.filter_focus = app.filter_focus.saturating_sub(3);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.filter_focus = (app.filter_focus + 3).min(QUALITIES.len() - 1);
        }
        KeyCode::Char(' ') => {
            let quality = QUALITIES[app.filter_focus.min(QUALITIES.len() - 1)];
            if app.filter.qualities.contains(&quality) {
                app.filter.qualities.remove(&quality);
            } else {
                app.filter.qualities.insert(quality);
            }
        }
        KeyCode::Char('a') => {
            if app.filter.qualities.len() == QUALITIES.len() {
                app.filter.qualities.clear();
            } else {
                app.filter.qualities = QUALITIES.into_iter().collect();
            }
        }
        KeyCode::Char('o') => app.filter.original_only = !app.filter.original_only,
        _ => {}
    }
    app.config.allowed_qualities = app.filter.qualities.iter().copied().collect();
}

/// Keys for the release detail pane.
async fn handle_detail_key(
    app: &mut App,
    key: KeyEvent,
    download_tx: &mpsc::UnboundedSender<DownloadEvent>,
) {
    match key.code {
        KeyCode::Esc => app.screen = Screen::Browser,
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.screen = Screen::Help,
        KeyCode::Char('o') => app.detail_only_original = !app.detail_only_original,
        KeyCode::Char('d') => app.screen = Screen::Downloads,
        KeyCode::Up | KeyCode::Char('k') => {
            app.detail_selected = app.detail_selected.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.visible_releases().len().saturating_sub(1);
            app.detail_selected = (app.detail_selected + 1).min(max);
        }
        KeyCode::Char('m') => {
            if let Some(release) = selected_release(app) {
                if let Some(magnet) = &release.magnet {
                    copy_to_clipboard(magnet);
                    app.status = "magnet copied to the clipboard".to_string();
                } else {
                    app.status = "this release has no magnet link".to_string();
                }
            }
        }
        KeyCode::Char('t') => {
            start_download(app, download_tx);
        }
        KeyCode::Enter => {
            start_download(app, download_tx);
        }
        KeyCode::Tab => app.screen = Screen::Downloads,
        _ => {}
    }
}

/// Keys for the download queue.
fn handle_downloads_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => app.screen = Screen::Browser,
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.screen = Screen::Help,
        KeyCode::Up | KeyCode::Char('k') => {
            app.download_selected = app.download_selected.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.downloads.len().saturating_sub(1);
            app.download_selected = (app.download_selected + 1).min(max);
        }
        KeyCode::Tab => app.screen = Screen::Browser,
        _ => {}
    }
}

/// The release the cursor is on.
fn selected_release(app: &App) -> Option<&crate::model::Release> {
    let thread = app.detail_thread()?;
    let visible = app.visible_releases();
    let index = visible.get(app.detail_selected).copied()?;
    thread.releases.get(index)
}

/// Start a download for the selected release.
fn start_download(
    app: &mut App,
    download_tx: &mpsc::UnboundedSender<DownloadEvent>,
) {
    let Some(release) = selected_release(app) else {
        return;
    };

    let (kind_str, url) = release.preferred_link().unwrap_or(("direct", String::new()));
    let kind = match kind_str {
        "magnet" => DownloadKind::Magnet,
        "torrent" => DownloadKind::Torrent,
        _ => DownloadKind::Direct,
    };

    if url.is_empty() {
        app.status = "this release has no download link".to_string();
        return;
    }

    let id = app.downloads.len() as u64 + 1;
    let name = release.name.clone();
    let task = DownloadTask {
        id,
        name: name.clone(),
        kind,
        url: url.clone(),
        status: DownloadStatus::Queued,
    };
    app.queue_download(task);
    app.status = format!("queued: {name}");
    app.screen = Screen::Downloads;

    let client = app.client.clone();
    let target_dir = app.config.download_dir.clone();
    let tx = download_tx.clone();
    tokio::spawn(async move {
        let result = match kind {
            DownloadKind::Direct => {
                download::download_direct(&client, &url, &target_dir, &name, id, &tx).await
            }
            _ => {
                download::hand_off(kind, &url, &client, &target_dir, &name, id, &tx).await
            }
        };
        if let Err(e) = result {
            let _ = tx.send(DownloadEvent::Failed {
                id,
                error: e.to_string(),
            });
        }
    });
}

/// Copy the resolved languages from the cache onto the threads.
fn apply_ai_to_threads(app: &mut App) {
    for thread in &mut app.threads {
        let key = cache_key(&thread.movie, thread.year);
        if let Some(entry) = app.cache.get(&key) {
            thread.original = Some(crate::model::OriginalLang {
                lang: entry.lang,
                confidence: entry.confidence,
                source: entry.source,
            });
        }
    }
}

/// Truncate a string for the text output.
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(width - 1).collect();
        out.push('…');
        out
    }
}

/// Unused import guard.
#[allow(dead_code)]
fn _unused(_: HashMap<String, CacheEntry>, _: Lang, _: &Thread) {}
