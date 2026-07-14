# CLAUDE.md

## What this is

This project is **Recall** (named after Ancestral Recall), a Rust port of the official
17Lands MTG Arena log client. The core (and the `recall` CLI built on it) is a **drop-in
replacement**: it tails the same `Player.log`,
parses the same messages, and POSTs the **same payloads to the same endpoints** as the original,
so the 17Lands server accepts its uploads identically. A desktop app reuses the same core (see
Module map).

## What it's based on (track this for compatibility)

Upstream: **https://github.com/rconroy293/mtga-log-client** (the Python `seventeenlands`
package). This port targets upstream **`CLIENT_VERSION = "0.1.44.p"`**, sent verbatim in
every payload so the server treats us as the trusted Python client.

Reference available while working: the Python source we ported from, at
`/Users/fred/code/oss/17l/src/python/seventeenlands/` (`mtga_follower.py`, `api_client.py`,
`retry_utils.py`) — the authoritative behavior.

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

Parity is enforced by the fixture tests in `crates/core/tests/parity.rs` (`cargo test`).
Their expected payloads were captured byte-for-byte from the live Python client during the
port; treat them as the wire contract. (A live "oracle" harness that diffed against a
running Python client existed early on and was removed — the Python client is no longer
installed here, so parity is now source-diff + fixtures.)

When upstream bumps its version or changes a handler:
1. Diff upstream `mtga_follower.py` / `api_client.py` against the reference copy at
   `/Users/fred/code/oss/17l/...`.
2. Port the change here; update `api_client::CLIENT_VERSION` if upstream's changed; derive
   any new/changed expected fixture payloads from the Python source (order, null-vs-absent,
   coercions).
3. `cargo test`, then update the reference copy so the next upstream diff starts clean.

## Module map

The repo is a Cargo workspace with three crates under `crates/`:

- **`crates/core`** (`recall-core`) — the wire-compatible library. Modules:
  `config.rs` (token) · `paths.rs` (log discovery) · `follower.rs` (tailing, dispatch table,
  all handlers, game-state machine) · `api_client.rs` (endpoints, envelope, gzip, JSON) ·
  `retry.rs` · `time_parse.rs`. Each module's doc comment cites the relevant `mtga_follower.py`
  lines. Parity tests + an offline `replay` example live here (`tests/`, `examples/`).
- **`crates/cli`** (`recall`) — `main.rs` (CLI + processing loop). Produces the
  installable `recall` binary.
- **`crates/desktop`** (`recall-desktop`) — a **Tauri v2 menu-bar app** reusing the core
  as a library, never touching payload construction. It observes uploads structurally via an
  `ObservingSubmitter` (a `Submitter` decorator passed to `Follower::with_submitter`) and mirrors
  the `log` feed into the webview. See `crates/desktop/README.md`.

`default-members` is core + cli, so bare `cargo` commands (and Linux CI) skip the desktop crate.

## Conventions

- **Do not POST to the live `api.17lands.com`** during development without explicit user
  approval; point at the local mock (`tools/mock_server.py`) instead.
- **Documentation is crisp and minimal.** No long preambles, no marketing fluff, no
  restating what the code shows; prefer a short table or list over paragraphs. Same for
  the justfile: only commands that encode non-obvious knowledge, never bare cargo aliases.
- Deviations from upstream are intentional and limited: token at
  `<config_dir>/recall/config.toml` (migrated from `~/.mtga_follower.ini`), no GUI prompts,
  no startup version check, no server-side error reporting, stdout/stderr logging only.
- **Naming:** the product is Recall (`recall` CLI, Recall.app), but on the wire it stays the
  Python client — never rename anything that reaches the server (payloads, `CLIENT_VERSION`,
  endpoints), and keep "17Lands" wherever it refers to the service itself.
- A few **additive, non-wire** seams exist for the desktop app and do not change any payload,
  dispatch order, or send timing (parity tests prove this): `config::{read_toml_token,
  write_toml_token}` are `pub`; `Follower::parse_log_cancellable` adds cooperative
  cancellation (`parse_log` delegates to it with an always-false flag); and
  `Follower::parse_log_cancellable_from` adds resume-from-offset + read-position reporting
  (`parse_log_cancellable` delegates to it with `start_offset = 0` / no position sink, which
  is byte-identical to the original loop).
