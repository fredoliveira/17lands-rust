# CLAUDE.md

## What this is

`seventeenlands-rust` is a Rust port of the official 17Lands MTG Arena log client. It is a
**drop-in replacement**: it tails the same `Player.log`, parses the same messages, and POSTs
the **same payloads to the same endpoints** as the original, so the 17Lands server accepts
its uploads identically.

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
cargo run --example oracle_diff -- <Player.log> out.jsonl   # must report ALL ... byte-identical
```

When upstream bumps its version or changes a handler:
1. Diff upstream `mtga_follower.py` / `api_client.py` against `/Users/fred/code/oss/17l/...`.
2. Port the change here; update `api_client::CLIENT_VERSION` if upstream's changed.
3. Re-capture the oracle and re-run `oracle_diff` until byte-identical again.

## Module map

`main.rs` (CLI + processing loop) · `config.rs` (token) · `paths.rs` (log discovery) ·
`follower.rs` (tailing, dispatch table, all handlers, game-state machine) ·
`api_client.rs` (endpoints, envelope, gzip, JSON) · `retry.rs` · `time_parse.rs`.
Each module's doc comment cites the relevant `mtga_follower.py` lines.

## Conventions

- **Do not POST to the live `api.17lands.com`** during development without explicit user
  approval; validate against the local mock/oracle instead.
- Deviations from upstream are intentional and limited: token at the
  platform config dir (migrated from `~/.mtga_follower.ini`), no GUI prompts, no startup
  version check, no server-side error reporting, stdout/stderr logging only.
