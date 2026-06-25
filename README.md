# seventeenlands-rust

A Rust port of the [17Lands](https://www.17lands.com) MTG Arena log client — tails
Arena's `Player.log`, parses gameplay events, and uploads them to the 17Lands REST API.
It is a **drop-in replacement** for the Python `seventeenlands` client: same parsing, same
payloads, same endpoints.

> Status: **functional**. All ~20 message handlers are ported. Output is validated
> **byte-for-byte against the reference Python client** via the oracle harness: 173
> submissions across two real logs plus a synthetic gap fixture are identical (see
> _Testing_ below). See `SPEC.md` for the full build specification.

## Layout

| Path | Purpose |
|---|---|
| `SPEC.md` | The build specification — every decision and the porting reference. |
| `src/main.rs` | CLI + processing loop (SPEC §5.3). |
| `src/config.rs` | Token resolution + `~/.config/17l/config.toml` (SPEC §5.1). |
| `src/paths.rs` | `Player.log` discovery (SPEC §5.2). |
| `src/follower.rs` | Tailing, dispatch, handlers, game-state machine (SPEC §5–8). |
| `src/api_client.rs` | REST client + envelope + gzip (SPEC §9). |
| `src/retry.rs` | Exponential-backoff retry (SPEC §10). |
| `src/time_parse.rs` | Timestamp parsing/serialization (SPEC §5.8, §11.3). |
| `tests/parity.rs` | Fixture + oracle parity tests (SPEC §12). |

## Differences from the Python client

- Token lives at `~/.config/17l/config.toml` (migrated once from `~/.mtga_follower.ini`).
- No GUI prompts, no startup version check, no server-side error reporting, no rotating
  file logs (logs to stdout/stderr). See `SPEC.md` §2.

## Build & run

```sh
cargo build --release
# resolves a token (flag > ~/.config/17l/config.toml > legacy ~/.mtga_follower.ini > stdin),
# then tails the auto-discovered Player.log:
./target/release/seventeenlands-rust
# or point it at a specific log and parse once:
./target/release/seventeenlands-rust --log-file path/to/Player.log --once
```

## Testing & the oracle harness (SPEC §12)

```sh
cargo test          # unit + integration (HTTP, fixture parity)
```

Parity with the Python client is proven by capturing the reference client's payloads and
diffing them against this client's output:

```sh
# 1. Capture the Python client's POSTs for a log (runs it against a local mock server in a
#    sandboxed HOME — never touches the live API or your real config):
tools/oracle/run_oracle.sh path/to/Player.log out.jsonl

# 2. Diff this client's output against that capture (exits non-zero on any difference):
cargo run --example oracle_diff -- path/to/Player.log out.jsonl

# Inspect what a log would submit, without sending anything:
cargo run --example replay -- path/to/Player.log
```

`tests/fixtures/gaps.log` + `tests/parity.rs` cover the dispatch branches absent from the
sample logs (bot draft, combined human draft, claim prize, event course, inventory,
collection); the branches present in real logs — including the full game-state machine —
are covered by the oracle diff.

## License

GPL-3.0-only, matching the upstream 17Lands client.
