//! Post-test report screen with export options.

use crate::stats::ReportData;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Row, Table};
use std::time::Duration;

use super::{
    confirm_quit, is_too_small, render_banner, render_thin_shadow, render_too_small, RunResult,
    TestConfig, ACCENT, BG, BORDER, ERROR, FG, MUTED, SUCCESS, SURFACE,
};

pub(super) fn run_report(
    terminal: &mut ratatui::DefaultTerminal,
    report: &ReportData,
) -> anyhow::Result<RunResult> {
    let mut export_msg: Option<(String, bool)> = None;
    loop {
        terminal.draw(|f| draw_report(f, report, &export_msg))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(e) = event::read()? {
                if e.kind != KeyEventKind::Press {
                    continue;
                }
                match e.code {
                    KeyCode::Esc if confirm_quit(terminal) => {
                        return Ok(RunResult::Quit);
                    }
                    KeyCode::Enter | KeyCode::Char('q') => {
                        let dummy = TestConfig {
                            request: crate::curl::CurlRequest {
                                url: report.url.clone(),
                                method: reqwest::Method::from_bytes(report.method.as_bytes())
                                    .unwrap_or(reqwest::Method::GET),
                                headers: reqwest::header::HeaderMap::new(),
                                body: None,
                            },
                            concurrency: 10,
                            rpm: 600,
                            total_requests: None,
                            duration_secs: None,
                        };
                        return Ok(RunResult::BackToForm(Box::new(dummy)));
                    }
                    KeyCode::Char('c') => match crate::export::export_csv(report) {
                        Ok(path) => {
                            export_msg = Some((format!("CSV saved: {}", path.display()), false));
                        }
                        Err(e) => {
                            export_msg = Some((format!("CSV failed: {e}"), true));
                        }
                    },
                    KeyCode::Char('h') => match crate::export::export_html(report) {
                        Ok(path) => {
                            export_msg = Some((format!("HTML saved: {}", path.display()), false));
                        }
                        Err(e) => {
                            export_msg = Some((format!("HTML failed: {e}"), true));
                        }
                    },
                    KeyCode::Char('a') => {
                        let ascii = crate::export::export_ascii(report);
                        ratatui::restore();
                        let header_lines = crate::export::ascii_header_line_count(report);
                        let pager = std::process::Command::new("less")
                            .arg("-R")
                            .arg(format!("--header={header_lines}"))
                            .stdin(std::process::Stdio::piped())
                            .spawn();
                        match pager {
                            Ok(mut child) => {
                                use std::io::Write;
                                if let Some(ref mut stdin) = child.stdin {
                                    let _ = stdin.write_all(ascii.as_bytes());
                                }
                                drop(child.stdin.take());
                                let _ = child.wait();
                                std::process::exit(0);
                            }
                            Err(_) => {
                                println!("{ascii}");
                                std::process::exit(0);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn draw_report(f: &mut Frame, r: &ReportData, export_msg: &Option<(String, bool)>) {
    let full = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), full);

    if is_too_small(full) {
        render_too_small(f);
        return;
    }

    let area = Rect::new(full.x, full.y, full.width.saturating_sub(2), full.height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),  // banner
            Constraint::Length(1),  // title
            Constraint::Length(10), // summary
            Constraint::Length(10), // status codes + histogram
            Constraint::Length(9),  // latency over time
            Constraint::Min(4),     // errors & outliers
            Constraint::Length(3),  // footer
        ])
        .split(area);

    render_banner(f, chunks[0]);
    render_title(f, r, chunks[1]);
    render_summary(f, r, chunks[2]);

    let mid_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(chunks[3]);
    render_status_codes(f, r, mid_cols[0]);
    render_histogram(f, r, mid_cols[1]);

    render_timeline(f, r, chunks[4]);
    render_outliers(f, r, chunks[5]);
    render_footer(f, export_msg, chunks[6]);
}

// ── Title ──────────────────────────────────────────────────────────

fn render_title(f: &mut Frame, r: &ReportData, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled("  TEST REPORT  ", Style::default().fg(ACCENT).bold()),
        Span::styled(
            format!("  {}  {} ", r.method, r.url),
            Style::default().fg(MUTED),
        ),
    ]))
    .style(Style::default().bg(BG));
    f.render_widget(title, area);
}

// ── Summary ────────────────────────────────────────────────────────

fn render_summary(f: &mut Frame, r: &ReportData, area: Rect) {
    let elapsed_s = r.elapsed.as_secs();
    let elapsed_ms = r.elapsed.subsec_millis();
    let summary_text = vec![
        Line::from(vec![
            Span::styled("Total: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.total), Style::default().fg(FG).bold()),
            Span::styled("   OK: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.ok), Style::default().fg(SUCCESS).bold()),
            Span::styled("   Err: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.err), Style::default().fg(ERROR).bold()),
        ]),
        Line::from(vec![
            Span::styled("Success: ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{:.1}%", r.success_rate * 100.0),
                Style::default().fg(if r.success_rate > 0.95 {
                    SUCCESS
                } else {
                    ERROR
                }),
            ),
            Span::styled("   RPS: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.rps), Style::default().fg(ACCENT).bold()),
            Span::styled(
                format!("   Elapsed: {elapsed_s}.{elapsed_ms:03}s"),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Latency ", Style::default().fg(FG).bold()),
            Span::styled("(ms)", Style::default().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::styled("  min: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.min_latency), Style::default().fg(FG)),
            Span::styled("  p50: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.p50), Style::default().fg(FG)),
            Span::styled("  p95: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.p95), Style::default().fg(FG)),
            Span::styled("  p99: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.p99), Style::default().fg(FG)),
            Span::styled("  max: ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", r.max_latency), Style::default().fg(FG)),
        ]),
        Line::from(vec![
            Span::styled("  avg: ", Style::default().fg(MUTED)),
            Span::styled(format!("{:.1}", r.avg_latency), Style::default().fg(FG)),
        ]),
    ];
    let summary = Paragraph::new(summary_text).block(
        Block::default()
            .title(" Summary ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(SURFACE)),
    );
    render_thin_shadow(f, area, ACCENT);
    f.render_widget(summary, area);
}

// ── Status Codes ───────────────────────────────────────────────────

fn render_status_codes(f: &mut Frame, r: &ReportData, area: Rect) {
    let mut codes: Vec<_> = r.status_codes.iter().collect();
    codes.sort_by_key(|(code, _)| **code);
    let rows: Vec<Row> = codes
        .iter()
        .map(|(code, count)| {
            let color = if **code < 300 {
                SUCCESS
            } else if **code < 500 {
                ACCENT
            } else {
                ERROR
            };
            Row::new(vec![
                Span::styled(format!("{code}"), Style::default().fg(color).bold()),
                Span::styled(format!("{count}"), Style::default().fg(FG)),
            ])
        })
        .collect();
    let widths = [Constraint::Length(8), Constraint::Min(6)];
    let table = Table::new(rows, widths)
        .header(
            Row::new(vec![
                Span::styled("Status", Style::default().fg(MUTED)),
                Span::styled("Count", Style::default().fg(MUTED)),
            ])
            .style(Style::default().bold()),
        )
        .block(
            Block::default()
                .title(" Status Codes ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(SURFACE)),
        );
    render_thin_shadow(f, area, ACCENT);
    f.render_widget(table, area);
}

// ── Latency Distribution Histogram ─────────────────────────────────

struct Bucket {
    label: &'static str,
    lo: u64,
    hi: u64,
}

const BUCKETS: &[Bucket] = &[
    Bucket {
        label: "  0-10ms",
        lo: 0,
        hi: 10,
    },
    Bucket {
        label: " 10-25ms",
        lo: 10,
        hi: 25,
    },
    Bucket {
        label: " 25-50ms",
        lo: 25,
        hi: 50,
    },
    Bucket {
        label: "50-100ms",
        lo: 50,
        hi: 100,
    },
    Bucket {
        label: " 100-250",
        lo: 100,
        hi: 250,
    },
    Bucket {
        label: "  250ms+",
        lo: 250,
        hi: u64::MAX,
    },
];

fn render_histogram(f: &mut Frame, r: &ReportData, area: Rect) {
    let mut counts = vec![0u64; BUCKETS.len()];
    for req in &r.requests {
        for (i, b) in BUCKETS.iter().enumerate() {
            if req.latency_ms >= b.lo && req.latency_ms < b.hi {
                counts[i] += 1;
                break;
            }
        }
    }
    let max_count = counts.iter().copied().max().unwrap_or(1).max(1);

    let inner_w = area.width.saturating_sub(4);
    let label_w: u16 = 10;
    let count_w: u16 = 7;
    let bar_w = inner_w.saturating_sub(label_w + count_w + 1);

    let mut lines: Vec<Line> = Vec::new();
    for (i, b) in BUCKETS.iter().enumerate() {
        let c = counts[i];
        let filled = if max_count > 0 {
            ((c as f64 / max_count as f64) * bar_w as f64).round() as u16
        } else {
            0
        };
        let bar: String = "\u{2588}".repeat(filled as usize);
        let pad: String = " ".repeat(bar_w.saturating_sub(filled) as usize);

        let color = if b.hi <= 25 || (b.hi == u64::MAX && b.lo <= 10) {
            SUCCESS
        } else if b.hi <= 100 {
            ACCENT
        } else {
            ERROR
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", b.label), Style::default().fg(MUTED)),
            Span::styled(bar, Style::default().fg(color)),
            Span::raw(pad),
            Span::styled(format!(" {:>5}", c), Style::default().fg(FG)),
        ]));
    }

    let para = Paragraph::new(lines).block(
        Block::default()
            .title(" Latency Distribution ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(SURFACE)),
    );
    render_thin_shadow(f, area, ACCENT);
    f.render_widget(para, area);
}

// ── Latency Over Time ──────────────────────────────────────────────

fn render_timeline(f: &mut Frame, r: &ReportData, area: Rect) {
    let inner_h = area.height.saturating_sub(4) as usize;
    let inner_w = area.width.saturating_sub(4) as usize;

    if r.requests.is_empty() || inner_w < 10 || inner_h < 2 {
        let empty = Paragraph::new("  No request data").block(
            Block::default()
                .title(" Latency Over Time ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(SURFACE)),
        );
        render_thin_shadow(f, area, ACCENT);
        f.render_widget(empty, area);
        return;
    }

    let y_label_w: usize = 7;
    let plot_w = inner_w.saturating_sub(y_label_w);
    let plot_h = inner_h;

    let max_elapsed = r
        .requests
        .iter()
        .map(|rq| rq.elapsed_ms)
        .max()
        .unwrap_or(1)
        .max(1);
    let max_latency = r
        .requests
        .iter()
        .map(|rq| rq.latency_ms)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut grid: Vec<Vec<(char, Style)>> = vec![vec![(' ', Style::default()); plot_w]; plot_h];

    for req in &r.requests {
        let col = ((req.elapsed_ms as f64 / max_elapsed as f64) * plot_w.saturating_sub(1) as f64)
            .round() as usize;
        let row_idx = plot_h.saturating_sub(1).saturating_sub(
            ((req.latency_ms as f64 / max_latency as f64) * plot_h.saturating_sub(1) as f64).round()
                as usize,
        );

        let col = col.min(plot_w.saturating_sub(1));
        let row_idx = row_idx.min(plot_h.saturating_sub(1));

        let color = if !req.ok || req.status >= 500 {
            ERROR
        } else if req.status >= 400 {
            ACCENT
        } else {
            SUCCESS
        };

        let marker = if !req.ok || req.status >= 400 {
            '×'
        } else {
            '·'
        };
        grid[row_idx][col] = (marker, Style::default().fg(color));
    }

    let elapsed_secs = max_elapsed as f64 / 1000.0;

    let mut lines: Vec<Line> = Vec::new();
    for (row_i, row) in grid.iter().enumerate() {
        let show_label = row_i == 0 || row_i == plot_h / 2 || row_i == plot_h.saturating_sub(1);

        let label = if show_label {
            let frac = 1.0 - (row_i as f64 / plot_h.saturating_sub(1).max(1) as f64);
            let y_val = (frac * max_latency as f64).round() as u64;
            format!("{:>5}\u{2502}", fmt_lat(y_val))
        } else {
            " ".repeat(5) + "\u{2502}"
        };

        let mut spans = vec![Span::styled(label, Style::default().fg(MUTED))];
        for &(ch, style) in row {
            spans.push(Span::styled(String::from(ch), style));
        }
        lines.push(Line::from(spans));
    }

    let mut x_axis = " ".repeat(5) + "\u{2514}";
    for _ in 0..plot_w {
        x_axis.push('\u{2500}');
    }
    lines.push(Line::from(Span::styled(x_axis, Style::default().fg(MUTED))));

    let x_label = format!(
        "      0s{:>w$}",
        format!("{:.0}s", elapsed_secs),
        w = plot_w.saturating_sub(2)
    );
    lines.push(Line::from(Span::styled(
        x_label,
        Style::default().fg(MUTED),
    )));

    let para = Paragraph::new(lines).block(
        Block::default()
            .title(" Latency Over Time ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(SURFACE)),
    );
    render_thin_shadow(f, area, ACCENT);
    f.render_widget(para, area);
}

fn fmt_lat(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

// ── Errors & Outliers ──────────────────────────────────────────────

fn render_outliers(f: &mut Frame, r: &ReportData, area: Rect) {
    let max_rows = area.height.saturating_sub(4) as usize;

    let mut errors: Vec<_> = r.requests.iter().filter(|req| !req.ok).collect();
    errors.sort_by_key(|req| std::cmp::Reverse(req.latency_ms));

    let mut slowest: Vec<_> = r.requests.iter().filter(|req| req.ok).collect();
    slowest.sort_by_key(|req| std::cmp::Reverse(req.latency_ms));

    let error_count = errors.len();
    let error_take = errors.len().min(max_rows);
    let slow_take = max_rows.saturating_sub(error_take).min(slowest.len());

    let combined: Vec<_> = errors
        .into_iter()
        .take(error_take)
        .chain(slowest.into_iter().take(slow_take))
        .collect();

    let body_w = area.width.saturating_sub(40).max(6) as usize;

    let rows: Vec<Row> = combined
        .iter()
        .map(|req| {
            let status_color = if req.status >= 500 {
                ERROR
            } else if req.status >= 400 {
                ACCENT
            } else {
                SUCCESS
            };
            let elapsed_s = req.elapsed_ms as f64 / 1000.0;
            let body: String = req.body_preview.chars().take(body_w).collect();
            Row::new(vec![
                Span::styled(format!("{:>5}", req.seq), Style::default().fg(MUTED)),
                Span::styled(format!("{:>7.1}s", elapsed_s), Style::default().fg(MUTED)),
                Span::styled(
                    format!("  {:>3}", req.status),
                    Style::default().fg(status_color).bold(),
                ),
                Span::styled(
                    format!("  {:>6}ms", req.latency_ms),
                    Style::default().fg(FG),
                ),
                Span::styled(format!("  {body}"), Style::default().fg(MUTED)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(9),
        Constraint::Min(6),
    ];

    let title = if error_count > 0 {
        format!(
            " Errors & Outliers \u{2500} {} err, {} slowest ",
            error_count, slow_take
        )
    } else if slow_take > 0 {
        format!(" Slowest Requests \u{2500} top {} ", slow_take)
    } else {
        " Errors & Outliers ".to_string()
    };

    let table = Table::new(rows, widths)
        .header(
            Row::new(vec![
                Span::styled("  #  ", Style::default().fg(MUTED)),
                Span::styled("     @  ", Style::default().fg(MUTED)),
                Span::styled(" Code", Style::default().fg(MUTED)),
                Span::styled("  Latency", Style::default().fg(MUTED)),
                Span::styled("  Body", Style::default().fg(MUTED)),
            ])
            .style(Style::default().bold()),
        )
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(SURFACE)),
        );
    render_thin_shadow(f, area, ACCENT);
    f.render_widget(table, area);
}

// ── Footer ─────────────────────────────────────────────────────────

fn render_footer(f: &mut Frame, export_msg: &Option<(String, bool)>, area: Rect) {
    let mut footer_spans = vec![
        Span::styled(" [c]", Style::default().fg(ACCENT).bold()),
        Span::styled(" CSV  ", Style::default().fg(FG)),
        Span::styled("[h]", Style::default().fg(ACCENT).bold()),
        Span::styled(" HTML  ", Style::default().fg(FG)),
        Span::styled("[a]", Style::default().fg(ACCENT).bold()),
        Span::styled(" ASCII  ", Style::default().fg(FG)),
        Span::styled("[Enter]", Style::default().fg(ACCENT).bold()),
        Span::styled(" back  ", Style::default().fg(FG)),
        Span::styled("[Esc]", Style::default().fg(ACCENT).bold()),
        Span::styled(" quit", Style::default().fg(FG)),
    ];
    if let Some((msg, is_err)) = export_msg {
        footer_spans.push(Span::styled(
            format!("  {msg}"),
            Style::default().fg(if *is_err { ERROR } else { SUCCESS }),
        ));
    }
    let footer = Paragraph::new(Line::from(footer_spans))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(BG)),
        );
    f.render_widget(footer, area);
}
