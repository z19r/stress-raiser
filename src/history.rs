//! Persistent history for form fields. Up/Down to cycle. Survives app restart.
//!
//! Persistence is best-effort: load/save errors (missing file, invalid JSON,
//! permission errors) are ignored; no logging or user notification.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const HISTORY_LEN: usize = 50;
const HISTORY_FILE: &str = "stress-raiser/history.json";

/// One saved form state (URL, method, headers, body, concurrency, RPM, limits).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub method: String,
    pub headers: String,
    pub body: String,
    pub conc: usize,
    pub rpm: u64,
    #[serde(default)]
    pub total_requests: Option<u64>,
    #[serde(default)]
    pub duration_secs: Option<u64>,
}

impl HistoryEntry {
    /// Build a history entry from form field values.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: &str,
        method: &str,
        headers: &str,
        body: &str,
        conc: usize,
        rpm: u64,
        total_requests: Option<u64>,
        duration_secs: Option<u64>,
    ) -> Self {
        Self {
            url: url.to_string(),
            method: method.to_string(),
            headers: headers.to_string(),
            body: body.to_string(),
            conc,
            rpm,
            total_requests,
            duration_secs,
        }
    }
}

fn history_path() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
        .join(HISTORY_FILE)
}

/// Load history from disk. Returns empty vec on error or missing file.
pub fn load_history() -> Vec<HistoryEntry> {
    let path = history_path();
    let data = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    serde_json::from_str(&data).unwrap_or_default()
}

/// Save history to disk. Ignores errors (best-effort).
pub fn save_history(entries: &[HistoryEntry]) {
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(entries).unwrap_or_default();
    let _ = fs::write(path, json);
}

/// Prepend `new` to history, dedupe by content, cap length.
pub fn add_to_history(entries: &mut Vec<HistoryEntry>, new: HistoryEntry) {
    entries.retain(|e| {
        !(e.url == new.url
            && e.headers == new.headers
            && e.body == new.body
            && e.conc == new.conc
            && e.rpm == new.rpm
            && e.total_requests == new.total_requests
            && e.duration_secs == new.duration_secs)
    });
    entries.insert(0, new);
    if entries.len() > HISTORY_LEN {
        entries.truncate(HISTORY_LEN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, conc: usize) -> HistoryEntry {
        HistoryEntry::new(url, "GET", "", "", conc, 100, None, None)
    }

    #[test]
    fn add_prepends_to_front() {
        let mut hist = vec![entry("http://a.com", 1)];
        add_to_history(&mut hist, entry("http://b.com", 1));
        assert_eq!(hist[0].url, "http://b.com");
        assert_eq!(hist[1].url, "http://a.com");
    }

    #[test]
    fn add_deduplicates_by_content() {
        let mut hist = vec![entry("http://a.com", 1), entry("http://b.com", 1)];
        add_to_history(&mut hist, entry("http://a.com", 1));
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].url, "http://a.com");
    }

    #[test]
    fn add_caps_at_history_len() {
        let mut hist: Vec<HistoryEntry> = (0..HISTORY_LEN)
            .map(|i| entry(&format!("http://{i}.com"), 1))
            .collect();
        add_to_history(&mut hist, entry("http://new.com", 1));
        assert_eq!(hist.len(), HISTORY_LEN);
        assert_eq!(hist[0].url, "http://new.com");
    }

    #[test]
    fn new_preserves_all_fields() {
        let e = HistoryEntry::new(
            "http://x.com",
            "POST",
            "Content-Type: application/json",
            "{\"key\":1}",
            10,
            500,
            Some(1000),
            Some(60),
        );
        assert_eq!(e.method, "POST");
        assert_eq!(e.conc, 10);
        assert_eq!(e.rpm, 500);
        assert_eq!(e.total_requests, Some(1000));
        assert_eq!(e.duration_secs, Some(60));
    }

    #[test]
    fn serde_roundtrip() {
        let e = HistoryEntry::new("http://x.com", "GET", "", "", 5, 100, Some(50), None);
        let json = serde_json::to_string(&e).unwrap();
        let parsed: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.url, e.url);
        assert_eq!(parsed.conc, e.conc);
        assert_eq!(parsed.total_requests, Some(50));
        assert_eq!(parsed.duration_secs, None);
    }

    #[test]
    fn serde_missing_optional_fields_default() {
        let json =
            r#"{"url":"http://x.com","method":"GET","headers":"","body":"","conc":1,"rpm":10}"#;
        let parsed: HistoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.total_requests, None);
        assert_eq!(parsed.duration_secs, None);
    }
}
