//! Persistent history for form fields. Up/Down to cycle. Survives app restart.
//!
//! Persistence is best-effort: load/save errors (missing file, invalid JSON,
//! permission errors) are ignored; no logging or user notification.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const HISTORY_LEN: usize = 50;
const HISTORY_FILE: &str = "stress-riser/history.json";

/// One saved form state (URL, method, headers, body, concurrency, RPM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub method: String,
    pub headers: String,
    pub body: String,
    pub conc: usize,
    pub rpm: u64,
}

impl HistoryEntry {
    /// Build a history entry from form field values.
    pub fn new(url: &str, method: &str, headers: &str, body: &str, conc: usize, rpm: u64) -> Self {
        Self {
            url: url.to_string(),
            method: method.to_string(),
            headers: headers.to_string(),
            body: body.to_string(),
            conc,
            rpm,
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
            && e.rpm == new.rpm)
    });
    entries.insert(0, new);
    if entries.len() > HISTORY_LEN {
        entries.truncate(HISTORY_LEN);
    }
}
