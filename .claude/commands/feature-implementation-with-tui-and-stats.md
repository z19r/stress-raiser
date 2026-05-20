---
name: feature-implementation-with-tui-and-stats
description: Workflow command scaffold for feature-implementation-with-tui-and-stats in stress-raiser.
allowed_tools: ["Bash", "Read", "Write", "Grep", "Glob"]
---

# /feature-implementation-with-tui-and-stats

Use this workflow when working on **feature-implementation-with-tui-and-stats** in `stress-raiser`.

## Goal

Implements a new feature that affects both the TUI (text user interface) and the statistics or history tracking logic.

## Common Files

- `src/tui/dashboard.rs`
- `src/tui/form.rs`
- `src/tui/report.rs`
- `src/tui/mod.rs`
- `src/history.rs`
- `src/stats.rs`

## Suggested Sequence

1. Understand the current state and failure mode before editing.
2. Make the smallest coherent change that satisfies the workflow goal.
3. Run the most relevant verification for touched files.
4. Summarize what changed and what still needs review.

## Typical Commit Signals

- Modify or add files in src/tui/ (e.g., dashboard.rs, form.rs, report.rs, mod.rs) to update or extend the TUI.
- Update or create files in src/ (e.g., stats.rs, history.rs, export.rs) to handle new data, stats, or export logic.
- Update src/main.rs and/or src/lib.rs to wire up new features or data flows.

## Notes

- Treat this as a scaffold, not a hard-coded script.
- Update the command if the workflow evolves materially.