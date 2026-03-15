//! Form UI: URL, method, headers, body, concurrency, RPM. Enter to start.

use crate::curl::CurlRequest;
use crate::editor::Editor;
use crate::error::AppError;
use crate::history::{self, HistoryEntry};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::{ACCENT, BORDER, CURSOR_STYLE, ERROR, FG, MUTED, SURFACE};

/// Mutable form editors passed to apply_history to stay under clippy arg limit.
struct FormEditorsMut<'a> {
    url: &'a mut Editor,
    headers: &'a mut Editor,
    body: &'a mut Editor,
    method_idx: &'a mut usize,
    conc_edit: &'a mut Editor,
    rpm_edit: &'a mut Editor,
}

/// Read-only form state passed to draw_form to stay under clippy arg limit.
struct FormView<'a> {
    url: &'a Editor,
    headers: &'a Editor,
    body: &'a Editor,
    conc: &'a Editor,
    rpm: &'a Editor,
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
    Start,
}

pub(super) const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE"];

pub(super) fn method_idx_from(req: &CurlRequest) -> usize {
    let m = req.method.as_str();
    METHODS.iter().position(|&x| x == m).unwrap_or(0)
}

/// Form: URL, method, headers, body, concurrency, RPM.
/// Enter to start. ↑/↓ in URL/Headers/Body = history. Esc = quit.
pub async fn run_form(
    init: Option<(CurlRequest, usize, u64)>,
    history: &mut Vec<HistoryEntry>,
) -> Result<(CurlRequest, usize, u64), AppError> {
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableBracketedPaste)?;

    let (mut url, mut headers, mut body, mut method_idx, mut conc_edit, mut rpm_edit) =
        if let Some((req, conc, r)) = init {
            let idx = method_idx_from(&req);
            let u = req.url.replace(['\n', '\r'], " ");
            let h = req.headers_display().join("\n");
            let b = req
                .body
                .as_ref()
                .map_or(String::new(), |x| String::from_utf8_lossy(x).into_owned());
            (
                Editor::new(u),
                Editor::new(h),
                Editor::new(b),
                idx,
                Editor::new(conc.to_string()),
                Editor::new(r.to_string()),
            )
        } else {
            (
                Editor::default(),
                Editor::default(),
                Editor::default(),
                0,
                Editor::new("10"),
                Editor::new("600"),
            )
        };

    let mut focused = FormField::Url;
    let mut error = None::<String>;
    let mut history_idx: Option<usize> = None;

    let try_submit = |url: &str,
                      method_idx: usize,
                      headers: &str,
                      body: &str,
                      conc_s: &str,
                      rpm_s: &str|
     -> anyhow::Result<(CurlRequest, usize, u64)> {
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
        Ok((req, conc, rpm))
    };

    let cursor_style = Style::default().bg(CURSOR_STYLE.0).fg(CURSOR_STYLE.1);

    loop {
        let method = METHODS[method_idx];
        let url_s = url.as_str().replace('\r', " ");
        let headers_s = headers.as_str().to_string();
        let body_s = body.as_str().to_string();
        let conc_s = conc_edit.as_str().to_string();
        let rpm_s = rpm_edit.as_str().to_string();
        terminal.draw(|f| {
            draw_form(
                f,
                FormView {
                    url: &url,
                    headers: &headers,
                    body: &body,
                    conc: &conc_edit,
                    rpm: &rpm_edit,
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
                        } else {
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
                        match try_submit(&url_s, method_idx, &headers_s, &body_s, &conc_s, &rpm_s) {
                            Ok((req, conc, rpm)) => {
                                let entry = HistoryEntry::new(
                                    &url_s,
                                    METHODS[method_idx],
                                    &headers_s,
                                    &body_s,
                                    conc,
                                    rpm,
                                );
                                history::add_to_history(history, entry);
                                history::save_history(history);
                                execute!(std::io::stdout(), DisableBracketedPaste)?;
                                ratatui::restore();
                                return Ok((req, conc, rpm));
                            }
                            Err(err) => error = Some(err.to_string()),
                        }
                    }

                    match focused {
                        FormField::Url => {
                            if (is_enter && !ctrl) || e.code == KeyCode::Tab {
                                focused = FormField::Method;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Start;
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
                                focused = FormField::Headers;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Url;
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
                                focused = FormField::Method;
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
                                focused = FormField::Start;
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
                        FormField::Start => {
                            if e.code == KeyCode::Tab {
                                focused = FormField::Url;
                            } else if e.code == KeyCode::BackTab {
                                focused = FormField::Rpm;
                            } else if matches!(
                                e.code,
                                KeyCode::Char('\n' | '\r' | ' ') | KeyCode::Enter
                            ) {
                                match try_submit(
                                    &url_s, method_idx, &headers_s, &body_s, &conc_s, &rpm_s,
                                ) {
                                    Ok((req, conc, rpm)) => {
                                        execute!(std::io::stdout(), DisableBracketedPaste)?;
                                        ratatui::restore();
                                        return Ok((req, conc, rpm));
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
    }
}

fn draw_form(f: &mut Frame, v: FormView<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(4),
            Constraint::Min(2),
        ])
        .split(f.area());

    let highlight = |field: FormField| {
        if v.focused == field {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(BORDER)
        }
    };

    let style_fg = Style::default().fg(FG);
    let style_muted = Style::default().fg(MUTED);
    let url_line = v.url.render_line(
        chunks[0].width,
        "(enter URL)",
        v.focused == FormField::Url,
        if v.url.is_empty() {
            style_muted
        } else {
            style_fg
        },
        v.cursor_style,
    );
    let url_para = Paragraph::new(url_line).block(
        Block::default()
            .title(" 1. URL ")
            .borders(Borders::ALL)
            .border_style(highlight(FormField::Url))
            .style(Style::default().bg(SURFACE)),
    );
    f.render_widget(url_para, chunks[0]);

    let method_para = Paragraph::new(v.method)
        .block(
            Block::default()
                .title(" 2. Method ←/→ ")
                .borders(Borders::ALL)
                .border_style(highlight(FormField::Method))
                .style(Style::default().bg(SURFACE)),
        )
        .style(Style::default().fg(FG));
    f.render_widget(method_para, chunks[1]);

    let h_lines = v.headers.render_lines(
        chunks[2].width,
        4,
        "(Name: value per line)",
        v.focused == FormField::Headers,
        if v.headers.is_empty() {
            style_muted
        } else {
            style_fg
        },
        v.cursor_style,
    );
    let headers_para = Paragraph::new(h_lines)
        .block(
            Block::default()
                .title(" 3. Headers ")
                .borders(Borders::ALL)
                .border_style(highlight(FormField::Headers))
                .style(Style::default().bg(SURFACE)),
        )
        .wrap(Wrap::default());
    f.render_widget(headers_para, chunks[2]);

    let b_lines = v.body.render_lines(
        chunks[3].width,
        4,
        "(JSON or raw body)",
        v.focused == FormField::Body,
        if v.body.is_empty() {
            style_muted
        } else {
            style_fg
        },
        v.cursor_style,
    );
    let body_para = Paragraph::new(b_lines)
        .block(
            Block::default()
                .title(" 4. Body ")
                .borders(Borders::ALL)
                .border_style(highlight(FormField::Body))
                .style(Style::default().bg(SURFACE)),
        )
        .wrap(Wrap::default());
    f.render_widget(body_para, chunks[3]);

    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[4]);
    let conc_line = v.conc.render_line(
        8,
        "10",
        v.focused == FormField::Concurrency,
        style_fg,
        v.cursor_style,
    );
    let conc_para = Paragraph::new(conc_line).block(
        Block::default()
            .title(" 5. Concurrency (↑↓ ±10 or type) ")
            .borders(Borders::ALL)
            .border_style(highlight(FormField::Concurrency))
            .style(Style::default().bg(SURFACE)),
    );
    f.render_widget(conc_para, row[0]);
    let rpm_line = v.rpm.render_line(
        8,
        "600",
        v.focused == FormField::Rpm,
        style_fg,
        v.cursor_style,
    );
    let rpm_para = Paragraph::new(rpm_line).block(
        Block::default()
            .title(" 6. RPM (↑↓ ±10 or type) ")
            .borders(Borders::ALL)
            .border_style(highlight(FormField::Rpm))
            .style(Style::default().bg(SURFACE)),
    );
    f.render_widget(rpm_para, row[1]);

    let start_text = if v.focused == FormField::Start {
        " ENTER or SPACE: start load test "
    } else {
        " Tab: next  Shift+Tab: prev  Enter: start "
    };
    let footer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(2)])
        .split(chunks[5]);
    let start_para = Paragraph::new(start_text)
        .block(
            Block::default()
                .title(" 7. Start ")
                .borders(Borders::ALL)
                .border_style(highlight(FormField::Start)),
        )
        .style(Style::default().fg(if v.focused == FormField::Start {
            ACCENT
        } else {
            MUTED
        }))
        .alignment(Alignment::Center);
    f.render_widget(start_para, footer_chunks[0]);

    let (help, is_err) = if let Some(e) = v.error {
        (e.clone(), true)
    } else {
        (
            " ↑/↓ in URL: prev request  Ctrl+↑/↓ in Hdrs/Body  Esc: quit ".into(),
            false,
        )
    };
    let err_block = if is_err {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ERROR))
            .style(Style::default().bg(SURFACE).fg(ERROR))
    } else {
        Block::default()
    };
    f.render_widget(
        Paragraph::new(help)
            .block(err_block)
            .wrap(Wrap::default())
            .style(Style::default().fg(if is_err { ERROR } else { MUTED })),
        footer_chunks[1],
    );
}
