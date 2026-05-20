//! Load-test dashboard: event loop, worker, and UI (stats, circuit breaker, charts).

use crate::curl::CurlRequest;
use crate::stats::{CircuitState, Stats};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    symbols,
    widgets::{
        canvas::{Canvas, Line as CanvasLine},
        Bar, BarChart, BarGroup, Block, BorderType, Borders, Cell, Gauge, Paragraph, Row, Table,
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::{
    confirm_quit, render_banner, render_thin_shadow, ACCENT, ACCENT2, BG, BORDER, ERROR, FG, MUTED,
    SUCCESS, SURFACE,
};
use super::{RunResult, TestConfig};

#[allow(clippy::too_many_arguments)]
pub(super) async fn load_worker(
    client: reqwest::Client,
    req_template: reqwest::Request,
    stats: Arc<RwLock<Stats>>,
    concurrency: Arc<RwLock<usize>>,
    rpm: Arc<RwLock<u64>>,
    running: Arc<RwLock<bool>>,
    total_limit: Option<u64>,
    duration_limit: Option<Duration>,
) {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let mut next_tick = Instant::now();

    loop {
        if !*running.read().await {
            break;
        }

        {
            let st = stats.read().await;
            let hit_total = total_limit.is_some_and(|lim| st.total >= lim);
            let hit_duration = duration_limit.is_some_and(|dur| st.test_elapsed >= dur);
            if hit_total || hit_duration {
                drop(st);
                *running.write().await = false;
                break;
            }
        }

        let r = *rpm.read().await;
        let conc = *concurrency.read().await;
        if r == 0 || conc == 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }

        let now = Instant::now();
        let can_send = {
            let mut st = stats.write().await;
            st.circuit.can_send(now)
        };
        if !can_send {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }

        let is_probe = {
            let st = stats.read().await;
            matches!(
                st.circuit.state,
                CircuitState::HalfOpen { probe_sent: false }
            )
        };
        if is_probe {
            stats.write().await.circuit.mark_probe_sent();
        }

        if in_flight.load(Ordering::Relaxed) >= conc {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }

        let interval_ms = 60_000 / r.max(1);
        let interval = Duration::from_millis(interval_ms);
        if next_tick.elapsed() < interval {
            tokio::time::sleep(interval - next_tick.elapsed()).await;
        }
        next_tick = Instant::now();

        in_flight.fetch_add(1, Ordering::Relaxed);
        let client = client.clone();
        let req = req_template
            .try_clone()
            .expect("cloneable (checked at start)");
        let stats = stats.clone();
        let in_flight_clone = in_flight.clone();

        tokio::spawn(async move {
            let start = Instant::now();
            let (ok, status, body_preview) = match client.execute(req).await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let ok = resp.status().is_success();
                    let body = resp.text().await.unwrap_or_default();
                    let preview = body
                        .chars()
                        .filter(|c| !c.is_control() || *c == '\n')
                        .take(200)
                        .collect::<String>()
                        .replace('\n', " ");
                    (ok, status, preview)
                }
                Err(_) => (false, 0, "request failed".into()),
            };
            let elapsed = start.elapsed().as_millis() as u64;
            stats
                .write()
                .await
                .record(ok, elapsed, status, body_preview);
            in_flight_clone.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    request: &CurlRequest,
    stats: Arc<RwLock<Stats>>,
    concurrency: Arc<RwLock<usize>>,
    rpm: Arc<RwLock<u64>>,
    running: Arc<RwLock<bool>>,
    total_limit: Option<u64>,
    duration_limit: Option<Duration>,
) -> anyhow::Result<RunResult> {
    let (tx, rx) = mpsc::channel();
    let rx = Arc::new(Mutex::new(rx));
    let _event_thread = thread::spawn(move || loop {
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(ev) = event::read() {
                if tx.send(ev).is_err() {
                    break;
                }
            }
        }
    });

    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let poll_timeout = Duration::from_millis(50);
    let request_clone = request.clone();
    let mut tick_count: u64 = 0;

    loop {
        stats.write().await.tick_rps();

        let ev_opt = {
            let rx = rx.clone();
            tokio::task::spawn_blocking(move || {
                rx.lock()
                    .ok()
                    .and_then(|r| r.recv_timeout(poll_timeout).ok())
            })
            .await
            .ok()
            .flatten()
        };

        if let Some(Event::Key(e)) = ev_opt {
            if e.kind == KeyEventKind::Press {
                if let Some(action) = handle_key(e, &concurrency, &rpm, &running).await {
                    match action {
                        RunResult::Quit => {
                            if confirm_quit(terminal) {
                                return Ok(RunResult::Quit);
                            }
                        }
                        other => return Ok(other),
                    }
                }
            }
        }

        tick.tick().await;

        let st = stats.read().await.clone();
        let conc = *concurrency.read().await;
        let rpm_val = *rpm.read().await;
        tick_count += 1;
        terminal.draw(|f| {
            ui(
                f,
                &request_clone,
                &st,
                conc,
                rpm_val,
                total_limit,
                duration_limit,
                tick_count,
            )
        })?;

        if !*running.read().await {
            return Ok(RunResult::BackToForm(Box::new(TestConfig {
                request: request_clone.clone(),
                concurrency: conc,
                rpm: rpm_val,
                total_requests: total_limit,
                duration_secs: duration_limit.map(|d| d.as_secs()),
            })));
        }
    }
}

async fn handle_key(
    e: KeyEvent,
    concurrency: &Arc<RwLock<usize>>,
    rpm: &Arc<RwLock<u64>>,
    running: &Arc<RwLock<bool>>,
) -> Option<RunResult> {
    match e.code {
        KeyCode::Esc => return Some(RunResult::Quit),
        KeyCode::Char('q') | KeyCode::Backspace => {
            *running.write().await = false;
            return None;
        }
        KeyCode::Up => {
            let mut c = concurrency.write().await;
            *c = (*c + 10).min(500);
        }
        KeyCode::Down => {
            let mut c = concurrency.write().await;
            *c = (*c).saturating_sub(10).max(1);
        }
        KeyCode::PageUp => {
            let mut r = rpm.write().await;
            *r = (*r + 10).min(60_000);
        }
        KeyCode::PageDown => {
            let mut r = rpm.write().await;
            *r = (*r).saturating_sub(10).max(60);
        }
        _ => {}
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn ui(
    f: &mut Frame,
    request: &CurlRequest,
    stats: &Stats,
    conc: usize,
    rpm_val: u64,
    total_limit: Option<u64>,
    duration_limit: Option<Duration>,
    tick_count: u64,
) {
    let circuit_active = !matches!(stats.circuit.state, CircuitState::Closed);
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), area);
    let area = Rect::new(area.x, area.y, area.width.saturating_sub(2), area.height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // 0: banner
            Constraint::Length(1), // 1: method + url
            Constraint::Length(4), // 2: circuit breaker
            Constraint::Length(6), // 3: stats + status codes (side-by-side)
            Constraint::Min(4),    // 4: RPS braille chart (full width, grows)
            Constraint::Length(7), // 5: response log table
            Constraint::Length(3), // 6: success rate gauge
            Constraint::Length(3), // 7: conc + rpm dials
            Constraint::Length(1), // 8: footer
        ])
        .split(area);

    // ── 0: Banner ──
    render_banner(f, chunks[0]);

    // ── 1: Method + URL ──
    let url = truncate(&request.url, chunks[1].width as usize - 12);
    let method = request.method.as_str();
    let method_url = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("  {} ", method),
            Style::default()
                .fg(BG)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {}", url), Style::default().fg(MUTED)),
    ]))
    .style(Style::default().bg(BG));
    f.render_widget(method_url, chunks[1]);

    // ── 1: Circuit Breaker ──
    let now = std::time::Instant::now();
    let (cb_color, border_color) = match stats.circuit.state {
        CircuitState::Closed => (SUCCESS, BORDER),
        CircuitState::Open { .. } => {
            let pulse = if tick_count % 4 < 2 { ERROR } else { ACCENT };
            (ERROR, pulse)
        }
        CircuitState::HalfOpen { .. } => (ACCENT2, ACCENT2),
    };
    let (lever, lever_desc) = match stats.circuit.state {
        CircuitState::Closed => ("▲", "CLOSED"),
        CircuitState::Open { .. } => ("▼", "OPEN"),
        CircuitState::HalfOpen { .. } => ("◐", "HALF-OPEN"),
    };
    let bad = stats.circuit.consecutive_bad;
    let threshold = crate::stats::CIRCUIT_CONSECUTIVE_THRESHOLD;
    let cb_detail: Vec<Span> = match stats.circuit.state {
        CircuitState::Closed => {
            vec![
                Span::styled(
                    format!("errors: {bad}/{threshold}"),
                    Style::default().fg(if bad > 0 { ACCENT } else { MUTED }),
                ),
                Span::styled("  sending requests", Style::default().fg(MUTED)),
            ]
        }
        CircuitState::Open { open_until } => {
            let rem = if now >= open_until {
                0
            } else {
                open_until.duration_since(now).as_secs()
            };
            let fib = crate::stats::fib_secs(stats.circuit.open_count);
            vec![
                Span::styled(
                    format!("tripped at {bad} errors"),
                    Style::default().fg(ERROR),
                ),
                Span::styled(format!("  backoff: {fib}s"), Style::default().fg(MUTED)),
                Span::styled(format!("  retry in {rem}s"), Style::default().fg(ACCENT)),
            ]
        }
        CircuitState::HalfOpen { probe_sent } => {
            if probe_sent {
                vec![Span::styled(
                    "probe sent, awaiting response…",
                    Style::default().fg(ACCENT2),
                )]
            } else {
                vec![Span::styled(
                    "sending 1 probe request…",
                    Style::default().fg(ACCENT2),
                )]
            }
        }
    };
    let mut cb_spans = vec![
        Span::styled(
            format!(" {} {} ", lever, lever_desc),
            Style::default().fg(cb_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(BORDER)),
    ];
    cb_spans.extend(cb_detail);
    let cb_para = Paragraph::new(Line::from(cb_spans)).block(
        Block::default()
            .title(" Circuit Breaker ")
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(SURFACE)),
    );
    render_thin_shadow(f, chunks[2], if circuit_active { ERROR } else { ACCENT });
    f.render_widget(cb_para, chunks[2]);

    // ── 2: Stats + Status Codes side-by-side ──
    let stats_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[3]);

    let stats_text = vec![
        Line::from(vec![
            Span::styled("Total ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", stats.total), Style::default().fg(FG)),
            Span::styled("  OK ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", stats.ok), Style::default().fg(SUCCESS)),
            Span::styled("  Err ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", stats.err), Style::default().fg(ERROR)),
        ]),
        Line::from(vec![
            Span::styled("RPS ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{}", stats.rps()),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  p50 ", Style::default().fg(MUTED)),
            Span::styled(format!("{}ms", stats.p50()), Style::default().fg(ACCENT2)),
            Span::styled("  p95 ", Style::default().fg(MUTED)),
            Span::styled(format!("{}ms", stats.p95()), Style::default().fg(ACCENT2)),
            Span::styled("  p99 ", Style::default().fg(MUTED)),
            Span::styled(format!("{}ms", stats.p99()), Style::default().fg(ACCENT2)),
        ]),
        Line::from(vec![
            Span::styled("Elapsed ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{:.1}s", stats.test_elapsed.as_secs_f64()),
                Style::default().fg(FG),
            ),
        ]),
    ];
    let stats_block = Paragraph::new(stats_text)
        .block(
            Block::default()
                .title(" Stats ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(SURFACE)),
        )
        .style(Style::default().fg(FG));
    let stats_rect = inset(stats_cols[0], 2, 1);
    render_thin_shadow(f, stats_rect, ACCENT);
    f.render_widget(stats_block, stats_rect);

    // Status code BarChart
    let mut codes: Vec<(String, u64)> = stats
        .status_codes
        .iter()
        .map(|(&code, &count)| (code.to_string(), count))
        .collect();
    codes.sort_by_key(|(c, _)| c.clone());
    let codes_rect = inset(stats_cols[1], 0, 1);
    render_thin_shadow(f, codes_rect, ACCENT);
    if codes.is_empty() {
        let placeholder =
            Paragraph::new(Span::styled("(no responses)", Style::default().fg(MUTED))).block(
                Block::default()
                    .title(" Status Codes ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(BORDER))
                    .style(Style::default().bg(SURFACE)),
            );
        f.render_widget(placeholder, codes_rect);
    } else {
        let bars: Vec<Bar> = codes
            .iter()
            .map(|(label, val)| {
                let code: u16 = label.parse().unwrap_or(0);
                let color = if code < 300 {
                    SUCCESS
                } else if code < 400 {
                    ACCENT2
                } else if code < 500 {
                    ACCENT
                } else {
                    ERROR
                };
                Bar::default()
                    .value(*val)
                    .label(Line::from(label.clone()))
                    .style(Style::default().fg(color))
                    .value_style(
                        Style::default()
                            .fg(BG)
                            .bg(color)
                            .add_modifier(Modifier::BOLD),
                    )
            })
            .collect();
        let chart = BarChart::default()
            .block(
                Block::default()
                    .title(" Status Codes ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(BORDER))
                    .style(Style::default().bg(SURFACE)),
            )
            .data(BarGroup::default().bars(&bars))
            .bar_width(5)
            .bar_gap(1)
            .direction(Direction::Vertical);
        f.render_widget(chart, codes_rect);
    }

    // ── 3: RPS Braille Chart (full width) ──
    let spark_data = stats.sparkline_data();
    let max_rps = spark_data.iter().copied().max().unwrap_or(1).max(1) as f64;
    let data_len = spark_data.len();
    let rps_block = Block::default()
        .title(format!(" RPS  (max: {}) ", max_rps as u64))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    let y_top = max_rps * 1.1;
    let rps_canvas = Canvas::default()
        .block(rps_block)
        .x_bounds([0.0, data_len as f64])
        .y_bounds([0.0, y_top])
        .marker(symbols::Marker::Braille)
        .paint(move |ctx| {
            if data_len < 2 {
                return;
            }
            let baseline = max_rps * 0.5;
            ctx.draw(&CanvasLine::new(
                0.0,
                baseline,
                data_len as f64,
                baseline,
                BORDER,
            ));
            for i in 1..data_len {
                let x0 = i as f64 - 1.0;
                let x1 = i as f64;
                let y0 = spark_data[i - 1] as f64;
                let y1 = spark_data[i] as f64;
                let color = if y1 < baseline { ERROR } else { ACCENT };
                ctx.draw(&CanvasLine::new(x0, y0, x1, y1, color));
            }
        });

    let chart_area = chunks[4];
    let chart_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(8), Constraint::Min(1)])
        .split(chart_area);

    let label_height = chart_cols[0].height;
    let top_label = format!("{} rps", y_top as u64);
    let mid_label = format!("{}", (max_rps * 0.5) as u64);
    let mut label_lines: Vec<Line> = Vec::new();
    if label_height >= 3 {
        label_lines.push(
            Line::from(Span::styled(&top_label, Style::default().fg(MUTED)))
                .alignment(Alignment::Right),
        );
        let pad = (label_height - 3) / 2;
        for _ in 0..pad {
            label_lines.push(Line::from(""));
        }
        label_lines.push(
            Line::from(Span::styled(&mid_label, Style::default().fg(MUTED)))
                .alignment(Alignment::Right),
        );
        let remaining = label_height.saturating_sub(label_lines.len() as u16 + 1);
        for _ in 0..remaining {
            label_lines.push(Line::from(""));
        }
        label_lines.push(
            Line::from(Span::styled("0", Style::default().fg(MUTED))).alignment(Alignment::Right),
        );
    }
    let y_axis = Paragraph::new(label_lines).style(Style::default().bg(BG));
    f.render_widget(y_axis, chart_cols[0]);

    let rps_rect = inset(chart_cols[1], 0, 1);
    render_thin_shadow(f, rps_rect, ACCENT);
    f.render_widget(rps_canvas, rps_rect);

    // ── 4: Response Log Table ──
    let log_block = Block::default()
        .title(" Response Log ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    let log_rect = inset(chunks[5], 0, 1);
    render_thin_shadow(f, log_rect, ACCENT);
    if stats.response_log.is_empty() {
        let placeholder = Paragraph::new(Span::styled(
            " (responses will appear here)",
            Style::default().fg(MUTED),
        ))
        .block(log_block);
        f.render_widget(placeholder, log_rect);
    } else {
        let max_rows = (log_rect.height.saturating_sub(3)) as usize;
        let rows: Vec<Row> = stats
            .response_log
            .iter()
            .rev()
            .take(max_rows)
            .enumerate()
            .map(|(i, e)| {
                let status_color = if e.ok { SUCCESS } else { ERROR };
                let preview = truncate(&e.body_preview, 50);
                let bg = if i % 2 == 0 { SURFACE } else { BG };
                Row::new(vec![
                    Cell::from(Span::styled(
                        format!("{}", e.status),
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Cell::from(Span::styled(
                        format!("{}ms", e.latency_ms),
                        Style::default().fg(ACCENT2),
                    )),
                    Cell::from(Span::styled(preview, Style::default().fg(MUTED))),
                ])
                .style(Style::default().bg(bg))
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Length(6),
                Constraint::Length(8),
                Constraint::Min(20),
            ],
        )
        .header(
            Row::new(vec![
                Cell::from(Span::styled(
                    "Code",
                    Style::default().fg(FG).add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    "Latency",
                    Style::default().fg(FG).add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    "Preview",
                    Style::default().fg(FG).add_modifier(Modifier::BOLD),
                )),
            ])
            .style(Style::default().bg(SURFACE))
            .bottom_margin(0),
        )
        .block(log_block);
        f.render_widget(table, log_rect);
    }

    // ── 5: Success Rate Gauge ──
    let success = stats.success_rate();
    let success_gauge = Gauge::default()
        .gauge_style(if success >= 0.99 {
            Style::default().fg(SUCCESS).bg(BG)
        } else if success >= 0.9 {
            Style::default().fg(ACCENT).bg(BG)
        } else {
            Style::default().fg(ERROR).bg(BG)
        })
        .block(
            Block::default()
                .title(" Success Rate ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(SURFACE)),
        )
        .ratio(success)
        .label(Span::styled(
            format!("{:.1}%", success * 100.0),
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ));
    render_thin_shadow(f, chunks[6], ACCENT);
    f.render_widget(success_gauge, chunks[6]);

    // ── 6: Concurrency + RPM Dials ──
    let dial_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[7]);

    let conc_pct = (conc as f64 / 500.0).min(1.0);
    let conc_gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT).bg(BG))
        .block(
            Block::default()
                .title(" Concurrency ↑/↓ ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(SURFACE)),
        )
        .ratio(conc_pct)
        .label(Span::styled(
            format!("{} workers", conc),
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ));
    let conc_rect = inset(dial_chunks[0], 2, 0);
    render_thin_shadow(f, conc_rect, ACCENT);
    f.render_widget(conc_gauge, conc_rect);

    let rpm_pct = (rpm_val as f64 / 6000.0).min(1.0);
    let rpm_gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT2).bg(BG))
        .block(
            Block::default()
                .title(" RPM PgUp/PgDn ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER))
                .style(Style::default().bg(SURFACE)),
        )
        .ratio(rpm_pct)
        .label(Span::styled(
            format!("{} req/min", rpm_val),
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ));
    let rpm_rect = inset(dial_chunks[1], 0, 0);
    render_thin_shadow(f, rpm_rect, ACCENT2);
    f.render_widget(rpm_gauge, rpm_rect);

    // ── 7: Footer ──
    let mut limit_spans: Vec<Span> = Vec::new();
    if let Some(lim) = total_limit {
        limit_spans.push(Span::styled(
            format!(" {}/{} reqs", stats.total, lim),
            Style::default().fg(ACCENT2),
        ));
        if stats.total >= lim {
            limit_spans.push(Span::styled(" DONE", Style::default().fg(SUCCESS)));
        }
    }
    if let Some(dur) = duration_limit {
        limit_spans.push(Span::styled(
            format!(
                " {:.0}s/{:.0}s",
                stats.test_elapsed.as_secs_f64(),
                dur.as_secs_f64()
            ),
            Style::default().fg(ACCENT2),
        ));
        if stats.test_elapsed >= dur {
            limit_spans.push(Span::styled(" DONE", Style::default().fg(SUCCESS)));
        }
    }

    let mut footer_spans = vec![
        Span::styled(" ↑/↓ conc  PgUp/Dn rpm  ", Style::default().fg(MUTED)),
        Span::styled(
            "q",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" back  ", Style::default().fg(MUTED)),
        Span::styled(
            "Esc",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" quit", Style::default().fg(MUTED)),
    ];
    footer_spans.extend(limit_spans);
    let footer_para = Paragraph::new(Line::from(footer_spans))
        .style(Style::default().bg(BG))
        .alignment(Alignment::Center);
    f.render_widget(footer_para, chunks[8]);
}

fn inset(r: Rect, right: u16, bottom: u16) -> Rect {
    Rect {
        width: r.width.saturating_sub(right),
        height: r.height.saturating_sub(bottom),
        ..r
    }
}

fn truncate(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max <= 1 {
        s.to_string()
    } else {
        let skip = n - (max - 1);
        let start = s
            .char_indices()
            .nth(skip)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("…{}", &s[start..])
    }
}
