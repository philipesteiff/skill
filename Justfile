# Common commands for this repo. Run with `just <task>`.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# List available tasks.
default:
  @just --list

# Build the CLI in debug mode.
build:
  cargo build

# Run the CLI (pass args after --).
run *args:
  cargo run -- {{args}}

# Run tests.
test:
  cargo test

# Format code with rustfmt.
fmt:
  cargo fmt

# Run clippy lints.
clippy:
  cargo clippy --all-targets --all-features

# Format + lint.
lint:
  cargo fmt
  cargo clippy --all-targets --all-features

# Build release binary.
release:
  cargo build --release

# Package a release archive.
package:
  scripts/package.sh

# Install the release binary locally.
install:
  scripts/install.sh

# Clean build artifacts.
clean:
  cargo clean

# Create a local playground with sample repos.
playground:
  scripts/playground_setup.sh

# Reset the playground (deletes playground/work).
playground-clean:
  CLEAN=1 scripts/playground_setup.sh
