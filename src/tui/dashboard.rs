//! Load-test dashboard: event loop, worker, and UI (stats, circuit breaker, spiral).

use crate::curl::CurlRequest;
use crate::stats::{CircuitState, Stats};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    symbols,
    widgets::{
        canvas::{Canvas, Line as CanvasLine},
        Block, Borders, Gauge, Paragraph, Sparkline, Wrap,
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::RunResult;
use super::{ACCENT, ACCENT2, BG, ERROR, FG, MUTED, SUCCESS, SURFACE};

pub(super) async fn load_worker(
    client: reqwest::Client,
    req_template: reqwest::Request,
    stats: Arc<RwLock<Stats>>,
    concurrency: Arc<RwLock<usize>>,
    rpm: Arc<RwLock<u64>>,
    running: Arc<RwLock<bool>>,
) {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let mut next_tick = Instant::now();

    loop {
        if !*running.read().await {
            break;
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

pub(super) async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    request: &CurlRequest,
    stats: Arc<RwLock<Stats>>,
    concurrency: Arc<RwLock<usize>>,
    rpm: Arc<RwLock<u64>>,
    running: Arc<RwLock<bool>>,
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
                    return Ok(action);
                }
            }
        }

        tick.tick().await;

        let st = stats.read().await.clone();
        let conc = *concurrency.read().await;
        let rpm_val = *rpm.read().await;
        terminal.draw(|f| ui(f, &request_clone, &st, conc, rpm_val))?;

        if !*running.read().await {
            return Ok(RunResult::BackToForm((
                request_clone.clone(),
                conc,
                rpm_val,
            )));
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

fn ui(f: &mut Frame, request: &CurlRequest, stats: &Stats, conc: usize, rpm_val: u64) {
    let circuit_active = !matches!(stats.circuit.state, CircuitState::Closed);
    let area = f.area();
    const MIN_SPIRAL_WIDTH: u16 = 50;
    const MIN_SPIRAL_HEIGHT: u16 = 18;
    let show_spiral = circuit_active && area.width >= MIN_SPIRAL_WIDTH && area.height >= 42;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Min(6),
            Constraint::Length(6),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(if show_spiral { MIN_SPIRAL_HEIGHT } else { 0 }),
        ])
        .split(area);

    let url = truncate(&request.url, chunks[0].width as usize - 4);
    let method = request.method.as_str();
    let header = Paragraph::new(format!("{} {}", method, url))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(ACCENT))
                .style(Style::default().bg(BG)),
        )
        .style(Style::default().fg(FG));
    f.render_widget(header, chunks[0]);

    let now = std::time::Instant::now();
    let (_, detail) = stats.circuit.display(now);
    let (cb_color, border_color) = match stats.circuit.state {
        CircuitState::Closed => (SUCCESS, SUCCESS),
        CircuitState::Open { .. } => (ERROR, ERROR),
        CircuitState::HalfOpen { .. } => (ACCENT2, ACCENT2),
    };
    let (lever, lever_desc) = match stats.circuit.state {
        CircuitState::Closed => ("▲", "ON "),
        CircuitState::Open { .. } => ("▼", "TRIP"),
        CircuitState::HalfOpen { .. } => ("◐", "TEST"),
    };
    let bad_str = if stats.circuit.consecutive_bad > 0 {
        format!("  ({} bad in a row)", stats.circuit.consecutive_bad)
    } else {
        String::new()
    };
    let breaker_art = vec![
        Line::from(Span::styled(" ┌──┐ ", Style::default().fg(cb_color))),
        Line::from(Span::styled(
            format!(" │{} │ ", lever),
            Style::default().fg(cb_color),
        )),
        Line::from(Span::styled(" └──┘ ", Style::default().fg(cb_color))),
    ];
    let breaker_block = Paragraph::new(breaker_art).style(Style::default().fg(cb_color));
    let status_line = Line::from(vec![
        Span::styled(
            format!(" {} ", lever_desc),
            Style::default().fg(cb_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" — ", Style::default().fg(MUTED)),
        Span::styled(detail, Style::default().fg(FG)),
        Span::styled(bad_str, Style::default().fg(MUTED)),
    ]);
    let circuit_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(8), Constraint::Min(10)])
        .split(chunks[1]);
    f.render_widget(breaker_block, circuit_chunks[0]);
    let status_para = Paragraph::new(status_line)
        .block(
            Block::default()
                .title(" Circuit breaker ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(SURFACE)),
        )
        .style(Style::default().fg(FG));
    f.render_widget(status_para, circuit_chunks[1]);

    let headers = request.headers_display();
    let body = request.body_preview();
    let header_lines: Vec<Line> = headers
        .iter()
        .map(|h| Line::from(Span::styled(h, Style::default().fg(MUTED))))
        .collect();
    let body_lines: Vec<Line> = body
        .lines()
        .take(3)
        .map(|l| Line::from(Span::styled(truncate(l, 76), Style::default().fg(FG))))
        .collect();
    let req_lines = if header_lines.is_empty() && body_lines.is_empty() {
        vec![Line::from(Span::styled(
            "(no headers or body)",
            Style::default().fg(MUTED),
        ))]
    } else {
        let mut out = vec![];
        if !header_lines.is_empty() {
            out.push(Line::from(Span::styled(
                "Headers:",
                Style::default().fg(ACCENT2),
            )));
            out.extend(header_lines.into_iter().take(4));
        }
        if !body_lines.is_empty() {
            out.push(Line::from(Span::styled(
                "Body:",
                Style::default().fg(ACCENT2),
            )));
            out.extend(body_lines);
        }
        out
    };
    let req_block = Paragraph::new(req_lines)
        .block(
            Block::default()
                .title(" Request (headers + body) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SURFACE))
                .style(Style::default().bg(SURFACE)),
        )
        .wrap(Wrap::default());
    f.render_widget(req_block, chunks[2]);

    let log_lines: Vec<Line> = stats
        .response_log
        .iter()
        .rev()
        .take(5)
        .map(|e| {
            let status_color = if e.ok { SUCCESS } else { ERROR };
            let body = truncate(&e.body_preview, 70);
            Line::from(vec![
                Span::styled(format!("{} ", e.status), Style::default().fg(status_color)),
                Span::raw(body),
            ])
        })
        .collect();
    let log_block = Paragraph::new(if log_lines.is_empty() {
        vec![Line::from(Span::styled(
            "(responses will appear here)",
            Style::default().fg(MUTED),
        ))]
    } else {
        log_lines
    })
    .block(
        Block::default()
            .title(" Response log (status + body) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SURFACE))
            .style(Style::default().bg(SURFACE)),
    )
    .wrap(Wrap::default());
    f.render_widget(log_block, chunks[3]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[4]);

    let stats_text = vec![
        Line::from(vec![
            Span::styled("Total ", Style::default().fg(MUTED)),
            Span::raw(format!("{} ", stats.total)),
            Span::styled("  OK ", Style::default().fg(MUTED)),
            Span::styled(format!("{} ", stats.ok), Style::default().fg(SUCCESS)),
            Span::styled("  Err ", Style::default().fg(MUTED)),
            Span::styled(format!("{}", stats.err), Style::default().fg(ERROR)),
        ]),
        Line::from(vec![
            Span::styled("RPS ", Style::default().fg(MUTED)),
            Span::styled(format!("{} ", stats.rps()), Style::default().fg(ACCENT2)),
            Span::styled("  p50 ", Style::default().fg(MUTED)),
            Span::raw(format!("{}ms ", stats.p50())),
            Span::styled(" p95 ", Style::default().fg(MUTED)),
            Span::raw(format!("{}ms ", stats.p95())),
            Span::styled(" p99 ", Style::default().fg(MUTED)),
            Span::raw(format!("{}ms", stats.p99())),
        ]),
    ];
    let stats_block = Paragraph::new(stats_text)
        .block(
            Block::default()
                .title(" Stats ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SURFACE))
                .style(Style::default().bg(SURFACE)),
        )
        .style(Style::default().fg(FG));
    f.render_widget(stats_block, main_chunks[0]);

    let spark_data = stats.sparkline_data();
    let max = spark_data.iter().copied().max().unwrap_or(1).max(1);
    let spark = Sparkline::default()
        .block(
            Block::default()
                .title(" RPS over time ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SURFACE))
                .style(Style::default().bg(SURFACE)),
        )
        .data(&spark_data)
        .style(Style::default().fg(ACCENT2))
        .max(max);
    f.render_widget(spark, main_chunks[1]);

    let success = stats.success_rate();
    let success_gauge = Gauge::default()
        .gauge_style(if success >= 0.99 {
            Style::default().fg(SUCCESS)
        } else if success >= 0.9 {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(ERROR)
        })
        .block(
            Block::default()
                .title(" Success rate ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SURFACE))
                .style(Style::default().bg(SURFACE)),
        )
        .ratio(success)
        .label(Span::raw(format!("{:.1}%", success * 100.0)));
    f.render_widget(success_gauge, chunks[5]);

    let dial_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[6]);

    let conc_pct = (conc as f64 / 500.0 * 100.0).min(100.0);
    let conc_gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT))
        .block(
            Block::default()
                .title(" Concurrency (↑/↓) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SURFACE))
                .style(Style::default().bg(SURFACE)),
        )
        .ratio(conc_pct / 100.0)
        .label(Span::raw(format!("{} workers", conc)));
    f.render_widget(conc_gauge, dial_chunks[0]);

    let rpm_pct = (rpm_val as f64 / 6000.0 * 100.0).min(100.0);
    let rpm_gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT2))
        .block(
            Block::default()
                .title(" RPM (PgUp/PgDn) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SURFACE))
                .style(Style::default().bg(SURFACE)),
        )
        .ratio(rpm_pct / 100.0)
        .label(Span::raw(format!("{} req/min", rpm_val)));
    f.render_widget(rpm_gauge, dial_chunks[1]);

    let footer = Line::from(vec![
        Span::styled(" ↑/↓: conc  PgUp/Dn: rpm  ", Style::default().fg(MUTED)),
        Span::styled(
            "q",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("/", Style::default().fg(MUTED)),
        Span::styled("Backsp", Style::default().fg(ACCENT)),
        Span::styled(": back to form  Esc: quit ", Style::default().fg(MUTED)),
    ]);
    let footer_para = Paragraph::new(footer)
        .style(Style::default().bg(BG))
        .alignment(Alignment::Center);
    f.render_widget(footer_para, chunks[7]);

    if show_spiral
        && chunks.len() > 8
        && chunks[8].height > 0
        && chunks[8].width >= MIN_SPIRAL_WIDTH
    {
        let spiral_area = chunks[8];
        let side_cols = (spiral_area.height * 2).min(spiral_area.width);
        let x_off = (spiral_area.width.saturating_sub(side_cols)) / 2;
        let square_rect = Rect {
            x: spiral_area.x + x_off,
            y: spiral_area.y,
            width: side_cols,
            height: spiral_area.height,
        };
        draw_fib_spiral(f, square_rect, stats.circuit.open_count);
    }
}

fn draw_fib_spiral(f: &mut Frame, area: Rect, open_count: u32) {
    const FIB: [f64; 14] = [
        1., 1., 2., 3., 5., 8., 13., 21., 34., 55., 89., 144., 233., 377.,
    ];
    const CENTERS: [(f64, f64); 14] = [
        (0., 0.),
        (-1., 1.),
        (-1., -1.),
        (1., -1.),
        (1., 2.),
        (-2., 2.),
        (-2., -3.),
        (3., -3.),
        (3., 5.),
        (-5., 5.),
        (-5., -8.),
        (8., -8.),
        (8., 13.),
        (-13., 13.),
    ];
    let open_count = open_count as usize;
    let mut lines: Vec<CanvasLine> = Vec::new();
    let points_per_arc = 50usize;
    for (i, (&r, &(cx, cy))) in FIB.iter().zip(CENTERS.iter()).enumerate() {
        let start_deg = (i % 4) as f64 * 90.0;
        let end_deg = start_deg + 90.0;
        let start_rad = start_deg.to_radians();
        let end_rad = end_deg.to_radians();
        let mut px = cx + r * start_rad.cos();
        let mut py = cy + r * start_rad.sin();
        for k in 1..=points_per_arc {
            let t = k as f64 / points_per_arc as f64;
            let angle = start_rad + t * (end_rad - start_rad);
            let x = cx + r * angle.cos();
            let y = cy + r * angle.sin();
            let (r8, g8, b8): (u8, u8, u8) = if i <= open_count {
                let d = 75 + (open_count.saturating_sub(i)) * 12;
                let d = d.min(180) as u8;
                (d, d, (d + 8).min(188))
            } else {
                (58, 58, 62)
            };
            let color = Color::Rgb(r8, g8, b8);
            lines.push(CanvasLine::new(px, py, x, y, color));
            px = x;
            py = y;
        }
    }
    let x_bounds = [-18.0, 18.0];
    let y_bounds = [-18.0, 18.0];
    let canvas = Canvas::default()
        .x_bounds(x_bounds)
        .y_bounds(y_bounds)
        .marker(symbols::Marker::HalfBlock)
        .paint(move |ctx| {
            for line in &lines {
                ctx.draw(line);
            }
        });
    f.render_widget(canvas, area);
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
