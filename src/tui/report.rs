//! Post-test report screen with export options.

use crate::stats::ReportData;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Row, Table};
use std::time::Duration;

use super::{
    confirm_quit, render_banner, render_thin_shadow, RunResult, TestConfig, ACCENT, BG, BORDER,
    ERROR, FG, MUTED, SUCCESS, SURFACE,
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
    let area = Rect::new(full.x, full.y, full.width.saturating_sub(2), full.height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(1),
            Constraint::Length(10),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);

    render_banner(f, chunks[0]);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("  TEST REPORT  ", Style::default().fg(ACCENT).bold()),
        Span::styled(
            format!("  {}  {} ", r.method, r.url),
            Style::default().fg(MUTED),
        ),
    ]))
    .style(Style::default().bg(BG));
    f.render_widget(title, chunks[1]);

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
    render_thin_shadow(f, chunks[2], ACCENT);
    f.render_widget(summary, chunks[2]);

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
    let widths = [Constraint::Length(8), Constraint::Min(10)];
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
    render_thin_shadow(f, chunks[3], ACCENT);
    f.render_widget(table, chunks[3]);

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
    f.render_widget(footer, chunks[4]);
}
