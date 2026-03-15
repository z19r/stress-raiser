//! Beautiful TUI with stats, dials, and sparklines.

mod dashboard;
mod form;

pub use form::run_form;

use crate::curl::CurlRequest;
use crate::stats::Stats;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Theme: sharp, readable, high contrast.
pub(super) const BG: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(12, 12, 14);
pub(super) const SURFACE: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(22, 22, 26);
pub(super) const BORDER: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(60, 60, 70);
pub(super) const MUTED: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(120, 120, 130);
pub(super) const FG: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(248, 248, 248);
pub(super) const ACCENT: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(0, 200, 255);
pub(super) const ACCENT2: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(100, 220, 160);
pub(super) const SUCCESS: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(80, 220, 120);
pub(super) const ERROR: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(255, 90, 90);
pub(super) const CURSOR_STYLE: (ratatui::prelude::Color, ratatui::prelude::Color) = (
    ratatui::prelude::Color::Rgb(0, 200, 255),
    ratatui::prelude::Color::Black,
);

/// Outcome of the dashboard: quit app or go back to form with current request.
#[derive(Debug)]
pub enum RunResult {
    /// User chose Esc; exit the application.
    Quit,
    /// User chose q/Backspace; return to form with (request, concurrency, rpm).
    BackToForm((CurlRequest, usize, u64)),
}

/// Run the load-test dashboard: spawns worker, runs event loop and UI until
/// quit (Esc) or back to form (q/Backspace).
pub async fn run_tui(
    request: CurlRequest,
    stats: Arc<RwLock<Stats>>,
    concurrency: Arc<RwLock<usize>>,
    rpm: Arc<RwLock<u64>>,
    running: Arc<RwLock<bool>>,
) -> anyhow::Result<RunResult> {
    let client = request.build_client()?;
    let req_template = request.build_request(&client)?;
    req_template
        .try_clone()
        .ok_or_else(|| anyhow::anyhow!("request body cannot be cloned for load test"))?;

    let stats_w = stats.clone();
    let conc_w = concurrency.clone();
    let rpm_w = rpm.clone();
    let run_w = running.clone();
    tokio::spawn(async move {
        dashboard::load_worker(client, req_template, stats_w, conc_w, rpm_w, run_w).await;
    });

    let mut terminal = ratatui::init();
    let result = dashboard::run_loop(
        &mut terminal,
        &request,
        stats.clone(),
        concurrency.clone(),
        rpm.clone(),
        running.clone(),
    )
    .await?;
    ratatui::restore();
    Ok(result)
}
