---
name: documentation-update-for-architecture-or-workflows
description: Workflow command scaffold for documentation-update-for-architecture-or-workflows in stress-raiser.
allowed_tools: ["Bash", "Read", "Write", "Grep", "Glob"]
---

# /documentation-update-for-architecture-or-workflows

Use this workflow when working on **documentation-update-for-architecture-or-workflows** in `stress-raiser`.

## Goal

Updates or adds documentation files to reflect changes in architecture or workflows, especially after significant feature or CI changes.

## Common Files

- `CLAUDE.md`
- `AGENTS.md`
- `STACK-SETUP.md`
- `.github/copilot-instructions.md`

## Suggested Sequence

1. Understand the current state and failure mode before editing.
2. Make the smallest coherent change that satisfies the workflow goal.
3. Run the most relevant verification for touched files.
4. Summarize what changed and what still needs review.

## Typical Commit Signals

- Add or update markdown documentation files (e.g., CLAUDE.md, AGENTS.md, STACK-SETUP.md).
- Update Copilot or contributor instructions if needed.
- Reflect new modules or workflows in architecture docs.

## Notes

- Treat this as a scaffold, not a hard-coded script.
- Update the command if the workflow evolves materially.