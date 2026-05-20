```markdown
# stress-raiser Development Patterns

> Auto-generated skill from repository analysis

## Overview

This skill covers the core development patterns and workflows for the `stress-raiser` Rust codebase. The repository focuses on building a terminal user interface (TUI) with integrated statistics/history tracking, and emphasizes clear, maintainable code and documentation practices. You'll learn how to implement new features, update documentation, and follow the project's coding conventions.

## Coding Conventions

- **File Naming:** Use `camelCase` for file names.
  - Example: `dashboard.rs`, `history.rs`, `export.rs`
- **Import Style:** Use relative imports within the crate.
  ```rust
  // In src/tui/dashboard.rs
  use super::form;
  use crate::stats;
  ```
- **Export Style:** Use named exports.
  ```rust
  // In src/stats.rs
  pub fn calculate_stats(data: &[u32]) -> Stats { ... }
  ```
- **Commit Messages:** Follow [Conventional Commits](https://www.conventionalcommits.org/) with prefixes like `feat`, `fix`, `docs`, `ci`.
  - Example: `feat: add export to CSV functionality`

## Workflows

### Feature Implementation with TUI and Stats
**Trigger:** When adding or significantly enhancing a feature that affects both the TUI and statistics/history logic  
**Command:** `/new-tui-feature`

1. **Update the TUI:**
   - Modify or add files in `src/tui/` such as `dashboard.rs`, `form.rs`, `report.rs`, or `mod.rs` to update the user interface.
   - Example:
     ```rust
     // src/tui/dashboard.rs
     pub fn draw_dashboard<B: Backend>(f: &mut Frame<B>, app: &App) { ... }
     ```
2. **Update Backend Logic:**
   - Update or create files in `src/` such as `stats.rs`, `history.rs`, or `export.rs` to handle new data, statistics, or export logic.
   - Example:
     ```rust
     // src/stats.rs
     pub fn update_stats(history: &History) -> Stats { ... }
     ```
3. **Wire Up in Main/Lib:**
   - Update `src/main.rs` and/or `src/lib.rs` to connect the new feature or data flow.
   - Example:
     ```rust
     // src/main.rs
     mod tui;
     mod stats;
     fn main() {
         // Initialize app and run TUI
     }
     ```
4. **Test the Feature:**
   - Ensure relevant tests are updated or added (see Testing Patterns below).

### Documentation Update for Architecture or Workflows
**Trigger:** When documenting new features, updating architecture diagrams, or describing workflows  
**Command:** `/update-docs`

1. **Update Markdown Docs:**
   - Add or update documentation files such as `CLAUDE.md`, `AGENTS.md`, `STACK-SETUP.md`.
   - Example:
     ```markdown
     # AGENTS.md
     ## Agent Workflow
     1. Receive input...
     ```
2. **Update Contributor Instructions:**
   - Edit `.github/copilot-instructions.md` if contributor guidance changes.
3. **Reflect New Modules/Workflows:**
   - Ensure architecture or workflow changes are described in the docs.
   - Example:
     ```markdown
     ## Export Module
     Handles exporting stats to CSV and JSON.
     ```

## Testing Patterns

- **Test File Naming:** Use `*.test.*` pattern for test files.
  - Example: `stats.test.rs`, `history.test.rs`
- **Testing Framework:** Not explicitly specified; likely uses Rust's built-in test framework.
- **Test Example:**
  ```rust
  // src/stats.test.rs
  #[cfg(test)]
  mod tests {
      use super::*;
      #[test]
      fn test_calculate_stats() {
          let data = vec![1, 2, 3];
          let stats = calculate_stats(&data);
          assert_eq!(stats.mean, 2.0);
      }
  }
  ```

## Commands

| Command           | Purpose                                                        |
|-------------------|----------------------------------------------------------------|
| /new-tui-feature  | Start a new feature involving both the TUI and stats/history   |
| /update-docs      | Update or add documentation for architecture or workflows      |
```
