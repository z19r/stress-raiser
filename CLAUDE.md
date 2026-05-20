# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

stress-raiser is a terminal-based HTTP load testing tool built with Rust. It has a two-phase TUI: a form for configuring requests (URL, method, headers, body, concurrency, RPM), then a live dashboard showing stats, a circuit breaker, RPS sparkline, and response log. Published on crates.io.

## Commands

```bash
just              # list available recipes
just run          # cargo run (launch the TUI)
just build        # cargo build (debug)
just build-release # cargo build --release
just test         # cargo test
just fmt          # cargo fmt
just check        # cargo check && cargo clippy
just doc          # cargo doc --no-deps --open
just package      # cargo package (create .crate for inspection)
```

Run a single test: `cargo test test_name`

### Release

```bash
just release-check              # quality gate: fmt + clippy + test
just release-dry-run patch      # preview release without changes
just release patch              # bump version, create release/vX.Y.Z branch + PR
```

After `just release`, the human workflow is:
```bash
gh pr checks                    # watch CI on the PR
gh pr merge --squash --delete-branch
gh run watch                    # watch release workflow
```

CI (`release.yml`) handles: tag creation, cross-platform builds (4 targets), GitHub Release with checksums, and crates.io publish. See `~/.claude/skills/rust-release/SKILL.md` for the full workflow spec.

## Architecture

```
src/
├── main.rs          # Entry point: form → dashboard loop
├── lib.rs           # Public API re-exports
├── curl.rs          # CurlRequest: builds reqwest Client/Request from form fields
├── editor.rs        # Editor: grapheme-aware text buffer with cursor (used by form fields)
├── error.rs         # AppError enum (thiserror + anyhow)
├── export.rs        # CSV, HTML, ASCII, and terminal summary export for test reports
├── history.rs       # Persistent request history (JSON, XDG paths, best-effort I/O)
├── stats.rs         # Stats, ReportData, percentiles, CircuitBreaker (Fibonacci backoff)
└── tui/
    ├── mod.rs       # Theme constants (Whetstone design system), RunResult, run_tui()
    ├── form.rs      # Form phase: field editors, Tab/Enter/Esc, history cycling (Up/Down)
    ├── dashboard.rs # Dashboard phase: load_worker (async HTTP loop), run_loop (event + render)
    └── report.rs    # Post-test report screen with export options (CSV/HTML/ASCII)
```

Key data flow: `main.rs` loops between `run_form()` → `run_tui()`. `run_tui()` spawns `load_worker` as a tokio task that sends HTTP requests respecting concurrency/RPM limits and the circuit breaker, writing results into `Arc<RwLock<Stats>>`. The dashboard event loop reads stats to render the UI.

The circuit breaker uses Fibonacci backoff (1, 1, 2, 3, 5, 8… seconds, capped at 377s) and transitions through Closed → Open → HalfOpen states.

## Key dependencies

- **ratatui** + **crossterm** — TUI rendering and terminal events
- **reqwest** (with json feature) — async HTTP client
- **tokio** (full) — async runtime
- **anyhow** / **thiserror** — error handling (anyhow at app boundary, thiserror for AppError)
- **unicode-segmentation** — grapheme-correct cursor movement in Editor

## CI

- `.github/workflows/rust.yml` — builds and tests on push/PR to main (fmt, clippy, test)
- `.github/workflows/release.yml` — triggered on push to main when `Cargo.toml` changes; reads version, verifies, cross-builds 4 targets (x86_64/aarch64 linux, x86_64/aarch64 macOS), creates git tag, GitHub Release with checksums, and publishes to crates.io

## History persistence

Stored at `$XDG_DATA_HOME/stress-raiser/history.json` (falls back to `~/.local/share/` then `./`). Load/save errors are silently ignored.

<!-- icm:start -->
## Persistent memory (ICM) — MANDATORY

This project uses [ICM](https://github.com/rtk-ai/icm) for persistent memory across sessions.
You MUST use it actively. Not optional.

### Recall (before starting work)
```bash
icm recall "query"                        # search memories
icm recall "query" -t "topic-name"        # filter by topic
icm recall-context "query" --limit 5      # formatted for prompt injection
```

### Store — MANDATORY triggers
You MUST call `icm store` when ANY of the following happens:
1. **Error resolved** → `icm store -t errors-resolved -c "description" -i high -k "keyword1,keyword2"`
2. **Architecture/design decision** → `icm store -t decisions-{project} -c "description" -i high`
3. **User preference discovered** → `icm store -t preferences -c "description" -i critical`
4. **Significant task completed** → `icm store -t context-{project} -c "summary of work done" -i high`
5. **Conversation exceeds ~20 tool calls without a store** → store a progress summary

Do this BEFORE responding to the user. Not after. Not later. Immediately.

Do NOT store: trivial details, info already in CLAUDE.md, ephemeral state (build logs, git status).

### Other commands
```bash
icm update <id> -c "updated content"     # edit memory in-place
icm health                                # topic hygiene audit
icm topics                                # list all topics
```
<!-- icm:end -->
