# Justfile for minicode ⚡
# Optimized for low-resource environments (limited concurrency, low CPU/RAM usage)

# Set shell
set shell := ["bash", "-c"]

# Default recipe lists all available commands
default:
    @just --list

# Fast compilation check (2 parallel jobs)
check:
    cargo check -j 2

# Fast check with all features enabled
check-all:
    cargo check --all-features -j 2

# Build debug binary (2 parallel jobs)
build:
    cargo build -j 2

# Build optimized release binary (2 parallel jobs)
build-release:
    cargo build --release -j 2

# Run all unit and integration tests (2 jobs, 2 test threads to prevent lag)
test:
    cargo test -j 2 -- --test-threads=2

# Run a specific test by name
test-exact name:
    cargo test -j 2 -- {{name}} --exact

# Run clippy linter with strict warning enforcement
clippy:
    cargo clippy -j 2 -- -D warnings

# Check code formatting without modifying files
fmt-check:
    cargo fmt --check

# Auto-format all Rust source files
fmt:
    cargo fmt

# Run interactive TUI mode (default)
run *args:
    cargo run -j 2 -- {{args}}

# Run interactive configuration wizard
configure:
    cargo run -j 2 -- configure

# Run headless machine-readable NDJSON streaming mode
run-stream *args:
    cargo run -j 2 -- --json-stream {{args}}

# Run a one-shot autonomous task
run-task task *args:
    cargo run -j 2 -- run "{{task}}" {{args}}

# Full developer pre-commit verification suite (fmt, check, clippy, test)
ci: fmt-check check clippy test
    @echo "✅ All CI checks passed cleanly!"

# Clean build artifacts to free disk space
clean:
    cargo clean
