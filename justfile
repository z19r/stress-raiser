# Stress raiser - beautiful Rust stress testing TUI

# Show this help
default:
  just --list

# Run the stress-test TUI
run:
  cargo run

# Build debug
build:
  cargo build

# Build release binary
build-release:
  cargo build --release

# Run tests
test:
  cargo test

# Format code
fmt:
  cargo fmt

# Check + clippy
check:
  cargo check
  cargo clippy

# Build and open API docs
doc:
  cargo doc --no-deps --open

# Create .crate in target/package for inspection (no upload)
package:
  cargo package

# Run fmt, clippy, tests, then dry-run publish (gate before publish)
publish-check:
  just fmt
  just check
  just test
  cargo publish --dry-run

# Pack and verify without uploading (run before publish)
publish-dry-run:
  cargo publish --dry-run

# Publish to crates.io (run just publish-check first; requires cargo login)
publish:
  cargo publish
