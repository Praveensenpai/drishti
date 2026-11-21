//! Fuzzy movie-title search.
//!
//! Provides subsequence matching with a score (more matches = higher score)
//! and character indices for UI highlighting.
//!
//! Matches `query` as a subsequence of `text` (case-insensitive),
//! allowing interspersed characters.

/// A single ranked search result with the matched-character indices for highlighting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchResult {
    /// Score: number of matched characters (higher is better).
    pub score: i64,
    /// The text to display (movie name or thread title).
    pub text: String,
    /// Byte offsets in `text` of each matched character, for highlighting.
    pub indices: Vec<u32>,
    /// Page the matched thread belongs to.
    pub page_id: u32,
    /// Index of the matched thread within that page.
    pub thread_index: usize,
}

impl MatchResult {
    /// The human-readable label.
    #[allow(dead_code)]
    pub fn label(&self) -> &str {
        &self.text
    }
}

/// Fuzzy‑match a query against a text string.
///
/// Case‑insensitive subsequence matching: every character of the query
/// must appear in order in the target, but other characters can be interspersed.
/// Scores consecutive matches, early matches, and word-boundary matches higher.
pub fn fuzzy_match(query: &str, text: &str) -> Option<MatchResult> {
    let q_chars: Vec<char> = query.to_lowercase().chars().collect();
    if q_chars.is_empty() {
        return None;
    }

    let t_chars: Vec<char> = text.chars().collect();
    let t_lower: Vec<char> = text.to_lowercase().chars().collect();

    let mut char_byte_offsets: Vec<u32> = Vec::with_capacity(t_chars.len());
    let mut byte_pos = 0u32;
    for c in &t_chars {
        char_byte_offsets.push(byte_pos);
        byte_pos += c.len_utf8() as u32;
    }

    let mut indices = Vec::with_capacity(q_chars.len());
    let mut q_pos = 0usize;
    let mut score: i64 = 0;
    let mut prev_match: Option<usize> = None;
    let mut first_match: Option<usize> = None;

    for (i, &tc) in t_lower.iter().enumerate() {
        if q_pos < q_chars.len() && tc == q_chars[q_pos] {
            indices.push(char_byte_offsets[i]);

            if first_match.is_none() {
                first_match = Some(i);
            }

            // Base score per matched char.
            score += 10;

            // Strong bonus for consecutive matches.
            if let Some(prev) = prev_match {
                if i == prev + 1 {
                    score += 20;
                } else {
                    // Small gap penalty.
                    let gap = (i - prev - 1) as i64;
                    score -= gap.min(10);
                }
            }

            // Bonus for match at start or word boundary.
            if i == 0 {
                score += 15;
            } else {
                let prev_c = t_chars[i - 1];
                if prev_c.is_whitespace()
                    || prev_c == '-'
                    || prev_c == '_'
                    || prev_c == '.'
                    || prev_c == '('
                    || prev_c == '['
                {
                    score += 10;
                }
            }

            prev_match = Some(i);
            q_pos += 1;
        }
    }

    if q_pos < q_chars.len() {
        return None;
    }

    // Prefer earlier first match and shorter overall text.
    if let Some(first) = first_match {
        score -= first as i64;
    }
    score -= (t_chars.len() as i64) / 20;

    Some(MatchResult {
        score,
        text: text.to_string(),
        indices,
        page_id: 0,
        thread_index: 0,
    })
}

/// Match a search query against a thread: first the cleaned movie name, then the
/// full thread title, keeping the best score.
pub fn match_thread(query: &str, movie: &str, title: &str) -> Option<MatchResult> {
    // Try the cleaned movie name first.
    if let Some(mut best) = fuzzy_match(query, movie) {
        // Now try the full title; keep the result with the higher score.
        if let Some(other) = fuzzy_match(query, title)
            && other.score > best.score
        {
            best = other;
        }
        Some(best)
    } else {
        // Fall back to title only.
        fuzzy_match(query, title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pddi_against_peddi() {
        let result = fuzzy_match("pddi", "Peddi (2026) TRUE WEB-DL - 19.5GB - ESub");
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(m.score > 0);
        assert!(!m.indices.is_empty());
        // Indices should be in increasing order.
        for window in m.indices.windows(2) {
            assert!(window[0] < window[1]);
        }
    }

    #[test]
    fn matches_jlr2_against_jailer_2() {
        let result = fuzzy_match("jlr2", "Jailer 2");
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(m.score > 0);
        assert!(!m.indices.is_empty());
    }

    #[test]
    fn ranking_higher_score_better() {
        let a = fuzzy_match("peddi", "Peddi (2026) WEB-DL").unwrap().score;
        let b = fuzzy_match("ped", "Peddi (2026) WEB-DL").unwrap().score;
        // More specific query scores higher (same length match, but we compare
        // by total matched chars; both match all chars of the shorter query).
        // For this simple matcher, scores are just query.len() so they'll be equal
        // when both match fully. This test documents the behavior.
        assert!(a >= b);
    }

    #[test]
    fn no_match_when_not_subsequence() {
        let result = fuzzy_match("xyz", "Some random title");
        assert!(result.is_none());
    }

    #[test]
    fn case_insensitive() {
        let a = fuzzy_match("peddi", "Peddi (2026)").unwrap().score;
        let b = fuzzy_match("PEDDI", "Peddi (2026)").unwrap().score;
        // Our matcher is case-insensitive, scores should match.
        assert_eq!(a, b);
    }

    #[test]
    fn movie_takes_precedence_over_title_when_both_match() {
        let movie = "Inception";
        let title = "Inception (2010) BluRay";
        let q = "inception";

        let movie_match = match_thread(q, movie, title);
        assert!(movie_match.is_some());
        // The text should be the movie name (shorter = potentially higher score).
        if let Some(m) = &movie_match {
            assert_eq!(m.text, movie);
        }
    }

    #[test]
    fn title_fallback_when_movie_has_no_match() {
        let movie = "RandomXyzTitle";
        let title = "Some Movie Title";
        let q = "movie";

        let result = match_thread(q, movie, title);
        // Should fall back to title match.
        assert!(result.is_some());
        // The text should be the title.
        if let Some(m) = result {
            assert_eq!(m.text, title);
        }
    }

    #[test]
    fn indices_increasing_order() {
        let result = fuzzy_match("abc", "axbycz");
        assert!(result.is_some());
        let m = result.unwrap();
        // Indices should be strictly increasing since we match in order.
        for window in m.indices.windows(2) {
            assert!(window[0] < window[1]);
        }
    }
}
