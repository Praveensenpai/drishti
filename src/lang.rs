//! Language identification.
//!
//! Release names on the site follow scene conventions, so the audio languages
//! of a release can be recovered from two places:
//!
//! * bracketed multi-audio lists — `[Tamil + Telugu + Malayalam + Kannada]`
//! * a language word in the title prefix — `Peddi (2026) Kannada TRUE WEB-DL`
//!
//! The first language of a bracketed list is, by the uploader's convention, the
//! language the film was originally made in.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A spoken language we can reason about.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Lang {
    Tamil,
    Telugu,
    Hindi,
    Malayalam,
    Kannada,
    English,
    Bengali,
    Marathi,
    Gujarati,
    Punjabi,
    Odia,
    Assamese,
    Japanese,
    Korean,
    Chinese,
    Spanish,
    French,
    German,
    Russian,
    Portuguese,
    Italian,
    Arabic,
    Thai,
    Indonesian,
    Vietnamese,
    Turkish,
    Other,
}

impl Lang {
    /// ISO 639-1 code, used for compact columns and as the Gemini reply format.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Tamil => "tam",
            Lang::Telugu => "tel",
            Lang::Hindi => "hin",
            Lang::Malayalam => "mal",
            Lang::Kannada => "kan",
            Lang::English => "eng",
            Lang::Bengali => "ben",
            Lang::Marathi => "mar",
            Lang::Gujarati => "guj",
            Lang::Punjabi => "pun",
            Lang::Odia => "ori",
            Lang::Assamese => "asm",
            Lang::Japanese => "jpn",
            Lang::Korean => "kor",
            Lang::Chinese => "chi",
            Lang::Spanish => "spa",
            Lang::French => "fre",
            Lang::German => "ger",
            Lang::Russian => "rus",
            Lang::Portuguese => "por",
            Lang::Italian => "ita",
            Lang::Arabic => "ara",
            Lang::Thai => "tha",
            Lang::Indonesian => "ind",
            Lang::Vietnamese => "vie",
            Lang::Turkish => "tur",
            Lang::Other => "und",
        }
    }

    /// Human readable name.
    pub fn label(self) -> &'static str {
        match self {
            Lang::Tamil => "Tamil",
            Lang::Telugu => "Telugu",
            Lang::Hindi => "Hindi",
            Lang::Malayalam => "Malayalam",
            Lang::Kannada => "Kannada",
            Lang::English => "English",
            Lang::Bengali => "Bengali",
            Lang::Marathi => "Marathi",
            Lang::Gujarati => "Gujarati",
            Lang::Punjabi => "Punjabi",
            Lang::Odia => "Odia",
            Lang::Assamese => "Assamese",
            Lang::Japanese => "Japanese",
            Lang::Korean => "Korean",
            Lang::Chinese => "Chinese",
            Lang::Spanish => "Spanish",
            Lang::French => "French",
            Lang::German => "German",
            Lang::Russian => "Russian",
            Lang::Portuguese => "Portuguese",
            Lang::Italian => "Italian",
            Lang::Arabic => "Arabic",
            Lang::Thai => "Thai",
            Lang::Indonesian => "Indonesian",
            Lang::Vietnamese => "Vietnamese",
            Lang::Turkish => "Turkish",
            Lang::Other => "Unknown",
        }
    }

    /// Every language, ordered by prominence in the source material.
    pub const ALL: [Lang; 27] = [
        Lang::Tamil,
        Lang::Telugu,
        Lang::Hindi,
        Lang::Malayalam,
        Lang::Kannada,
        Lang::English,
        Lang::Bengali,
        Lang::Marathi,
        Lang::Gujarati,
        Lang::Punjabi,
        Lang::Odia,
        Lang::Assamese,
        Lang::Japanese,
        Lang::Korean,
        Lang::Chinese,
        Lang::Spanish,
        Lang::French,
        Lang::German,
        Lang::Russian,
        Lang::Portuguese,
        Lang::Italian,
        Lang::Arabic,
        Lang::Thai,
        Lang::Indonesian,
        Lang::Vietnamese,
        Lang::Turkish,
        Lang::Other,
    ];

    /// Map an ISO 639-1 code (or a name) to a language.
    pub fn from_code(code: &str) -> Lang {
        let c = code.trim().to_ascii_lowercase();
        match c.as_str() {
            "tam" | "ta" => Lang::Tamil,
            "tel" | "te" => Lang::Telugu,
            "hin" | "hi" => Lang::Hindi,
            "mal" | "ml" => Lang::Malayalam,
            "kan" | "kn" => Lang::Kannada,
            "eng" | "en" => Lang::English,
            "ben" | "bn" => Lang::Bengali,
            "mar" | "mr" => Lang::Marathi,
            "guj" => Lang::Gujarati,
            "pun" | "pa" => Lang::Punjabi,
            "ori" | "or" => Lang::Odia,
            "asm" => Lang::Assamese,
            "jpn" | "ja" => Lang::Japanese,
            "kor" | "ko" => Lang::Korean,
            "chi" | "zh" | "cmn" => Lang::Chinese,
            "spa" | "es" => Lang::Spanish,
            "fre" | "fra" | "fr" => Lang::French,
            "ger" | "deu" | "de" => Lang::German,
            "rus" | "ru" => Lang::Russian,
            "por" | "pt" => Lang::Portuguese,
            "ita" | "it" => Lang::Italian,
            "ara" | "ar" => Lang::Arabic,
            "tha" | "th" => Lang::Thai,
            "ind" | "id" => Lang::Indonesian,
            "vie" | "vi" => Lang::Vietnamese,
            "tur" | "tr" => Lang::Turkish,
            _ => Lang::Other,
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Scene-name aliases, matched case-insensitively against whole tokens.
const ALIASES: &[(&str, Lang)] = &[
    ("tamil", Lang::Tamil),
    ("tam", Lang::Tamil),
    ("tml", Lang::Tamil),
    ("tamiladu", Lang::Tamil),
    ("telugu", Lang::Telugu),
    ("tel", Lang::Telugu),
    ("telng", Lang::Telugu),
    ("hindi", Lang::Hindi),
    ("hin", Lang::Hindi),
    ("malayalam", Lang::Malayalam),
    ("mal", Lang::Malayalam),
    ("mlym", Lang::Malayalam),
    ("kannada", Lang::Kannada),
    ("kan", Lang::Kannada),
    ("knd", Lang::Kannada),
    ("english", Lang::English),
    ("eng", Lang::English),
    ("bengali", Lang::Bengali),
    ("bangla", Lang::Bengali),
    ("ben", Lang::Bengali),
    ("marathi", Lang::Marathi),
    ("mar", Lang::Marathi),
    ("gujarati", Lang::Gujarati),
    ("guj", Lang::Gujarati),
    ("punjabi", Lang::Punjabi),
    ("pun", Lang::Punjabi),
    ("odia", Lang::Odia),
    ("ori", Lang::Odia),
    ("oriya", Lang::Odia),
    ("assamese", Lang::Assamese),
    ("asm", Lang::Assamese),
    ("japanese", Lang::Japanese),
    ("jap", Lang::Japanese),
    ("kor", Lang::Korean),
    ("korean", Lang::Korean),
    ("chinese", Lang::Chinese),
    ("chi", Lang::Chinese),
    ("mandarin", Lang::Chinese),
    ("spanish", Lang::Spanish),
    ("spa", Lang::Spanish),
    ("castellano", Lang::Spanish),
    ("french", Lang::French),
    ("fre", Lang::French),
    ("fra", Lang::French),
    ("german", Lang::German),
    ("ger", Lang::German),
    ("deu", Lang::German),
    ("russian", Lang::Russian),
    ("rus", Lang::Russian),
    ("portuguese", Lang::Portuguese),
    ("por", Lang::Portuguese),
    ("italian", Lang::Italian),
    ("ita", Lang::Italian),
    ("arabic", Lang::Arabic),
    ("ara", Lang::Arabic),
    ("thai", Lang::Thai),
    ("tha", Lang::Thai),
    ("indonesian", Lang::Indonesian),
    ("indon", Lang::Indonesian),
    ("vietnamese", Lang::Vietnamese),
    ("vie", Lang::Vietnamese),
    ("turkish", Lang::Turkish),
    ("tur", Lang::Turkish),
];

/// Resolve a single bare token to a language.
///
/// Only unambiguous three-letter and longer aliases are accepted here. The
/// two-letter ISO codes are only honoured inside explicit language lists,
/// where the context makes them unambiguous.
pub fn lang_from_token(token: &str) -> Option<Lang> {
    let t = token.trim().trim_matches(|c: char| !c.is_alphanumeric());
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower)
        .map(|(_, lang)| *lang)
}

/// Resolve a token that appears inside an explicit language list, where the
/// two-letter ISO codes are also accepted.
pub fn lang_from_list_token(token: &str) -> Option<Lang> {
    let t = token.trim().trim_matches(|c: char| !c.is_alphanumeric());
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if let Some(lang) = ALIASES.iter().find(|(a, _)| *a == lower).map(|(_, l)| *l) {
        return Some(lang);
    }
    match lower.as_str() {
        "ta" => Some(Lang::Tamil),
        "te" => Some(Lang::Telugu),
        "hi" => Some(Lang::Hindi),
        "ml" => Some(Lang::Malayalam),
        "kn" => Some(Lang::Kannada),
        "en" => Some(Lang::English),
        "bn" => Some(Lang::Bengali),
        "mr" => Some(Lang::Marathi),
        "ja" => Some(Lang::Japanese),
        "ko" => Some(Lang::Korean),
        "zh" => Some(Lang::Chinese),
        "es" => Some(Lang::Spanish),
        "fr" => Some(Lang::French),
        "de" => Some(Lang::German),
        "ru" => Some(Lang::Russian),
        "pt" => Some(Lang::Portuguese),
        "it" => Some(Lang::Italian),
        "ar" => Some(Lang::Arabic),
        "th" => Some(Lang::Thai),
        "id" => Some(Lang::Indonesian),
        "vi" => Some(Lang::Vietnamese),
        "tr" => Some(Lang::Turkish),
        _ => None,
    }
}

/// Languages mentioned inside a bracketed group such as `[Tam + Tel + Mal + Kan]`.
pub fn parse_list_group(group: &str) -> Vec<Lang> {
    let mut out = Vec::new();
    for part in group.split(['+', '/', ',', '&', '|']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(l) = lang_from_list_token(part)
            && !out.contains(&l) {
                out.push(l);
            }
    }
    out
}

/// All languages mentioned anywhere in free text, in order of appearance.
pub fn scan_text(text: &str) -> Vec<Lang> {
    let mut out = Vec::new();
    for token in text.split(|c: char| !c.is_alphanumeric()) {
        if let Some(l) = lang_from_token(token)
            && !out.contains(&l) {
                out.push(l);
            }
    }
    out
}

/// The language set of a single release.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LangSet {
    /// Languages listed in a bracketed multi-audio group.
    pub audio: Vec<Lang>,
    /// Languages named in the free-text part of the name.
    pub text: Vec<Lang>,
    /// The release is explicitly labelled as a dub.
    pub dubbed: bool,
    /// The release is explicitly labelled as subtitled.
    pub subtitled: bool,
}

impl LangSet {
    /// Every language mentioned, bracketed ones first.
    pub fn all(&self) -> Vec<Lang> {
        let mut out = self.audio.clone();
        for l in &self.text {
            if !out.contains(l) {
                out.push(*l);
            }
        }
        out
    }

    /// True when the release carries more than one audio track.
    pub fn is_multi_audio(&self) -> bool {
        self.audio.len() > 1
    }


    /// Compact display, e.g. `Tam+Tel+Mal+Kan`.
    pub fn compact(&self) -> String {
        let langs = self.all();
        if langs.is_empty() {
            return "-".to_string();
        }
        langs
            .iter()
            .map(|l| short_code(*l))
            .collect::<Vec<_>>()
            .join("+")
    }
}

fn short_code(l: Lang) -> &'static str {
    match l {
        Lang::Tamil => "Tam",
        Lang::Telugu => "Tel",
        Lang::Hindi => "Hin",
        Lang::Malayalam => "Mal",
        Lang::Kannada => "Kan",
        Lang::English => "Eng",
        Lang::Bengali => "Ben",
        Lang::Marathi => "Mar",
        Lang::Gujarati => "Guj",
        Lang::Punjabi => "Pun",
        Lang::Odia => "Ori",
        Lang::Assamese => "Asm",
        Lang::Japanese => "Jap",
        Lang::Korean => "Kor",
        Lang::Chinese => "Chi",
        Lang::Spanish => "Spa",
        Lang::French => "Fre",
        Lang::German => "Ger",
        Lang::Russian => "Rus",
        Lang::Portuguese => "Por",
        Lang::Italian => "Ita",
        Lang::Arabic => "Ara",
        Lang::Thai => "Tha",
        Lang::Indonesian => "Ind",
        Lang::Vietnamese => "Vie",
        Lang::Turkish => "Tur",
        Lang::Other => "Und",
    }
}

/// Markers that mark a release as a dub rather than the original audio.
const DUB_MARKERS: &[&str] = &["dubbed", "dub", "hindi dub", "kan dub", "tel dub", "mal dub"];

/// Markers that mark a release as subtitled only.
const SUB_MARKERS: &[&str] = &["subtitled", "subs", "subbed"];

/// Parse the languages of a release name.
pub fn parse_release_langs(name: &str) -> LangSet {
    let mut set = LangSet::default();
    let lower = name.to_ascii_lowercase();

    for marker in DUB_MARKERS {
        if contains_word(&lower, marker) {
            set.dubbed = true;
            break;
        }
    }
    for marker in SUB_MARKERS {
        if contains_word(&lower, marker) {
            set.subtitled = true;
            break;
        }
    }

    // Bracketed groups hold the explicit audio track list.
    let mut rest = String::new();
    let mut chars = name.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '[' {
            let mut group = String::new();
            let mut depth = 1;
            for (_, c2) in chars.by_ref() {
                if c2 == '[' {
                    depth += 1;
                } else if c2 == ']' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                group.push(c2);
            }
            for l in parse_list_group(&group) {
                if !set.audio.contains(&l) {
                    set.audio.push(l);
                }
            }
        } else if c == '(' {
            // Parenthesised groups hold audio/subspec info, not languages.
            let mut depth = 1;
            for (_, c2) in chars.by_ref() {
                if c2 == '(' {
                    depth += 1;
                } else if c2 == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
        } else {
            rest.push(c);
            let _ = i;
        }
    }

    set.text = scan_text(&rest);
    set
}

/// Word-boundary aware case-insensitive containment.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0
            || !haystack[..abs]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        let after = abs + needle.len();
        let after_ok = after >= haystack.len()
            || !haystack[after..]
                .chars()
                .next()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_audio_bracket() {
        let s = parse_release_langs(
            "www.1TamilMV.report - Peddi (2026) TRUE WEB-DL - 4K SDR - HEVC - [Tam + Tel + Mal + Kan] - (DD+5.1 - 640Kbps & AAC) - 19.5GB - ESub.mkv.torrent",
        );
        assert_eq!(
            s.audio,
            vec![Lang::Tamil, Lang::Telugu, Lang::Malayalam, Lang::Kannada]
        );
        assert!(s.is_multi_audio());
        assert!(!s.dubbed);
    }

    #[test]
    fn parses_prefix_language() {
        let s = parse_release_langs(
            "Peddi (2026) Kannada TRUE WEB-DL - 1080p - AVC - UNTOUCHED - (DD+5.1 - 640Kbps & AAC 2.0) - 3.3GB - ESub",
        );
        assert_eq!(s.text, vec![Lang::Kannada]);
        assert!(s.audio.is_empty());
    }

    #[test]
    fn detects_dub_marker() {
        let s = parse_release_langs("KGF Chapter 2 (2026) Hindi Dubbed WEB-DL 1080p");
        assert!(s.dubbed);
        assert_eq!(s.text, vec![Lang::Hindi]);
    }

    #[test]
    fn long_names_do_not_false_match() {
        assert!(scan_text("Malayalam").contains(&Lang::Malayalam));
        assert!(!scan_text("Chennai Express 1080p").contains(&Lang::Chinese));
    }
}
