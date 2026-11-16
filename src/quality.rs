//! Release quality, source, resolution and size extraction.
//!
//! The site's "WEB-HD / iTunes-HD / BluRay" forums mix true WEB-DL rips with
//! theatrical prints, DVDRips and HDRips. Everything needed to separate them
//! is present in the release name, so classification is purely lexical.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The broad quality bucket of a release.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Quality {
    /// Digital storefront / streaming WEB-DL. The only bucket enabled by default.
    WebDl,
    /// Blu-ray derived (BDRip, BRRip, Remux).
    Bluray,
    /// WEBRip — a re-encode of a streaming source, not a WEB-DL.
    WebRip,
    /// TV capture (HDTV/SDTV).
    TvRip,
    /// DVD derived (DVDRip, DVD9/5, NTSC/PAL).
    DvdRip,
    /// HDRip / HQ rip.
    HdRip,
    /// Theatrical print or screener (PreDVD, DVD-Scr, Theater, EVENT).
    Theater,
    /// Cam / telecine / telecast.
    Cam,
    /// Unclassifiable.
    Unknown,
}

impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Quality::WebDl => "WEB-DL",
            Quality::Bluray => "BluRay",
            Quality::WebRip => "WEBRip",
            Quality::TvRip => "HDTV",
            Quality::DvdRip => "DVDRip",
            Quality::HdRip => "HDRip",
            Quality::Theater => "Theater",
            Quality::Cam => "Cam",
            Quality::Unknown => "Unknown",
        }
    }

    /// True for the buckets that are acceptable when the user wants WEB-DL only.
    pub fn is_web_dl(self) -> bool {
        matches!(self, Quality::WebDl)
    }
}

impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Where a WEB-DL was sourced from, when the name says so.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Source {
    /// "TRUE WEB-DL" — the untouched original digital file, best quality.
    TrueWebDl,
    Netflix,
    Amazon,
    Hotstar,
    ITunes,
    AppleTv,
    Zee5,
    SunNxt,
    Disney,
    Other,
    Unknown,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::TrueWebDl => "TRUE",
            Source::Netflix => "NF",
            Source::Amazon => "AMZN",
            Source::Hotstar => "HOTSTAR",
            Source::ITunes => "iTunes",
            Source::AppleTv => "AppleTV",
            Source::Zee5 => "ZEE5",
            Source::SunNxt => "SUN NXT",
            Source::Disney => "Disney",
            Source::Other => "WEB",
            Source::Unknown => "-",
        }
    }
}

/// Vertical resolution.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Resolution {
    Uhd,
    Fhd,
    Hd,
    Sd,
    Unknown,
}

impl Resolution {
    pub fn label(self) -> &'static str {
        match self {
            Resolution::Uhd => "4K",
            Resolution::Fhd => "1080p",
            Resolution::Hd => "720p",
            Resolution::Sd => "480p",
            Resolution::Unknown => "-",
        }
    }
}

/// Everything we can tell about a release name.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub quality: Quality,
    pub source: Source,
    pub resolution: Resolution,
    pub size_bytes: Option<u64>,
    /// True when the name contains an explicit theatre/pre-DVD marker.
    pub theatrical: bool,
    /// True when the name claims an untouched original file.
    pub untouched: bool,
}

impl Default for ReleaseInfo {
    fn default() -> Self {
        Self {
            quality: Quality::Unknown,
            source: Source::Unknown,
            resolution: Resolution::Unknown,
            size_bytes: None,
            theatrical: false,
            untouched: false,
        }
    }
}

/// Words that always disqualify a release from being a WEB-DL.
const THEATER_WORDS: &[&str] = &[
    "theater",
    "theatre",
    "theatrical",
    "screening",
    "prescreened",
    "pre-dvd",
    "predvd",
    "dvd-scr",
    "dvdscr",
    "dvdscreener",
    "event",
    "campus",
    "unrated theatrical",
];

/// Quality markers, most specific first. Each entry is (needle, quality).
const QUALITY_MARKERS: &[(&str, Quality)] = &[
    ("camrip", Quality::Cam),
    ("hdcam", Quality::Cam),
    ("telecast", Quality::Cam),
    ("hdtc", Quality::Cam),
    ("hdts", Quality::Cam),
    ("predvd", Quality::Theater),
    ("dvd-scr", Quality::Theater),
    ("dvdscr", Quality::Theater),
    ("theatrical", Quality::Theater),
    ("theatre", Quality::Theater),
    ("theater", Quality::Theater),
    ("screening", Quality::Theater),
    ("blu-ray", Quality::Bluray),
    ("bluray", Quality::Bluray),
    ("bdrip", Quality::Bluray),
    ("brrip", Quality::Bluray),
    ("bdremux", Quality::Bluray),
    ("remux", Quality::Bluray),
    ("webrip", Quality::WebRip),
    ("web-rip", Quality::WebRip),
    ("hmax", Quality::WebRip),
    ("hdtv", Quality::TvRip),
    ("sdtv", Quality::TvRip),
    ("tvrip", Quality::TvRip),
    ("dvdrip", Quality::DvdRip),
    ("dvd9", Quality::DvdRip),
    ("dvd5", Quality::DvdRip),
    ("ntsc", Quality::DvdRip),
    ("hddvd", Quality::DvdRip),
    ("hdrip", Quality::HdRip),
    ("hq hq", Quality::HdRip),
    ("hq hdrip", Quality::HdRip),
    ("hq preview", Quality::HdRip),
    ("true web-dl", Quality::WebDl),
    ("true web dl", Quality::WebDl),
    ("web-dl", Quality::WebDl),
    ("web dl", Quality::WebDl),
    ("webdl", Quality::WebDl),
    ("web-hd", Quality::WebDl),
    ("itunes-hd", Quality::WebDl),
    ("itunes hd", Quality::WebDl),
];

/// Source markers.
const SOURCE_MARKERS: &[(&str, Source)] = &[
    ("true web-dl", Source::TrueWebDl),
    ("true web dl", Source::TrueWebDl),
    ("netflix", Source::Netflix),
    ("amazon", Source::Amazon),
    ("amzn", Source::Amazon),
    ("prime video", Source::Amazon),
    ("hotstar", Source::Hotstar),
    ("itunes", Source::ITunes),
    ("apple tv", Source::AppleTv),
    ("appletv", Source::AppleTv),
    ("zee5", Source::Zee5),
    ("sun nxt", Source::SunNxt),
    ("sunnxt", Source::SunNxt),
    ("hotstar", Source::Hotstar),
    ("disney", Source::Disney),
];

/// Classify a release name.
pub fn analyse(name: &str) -> ReleaseInfo {
    let lower = name.to_ascii_lowercase();
    let mut info = ReleaseInfo::default();

    for word in THEATER_WORDS {
        if contains_token(&lower, word) {
            info.theatrical = true;
            break;
        }
    }
    if contains_token(&lower, "untouched") {
        info.untouched = true;
    }

    // WEB-DL style markers win over generic ones, but a theatre marker always wins.
    for (needle, quality) in QUALITY_MARKERS {
        if contains_token(&lower, needle) {
            info.quality = *quality;
            break;
        }
    }
    if info.theatrical {
        info.quality = match info.quality {
            Quality::Cam => Quality::Cam,
            _ => Quality::Theater,
        };
    }

    // A storefront name only means something for a digital release.
    if info.quality == Quality::WebDl {
        for (needle, source) in SOURCE_MARKERS {
            if contains_token(&lower, needle) {
                info.source = *source;
                break;
            }
        }
        if info.source == Source::Unknown {
            info.source = Source::Other;
        }
    }
    // A true WEB-DL marker outranks a storefront name in the same title.
    if contains_token(&lower, "true web-dl") || contains_token(&lower, "true web dl") {
        info.source = Source::TrueWebDl;
    }

    info.resolution = detect_resolution(&lower);
    info.size_bytes = detect_size(&lower);
    info
}

/// Detect the vertical resolution.
fn detect_resolution(lower: &str) -> Resolution {
    if contains_token(lower, "2160p") || contains_token(lower, "4k") || contains_token(lower, "uhd")
    {
        Resolution::Uhd
    } else if contains_token(lower, "1080p") || contains_token(lower, "fullhd") {
        Resolution::Fhd
    } else if contains_token(lower, "720p") || contains_token(lower, "hd") {
        Resolution::Hd
    } else if contains_token(lower, "480p") || contains_token(lower, "sd") {
        Resolution::Sd
    } else {
        Resolution::Unknown
    }
}

/// Detect the file size, e.g. `19.5GB`, `950MB`.
fn detect_size(lower: &str) -> Option<u64> {
    let mut best: Option<u64> = None;
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let num: String = lower[start..i].to_string();
            let mut j = i;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            let unit_start = j;
            while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                j += 1;
            }
            let unit = &lower[unit_start..j];
            let multiplier = match unit {
                "gb" | "gib" => Some(1_000_000_000u64),
                "mb" | "mib" => Some(1_000_000u64),
                "tb" | "tib" => Some(1_000_000_000_000u64),
                _ => None,
            };
            if let Some(mult) = multiplier
                && let Ok(value) = num.parse::<f64>()
            {
                let bytes = (value * mult as f64) as u64;
                best = Some(best.map_or(bytes, |b: u64| b.max(bytes)));
            }
            continue;
        }
        i += 1;
    }
    best
}

/// Case-insensitive containment that respects word boundaries.
pub fn contains_token(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_word_char(haystack[..abs].chars().next_back().unwrap());
        let after = abs + needle.len();
        let after_ok =
            after >= haystack.len() || !is_word_char(haystack[after..].chars().next().unwrap());
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
        if start >= haystack.len() {
            break;
        }
    }
    false
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Format a byte count for the table column.
pub fn human_size(bytes: Option<u64>) -> String {
    match bytes {
        None => "-".to_string(),
        Some(b) if b >= 1_000_000_000 => format!("{:.1}GB", b as f64 / 1_000_000_000.0),
        Some(b) if b >= 1_000_000 => format!("{:.0}MB", b as f64 / 1_000_000.0),
        Some(b) => format!("{:.0}KB", b as f64 / 1_000.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn true_web_dl_is_web_dl() {
        let i = analyse("Peddi (2026) TRUE WEB-DL - 4K SDR - HEVC - 19.5GB - ESub");
        assert_eq!(i.quality, Quality::WebDl);
        assert_eq!(i.source, Source::TrueWebDl);
        assert_eq!(i.resolution, Resolution::Uhd);
        assert_eq!(i.size_bytes, Some(19_500_000_000));
    }

    #[test]
    fn hdrip_is_not_web_dl() {
        let i = analyse("Peddi (2026) Kannada HQ HDRip - 720p - x264 - 1.6GB - ESub");
        assert_eq!(i.quality, Quality::HdRip);
        assert!(!i.quality.is_web_dl());
    }

    #[test]
    fn theater_print_is_detected() {
        let i = analyse("Movie (2025) PRE-DVD 1080p XviD");
        assert_eq!(i.quality, Quality::Theater);
        assert!(i.theatrical);
    }

    #[test]
    fn dvdrip_is_not_web_dl() {
        let i = analyse("Movie (2025) DVDRip XviD");
        assert_eq!(i.quality, Quality::DvdRip);
    }

    #[test]
    fn dd_audio_is_not_mistaken_for_size() {
        let i = analyse("Movie (2025) WEB-DL 1080p (DD+5.1 - 640Kbps & AAC 2.0) - 3.3GB");
        assert_eq!(i.size_bytes, Some(3_300_000_000));
    }
}
