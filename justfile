# recall task runner (`brew install just`). `just` lists recipes.
# Plain builds are just cargo: `cargo build`, `cargo test`, `cargo clean`, …

# Show available recipes.
default:
    @just --list

# --- Run ---------------------------------------------------------------------

# Extra args pass through: `just run --once`, `just run -l path/to/Player.log`.
# Build and run the CLI, following the auto-detected Player.log.
run *ARGS:
    cargo run -p recall -- {{ARGS}}

# --- Desktop app -------------------------------------------------------------

# Build and run the desktop app (release; self-contained binary, no Tauri CLI needed).
desktop-run:
    cargo run -p recall-desktop --release

# Needs the Tauri CLI (`cargo install tauri-cli --locked`) and the local mock so it
# never hits the live API: `python3 tools/oracle/mock_server.py 8732 /tmp/desktop-out.jsonl`.
# Desktop dev loop with webview hot-reload, pointed at the local mock on :8732.
desktop-dev:
    cd crates/desktop && RECALL_HOST=http://127.0.0.1:8732 cargo tauri dev

# Build the desktop bundle (.app + .dmg on macOS; needs the Tauri CLI).
desktop-build:
    cd crates/desktop && cargo tauri build

# --- Quality -----------------------------------------------------------------

# Run the full test suite.
test:
    cargo test

# Pre-commit gate, mirroring CI: format check + clippy (all crates) + tests.
lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo clippy -p recall-desktop --all-targets -- -D warnings
    cargo test

# --- Oracle parity (see CLAUDE.md) -------------------------------------------

# Capture the Python oracle's output for a log into OUT (local mock, sandboxed HOME).
oracle LOG OUT="out.jsonl":
    tools/oracle/run_oracle.sh {{LOG}} {{OUT}}

# Diff this client's output against a captured oracle file; must report byte-identical.
oracle-diff LOG OUT="out.jsonl":
    cargo run -p recall-core --example oracle_diff -- {{LOG}} {{OUT}}

# Full parity check: capture the oracle, then diff against it.
parity LOG OUT="out.jsonl": (oracle LOG OUT) (oracle-diff LOG OUT)

# Replay a log through the client offline (prints payloads, uploads nothing).
replay LOG:
    cargo run -p recall-core --example replay -- {{LOG}}
