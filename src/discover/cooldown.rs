//! Failure cooldown for domain discovery.
//!
//! A blocked search engine or a board that is briefly unreachable should not
//! trigger a search on every launch. Failed discovery writes a timestamp; while
//! it is fresh, discovery is skipped.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const COOLDOWN_SECS: u64 = 60 * 60;

/// A single timestamp file in the existing drushti cache directory.
pub struct Cooldown {
    path: PathBuf,
}

impl Cooldown {
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            path: cache_dir.join("domain_discovery"),
        }
    }

    /// True while a recent failed discovery still blocks another attempt.
    pub fn is_active(&self) -> bool {
        let Some(last) = read_timestamp(&self.path) else {
            return false;
        };
        now_secs().saturating_sub(last) < COOLDOWN_SECS
    }

    /// Record a failed discovery attempt.
    pub fn mark(&self) {
        if let Some(parent) = self.path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return;
        }
        let _ = std::fs::write(&self.path, now_secs().to_string());
    }
}

fn read_timestamp(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_fresh_stamp_as_active() {
        let dir = std::env::temp_dir().join(format!("drushti-cooldown-{}", now_secs()));
        let cooldown = Cooldown::new(&dir);
        assert!(!cooldown.is_active());
        cooldown.mark();
        assert!(cooldown.is_active());
        let _ = std::fs::remove_dir_all(dir);
    }
}
