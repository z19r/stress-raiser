# Stress riser - beautiful Rust stress testing TUI
# Run with no params: just

default:
  cargo run

build:
  cargo build

build-release:
  cargo build --release

test:
  cargo test

fmt:
  cargo fmt

check:
  cargo check
  cargo clippy

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
