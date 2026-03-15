//! Load tester: form (URL, method, headers, body), live TUI, circuit breaker, and sparklines.
//!
//! Two-phase flow: form (URL, method, headers, body, concurrency, RPM) then
//! dashboard (live load test). Keybindings: form Tab/Enter/Esc; dashboard
//! ↑/↓ conc, PgUp/PgDn rpm, q/Backspace back, Esc quit.

pub mod curl;
pub mod editor;
pub mod error;
pub mod history;
pub mod stats;
pub mod tui;

pub use curl::CurlRequest;
pub use editor::Editor;
pub use error::AppError;
pub use history::{add_to_history, load_history, save_history, HistoryEntry};
pub use stats::{CircuitBreaker, CircuitState, ResponseLogEntry, Stats};
pub use tui::{run_form, run_tui, RunResult};
