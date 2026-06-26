# seventeenlands-rust task runner.
# Install just: https://github.com/casey/just  (`brew install just`)
# List recipes: `just` or `just --list`.

# Show available recipes.
default:
    @just --list

# --- Build -----------------------------------------------------------------

# Debug build.
build:
    cargo build

# Optimized release build.
release:
    cargo build --release

# Type-check without producing binaries (fast feedback loop).
check:
    cargo check

# --- Run -------------------------------------------------------------------

# Build and run, following the auto-detected Player.log. Extra args pass through,
# e.g. `just run --once` or `just run -l path/to/Player.log`.
run *ARGS:
    cargo run -- {{ARGS}}

# Parse a specific log once and exit (no tailing).
run-once LOG:
    cargo run -- --log-file {{LOG}} --once

# Release run with passthrough args.
run-release *ARGS:
    cargo run --release -- {{ARGS}}

# --- Test / quality --------------------------------------------------------

# Run the full test suite.
test:
    cargo test

# Format the workspace.
fmt:
    cargo fmt

# Check formatting without modifying files (CI-style).
fmt-check:
    cargo fmt --check

# Lint with clippy, treating warnings as errors.
clippy:
    cargo clippy --all-targets -- -D warnings

# Format check + clippy + tests, the pre-commit gate.
lint: fmt-check clippy test

# --- Oracle parity (see CLAUDE.md) -----------------------------------------

# Capture the Python oracle's output for a log into OUT (local mock, sandboxed HOME).
oracle LOG OUT="out.jsonl":
    tools/oracle/run_oracle.sh {{LOG}} {{OUT}}

# Diff this client's output against a captured oracle file; must report byte-identical.
oracle-diff LOG OUT="out.jsonl":
    cargo run --example oracle_diff -- {{LOG}} {{OUT}}

# Full parity check: capture the oracle, then diff against it.
parity LOG OUT="out.jsonl": (oracle LOG OUT) (oracle-diff LOG OUT)

# Replay a log through the client offline (the replay example).
replay LOG:
    cargo run --example replay -- {{LOG}}

# --- Housekeeping ----------------------------------------------------------

# Remove build artifacts.
clean:
    cargo clean
