//! CSV, HTML, ASCII, and terminal summary export for test reports.

use crate::stats::ReportData;
use std::path::PathBuf;
use std::time::SystemTime;

const ACID: &str = "\x1b[38;2;214;255;46m";
const MIST: &str = "\x1b[38;2;220;208;255m";
const MAGENTA: &str = "\x1b[38;2;255;46;147m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

fn dot_line(name: &str, col: usize, value: &str, color: &str) {
    let dots = col.saturating_sub(name.len());
    println!("  {name}{DIM}{}{RESET}: {color}{value}{RESET}", ".".repeat(dots));
}

/// Print a k6-style colored summary to stdout (call after ratatui::restore).
pub fn print_terminal_summary(r: &ReportData) {
    let col = 30;

    println!();
    println!("{ACID}{BOLD}█▀ ▀█▀ █▀▄ █▀▀ █▀ █▀   █▀▄ ▄▀█ █ █▀ █▀▀ █▀▄{RESET}");
    println!("{ACID}{BOLD}▄█  █  █▀▄ ██▄ ▄█ ▄█   █▀▄ █▀█ █ ▄█ ██▄ █▀▄{RESET}");
    println!();
    println!("  target: {ACID}{} {}{RESET}", r.method, r.url);
    println!();

    let rps_str = format!("{}    {MIST}{:.1}/s{RESET}", r.total, r.rps as f64);
    dot_line("http_reqs", col, &rps_str, ACID);

    let dur_str = format!(
        "avg={ACID}{}ms{RESET}  min={MIST}{}ms{RESET}  med={MIST}{}ms{RESET}  max={MIST}{}ms{RESET}  p(95)={MIST}{}ms{RESET}  p(99)={MIST}{}ms{RESET}",
        r.avg_latency as u64, r.min_latency, r.p50, r.max_latency, r.p95, r.p99
    );
    dot_line("http_req_duration", col, &dur_str, "");

    let rate_color = if r.success_rate > 0.95 { ACID } else { MAGENTA };
    dot_line(
        "success_rate",
        col,
        &format!("{:.1}%", r.success_rate * 100.0),
        rate_color,
    );

    let mut codes: Vec<_> = r.status_codes.iter().collect();
    codes.sort_by_key(|(c, _)| **c);
    for (code, count) in &codes {
        let color = if **code < 300 {
            ACID
        } else if **code < 500 {
            MIST
        } else {
            MAGENTA
        };
        dot_line(&format!("status_{code}"), col, &count.to_string(), color);
    }

    let elapsed_s = r.elapsed.as_secs();
    let elapsed_ms = r.elapsed.subsec_millis();
    println!();
    println!("  {DIM}elapsed:{RESET} {ACID}{elapsed_s}.{elapsed_ms:03}s{RESET}");
    println!();
}

fn timestamp_filename(ext: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    PathBuf::from(format!("stress-raiser-{ts}.{ext}"))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn export_csv(r: &ReportData) -> anyhow::Result<PathBuf> {
    let path = timestamp_filename("csv");
    let mut out = String::new();
    out.push_str("metric,value\n");
    out.push_str(&format!("url,\"{}\"\n", r.url.replace('"', "\"\"")));
    out.push_str(&format!("method,{}\n", r.method));
    out.push_str(&format!("total,{}\n", r.total));
    out.push_str(&format!("ok,{}\n", r.ok));
    out.push_str(&format!("errors,{}\n", r.err));
    out.push_str(&format!("success_rate,{:.4}\n", r.success_rate));
    out.push_str(&format!("rps,{}\n", r.rps));
    out.push_str(&format!("elapsed_secs,{:.3}\n", r.elapsed.as_secs_f64()));
    out.push_str(&format!("min_latency_ms,{}\n", r.min_latency));
    out.push_str(&format!("avg_latency_ms,{:.1}\n", r.avg_latency));
    out.push_str(&format!("p50_ms,{}\n", r.p50));
    out.push_str(&format!("p95_ms,{}\n", r.p95));
    out.push_str(&format!("p99_ms,{}\n", r.p99));
    out.push_str(&format!("max_latency_ms,{}\n", r.max_latency));

    out.push_str("\nstatus_code,count\n");
    let mut codes: Vec<_> = r.status_codes.iter().collect();
    codes.sort_by_key(|(c, _)| **c);
    for (code, count) in codes {
        out.push_str(&format!("{code},{count}\n"));
    }

    out.push_str("\nseq,elapsed_ms,status,latency_ms,ok,body_preview\n");
    for req in &r.requests {
        out.push_str(&format!(
            "{},{},{},{},{},\"{}\"\n",
            req.seq,
            req.elapsed_ms,
            req.status,
            req.latency_ms,
            req.ok,
            req.body_preview.replace('"', "\"\""),
        ));
    }

    std::fs::write(&path, out)?;
    Ok(path)
}

pub fn export_html(r: &ReportData) -> anyhow::Result<PathBuf> {
    let path = timestamp_filename("html");
    let elapsed_s = r.elapsed.as_secs();
    let elapsed_ms = r.elapsed.subsec_millis();

    let mut codes_html = String::new();
    let mut codes: Vec<_> = r.status_codes.iter().collect();
    codes.sort_by_key(|(c, _)| **c);
    for (code, count) in &codes {
        let cls = if **code < 300 {
            "ok"
        } else if **code < 500 {
            "warn"
        } else {
            "err"
        };
        codes_html.push_str(&format!(
            "<tr><td class=\"{cls}\">{code}</td><td>{count}</td></tr>\n"
        ));
    }

    let mut requests_html = String::new();
    for req in &r.requests {
        let cls = if req.ok { "ok" } else { "err" };
        requests_html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"{}\">{}</td><td>{}</td><td class=\"{}\">{}</td><td>{}</td></tr>\n",
            req.seq,
            req.elapsed_ms,
            if req.status < 300 { "ok" } else if req.status < 500 { "warn" } else { "err" },
            req.status,
            req.latency_ms,
            cls,
            if req.ok { "OK" } else { "ERR" },
            html_escape(&req.body_preview),
        ));
    }

    let url_escaped = html_escape(&r.url);
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>stress-raiser report</title>
<style>
  :root {{
    --bg: #07030E; --surface: #110628; --border: #2A0F5C;
    --fg: #F5F2E8; --muted: #C8B4FF; --accent: #D6FF2E;
    --success: #D6FF2E; --error: #FF2E93;
  }}
  * {{ margin:0; padding:0; box-sizing:border-box; }}
  body {{ background:var(--bg); color:var(--fg); font-family:'JetBrains Mono','Fira Code',monospace; padding:2rem; }}
  h1 {{ color:var(--accent); font-size:1.4rem; margin-bottom:.5rem; }}
  .url {{ color:var(--muted); font-size:.85rem; margin-bottom:1.5rem; word-break:break-all; }}
  .grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(140px,1fr)); gap:1rem; margin-bottom:1.5rem; }}
  .card {{ background:var(--surface); border:1px solid var(--border); border-radius:8px; padding:1rem; }}
  .card .label {{ color:var(--muted); font-size:.7rem; text-transform:uppercase; letter-spacing:.05em; }}
  .card .value {{ font-size:1.3rem; font-weight:700; margin-top:.25rem; }}
  .card .value.ok {{ color:var(--success); }}
  .card .value.err {{ color:var(--error); }}
  table {{ width:100%; border-collapse:collapse; background:var(--surface); border:1px solid var(--border); border-radius:8px; overflow:hidden; }}
  th {{ background:var(--border); color:var(--muted); text-align:left; padding:.5rem 1rem; font-size:.75rem; text-transform:uppercase; }}
  td {{ padding:.4rem 1rem; border-top:1px solid var(--border); }}
  .ok {{ color:var(--success); }}
  .warn {{ color:var(--accent); }}
  .err {{ color:var(--error); }}
  footer {{ margin-top:2rem; color:var(--muted); font-size:.7rem; text-align:center; }}
</style>
</head>
<body>
<h1>stress-raiser</h1>
<div class="url">{method} {url_escaped}</div>
<div class="grid">
  <div class="card"><div class="label">Total</div><div class="value">{total}</div></div>
  <div class="card"><div class="label">OK</div><div class="value ok">{ok}</div></div>
  <div class="card"><div class="label">Errors</div><div class="value err">{err}</div></div>
  <div class="card"><div class="label">Success</div><div class="value{success_cls}">{success:.1}%</div></div>
  <div class="card"><div class="label">RPS</div><div class="value">{rps}</div></div>
  <div class="card"><div class="label">Elapsed</div><div class="value">{elapsed_s}.{elapsed_ms:03}s</div></div>
</div>
<h2 style="color:var(--fg);font-size:1rem;margin-bottom:.5rem;">Latency (ms)</h2>
<div class="grid">
  <div class="card"><div class="label">Min</div><div class="value">{min}</div></div>
  <div class="card"><div class="label">Avg</div><div class="value">{avg:.1}</div></div>
  <div class="card"><div class="label">p50</div><div class="value">{p50}</div></div>
  <div class="card"><div class="label">p95</div><div class="value">{p95}</div></div>
  <div class="card"><div class="label">p99</div><div class="value">{p99}</div></div>
  <div class="card"><div class="label">Max</div><div class="value">{max}</div></div>
</div>
<h2 style="color:var(--fg);font-size:1rem;margin:1rem 0 .5rem;">Status Codes</h2>
<table><tr><th>Code</th><th>Count</th></tr>
{codes_html}</table>
<h2 style="color:var(--fg);font-size:1rem;margin:1rem 0 .5rem;">Request Log ({req_count} requests)</h2>
<table><tr><th>#</th><th>Elapsed (ms)</th><th>Status</th><th>Latency (ms)</th><th>Result</th><th>Body Preview</th></tr>
{requests_html}</table>
<footer>Generated by stress-raiser</footer>
</body>
</html>"#,
        method = r.method,
        url_escaped = url_escaped,
        total = r.total,
        ok = r.ok,
        err = r.err,
        success = r.success_rate * 100.0,
        success_cls = if r.success_rate > 0.95 { " ok" } else { " err" },
        rps = r.rps,
        elapsed_s = elapsed_s,
        elapsed_ms = elapsed_ms,
        min = r.min_latency,
        avg = r.avg_latency,
        p50 = r.p50,
        p95 = r.p95,
        p99 = r.p99,
        max = r.max_latency,
        codes_html = codes_html,
        req_count = r.requests.len(),
        requests_html = requests_html,
    );

    std::fs::write(&path, html)?;
    Ok(path)
}

/// Number of header lines in ASCII output (everything before the request rows).
pub fn ascii_header_line_count(r: &ReportData) -> usize {
    // top border + title + sep + method/url + sep = 5
    // total/ok/err + success/rps + elapsed + sep = 4
    // latency header + min/avg/p50 + p95/p99/max + sep = 4
    // status codes header + N codes + sep = 2 + codes
    // request log header + sep + column header + sep = 4
    let code_count = r.status_codes.len();
    5 + 4 + 4 + 2 + code_count + 4
}

pub fn export_ascii(r: &ReportData) -> String {
    let elapsed_s = r.elapsed.as_secs();
    let elapsed_ms = r.elapsed.subsec_millis();

    let mut out = String::new();
    out.push_str("╔══════════════════════════════════════════╗\n");
    out.push_str("║         STRESS-RAISER REPORT             ║\n");
    out.push_str("╠══════════════════════════════════════════╣\n");
    out.push_str(&format!(
        "║ {} {:<36}║\n",
        r.method,
        r.url.chars().take(36).collect::<String>()
    ));
    out.push_str("╠══════════════════════════════════════════╣\n");
    out.push_str(&format!(
        "║ Total: {:<8} OK: {:<8} Err: {:<6}║\n",
        r.total, r.ok, r.err
    ));
    out.push_str(&format!(
        "║ Success: {:<6.1}%  RPS: {:<8} {:<6}║\n",
        r.success_rate * 100.0,
        r.rps,
        ""
    ));
    out.push_str(&format!(
        "║ Elapsed: {elapsed_s}.{elapsed_ms:03}s{:<24}║\n",
        ""
    ));
    out.push_str("╠══════════════════════════════════════════╣\n");
    out.push_str("║ Latency (ms)                             ║\n");
    out.push_str(&format!(
        "║  min: {:<8} avg: {:<8.1} p50: {:<6}║\n",
        r.min_latency, r.avg_latency, r.p50
    ));
    out.push_str(&format!(
        "║  p95: {:<8} p99: {:<8} max: {:<6}║\n",
        r.p95, r.p99, r.max_latency
    ));
    out.push_str("╠══════════════════════════════════════════╣\n");
    out.push_str("║ Status Codes                             ║\n");

    let mut codes: Vec<_> = r.status_codes.iter().collect();
    codes.sort_by_key(|(c, _)| **c);
    for (code, count) in &codes {
        out.push_str(&format!("║  {code}: {count:<34}║\n"));
    }
    out.push_str("╠══════════════════════════════════════════╣\n");
    out.push_str(&format!(
        "║ Request Log ({} requests){:<17}║\n",
        r.requests.len(),
        ""
    ));
    out.push_str("╠══════════════════════════════════════════╣\n");
    out.push_str("║  #     ms   stat  lat   result          ║\n");
    out.push_str("╠══════════════════════════════════════════╣\n");
    for req in &r.requests {
        let result = if req.ok { "OK" } else { "ERR" };
        let preview: String = req.body_preview.chars().take(16).collect();
        out.push_str(&format!(
            "║ {:<5} {:<6} {:<4} {:<5} {:<3} {:<16}║\n",
            req.seq, req.elapsed_ms, req.status, req.latency_ms, result, preview,
        ));
    }
    out.push_str("╚══════════════════════════════════════════╝\n");
    out
}
