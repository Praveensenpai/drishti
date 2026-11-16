//! Downloading.
//!
//! Direct links are streamed to disk with progress reporting. Magnets and
//! torrent files are handed to an external torrent client when one is
//! installed, and copied to the terminal clipboard otherwise.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::site::Client;

/// How a release should be fetched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadKind {
    /// A direct HTTP file.
    Direct,
    /// A magnet link.
    Magnet,
    /// A .torrent attachment.
    Torrent,
}


/// The state of a download.
#[derive(Clone, Debug)]
pub enum DownloadStatus {
    Queued,
    Running {
        downloaded: u64,
        total: Option<u64>,
    },
    Finished {
        path: PathBuf,
        bytes: u64,
    },
    /// Handed over to a torrent client.
    HandedOff {
        client: String,
    },
    Failed {
        error: String,
    },
}

impl DownloadTask {
    /// A short label for the download kind.
    pub fn kind_label(&self) -> String {
        match self.kind {
            DownloadKind::Magnet => "magnet".to_string(),
            DownloadKind::Direct => "direct".to_string(),
            DownloadKind::Torrent => "torrent".to_string(),
        }
    }
}

impl DownloadStatus {
    /// A short one-line description.
    pub fn summary(&self) -> String {
        match self {
            DownloadStatus::Queued => "queued".to_string(),
            DownloadStatus::Running { downloaded, total } => match total {
                Some(total) if *total > 0 => {
                    format!("{}/{}", human(*downloaded), human(*total))
                }
                _ => format!("{}/?", human(*downloaded)),
            },
            DownloadStatus::Finished { path, bytes } => {
                format!("{} ({})", path.display(), human(*bytes))
            }
            DownloadStatus::HandedOff { client } => format!("sent to {client}"),
            DownloadStatus::Failed { error } => error.clone(),
        }
    }

    /// Progress as a fraction, when it is known.
    pub fn fraction(&self) -> Option<f64> {
        match self {
            DownloadStatus::Running {
                downloaded,
                total: Some(total),
            } if *total > 0 => Some((*downloaded as f64 / *total as f64).clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

/// A unit of work.
#[derive(Clone, Debug)]
pub struct DownloadTask {
    pub id: u64,
    pub name: String,
    pub kind: DownloadKind,
    pub url: String,
    pub status: DownloadStatus,
}

/// Progress update sent to the UI.
#[derive(Debug)]
pub enum DownloadEvent {
    Started { id: u64 },
    Progress { id: u64, downloaded: u64, total: Option<u64> },
    Finished { id: u64, path: PathBuf, bytes: u64 },
    HandedOff { id: u64, client: String },
    Failed { id: u64, error: String },
}

/// Torrent clients we know how to drive.
const TORRENT_CLIENTS: &[(&str, &[&str])] = &[
    ("aria2c", &["--enable-dht=true", "--bt-enable-lpd=true", "--follow-torrent=mem"]),
    ("transmission-cli", &[]),
    ("qbittorrent-nox", &[]),
    ("deluge-console", &[]),
    ("ktorrent", &[]),
];

/// Find an installed torrent client.
pub fn find_torrent_client(preferred: &str) -> Option<(&'static str, Vec<String>)> {
    if !preferred.trim().is_empty() {
        let name = preferred.trim();
        if let Some((client, args)) = TORRENT_CLIENTS.iter().find(|(n, _)| *n == name)
            && which(name).is_some() {
                return Some((*client, args.iter().map(|a| a.to_string()).collect()));
            }
    }
    for (name, args) in TORRENT_CLIENTS {
        if which(name).is_some() {
            return Some((*name, args.iter().map(|a| a.to_string()).collect()));
        }
    }
    None
}

/// Locate an executable on `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Download a direct link to disk.
pub async fn download_direct(
    client: &Client,
    url: &str,
    target_dir: &Path,
    fallback_name: &str,
    id: u64,
    events: &tokio::sync::mpsc::UnboundedSender<DownloadEvent>,
) -> Result<()> {
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("creating {}", target_dir.display()))?;

    let resp = client
        .http
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("HTTP {status}");
    }
    let total = resp.content_length();
    let final_url = resp.url().to_string();

    let file_name = file_name_from(&resp, &final_url, fallback_name);
    let path = target_dir.join(&file_name);

    let mut file = tokio::fs::File::create(&path)
        .await
        .with_context(|| format!("creating {}", path.display()))?;

    let _ = events.send(DownloadEvent::Started { id });
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("reading chunk")?;
        file.write_all(&chunk).await.context("writing chunk")?;
        downloaded += chunk.len() as u64;
        let _ = events.send(DownloadEvent::Progress {
            id,
            downloaded,
            total,
        });
    }
    file.flush().await.context("flushing file")?;

    let _ = events.send(DownloadEvent::Finished {
        id,
        path,
        bytes: downloaded,
    });
    Ok(())
}

/// Work out a file name for a download.
fn file_name_from(resp: &reqwest::Response, final_url: &str, fallback: &str) -> String {
    if let Some(cd) = resp.headers().get(reqwest::header::CONTENT_DISPOSITION)
        && let Ok(value) = cd.to_str()
            && let Some(name) = parse_content_disposition(value) {
                return sanitise(&name);
            }
    if let Some(name) = final_url
        .split('?')
        .next()
        .and_then(|u| u.rsplit('/').next())
        .filter(|s| !s.is_empty())
        && name.contains('.') {
            return sanitise(name);
        }
    let mut base = sanitise(fallback);
    if base.is_empty() {
        base = "download".to_string();
    }
    base
}

/// Pull a file name out of a `Content-Disposition` header.
fn parse_content_disposition(value: &str) -> Option<String> {
    let mut filename = None;
    let mut charset_utf8 = false;
    for part in value.split(';') {
        let part = part.trim();
        let Some((key, val)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let val = val.trim().trim_matches('"');
        match key.as_str() {
            "filename" => filename = Some(val.to_string()),
            "filename*" => {
                if let Some(decoded) = decode_ext_value(val) {
                    filename = Some(decoded);
                }
            }
            "charset" if val.eq_ignore_ascii_case("utf-8") => charset_utf8 = true,
            _ => {}
        }
    }
    let _ = charset_utf8;
    filename
}

/// Decode an RFC 5987 extended parameter value.
fn decode_ext_value(value: &str) -> Option<String> {
    // charset'language'percent-encoded-value
    let mut parts = value.splitn(3, '\'');
    let _charset = parts.next()?;
    let _language = parts.next()?;
    let encoded = parts.next()?;
    let bytes = percent_encoding::percent_decode_str(encoded).collect::<Vec<u8>>();
    String::from_utf8(bytes).ok()
}

/// Make a string safe to use as a file name.
fn sanitise(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if (c as u32) < 0x20 => ' ',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed
    }
}

/// Hand a magnet or torrent file to an external client.
pub async fn hand_off(
    kind: DownloadKind,
    url: &str,
    client: &Client,
    target_dir: &Path,
    fallback_name: &str,
    id: u64,
    events: &tokio::sync::mpsc::UnboundedSender<DownloadEvent>,
) -> Result<()> {
    let _ = events.send(DownloadEvent::Started { id });

    let mut target_url = url.to_string();
    if kind == DownloadKind::Torrent {
        // Download the torrent file first, then hand the local path over.
        std::fs::create_dir_all(target_dir)?;
        let resp = client.http.get(url).send().await.context("fetching torrent")?;
        let status = resp.status();
        if !status.is_success() {
            bail!("HTTP {status}");
        }
        let final_url = resp.url().to_string();
        let name = file_name_from(&resp, &final_url, fallback_name);
        let path = target_dir.join(name);
        let bytes = resp.bytes().await.context("reading torrent body")?;
        tokio::fs::write(&path, &bytes).await.context("writing torrent")?;
        let _ = events.send(DownloadEvent::Finished {
            id,
            path: path.clone(),
            bytes: bytes.len() as u64,
        });
        target_url = path.to_string_lossy().to_string();
    }

    let Some((name, args)) = find_torrent_client("") else {
        bail!("no torrent client found; magnet: {url}");
    };
    let target = target_url;
    let mut command = tokio::process::Command::new(name);
    command.args(args);
    command.arg(target);
    command
        .spawn()
        .with_context(|| format!("spawning {name}"))?;
    let _ = events.send(DownloadEvent::HandedOff {
        id,
        client: name.to_string(),
    });
    Ok(())
}

/// Format a byte count.
pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_disposition() {
        assert_eq!(
            parse_content_disposition(r#"attachment; filename="movie.mkv""#).as_deref(),
            Some("movie.mkv")
        );
        assert_eq!(
            parse_content_disposition(
                "attachment; filename*=UTF-8''movie%20name.mkv"
            )
            .as_deref(),
            Some("movie name.mkv")
        );
    }

    #[test]
    fn sanitises_names() {
        assert_eq!(sanitise("a/b:c.mkv"), "a-b-c.mkv");
        assert_eq!(sanitise("   "), "download");
    }

    #[test]
    fn humanises_sizes() {
        assert_eq!(human(512), "512B");
        assert_eq!(human(1_500_000), "1.5MB");
    }
}
