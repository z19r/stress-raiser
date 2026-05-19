```markdown
# stress-raiser Development Patterns

> Auto-generated skill from repository analysis

## Overview
This skill teaches you the core development patterns used in the `stress-raiser` Rust codebase. You'll learn about the project's coding conventions, commit message style, file organization, and testing patterns. This guide is ideal for contributors who want to maintain consistency and quality in their contributions.

## Coding Conventions

### File Naming
- Use **camelCase** for file names.
  - Example: `stressCalculator.rs`, `inputParser.rs`

### Imports
- Use **relative imports** for referencing modules within the project.
  - Example:
    ```rust
    mod utils;
    use crate::utils::mathHelpers;
    ```

### Exports
- Use **named exports** to expose functions or structs.
  - Example:
    ```rust
    pub fn calculateStress() { ... }
    pub struct StressResult { ... }
    ```

### Commit Messages
- Follow the **Conventional Commits** style.
- Supported prefixes: `feat`, `ci`, `docs`
- Example:
  ```
  feat: add stress calculation for circular holes
  docs: update README with usage instructions
  ci: add GitHub Actions workflow for tests
  ```

## Workflows

### Feature Development
**Trigger:** When adding a new feature or module  
**Command:** `/feature-development`

1. Create a new file using camelCase, e.g., `newFeature.rs`.
2. Implement your feature using relative imports for dependencies.
3. Export new functions or structs using named exports.
4. Write or update corresponding test files (`*.test.rs`).
5. Commit changes with a `feat:` prefix and a concise description.

### Documentation Update
**Trigger:** When updating or adding documentation  
**Command:** `/docs-update`

1. Edit or add documentation files as needed.
2. Use clear, concise language.
3. Commit changes with a `docs:` prefix.

### Continuous Integration (CI) Update
**Trigger:** When modifying CI configurations  
**Command:** `/ci-update`

1. Edit CI configuration files (e.g., `.github/workflows/`).
2. Ensure all workflows run as expected.
3. Commit changes with a `ci:` prefix.

## Testing Patterns

- Test files use the pattern `*.test.*` (e.g., `stressCalculator.test.rs`).
- Testing framework is not explicitly specified; use Rust's built-in test framework by default.
- Example test:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_calculate_stress() {
          let result = calculateStress(...);
          assert_eq!(result, expected_value);
      }
  }
  ```

## Commands
| Command               | Purpose                                      |
|-----------------------|----------------------------------------------|
| /feature-development  | Start a new feature or module                |
| /docs-update          | Update or add documentation                  |
| /ci-update            | Update CI configuration files                |
```
