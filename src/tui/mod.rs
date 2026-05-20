//! Beautiful TUI with stats, dials, and sparklines.

mod dashboard;
mod form;
mod report;

pub use form::run_form;

use crate::curl::CurlRequest;
use crate::stats::Stats;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Whetstone design system — semantic token map (Default / on Void).
///   Surface  = Void #07030E    Foreground = Bone #F5F2E8
///   Muted    = Mist #DCD0FF    Accent     = Acid #D6FF2E
///   AccentHot= Magenta #FF2E93 Rule       = Lilac #A78BFA
pub(super) const BG: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(7, 3, 14); // Void
pub(super) const SURFACE: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(17, 6, 40); // Obsidian
pub(super) const BORDER: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(167, 139, 250); // Lilac (Rule)
pub(super) const MUTED: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(220, 208, 255); // Mist
pub(super) const FG: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(245, 242, 232); // Bone
pub(super) const ACCENT: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(214, 255, 46); // Acid
pub(super) const ACCENT2: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(167, 139, 250); // Lilac
pub(super) const SUCCESS: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(214, 255, 46); // Acid
pub(super) const ERROR: ratatui::prelude::Color = ratatui::prelude::Color::Rgb(255, 46, 147); // Magenta
pub(super) const CURSOR_STYLE: (ratatui::prelude::Color, ratatui::prelude::Color) = (
    ratatui::prelude::Color::Rgb(214, 255, 46),
    ratatui::prelude::Color::Rgb(7, 3, 14),
);

/// Render the big blocky "STRESS RAISER" banner.  Needs Length(4) constraint.
pub(super) fn render_banner(f: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
    use ratatui::prelude::*;
    use ratatui::widgets::Paragraph;
    let w = area.width as usize;
    let trim = |s: &str| -> String {
        if s.len() <= w {
            s.to_string()
        } else {
            s[..w].to_string()
        }
    };
    let banner = vec![
        Line::from(Span::styled(
            trim("█▀ ▀█▀ █▀▄ █▀▀ █▀ █▀   █▀▄ ▄▀█ █ █▀ █▀▀ █▀▄"),
            Style::default().fg(ACCENT),
        )),
        Line::from(Span::styled(
            trim("▄█  █  █▀▄ ██▄ ▄█ ▄█   █▀▄ █▀█ █ ▄█ ██▄ █▀▄"),
            Style::default().fg(ACCENT),
        )),
        Line::from(Span::styled(
            trim(&"─".repeat(w.clamp(1, 48))),
            Style::default().fg(BORDER),
        )),
    ];
    f.render_widget(
        Paragraph::new(banner)
            .alignment(Alignment::Center)
            .style(Style::default().bg(BG)),
        area,
    );
}

/// Thin half-cell drop shadow (right edge + bottom edge).
pub(super) fn render_thin_shadow(
    f: &mut ratatui::Frame,
    area: ratatui::prelude::Rect,
    color: ratatui::prelude::Color,
) {
    let term = f.area();

    // Right edge: ▐ (right half-block) column, offset 1 row down
    let rx = area.x + area.width;
    let ry = area.y + 1;
    if rx < term.width {
        let rh = area.height.min(term.height.saturating_sub(ry));
        for row in 0..rh {
            f.render_widget(
                ratatui::widgets::Paragraph::new("▐")
                    .style(ratatui::prelude::Style::default().fg(color).bg(BG)),
                ratatui::prelude::Rect::new(rx, ry + row, 1, 1),
            );
        }
    }

    // Bottom edge: ▀ (upper half-block) row, offset 1 col right
    let bot_y = area.y + area.height;
    let bot_x = area.x + 1;
    if bot_y < term.height && bot_x < term.width {
        let w = area.width.min(term.width.saturating_sub(bot_x));
        if w > 0 {
            f.render_widget(
                ratatui::widgets::Paragraph::new("▀".repeat(w as usize))
                    .style(ratatui::prelude::Style::default().fg(color).bg(BG)),
                ratatui::prelude::Rect::new(bot_x, bot_y, w, 1),
            );
        }
    }
}

/// Draw a centered "Are you sure?" modal and wait for y/n.  Returns true if confirmed.
pub(super) fn confirm_quit(terminal: &mut ratatui::DefaultTerminal) -> bool {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use ratatui::prelude::*;
    use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

    loop {
        let _ = terminal.draw(|f| {
            let full = f.area();
            let w: u16 = 36;
            let h: u16 = 5;
            let x = full.width.saturating_sub(w) / 2;
            let y = full.height.saturating_sub(h) / 2;
            let modal = Rect::new(x, y, w, h);

            f.render_widget(Clear, modal);

            let shadow_area = Rect::new(x, y, w, h);
            render_thin_shadow(f, shadow_area, ACCENT);

            let body = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Quit? ", Style::default().fg(FG).bold()),
                    Span::styled("[y]", Style::default().fg(ACCENT).bold()),
                    Span::styled(" yes  ", Style::default().fg(MUTED)),
                    Span::styled("[n]", Style::default().fg(ACCENT).bold()),
                    Span::styled(" no", Style::default().fg(MUTED)),
                ]),
            ];
            let para = Paragraph::new(body).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
                    .border_style(Style::default().fg(ACCENT))
                    .style(Style::default().bg(BG)),
            );
            f.render_widget(para, modal);
        });

        if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(e)) = event::read() {
                if e.kind != KeyEventKind::Press {
                    continue;
                }
                match e.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => return true,
                    _ => return false,
                }
            }
        }
    }
}

/// Config for a test run.
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub request: CurlRequest,
    pub concurrency: usize,
    pub rpm: u64,
    pub total_requests: Option<u64>,
    pub duration_secs: Option<u64>,
}

/// Outcome of the dashboard: quit app or go back to form with current request.
#[derive(Debug)]
pub enum RunResult {
    /// User chose Esc; exit the application.
    Quit,
    /// User chose q/Backspace; return to form with config.
    BackToForm(Box<TestConfig>),
}

/// Run the load-test dashboard: spawns worker, runs event loop and UI until
/// quit (Esc) or back to form (q/Backspace). Shows report screen on normal stop.
pub async fn run_tui(
    config: TestConfig,
    stats: Arc<RwLock<Stats>>,
    concurrency: Arc<RwLock<usize>>,
    rpm: Arc<RwLock<u64>>,
    running: Arc<RwLock<bool>>,
) -> anyhow::Result<RunResult> {
    let request = config.request.clone();
    let client = request.build_client()?;
    let req_template = request.build_request(&client)?;
    req_template
        .try_clone()
        .ok_or_else(|| anyhow::anyhow!("request body cannot be cloned for load test"))?;

    let total_limit = config.total_requests;
    let duration_limit = config.duration_secs.map(Duration::from_secs);

    let stats_w = stats.clone();
    let conc_w = concurrency.clone();
    let rpm_w = rpm.clone();
    let run_w = running.clone();
    tokio::spawn(async move {
        dashboard::load_worker(
            client,
            req_template,
            stats_w,
            conc_w,
            rpm_w,
            run_w,
            total_limit,
            duration_limit,
        )
        .await;
    });

    let mut terminal = ratatui::init();
    let result = dashboard::run_loop(
        &mut terminal,
        &request,
        stats.clone(),
        concurrency.clone(),
        rpm.clone(),
        running.clone(),
        total_limit,
        duration_limit,
    )
    .await?;

    match result {
        RunResult::Quit => {
            let snap = stats.read().await;
            let report = if snap.total > 0 {
                Some(snap.snapshot(&request.url, request.method.as_str()))
            } else {
                None
            };
            drop(snap);
            ratatui::restore();
            if let Some(ref r) = report {
                crate::export::print_terminal_summary(r);
            }
            Ok(RunResult::Quit)
        }
        RunResult::BackToForm(ref cfg) => {
            let snap = stats.read().await;
            if snap.total > 0 {
                let report = snap.snapshot(&request.url, request.method.as_str());
                drop(snap);
                let report_result = report::run_report(&mut terminal, &report)?;
                ratatui::restore();
                match report_result {
                    RunResult::Quit => {
                        crate::export::print_terminal_summary(&report);
                        Ok(RunResult::Quit)
                    }
                    RunResult::BackToForm(_) => Ok(RunResult::BackToForm(cfg.clone())),
                }
            } else {
                ratatui::restore();
                Ok(RunResult::BackToForm(cfg.clone()))
            }
        }
    }
}
