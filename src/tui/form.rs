//! Form UI: URL, method, headers, body, concurrency, RPM, limits. Enter to start.

use crate::curl::CurlRequest;
use crate::editor::Editor;
use crate::error::AppError;
use crate::history::{self, HistoryEntry};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use super::{confirm_quit, render_banner, render_thin_shadow, TestConfig, ACCENT, BG, BORDER, CURSOR_STYLE, ERROR, FG, MUTED, SURFACE};

struct FormEditorsMut<'a> {
    url: &'a mut Editor,
    headers: &'a mut Editor,
    body: &'a mut Editor,
    method_idx: &'a mut usize,
    conc_edit: &'a mut Editor,
    rpm_edit: &'a mut Editor,
    total_edit: &'a mut Editor,
    duration_edit: &'a mut Editor,
}

struct FormView<'a> {
    url: &'a Editor,
    headers: &'a Editor,
    body: &'a Editor,
    conc: &'a Editor,
    rpm: &'a Editor,
    total: &'a Editor,
    duration: &'a Editor,
    method: &'a str,
    focused: FormField,
    error: &'a Option<String>,
    cursor_style: Style,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FormField {
    Url,
    Method,
    Headers,
    Body,
    Concurrency,
    Rpm,
    TotalRequests,
    DurationSecs,
    Start,
}

pub(super) const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE"];

pub(super) fn method_idx_from(req: &CurlRequest) -> usize {
    let m = req.method.as_str();
    METHODS.iter().position(|&x| x == m).unwrap_or(0)
}

pub async fn run_form(
    init: Option<TestConfig>,
    history: &mut Vec<HistoryEntry>,
) -> Result<TestConfig, AppError> {
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableBracketedPaste)?;

    let (
        mut url,
        mut headers,
        mut body,
        mut method_idx,
        mut conc_edit,
        mut rpm_edit,
        mut total_edit,
        mut duration_edit,
    ) = if let Some(cfg) = init {
        let idx = method_idx_from(&cfg.request);
        let u = cfg.request.url.replace(['\n', '\r'], " ");
        let h = cfg.request.headers_display().join("\n");
        let b = cfg
            .request
            .body
            .as_ref()
            .map_or(String::new(), |x| String::from_utf8_lossy(x).into_owned());
        let t_str = cfg
            .total_requests
            .map_or("0".to_string(), |v| v.to_string());
        let d_str = cfg
            .duration_secs
            .map_or("0".to_string(), |v| v.to_string());
        (
            Editor::new(u),
            Editor::new(h),
            Editor::new(b),
            idx,
            Editor::new(cfg.concurrency.to_string()),
            Editor::new(cfg.rpm.to_string()),
            Editor::new(t_str),
            Editor::new(d_str),
        )
    } else {
        (
            Editor::default(),
            Editor::default(),
            Editor::default(),
            0,
            Editor::new("10"),
            Editor::new("600"),
            Editor::new("0"),
            Editor::new("0"),
        )
    };

    let mut focused = FormField::Method;
    let mut error = None::<String>;
    let mut history_idx: Option<usize> = None;

    let try_submit = |url: &str,
                      method_idx: usize,
                      headers: &str,
                      body: &str,
                      conc_s: &str,
                      rpm_s: &str,
                      total_s: &str,
                      duration_s: &str|
     -> anyhow::Result<TestConfig> {
        let req = CurlRequest::from_form(url, METHODS[method_idx], headers, body)?;
        let conc: usize = conc_s
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Concurrency must be a number (1-500)"))?;
        let rpm: u64 = rpm_s
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("RPM must be a number (60-60000)"))?;
        if !(1..=500).contains(&conc) {
            anyhow::bail!("Concurrency out of range: use 1–500");
        }
        if !(60..=60_000).contains(&rpm) {
            anyhow::bail!("RPM out of range: use 60–60000");
        }
        let total_requests: u64 = total_s
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Total requests must be a number (0=unlimited)"))?;
        let duration_secs: u64 = duration_s
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Duration must be a number of seconds (0=unlimited)"))?;
        Ok(TestConfig {
            request: req,
            concurrency: conc,
            rpm,
            total_requests: if total_requests == 0 {
                None
            } else {
                Some(total_requests)
            },
            duration_secs: if duration_secs == 0 {
                None
            } else {
                Some(duration_secs)
            },
        })
    };

    let cursor_style = Style::default().bg(CURSOR_STYLE.0).fg(CURSOR_STYLE.1);

    loop {
        let method = METHODS[method_idx];
        let url_s = url.as_str().replace('\r', " ");
        let headers_s = headers.as_str().to_string();
        let body_s = body.as_str().to_string();
        let conc_s = conc_edit.as_str().to_string();
        let rpm_s = rpm_edit.as_str().to_string();
        let total_s = total_edit.as_str().to_string();
        let duration_s = duration_edit.as_str().to_string();
        terminal.draw(|f| {
            draw_form(
                f,
                FormView {
                    url: &url,
                    headers: &headers,
                    body: &body,
                    conc: &conc_edit,
                    rpm: &rpm_edit,
                    total: &total_edit,
                    duration: &duration_edit,
                    method,
                    focused,
                    error: &error,
                    cursor_style,
                },
            )
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(e) if e.kind == KeyEventKind::Press => {
                    if e.code == KeyCode::Esc {
                        if error.is_some() {
                            error = None;
                        } else if confirm_quit(&mut terminal) {
                            execute!(std::io::stdout(), DisableBracketedPaste)?;
                            ratatui::restore();
                            return Err(AppError::UserCancelled);
                        }
                    }

                    let ctrl = e.modifiers.contains(KeyModifiers::CONTROL);
                    let is_enter = matches!(e.code, KeyCode::Char('\n' | '\r') | KeyCode::Enter);
                    let is_submit = is_enter || (ctrl && e.code == KeyCode::Char('m'));

                    if is_submit
                        && !url_s.trim().is_empty()
                        && (focused == FormField::Start
                            || focused == FormField::Url
                            || (ctrl
                                && (focused == FormField::Headers || focused == FormField::Body)))
                    {
                        match try_submit(
                            &url_s,
                            method_idx,
                            &headers_s,
                            &body_s,
                            &conc_s,
                            &rpm_s,
                            &total_s,
                            &duration_s,
                        ) {
                            Ok(cfg) => {
                                let entry = HistoryEntry::new(
                                    &url_s,
                                    METHODS[method_idx],
                                    &headers_s,
                                    &body_s,
                                    cfg.concurrency,
                                    cfg.rpm,
                                    cfg.total_requests,
                                    cfg.duration_secs,
                                );
                                history::add_to_history(history, entry);
                                history::save_history(history);
                                execute!(std::io::stdout(), DisableBracketedPaste)?;
                                ratatui::restore();
                                return Ok(cfg);
                            }
                            Err(err) => error = Some(err.to_string()),
                        }
                    }

                    match focused {
                        FormField::Url => {
                            if (is_enter && !ctrl) || e.code == KeyCode::Tab {
                                focused = FormField::Headers;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Method;
                            } else if (e.code == KeyCode::Up || e.code == KeyCode::Down)
                                && !history.is_empty()
                            {
                                apply_history(
                                    history,
                                    &mut history_idx,
                                    e.code == KeyCode::Up,
                                    FormEditorsMut {
                                        url: &mut url,
                                        headers: &mut headers,
                                        body: &mut body,
                                        method_idx: &mut method_idx,
                                        conc_edit: &mut conc_edit,
                                        rpm_edit: &mut rpm_edit,
                                        total_edit: &mut total_edit,
                                        duration_edit: &mut duration_edit,
                                    },
                                );
                            } else if ctrl && e.code == KeyCode::Char('a') {
                                url.ctrl_a();
                            } else if matches!(e.code, KeyCode::Left) {
                                url.left();
                            } else if matches!(e.code, KeyCode::Right) {
                                url.right();
                            } else if e.code == KeyCode::Backspace {
                                url.backspace();
                            } else if e.code == KeyCode::Delete {
                                url.delete();
                            } else if let KeyCode::Char(c) = e.code {
                                if c != '\n' {
                                    url.insert(c);
                                }
                                history_idx = None;
                                error = None;
                            }
                        }
                        FormField::Method => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::Url;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Start;
                            } else if matches!(e.code, KeyCode::Left) {
                                method_idx = (method_idx + METHODS.len() - 1) % METHODS.len();
                            } else if matches!(e.code, KeyCode::Right) {
                                method_idx = (method_idx + 1) % METHODS.len();
                            }
                        }
                        FormField::Headers => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::Body;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Url;
                            } else if ctrl
                                && (e.code == KeyCode::Up || e.code == KeyCode::Down)
                                && !history.is_empty()
                            {
                                apply_history(
                                    history,
                                    &mut history_idx,
                                    e.code == KeyCode::Up,
                                    FormEditorsMut {
                                        url: &mut url,
                                        headers: &mut headers,
                                        body: &mut body,
                                        method_idx: &mut method_idx,
                                        conc_edit: &mut conc_edit,
                                        rpm_edit: &mut rpm_edit,
                                        total_edit: &mut total_edit,
                                        duration_edit: &mut duration_edit,
                                    },
                                );
                            } else if is_enter {
                                headers.insert('\n');
                                error = None;
                            } else if ctrl && e.code == KeyCode::Char('a') {
                                headers.ctrl_a();
                            } else if matches!(e.code, KeyCode::Left) {
                                headers.left();
                            } else if matches!(e.code, KeyCode::Right) {
                                headers.right();
                            } else if matches!(e.code, KeyCode::Up) {
                                headers.up();
                            } else if matches!(e.code, KeyCode::Down) {
                                headers.down();
                            } else if e.code == KeyCode::Backspace {
                                headers.backspace();
                            } else if e.code == KeyCode::Delete {
                                headers.delete();
                            } else if let KeyCode::Char(c) = e.code {
                                headers.insert(c);
                                history_idx = None;
                                error = None;
                            }
                        }
                        FormField::Body => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::Concurrency;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Headers;
                            } else if ctrl
                                && (e.code == KeyCode::Up || e.code == KeyCode::Down)
                                && !history.is_empty()
                            {
                                apply_history(
                                    history,
                                    &mut history_idx,
                                    e.code == KeyCode::Up,
                                    FormEditorsMut {
                                        url: &mut url,
                                        headers: &mut headers,
                                        body: &mut body,
                                        method_idx: &mut method_idx,
                                        conc_edit: &mut conc_edit,
                                        rpm_edit: &mut rpm_edit,
                                        total_edit: &mut total_edit,
                                        duration_edit: &mut duration_edit,
                                    },
                                );
                            } else if is_enter {
                                body.insert('\n');
                                error = None;
                            } else if ctrl && e.code == KeyCode::Char('a') {
                                body.ctrl_a();
                            } else if matches!(e.code, KeyCode::Left) {
                                body.left();
                            } else if matches!(e.code, KeyCode::Right) {
                                body.right();
                            } else if matches!(e.code, KeyCode::Up) {
                                body.up();
                            } else if matches!(e.code, KeyCode::Down) {
                                body.down();
                            } else if e.code == KeyCode::Backspace {
                                body.backspace();
                            } else if e.code == KeyCode::Delete {
                                body.delete();
                            } else if let KeyCode::Char(c) = e.code {
                                body.insert(c);
                                history_idx = None;
                                error = None;
                            }
                        }
                        FormField::Concurrency => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::Rpm;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Body;
                            } else if e.code == KeyCode::Up {
                                let v: usize = conc_s.trim().parse().unwrap_or(10);
                                conc_edit = Editor::new(((v + 10).min(500)).to_string());
                            } else if e.code == KeyCode::Down {
                                let v: usize = conc_s.trim().parse().unwrap_or(10);
                                conc_edit = Editor::new((v.saturating_sub(10).max(1)).to_string());
                            } else if e.code == KeyCode::Backspace {
                                conc_edit.backspace();
                            } else if e.code == KeyCode::Delete {
                                conc_edit.delete();
                            } else if let KeyCode::Char(c) = e.code {
                                if c.is_ascii_digit() {
                                    conc_edit.insert(c);
                                    error = None;
                                }
                            }
                        }
                        FormField::Rpm => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::TotalRequests;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Concurrency;
                            } else if e.code == KeyCode::Up {
                                let v: u64 = rpm_s.trim().parse().unwrap_or(600);
                                rpm_edit = Editor::new(((v + 10).min(60_000)).to_string());
                            } else if e.code == KeyCode::Down {
                                let v: u64 = rpm_s.trim().parse().unwrap_or(600);
                                rpm_edit = Editor::new((v.saturating_sub(10).max(60)).to_string());
                            } else if e.code == KeyCode::Backspace {
                                rpm_edit.backspace();
                            } else if e.code == KeyCode::Delete {
                                rpm_edit.delete();
                            } else if let KeyCode::Char(c) = e.code {
                                if c.is_ascii_digit() {
                                    rpm_edit.insert(c);
                                    error = None;
                                }
                            }
                        }
                        FormField::TotalRequests => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::DurationSecs;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Rpm;
                            } else if e.code == KeyCode::Up {
                                let v: u64 = total_s.trim().parse().unwrap_or(0);
                                total_edit = Editor::new((v + 100).to_string());
                            } else if e.code == KeyCode::Down {
                                let v: u64 = total_s.trim().parse().unwrap_or(0);
                                total_edit = Editor::new(v.saturating_sub(100).to_string());
                            } else if e.code == KeyCode::Backspace {
                                total_edit.backspace();
                            } else if e.code == KeyCode::Delete {
                                total_edit.delete();
                            } else if let KeyCode::Char(c) = e.code {
                                if c.is_ascii_digit() {
                                    total_edit.insert(c);
                                    error = None;
                                }
                            }
                        }
                        FormField::DurationSecs => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::Start;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::TotalRequests;
                            } else if e.code == KeyCode::Up {
                                let v: u64 = duration_s.trim().parse().unwrap_or(0);
                                duration_edit = Editor::new((v + 10).to_string());
                            } else if e.code == KeyCode::Down {
                                let v: u64 = duration_s.trim().parse().unwrap_or(0);
                                duration_edit = Editor::new(v.saturating_sub(10).to_string());
                            } else if e.code == KeyCode::Backspace {
                                duration_edit.backspace();
                            } else if e.code == KeyCode::Delete {
                                duration_edit.delete();
                            } else if let KeyCode::Char(c) = e.code {
                                if c.is_ascii_digit() {
                                    duration_edit.insert(c);
                                    error = None;
                                }
                            }
                        }
                        FormField::Start => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::Method;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::DurationSecs;
                            } else if matches!(
                                e.code,
                                KeyCode::Char('\n' | '\r' | ' ') | KeyCode::Enter
                            ) {
                                match try_submit(
                                    &url_s,
                                    method_idx,
                                    &headers_s,
                                    &body_s,
                                    &conc_s,
                                    &rpm_s,
                                    &total_s,
                                    &duration_s,
                                ) {
                                    Ok(cfg) => {
                                        execute!(std::io::stdout(), DisableBracketedPaste)?;
                                        ratatui::restore();
                                        return Ok(cfg);
                                    }
                                    Err(err) => error = Some(err.to_string()),
                                }
                            }
                        }
                    }
                }
                Event::Paste(data) => {
                    let paste = data.replace('\r', "");
                    match focused {
                        FormField::Url => {
                            for c in paste.chars() {
                                if c != '\n' {
                                    url.insert(c);
                                }
                            }
                        }
                        FormField::Headers => headers.insert_str(&paste),
                        FormField::Body => body.insert_str(&paste),
                        FormField::Concurrency => {
                            let digits: String =
                                paste.chars().filter(|c| c.is_ascii_digit()).collect();
                            conc_edit.insert_str(&digits);
                        }
                        FormField::Rpm => {
                            let digits: String =
                                paste.chars().filter(|c| c.is_ascii_digit()).collect();
                            rpm_edit.insert_str(&digits);
                        }
                        FormField::TotalRequests => {
                            let digits: String =
                                paste.chars().filter(|c| c.is_ascii_digit()).collect();
                            total_edit.insert_str(&digits);
                        }
                        FormField::DurationSecs => {
                            let digits: String =
                                paste.chars().filter(|c| c.is_ascii_digit()).collect();
                            duration_edit.insert_str(&digits);
                        }
                        _ => {}
                    }
                    error = None;
                }
                _ => {}
            }
        }
    }
}

fn apply_history(
    history: &[HistoryEntry],
    history_idx: &mut Option<usize>,
    up: bool,
    editors: FormEditorsMut<'_>,
) {
    let idx = history_idx.get_or_insert(history.len());
    if up {
        *idx = if *idx >= history.len() {
            0
        } else {
            (*idx + 1).min(history.len() - 1)
        };
    } else {
        *idx = idx.saturating_sub(1);
    }
    if *idx < history.len() {
        let ent = &history[*idx];
        *editors.url = Editor::new(ent.url.clone());
        *editors.headers = Editor::new(ent.headers.clone());
        *editors.body = Editor::new(ent.body.clone());
        *editors.method_idx = METHODS.iter().position(|&m| m == ent.method).unwrap_or(0);
        *editors.conc_edit = Editor::new(ent.conc.to_string());
        *editors.rpm_edit = Editor::new(ent.rpm.to_string());
        *editors.total_edit = Editor::new(
            ent.total_requests
                .map_or("0".to_string(), |v| v.to_string()),
        );
        *editors.duration_edit = Editor::new(
            ent.duration_secs
                .map_or("0".to_string(), |v| v.to_string()),
        );
    }
}

fn draw_form(f: &mut Frame, v: FormView<'_>) {
    let full = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), full);

    let area = Rect::new(full.x + 1, full.y, full.width.saturating_sub(3), full.height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),  // 0: banner
            Constraint::Length(4),  // 1: Method + URL row
            Constraint::Length(1),  // 2: gap
            Constraint::Length(7),  // 3: Headers
            Constraint::Length(1),  // 4: gap
            Constraint::Length(7),  // 5: Body
            Constraint::Length(1),  // 6: gap
            Constraint::Length(5),  // 7: numeric row
            Constraint::Length(1),  // 8: hint
            Constraint::Length(1),  // 9: gap
            Constraint::Length(4),  // 10: START
            Constraint::Min(1),    // 11: footer
        ])
        .split(area);

    render_banner(f, chunks[0]);

    let highlight = |field: FormField| {
        if v.focused == field {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(BORDER)
        }
    };

    let shadow_color = |field: FormField| {
        if v.focused == field { BORDER } else { ACCENT }
    };

    let style_fg = Style::default().fg(FG);
    let style_muted = Style::default().fg(MUTED);

    let input_block = |field: FormField, title: &str| {
        Block::default()
            .title(format!(" {title} "))
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(highlight(field))
            .style(Style::default().bg(SURFACE))
    };

    let widget_rect = |chunk: Rect| -> Rect {
        Rect::new(chunk.x, chunk.y, chunk.width, chunk.height.saturating_sub(1))
    };

    // Method + URL on one row
    let method_url_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(16), // Method
            Constraint::Length(3),  // gap
            Constraint::Min(1),    // URL takes the rest
        ])
        .split(chunks[1]);

    let mwr = widget_rect(method_url_row[0]);
    render_thin_shadow(f, mwr, shadow_color(FormField::Method));
    f.render_widget(
        Paragraph::new(v.method)
            .block(input_block(FormField::Method, "Method ←/→"))
            .style(style_fg),
        mwr,
    );

    let uwr = widget_rect(method_url_row[2]);
    let url_line = v.url.render_line(
        uwr.width,
        "(enter URL)",
        v.focused == FormField::Url,
        if v.url.is_empty() { style_muted } else { style_fg },
        v.cursor_style,
    );
    render_thin_shadow(f, uwr, shadow_color(FormField::Url));
    f.render_widget(
        Paragraph::new(url_line).block(input_block(FormField::Url, "URL")),
        uwr,
    );

    let wr = widget_rect(chunks[3]);
    let h_lines = v.headers.render_lines(
        wr.width,
        4,
        "(Name: value per line)",
        v.focused == FormField::Headers,
        if v.headers.is_empty() { style_muted } else { style_fg },
        v.cursor_style,
    );
    render_thin_shadow(f, wr, shadow_color(FormField::Headers));
    f.render_widget(
        Paragraph::new(h_lines)
            .block(input_block(FormField::Headers, "Headers"))
            .wrap(Wrap::default()),
        wr,
    );

    let wr = widget_rect(chunks[5]);
    let b_lines = v.body.render_lines(
        wr.width,
        4,
        "(JSON or raw body)",
        v.focused == FormField::Body,
        if v.body.is_empty() { style_muted } else { style_fg },
        v.cursor_style,
    );
    render_thin_shadow(f, wr, shadow_color(FormField::Body));
    f.render_widget(
        Paragraph::new(b_lines)
            .block(input_block(FormField::Body, "Body"))
            .wrap(Wrap::default()),
        wr,
    );

    let num_chunk = chunks[7];
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Length(3), // gap
            Constraint::Ratio(1, 4),
            Constraint::Length(3), // gap
            Constraint::Ratio(1, 4),
            Constraint::Length(3), // gap
            Constraint::Ratio(1, 4),
        ])
        .split(num_chunk);

    let numeric_fields = [
        (FormField::Concurrency, "Concurrency", "10", &v.conc, 0usize),
        (FormField::Rpm, "RPM", "600", &v.rpm, 2),
        (FormField::TotalRequests, "Total Requests", "0", &v.total, 4),
        (FormField::DurationSecs, "Duration (s)", "0", &v.duration, 6),
    ];
    for (field, title, placeholder, editor, col) in numeric_fields.iter() {
        let wr = widget_rect(row[*col]);
        let line = editor.render_line(
            8,
            placeholder,
            v.focused == *field,
            style_fg,
            v.cursor_style,
        );
        render_thin_shadow(f, wr, shadow_color(*field));
        f.render_widget(
            Paragraph::new(line).block(input_block(*field, title)),
            wr,
        );
    }

    f.render_widget(
        Paragraph::new(" 0 = unlimited ")
            .style(style_muted)
            .alignment(Alignment::Right),
        chunks[8],
    );

    let wr = widget_rect(chunks[10]);
    if v.focused == FormField::Start {
        render_thin_shadow(f, wr, BORDER);
        f.render_widget(
            Paragraph::new(" ▸ START ")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Plain)
                        .border_style(Style::default().fg(ACCENT))
                        .style(Style::default().bg(ACCENT)),
                )
                .style(Style::default().fg(BG).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center),
            wr,
        );
    } else {
        render_thin_shadow(f, wr, ACCENT);
        f.render_widget(
            Paragraph::new(" START ")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Plain)
                        .border_style(Style::default().fg(FG))
                        .style(Style::default().bg(BG)),
                )
                .style(Style::default().fg(FG))
                .alignment(Alignment::Center),
            wr,
        );
    };

    let (help, is_err) = if let Some(e) = v.error {
        (e.clone(), true)
    } else {
        (" ↑/↓ history · Ctrl+Enter submit · Esc quit ".into(), false)
    };
    let footer_block = if is_err {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(ERROR))
            .style(Style::default().bg(BG).fg(ERROR))
    } else {
        Block::default().style(Style::default().bg(BG))
    };
    f.render_widget(
        Paragraph::new(help)
            .block(footer_block)
            .wrap(Wrap::default())
            .style(Style::default().fg(if is_err { ERROR } else { MUTED })),
        chunks[11],
    );
}
