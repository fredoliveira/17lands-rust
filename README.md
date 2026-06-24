# seventeenlands-rust

A Rust port of the [17Lands](https://www.17lands.com) MTG Arena log client — tails
Arena's `Player.log`, parses gameplay events, and uploads them to the 17Lands REST API.
It is a **drop-in replacement** for the Python `seventeenlands` client: same parsing, same
payloads, same endpoints.

> Status: **scaffold**. Not yet functional — modules are stubs. See `SPEC.md` for the full
> build specification and `SPEC.md` §13 for the implementation order.

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

## Build

```sh
cargo build
cargo test
```

## License

GPL-3.0-only, matching the upstream 17Lands client.
