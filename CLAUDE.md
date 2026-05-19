# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

stress-raiser is a terminal-based HTTP load testing tool built with Rust. It has a two-phase TUI: a form for configuring requests (URL, method, headers, body, concurrency, RPM), then a live dashboard showing stats, a circuit breaker, RPS sparkline, and response log. Published on crates.io.

## Commands

```bash
just              # list available recipes
just run          # cargo run (launch the TUI, cleans and runs first)
just build        # cargo build (debug)
just build-release # cargo build --release
just test         # cargo test
just fmt          # cargo fmt
just check        # cargo check && cargo clippy
just doc          # cargo doc --no-deps --open
just publish-check # fmt + clippy + test + dry-run publish (gate before release)
just publish      # cargo publish (requires cargo login)
```

Run a single test: `cargo test test_name`

## Architecture

```
src/
├── main.rs          # Entry point: form → dashboard loop
├── lib.rs           # Public API re-exports
├── curl.rs          # CurlRequest: builds reqwest Client/Request from form fields
├── editor.rs        # Editor: grapheme-aware text buffer with cursor (used by form fields)
├── error.rs         # AppError enum (thiserror + anyhow)
├── history.rs       # Persistent request history (JSON, XDG paths, best-effort I/O)
├── stats.rs         # Stats (latencies, RPS, percentiles) + CircuitBreaker (Fibonacci backoff)
└── tui/
    ├── mod.rs       # Theme constants, RunResult enum, run_tui() (spawns worker + dashboard)
    ├── form.rs      # Form phase: field editors, Tab/Enter/Esc, history cycling (Up/Down)
    └── dashboard.rs # Dashboard phase: load_worker (async HTTP loop), run_loop (event + render)
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

GitHub Actions (`.github/workflows/rust.yml`): builds and tests on push/PR to main.

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
