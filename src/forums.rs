//! Forum catalogue for 1TamilMV.
//!
//! The board is split per language, and each language has its own quality
//! sub-forum. The WEB-HD forum of every language is the one that holds the
//! WEB-DL material we care about.

use crate::lang::Lang;

/// A forum on the board.
#[derive(Clone, Copy, Debug)]
pub struct Forum {
    /// Numeric forum id.
    pub id: u32,
    /// URL slug without the id prefix.
    pub slug: &'static str,
    /// Human readable title.
    pub title: &'static str,
    /// Language the forum belongs to.
    pub lang: Lang,
    /// The quality bucket the forum is about.
    pub kind: ForumKind,
}

/// The quality bucket a forum covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForumKind {
    /// WEB-HD / iTunes-HD / BluRay — the forum we default to.
    WebHd,
    /// PreDVD / DVDScr / CAM / TC — theatrical prints.
    PreDvd,
    /// HD-Rips / DVD-Rips / BR-Rips.
    HdRips,
    /// DVD9 / DVD5.
    Dvd,
    /// HDTV / SDTV.
    HdTv,
    /// Hollywood films released with several audio tracks.
    Hollywood,
    /// Dubbed releases of another language.
    Dubbed,
    /// Web series and TV shows.
    WebSeries,
}

impl ForumKind {
    pub fn label(self) -> &'static str {
        match self {
            ForumKind::WebHd => "WEB-HD",
            ForumKind::PreDvd => "PRE-DVD",
            ForumKind::HdRips => "HD-RIP",
            ForumKind::Dvd => "DVD",
            ForumKind::HdTv => "HDTV",
            ForumKind::Hollywood => "MULTI-AUDIO",
            ForumKind::Dubbed => "DUBBED",
            ForumKind::WebSeries => "WEB-SERIES",
        }
    }
}

macro_rules! forum {
    ($id:expr, $slug:expr, $title:expr, $lang:expr, $kind:expr) => {
        Forum {
            id: $id,
            slug: $slug,
            title: $title,
            lang: $lang,
            kind: $kind,
        }
    };
}

/// Every forum worth listing.
pub const FORUMS: &[Forum] = &[
    // Tamil
    forum!(9, "tamil-language", "Tamil", Lang::Tamil, ForumKind::WebHd),
    forum!(10, "predvd-dvdscr-cam-tc", "Tamil PreDVD", Lang::Tamil, ForumKind::PreDvd),
    forum!(11, "web-hd-itunes-hd-bluray", "Tamil WEB-HD", Lang::Tamil, ForumKind::WebHd),
    forum!(12, "hd-rips-dvd-rips-br-rips", "Tamil HD-Rips", Lang::Tamil, ForumKind::HdRips),
    forum!(13, "dvd9-dvd5", "Tamil DVD", Lang::Tamil, ForumKind::Dvd),
    forum!(14, "hdtv-sdtv-hdtv-rips", "Tamil HDTV", Lang::Tamil, ForumKind::HdTv),
    forum!(17, "hollywood-movies-in-multi-audios", "Hollywood Multi-Audio", Lang::English, ForumKind::Hollywood),
    forum!(19, "web-series-tv-shows", "Tamil Web Series", Lang::Tamil, ForumKind::WebSeries),
    // Telugu
    forum!(22, "telugu-language", "Telugu", Lang::Telugu, ForumKind::WebHd),
    forum!(23, "predvd-dvdscr-cam-tc", "Telugu PreDVD", Lang::Telugu, ForumKind::PreDvd),
    forum!(24, "web-hd-itunes-hd-bluray", "Telugu WEB-HD", Lang::Telugu, ForumKind::WebHd),
    forum!(25, "hd-rips-dvd-rips-br-rips", "Telugu HD-Rips", Lang::Telugu, ForumKind::HdRips),
    forum!(26, "dvd9-dvd5", "Telugu DVD", Lang::Telugu, ForumKind::Dvd),
    forum!(27, "hdtv-sdtv-hdtv-rips", "Telugu HDTV", Lang::Telugu, ForumKind::HdTv),
    forum!(31, "telugu-dubbed-movies", "Telugu Dubbed", Lang::Telugu, ForumKind::Dubbed),
    forum!(33, "web-series-tv-shows", "Telugu Web Series", Lang::Telugu, ForumKind::WebSeries),
    // Hindi
    forum!(56, "hindi-language", "Hindi", Lang::Hindi, ForumKind::WebHd),
    forum!(57, "predvd-dvdscr-cam-tc", "Hindi PreDVD", Lang::Hindi, ForumKind::PreDvd),
    forum!(58, "web-hd-itunes-hd-bluray", "Hindi WEB-HD", Lang::Hindi, ForumKind::WebHd),
    forum!(59, "hd-rips-dvd-rips-br-rips", "Hindi HD-Rips", Lang::Hindi, ForumKind::HdRips),
    forum!(60, "dvd9-dvd5", "Hindi DVD", Lang::Hindi, ForumKind::Dvd),
    forum!(61, "hdtv-sdtv-hdtv-rips", "Hindi HDTV", Lang::Hindi, ForumKind::HdTv),
    forum!(64, "hindi-dubbed-movies", "Hindi Dubbed", Lang::Hindi, ForumKind::Dubbed),
    forum!(66, "web-series-tv-shows", "Hindi Web Series", Lang::Hindi, ForumKind::WebSeries),
    // Malayalam
    forum!(34, "malayalam-language", "Malayalam", Lang::Malayalam, ForumKind::WebHd),
    forum!(35, "predvd-dvdscr-cam-tc", "Malayalam PreDVD", Lang::Malayalam, ForumKind::PreDvd),
    forum!(36, "web-hd-itunes-hd-bluray", "Malayalam WEB-HD", Lang::Malayalam, ForumKind::WebHd),
    forum!(37, "hd-rips-dvd-rips-br-rips", "Malayalam HD-Rips", Lang::Malayalam, ForumKind::HdRips),
    forum!(38, "dvd9-dvd5", "Malayalam DVD", Lang::Malayalam, ForumKind::Dvd),
    forum!(39, "hdtv-sdtv-hdtv-rips", "Malayalam HDTV", Lang::Malayalam, ForumKind::HdTv),
    forum!(42, "malayalam-dubbed-subtitled-movies", "Malayalam Dubbed", Lang::Malayalam, ForumKind::Dubbed),
    forum!(44, "web-series-tv-shows", "Malayalam Web Series", Lang::Malayalam, ForumKind::WebSeries),
    // Kannada
    forum!(67, "kannada-language", "Kannada", Lang::Kannada, ForumKind::WebHd),
    forum!(68, "predvd-dvdscr-cam-tc", "Kannada PreDVD", Lang::Kannada, ForumKind::PreDvd),
    forum!(69, "web-hd-itunes-hd-bluray", "Kannada WEB-HD", Lang::Kannada, ForumKind::WebHd),
    forum!(70, "hd-rips-dvd-rips-br-rips", "Kannada HD-Rips", Lang::Kannada, ForumKind::HdRips),
    forum!(71, "dvd9-dvd5", "Kannada DVD", Lang::Kannada, ForumKind::Dvd),
    forum!(72, "hdtv-sdtv-hdtv-rips", "Kannada HDTV", Lang::Kannada, ForumKind::HdTv),
    forum!(77, "web-series-tv-shows", "Kannada Web Series", Lang::Kannada, ForumKind::WebSeries),
    // English
    forum!(45, "english-language", "English", Lang::English, ForumKind::WebHd),
    forum!(46, "predvd-dvdscr-cam-tc", "English PreDVD", Lang::English, ForumKind::PreDvd),
    forum!(49, "web-hd-itunes-hd-bluray", "English WEB-HD", Lang::English, ForumKind::WebHd),
    forum!(50, "hd-rips-dvd-rips-br-rips", "English HD-Rips", Lang::English, ForumKind::HdRips),
    forum!(54, "dvd9-dvd5", "English DVD", Lang::English, ForumKind::Dvd),
    forum!(55, "web-series-tv-shows", "English Web Series", Lang::English, ForumKind::WebSeries),
];

/// The root "language" forum of a language.
fn f_lang_root(lang: Lang) -> u32 {
    match lang {
        Lang::Tamil => 9,
        Lang::Telugu => 22,
        Lang::Hindi => 56,
        Lang::Malayalam => 34,
        Lang::Kannada => 67,
        Lang::English => 45,
        _ => 0,
    }
}

/// Look a forum up by id.
pub fn by_id(id: u32) -> Option<&'static Forum> {
    FORUMS.iter().find(|f| f.id == id)
}

/// The WEB-HD forum of a language, which is where WEB-DL material lives.
#[cfg(test)]
pub fn web_hd_forum(lang: Lang) -> Option<&'static Forum> {
    FORUMS
        .iter()
        .find(|f| f.kind == ForumKind::WebHd && f.lang == lang && f.id != f_lang_root(lang))
}

/// All WEB-HD forums, in board order.
pub fn all_web_hd() -> Vec<&'static Forum> {
    FORUMS
        .iter()
        .filter(|f| f.kind == ForumKind::WebHd && f.id != f_lang_root(f.lang))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kannada_web_hd_is_69() {
        let f = web_hd_forum(Lang::Kannada).expect("kannada web-hd forum");
        assert_eq!(f.id, 69);
    }

    #[test]
    fn every_language_has_a_web_hd_forum() {
        for lang in [
            Lang::Tamil,
            Lang::Telugu,
            Lang::Hindi,
            Lang::Malayalam,
            Lang::Kannada,
            Lang::English,
        ] {
            assert!(web_hd_forum(lang).is_some(), "missing {lang}");
        }
    }
}
