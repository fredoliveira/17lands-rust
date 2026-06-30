# CLAUDE.md

## What this is

This project is a Rust port of the official 17Lands MTG Arena log client. The core (and the
`seventeenlands` CLI built on it) is a **drop-in replacement**: it tails the same `Player.log`,
parses the same messages, and POSTs the **same payloads to the same endpoints** as the original,
so the 17Lands server accepts its uploads identically. A desktop app reuses the same core (see
Module map).

## What it's based on (track this for compatibility)

Upstream: **https://github.com/rconroy293/mtga-log-client** (the Python `seventeenlands`
package). This port targets upstream **`CLIENT_VERSION = "0.1.44.p"`**, sent verbatim in
every payload so the server treats us as the trusted Python client.

References available while working:
- Python source we ported from: `/Users/fred/code/oss/17l/src/python/seventeenlands/`
  (`mtga_follower.py`, `api_client.py`, `retry_utils.py`) — the authoritative behavior.
- Runnable oracle: the brew-installed `seventeenlands` binary (same 0.1.44 code).

## Prime directive: stay wire-compatible

Mirror the Python client **exactly**, including oddities (dispatch order, defaultdict
semantics, numeric coercions, variable-reuse quirks). When upstream and "clean" disagree,
match upstream. The make-or-break details — the ones that bit us:

- `serde_json` is built with `preserve_order`; payloads are emitted with a Python-`json.dumps`
  separator formatter (`api_client::to_python_json_vec`) so key order + spacing match.
- Null vs absent: build payloads from `serde_json::Value`/`Map` and **emit `null`** (never
  `skip_serializing_if`).
- Time serialization mimics Python `datetime.isoformat()` (naive, no offset; 6-digit µs only
  when non-zero). The `.NET`-ticks branch must reproduce Python's **lossy f64** division
  (see `time_parse::from_dotnet_ticks`), not exact integer math.
- Use `Map::shift_remove` (order-preserving) to mirror Python `dict.pop`, never `remove`
  (which is `swap_remove` under `preserve_order`).
- The game-history `_timestamp` is driven by `EventTime` (usually absent → `null`), not the
  utc timestamp — a deliberate upstream variable-reuse quirk (`follower::handle_blob`).

## How to verify / maintain compatibility

Parity is proven by diffing this client's output against the live Python client, byte for
byte. After **any** change here — or whenever **upstream releases a new version** — re-run:

```sh
cargo test
tools/oracle/run_oracle.sh <Player.log> out.jsonl      # capture Python client (local mock, sandboxed HOME)
cargo run -p seventeenlands-core --example oracle_diff -- <Player.log> out.jsonl   # must report ALL ... byte-identical
```

When upstream bumps its version or changes a handler:
1. Diff upstream `mtga_follower.py` / `api_client.py` against `/Users/fred/code/oss/17l/...`.
2. Port the change here; update `api_client::CLIENT_VERSION` if upstream's changed.
3. Re-capture the oracle and re-run `oracle_diff` until byte-identical again.

## Module map

The repo is a Cargo workspace with three crates under `crates/`:

- **`crates/core`** (`seventeenlands-core`) — the wire-compatible library. Modules:
  `config.rs` (token) · `paths.rs` (log discovery) · `follower.rs` (tailing, dispatch table,
  all handlers, game-state machine) · `api_client.rs` (endpoints, envelope, gzip, JSON) ·
  `retry.rs` · `time_parse.rs`. Each module's doc comment cites the relevant `mtga_follower.py`
  lines. Parity tests + oracle examples live here (`tests/`, `examples/`).
- **`crates/cli`** (`seventeenlands-rust`) — `main.rs` (CLI + processing loop). Produces the
  installable `seventeenlands` binary; keeps this crate name so `cargo install` keeps working.
- **`crates/desktop`** (`seventeenlands-desktop`) — a **Tauri v2 menu-bar app** reusing the core
  as a library, never touching payload construction. It observes uploads structurally via an
  `ObservingSubmitter` (a `Submitter` decorator passed to `Follower::with_submitter`) and mirrors
  the `log` feed into the webview. See `crates/desktop/README.md`.

`default-members` is core + cli, so bare `cargo` commands (and Linux CI) skip the desktop crate.

## Conventions

- **Do not POST to the live `api.17lands.com`** during development without explicit user
  approval; validate against the local mock/oracle instead.
- Deviations from upstream are intentional and limited: token at the
  platform config dir (migrated from `~/.mtga_follower.ini`), no GUI prompts, no startup
  version check, no server-side error reporting, stdout/stderr logging only.
- A few **additive, non-wire** seams exist for the desktop app and do not change any payload,
  dispatch order, or send timing (parity tests prove this): `config::{read_toml_token,
  write_toml_token}` are `pub`; `Follower::parse_log_cancellable` adds cooperative
  cancellation (`parse_log` delegates to it with an always-false flag); and
  `Follower::parse_log_cancellable_from` adds resume-from-offset + read-position reporting
  (`parse_log_cancellable` delegates to it with `start_offset = 0` / no position sink, which
  is byte-identical to the original loop).
