//! 1TamilMV browser — a TUI for finding WEB-DL releases and filtering them
//! by the language the film was originally made in.

mod ai;
mod ansi;
mod app;
mod config;
mod discover;
mod forums;
mod lang;
mod model;
mod quality;
mod search; // fuzzy search engine
mod site;
mod ui;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::ai::{CacheEntry, Gemini, LangCache, cache_key, effective_original, plan_batches};
use crate::app::{App, Msg, ScanProgress, Screen};
use crate::config::Config;
use crate::forums::{Forum, all_web_hd};
use crate::model::{OriginalLang, Thread};
use crate::quality::{Quality, analyse, human_size};
use crate::site::PageLoad;

#[derive(Parser)]
#[command(
    name = "drushti",
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
    /// Resolve the original production language of the threads on forum pages.
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
    /// Interactive setup: ask for all settings and save them.
    Config,
    /// Write a default configuration file.
    Init,
    /// Update drushti to the latest release (downloads from GitHub; works anywhere).
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
    if let Some(key) = cli.gemini_key.clone() {
        config.gemini_api_key = key;
    }

    let from_page = cli.from.unwrap_or(1);
    let to_page = cli.to;
    let no_cache = cli.no_cache;
    if no_cache {
        config.cache_ttl_minutes = 0;
    }

    // First run: no config file and launching the TUI interactively. Ask the
    // user for every setting once, then save it so this never prompts again.
    if cli.command.is_none() && !config_path.exists() && Config::interactive() {
        Config::configure(&mut config)?;
        config.save(&config_path)?;
        println!("configuration saved to {}", config_path.display());
    }

    maybe_rediscover(&mut config, &config_path).await;

    match cli.command {
        Some(Command::Probe) => probe(&config).await,
        Some(Command::Resolve { forum, page }) => resolve(&config, &forum, page, to_page).await,
        Some(Command::ConfigPath) => {
            println!("{}", config_path.display());
            Ok(())
        }
        Some(Command::Config) => {
            Config::configure(&mut config)?;
            config.save(&config_path)?;
            println!("configuration saved to {}", config_path.display());
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
    let spinner = ansi::Spinner::start("checking Gemini key");
    let models = gemini.probe().await?;
    spinner.finish(&format!("key valid, {} models available", models.len()));
    for name in &config.gemini_models {
        let mark = if models.iter().any(|m| m == name) {
            ansi::success("available")
        } else {
            ansi::muted("not listed")
        };
        println!("  {} {}", ansi::accent(name), mark);
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
    let spinner = ansi::Spinner::start(format!("loading {} page {page}", forum.title));
    let result = client.fetch_page(forum, page).await?;
    spinner.finish(&format!("page {page} has {} threads", result.threads.len()));
    let cache = LangCache::load(&ai::default_cache_path());
    let ttl = config.ai_cache_hours.saturating_mul(3600);
    let original_of = |thread: &Thread| {
        effective_original(thread, &cache, &cache_key(&thread.movie, thread.year), ttl)
    };

    if json {
        let rows: Vec<serde_json::Value> = result
            .threads
            .iter()
            .map(|thread| {
                let info = analyse(&thread.title);
                let original = original_of(thread);
                serde_json::json!({
                    "id": thread.id,
                    "movie": thread.movie,
                    "year": thread.year,
                    "title": thread.title,
                    "forum": thread.forum.title,
                    "page": result.page,
                    "total_pages": result.total_pages,
                    "from_cache": result.from_cache,
                    "quality": info.quality.label(),
                    "source": info.source.label(),
                    "resolution": info.resolution.label(),
                    "original_language": original.lang.code(),
                    "original_label": original.lang.label(),
                    "original_source": original.source.label(),
                    "original_confidence": original.confidence,
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

    let heading = format!(
        "{}  page {}/{}  ({} threads)",
        forum.title,
        result.page,
        result.total_pages,
        result.threads.len()
    );
    let cached = if result.from_cache {
        ansi::muted("  cached")
    } else {
        String::new()
    };
    println!("{}{}", ansi::accent(&heading), cached);
    for thread in &result.threads {
        let info = analyse(&thread.title);
        let original = original_of(thread);
        let quality = format!("{:<8}", info.quality.label());
        let quality = if info.quality == Quality::WebDl {
            ansi::good(&quality)
        } else {
            ansi::warn(&quality)
        };
        let lang = ansi::accent(&format!("{:<12}", original.lang.label()));
        println!(
            "  {:<44} {} {} {}",
            truncate(&thread.title, 44),
            quality,
            lang,
            ansi::muted(&thread.url)
        );
        if with_releases {
            for release in &thread.releases {
                let rel_quality = format!("{:<8}", release.info.quality.label());
                let rel_quality = if release.info.quality == Quality::WebDl {
                    ansi::good(&rel_quality)
                } else {
                    ansi::warn(&rel_quality)
                };
                println!(
                    "      {} {} {} {} {}",
                    rel_quality,
                    ansi::muted(&format!("{:<7}", release.info.resolution.label())),
                    ansi::muted(&format!("{:<22}", release.langs.compact())),
                    ansi::muted(&format!("{:<8}", human_size(release.info.size_bytes))),
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

/// Resolve the original production language of the threads on one or more
/// forum pages. The model is called only for films that are not already in the
/// one-day cache.
async fn resolve(
    config: &Config,
    forum_name: &str,
    from_page: u32,
    to_page: Option<u32>,
) -> Result<()> {
    let forum = resolve_forum(forum_name)?;
    let client = site::Client::new(
        &config.base_url,
        site::default_cache_dir(),
        std::time::Duration::from_secs(config.cache_ttl_minutes as u64 * 60),
    )?;

    let key = config.api_key();
    if key.trim().is_empty() {
        anyhow::bail!("no API key set. Use --gemini-key, GEMINI_API_KEY, or the config file.");
    }
    let gemini = Gemini::new(&key, config.gemini_models.clone())?;
    let mut cache = LangCache::load(&ai::default_cache_path());
    let ttl = config.ai_cache_hours.saturating_mul(3600);
    let batch_size = config.ai_batch_size.max(1);
    let batch_chars = config.ai_batch_chars.max(64);
    let pause = config.ai_batch_pause_ms;

    let mut page = from_page.max(1);
    let mut limit = to_page;
    loop {
        let spinner = ansi::Spinner::start(format!("{} page {page}", forum.title));
        let load = client.fetch_page(forum, page).await?;
        spinner.finish(&format!("page {page} loaded"));
        let total = load.total_pages.max(page);
        let last = limit.unwrap_or(total).min(total).max(page);
        limit = Some(last);
        let mut threads = load.threads;

        let mut keys: Vec<String> = Vec::new();
        for thread in &threads {
            let movie_key = cache_key(&thread.movie, thread.year);
            if cache.get_with_ttl(&movie_key, ttl).is_none() && !keys.contains(&movie_key) {
                keys.push(movie_key);
            }
        }

        println!(
            "{}  {}  {} threads, {} needing model resolution",
            ansi::accent(forum.title),
            ansi::muted(&format!("page {page}/{total}")),
            threads.len(),
            keys.len()
        );

        let batches = plan_batches(&keys, batch_size, batch_chars);
        let batch_total = batches.len();
        for (index, batch) in batches.iter().enumerate() {
            if index > 0 && pause > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(pause)).await;
            }
            let spinner =
                ansi::Spinner::start(format!("resolving batch {}/{}", index + 1, batch_total));
            let map = gemini.resolve_batch(batch).await?;
            spinner.finish(&format!("batch {}/{} answered", index + 1, batch_total));
            for (movie_key, entry) in &map {
                cache.put(movie_key.clone(), entry.clone());
            }
            apply_map(&mut threads, &map);
            cache.save(&ai::default_cache_path())?;
            println!(
                "  {} {} movies cached",
                ansi::success("resolved"),
                batch.len()
            );
        }

        for thread in &mut threads {
            let movie_key = cache_key(&thread.movie, thread.year);
            let original = match cache.get_with_ttl(&movie_key, ttl) {
                Some(entry) => OriginalLang {
                    lang: entry.lang,
                    confidence: entry.confidence,
                    source: entry.source,
                },
                None => ai::heuristic_original(thread),
            };
            thread.original = Some(original);
        }

        for thread in &threads {
            let Some(original) = thread.original else {
                continue;
            };
            let lang = format!("{:<10}", original.lang.label());
            let lang = if original.confidence >= 0.8 {
                ansi::success(&lang)
            } else if original.confidence >= 0.5 {
                ansi::warn(&lang)
            } else {
                ansi::error(&lang)
            };
            println!(
                "  {:<44} {} {} confidence {:.2}",
                truncate(&thread.title, 44),
                lang,
                ansi::muted(original.source.label()),
                original.confidence
            );
        }

        if page >= last {
            break;
        }
        page += 1;
    }
    Ok(())
}

/// Copy model answers onto a thread list.
fn apply_map(threads: &mut [Thread], map: &std::collections::HashMap<String, CacheEntry>) {
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

/// Update drushti by downloading the latest prebuilt release binary from
/// GitHub. Works from any directory — no local source checkout needed.
async fn update() -> Result<()> {
    const URL: &str =
        "https://github.com/Praveensenpai/drushti/releases/latest/download/drushti-linux-x86_64";
    println!("{}", ansi::accent("downloading latest release"));
    let spinner = ansi::Spinner::start("fetching release from GitHub");
    let client = reqwest::Client::builder()
        .user_agent("drushti-updater")
        .build()
        .context("building HTTP client")?;
    let resp = client
        .get(URL)
        .send()
        .await
        .context("downloading release")?;
    if !resp.status().is_success() {
        drop(spinner);
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await.context("reading release bytes")?;
    spinner.finish(&format!("downloaded {} bytes", bytes.len()));
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
    println!(
        "{} {}",
        ansi::success(ansi::check()),
        ansi::accent("drushti updated to the latest release")
    );
    Ok(())
}

/// The directory used for cache / update bookkeeping.
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("drushti")
}

/// Verify the configured board domain and persist a replacement if needed.
///
/// Discovery is best effort: a failure never prevents the app from starting.
async fn maybe_rediscover(config: &mut Config, config_path: &std::path::Path) {
    if !config.auto_discover_domain {
        return;
    }
    let discovery = match discover::Discovery::new(&cache_dir()) {
        Ok(discovery) => discovery,
        Err(error) => {
            eprintln!("{} {}", ansi::warn("warning:"), error);
            return;
        }
    };
    match discovery
        .resolve(
            &config.base_url,
            &config.domain_search_query,
            &config.domain_search_endpoint,
        )
        .await
    {
        Ok(Some(base)) => {
            config.base_url = base.clone();
            // Save only the discovered domain. CLI overrides such as a one-off
            // --gemini-key must not be silently written to config.toml.
            let mut stored = Config::load(config_path).unwrap_or_default();
            stored.base_url = base.clone();
            if let Err(error) = stored.save(config_path) {
                eprintln!("{} {}", ansi::warn("warning:"), error);
            }
            eprintln!(
                "{} {}",
                ansi::success(ansi::check()),
                ansi::accent(&format!("board domain updated to {base}"))
            );
        }
        Ok(None) => {}
        Err(error) => eprintln!("{} {}", ansi::warn("warning:"), error),
    }
}

/// Spawn `drushti update` in the background once a day (best effort).
fn maybe_auto_update(config: &Config) {
    if !config.auto_update {
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
    app.page = page_from.max(1);
    app.page_from = page_from.max(1);
    app.page_to = page_to;

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();

    // The language picker is shown first; the first page is loaded when the
    // user picks a language. Nothing is fetched until then.

    let mut terminal = ratatui::init();
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(150));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        tokio::select! {
            Some(Ok(event)) = events.next() => {
                if let Event::Key(key) = event
                    && key.kind == KeyEventKind::Press {
                        handle_key(&mut app, key, &tx).await;
                    }
            }
            Some(msg) = rx.recv() => {
                handle_message(&mut app, msg, &tx).await;
            }
            _ = ticker.tick() => {
                app.tick = app.tick.wrapping_add(1);
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

/// Start a background scan of the selected forum, from the first page to the
/// last page (or the configured limit). Every page is fully enriched before it
/// is delivered, so the table never changes its release information after the
/// fact.
fn start_scan(app: &mut App, tx: &mpsc::UnboundedSender<Msg>) {
    let forum = app.forum();
    let from = app.page_from.max(1);
    let to = app.page_to.map(|last| last.max(from));
    app.page_from = from;
    app.page_to = to;
    app.scan_id = app.scan_id.wrapping_add(1);
    let scan_id = app.scan_id;
    app.reset_scan();
    app.page = from;
    app.show_page(from);
    app.scan = ScanProgress {
        active: true,
        pages_total: to.map(|last| last.saturating_sub(from) + 1).unwrap_or(0),
        ..ScanProgress::default()
    };
    app.status = format!("scanning {}...", forum.title);

    let client = app.client.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut page = from;
        let mut limit = to;
        loop {
            match client.fetch_page(forum, page).await {
                Ok(load) => {
                    let total = load.total_pages.max(page);
                    let last = limit.unwrap_or(total).min(total).max(page);
                    limit = Some(last);
                    let _ = tx.send(Msg::PageReady {
                        forum_id: forum.id,
                        page,
                        total_pages: load.total_pages,
                        threads: load.threads,
                        from_cache: load.from_cache,
                        scan_id,
                    });
                    if page >= last {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send(Msg::PageFailed {
                        forum_id: forum.id,
                        page,
                        error: error.to_string(),
                        scan_id,
                    });
                    match limit {
                        Some(last) if page < last => {}
                        _ => break,
                    }
                }
            }
            page += 1;
        }
        let _ = tx.send(Msg::ScanFinished {
            forum_id: forum.id,
            scan_id,
        });
    });
}

/// Fetch one page in the background without touching the scan.
fn request_page(app: &mut App, page: u32, tx: &mpsc::UnboundedSender<Msg>) {
    if page == 0 || page > app.total_pages.max(1) {
        return;
    }
    if app.pages.contains_key(&page) {
        app.show_page(page);
        return;
    }
    app.show_page(page);
    app.pages_in_flight += 1;
    let client = app.client.clone();
    let forum = app.forum();
    let scan_id = app.scan_id;
    let tx = tx.clone();
    tokio::spawn(async move {
        match client.fetch_page(forum, page).await {
            Ok(load) => {
                let _ = tx.send(Msg::PageReady {
                    forum_id: forum.id,
                    page,
                    total_pages: load.total_pages,
                    threads: load.threads,
                    from_cache: load.from_cache,
                    scan_id,
                });
            }
            Err(error) => {
                let _ = tx.send(Msg::PageFailed {
                    forum_id: forum.id,
                    page,
                    error: error.to_string(),
                    scan_id,
                });
            }
        }
    });
}

/// Handle a message from a background task.
async fn handle_message(app: &mut App, msg: Msg, tx: &mpsc::UnboundedSender<Msg>) {
    match msg {
        Msg::PageReady {
            forum_id,
            page,
            total_pages,
            threads,
            from_cache,
            scan_id,
        } => {
            if scan_id != app.scan_id || forum_id != app.forum().id {
                return;
            }
            app.pages_in_flight = app.pages_in_flight.saturating_sub(1);
            app.queue_ai_threads(&threads);
            let count = threads.len() as u32;
            app.store_page(PageLoad {
                threads,
                page,
                total_pages,
                from_cache,
            });
            if app.scan.active {
                let last = app
                    .page_to
                    .unwrap_or(total_pages)
                    .min(total_pages)
                    .max(app.page_from);
                app.scan.pages_total = last.saturating_sub(app.page_from) + 1;
                if page >= app.page_from && page <= last && app.scan_done.insert(page) {
                    app.scan.pages_done += 1;
                    app.scan.threads_done += count;
                }
            } else {
                dispatch_ai(app, tx);
            }
        }
        Msg::PageFailed {
            forum_id,
            page,
            error,
            scan_id,
        } => {
            if scan_id != app.scan_id || forum_id != app.forum().id {
                return;
            }
            app.pages_in_flight = app.pages_in_flight.saturating_sub(1);
            if app.scan.active {
                let last = app.page_to.unwrap_or(page).max(app.page_from);
                if page >= app.page_from && page <= last && app.scan_done.insert(page) {
                    app.scan.pages_done += 1;
                }
            }
            if page == app.page {
                app.loading = false;
                if !app.pages.contains_key(&page) {
                    app.error = Some(error);
                }
            }
        }
        Msg::ScanFinished { forum_id, scan_id } => {
            if scan_id != app.scan_id || forum_id != app.forum().id {
                return;
            }
            app.scan.active = false;
            app.loading = !app.pages.contains_key(&app.page) && app.pages_in_flight > 0;
            if app.pages.contains_key(&app.page) {
                app.loading = false;
            }
            app.status = if app.ai_queue.is_empty() {
                format!(
                    "scan complete · {} pages · {} movies cached",
                    app.scan.pages_done,
                    app.cache.len()
                )
            } else {
                format!("scan complete · resolving {} movies", app.ai_queue.len())
            };
            dispatch_ai(app, tx);
        }
        Msg::AiResolved(map) => {
            app.ai_in_flight = false;
            app.ai_failures = 0;
            let count = map.len();
            for (key, entry) in &map {
                app.cache.put(key.clone(), entry.clone());
            }
            let _ = app.cache.save(&app.cache_path);
            app.apply_ai(&map);
            // Drop the in-flight batch: every key in it was either answered or
            // is now cached as unknown, so none should be retried.
            app.ai_in_flight_keys.clear();
            app.status = format!("resolved {count} movies");
            dispatch_ai(app, tx);
        }
        Msg::AiFailed(error) => {
            app.ai_in_flight = false;
            app.ai_failures += 1;
            // Put the failed batch back at the front so it is retried on the
            // next dispatch instead of being lost until the app restarts.
            let failed = std::mem::take(&mut app.ai_in_flight_keys);
            if app.ai_failures < 3 {
                for key in failed.into_iter().rev() {
                    if !app.ai_queue.contains(&key) {
                        app.ai_queue.insert(0, key);
                    }
                }
            }
            app.status = if app.ai_failures >= 3 {
                "language lookup unavailable; will retry next launch".to_string()
            } else {
                format!("language lookup failed: {error}")
            };
            dispatch_ai(app, tx);
        }
        Msg::MagnetPosted { ok, message } => {
            if ok {
                app.status = message;
            } else {
                app.status = format!("⚠ {message}");
            }
        }
    }
}

/// Send the next queued batch to the model.
fn dispatch_ai(app: &mut App, tx: &mpsc::UnboundedSender<Msg>) {
    if app.scan.active || app.ai_in_flight || app.ai_queue.is_empty() || app.ai_failures >= 3 {
        return;
    }
    let Some(gemini) = app.gemini.clone() else {
        return;
    };
    let batches = plan_batches(
        &app.ai_queue,
        app.config.ai_batch_size.max(1),
        app.config.ai_batch_chars.max(64),
    );
    let Some(batch) = batches.into_iter().next() else {
        return;
    };
    app.ai_queue.drain(..batch.len());
    app.ai_in_flight = true;
    app.ai_in_flight_keys = batch.clone();
    app.ai_batches_done += 1;
    let pause = app.config.ai_batch_pause_ms;
    let wait = app.ai_batches_done > 1;
    let tx = tx.clone();
    tokio::spawn(async move {
        if wait && pause > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(pause)).await;
        }
        match gemini.resolve_batch(&batch).await {
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
async fn handle_key(app: &mut App, key: KeyEvent, tx: &mpsc::UnboundedSender<Msg>) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char('c') = key.code {
            app.should_quit = true
        }
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

    if app.search_open {
        handle_search_key(app, key);
        return;
    }

    match app.screen {
        Screen::Browser => handle_browser_key(app, key, tx).await,
        Screen::Detail => handle_detail_key(app, key, tx).await,
        Screen::Help => {}
        Screen::Language => handle_language_key(app, key, tx).await,
    }
}

/// Keys for the language picker.
async fn handle_language_key(app: &mut App, key: KeyEvent, tx: &mpsc::UnboundedSender<Msg>) {
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
            let page = app.page.max(1);
            app.page_from = page;
            start_scan(app, tx);
        }
        KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Browser,
        KeyCode::Char('?') => app.screen = Screen::Help,
        _ => {}
    }
}

/// Keys for the thread browser.
async fn handle_browser_key(app: &mut App, key: KeyEvent, tx: &mpsc::UnboundedSender<Msg>) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.screen = Screen::Help,
        KeyCode::Char('/') => {
            app.search_open = true;
            app.saved_query = app.search_draft.clone();
            app.search_draft.clear();
            app.search_results.clear();
            app.result_selected = 0;
            app.search_dirty = false;
        }
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
            app.status = "rescanning...".to_string();
            start_scan(app, tx);
        }
        KeyCode::Char('r') => {
            let page = app.page;
            request_page(app, page, tx);
        }
        KeyCode::Char('[') => {
            if !app.forums.is_empty() {
                app.forum_index = (app.forum_index + app.forums.len() - 1) % app.forums.len();
                app.page = 1;
                app.page_from = 1;
                app.page_to = None;
                start_scan(app, tx);
            }
        }
        KeyCode::Char(']') => {
            if !app.forums.is_empty() {
                app.forum_index = (app.forum_index + 1) % app.forums.len();
                app.page = 1;
                app.page_from = 1;
                app.page_to = None;
                start_scan(app, tx);
            }
        }
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
                let page = app.page - 1;
                request_page(app, page, tx);
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if app.page < app.total_pages.max(1) {
                let page = app.page + 1;
                request_page(app, page, tx);
            }
        }
        KeyCode::Char('p') => {
            if let Some(thread) = app.threads.get(app.selected) {
                match open_in_browser(&thread.url) {
                    Ok(()) => app.status = "opened the post in your browser".to_string(),
                    Err(e) => app.status = format!("could not open browser: {e}"),
                }
            }
        }
        KeyCode::Char('u') => {
            if let Some(url) = app.threads.get(app.selected).map(|t| t.url.clone()) {
                copy_post_link(app, &url);
            }
        }
        KeyCode::Enter if !app.threads.is_empty() => {
            app.detail_index = Some(app.selected);
            app.detail_selected = 0;
            app.screen = Screen::Detail;
            app.detail_only_original = false;
        }
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

/// Keys for the search prompt (fuzzy movie finder).
#[allow(unreachable_patterns)]
fn handle_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.search_open = false;
            app.search_draft = app.saved_query.clone();
        }
        KeyCode::Char('/') => {
            // Keep search open, allow nesting (unlikely but harmless).
        }
        KeyCode::Enter => {
            // Commit the search and jump to the selected result.
            app.rebuild_search_results();
            if !app.search_results.is_empty() {
                let idx = app
                    .result_selected
                    .min(app.search_results.len().saturating_sub(1));
                app.jump_to_result(idx);
            } else {
                app.search_open = false;
                app.search_draft.clear();
                app.search_results.clear();
                app.result_selected = 0;
            }
        }
        KeyCode::Backspace => {
            if !app.search_draft.is_empty() {
                app.search_draft.pop();
                app.set_query(app.search_draft.clone());
                app.rebuild_search_results();
            } else {
                app.search_results.clear();
                app.result_selected = 0;
            }
        }
        KeyCode::Char(c) => {
            // c is already a validated printable key from crossterm KeyEvent.
            // Accept any printable ASCII char (not whitespace control chars).
            if c.is_ascii() && !c.is_ascii_whitespace() {
                app.search_draft.push(c);
                app.set_query(app.search_draft.clone());
                app.rebuild_search_results();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !app.search_results.is_empty() {
                if app.result_selected > 0 {
                    app.result_selected -= 1;
                } else {
                    app.result_selected = app.search_results.len() - 1;
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !app.search_results.is_empty() && app.result_selected < app.search_results.len() - 1
            {
                app.result_selected += 1;
            } else if !app.search_results.is_empty() {
                app.result_selected = 0;
            }
        }
        _ => {}
    }
}

/// Keys for the release detail pane.
async fn handle_detail_key(app: &mut App, key: KeyEvent, tx: &mpsc::UnboundedSender<Msg>) {
    match key.code {
        KeyCode::Esc => app.screen = Screen::Browser,
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.screen = Screen::Help,
        KeyCode::Char('o') => app.detail_only_original = !app.detail_only_original,
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
                    app.status = format!("magnet copied · {}", release_label(release));
                } else {
                    app.status = "this release has no magnet link".to_string();
                }
            }
        }
        KeyCode::Char('M') => {
            if let Some(release) = selected_release(app) {
                if let Some(magnet) = release.magnet.clone() {
                    let url = app.config.magnet_post_url.trim().to_string();
                    if url.is_empty() {
                        app.status =
                            "no magnet_post_url configured (set in config.toml)".to_string();
                    } else {
                        let tx = tx.clone();
                        let label = release_label(release);
                        tokio::spawn(async move {
                            let client = reqwest::Client::new();
                            let result = client
                                .post(&url)
                                .json(&serde_json::json!({"magnet": magnet}))
                                .send()
                                .await;
                            let (ok, msg) = match result {
                                Ok(resp) if resp.status().is_success() => {
                                    (true, format!("magnet sent ({label})"))
                                }
                                Ok(resp) => (
                                    false,
                                    format!("post failed: HTTP {} ({label})", resp.status()),
                                ),
                                Err(e) => (false, format!("post failed: {e} ({label})")),
                            };
                            let _ = tx.send(Msg::MagnetPosted { ok, message: msg });
                        });
                        app.status = "sending magnet...".to_string();
                    }
                } else {
                    app.status = "this release has no magnet link".to_string();
                }
            }
        }
        KeyCode::Char('p') | KeyCode::Enter => {
            if let Some(thread) = app.detail_thread() {
                match open_in_browser(&thread.url) {
                    Ok(()) => app.status = "opened the post in your browser".to_string(),
                    Err(e) => app.status = format!("could not open browser: {e}"),
                }
            }
        }
        KeyCode::Char('u') => {
            if let Some(url) = app.detail_thread().map(|t| t.url.clone()) {
                copy_post_link(app, &url);
            }
        }
        _ => {}
    }
}

/// A short label for a release, e.g. `4K · 19.5GB`.
fn release_label(release: &crate::model::Release) -> String {
    format!(
        "{} · {}",
        release.info.resolution.label(),
        crate::quality::human_size(release.info.size_bytes)
    )
}

/// Copy a post URL to the clipboard (OSC 52) and report it.
/// Useful in remote/SSH sessions where `p` cannot open a browser.
fn copy_post_link(app: &mut App, url: &str) {
    copy_to_clipboard(url);
    app.status = "post link copied".to_string();
}

/// Open a URL in the system's default browser.
fn open_in_browser(url: &str) -> std::io::Result<()> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", ""])
    } else {
        ("xdg-open", &[])
    };
    let status = std::process::Command::new(program)
        .args(args)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "{program} exited with {status}"
        )))
    }
}

/// The release the cursor is on.
fn selected_release(app: &App) -> Option<&crate::model::Release> {
    let thread = app.detail_thread()?;
    let visible = app.visible_releases();
    let index = visible.get(app.detail_selected).copied()?;
    thread.releases.get(index)
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
