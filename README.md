# 17Lands, but in rust!

[![CI](https://github.com/fredoliveira/17lands-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/fredoliveira/17lands-rust/actions/workflows/ci.yml)

A Rust port of the [17Lands](https://www.17lands.com) MTG Arena log client. It tails
Arena's `Player.log`, parses gameplay events, and uploads them to the 17Lands REST API.

This is a drop-in replacement for the official Python client
([`mtga-log-client`](https://github.com/rconroy293/mtga-log-client)), and as such it aims
for byte-for-byte compatibility with the original client. The motivation to build this stems
from wanting a single binary distribution that requires no old python dependencies.

## Install

```sh
# CLI via Homebrew (macOS/Linux):
brew install fredoliveira/tap/seventeenlands-rust

# Desktop menu-bar app via Homebrew (macOS):
brew install --cask fredoliveira/tap/seventeenlands-desktop

# CLI from source (builds and installs the binary onto your PATH):
cargo install --git https://github.com/fredoliveira/17lands-rust seventeenlands-rust
```

## Running

```sh
# Assuming a normal installation, run this in your terminal
seventeenlands
```

**Detailed Logs** must be enabled in MTGA (gear → Account → "Detailed Logs") for game data
to be captured.

### Forwarding events to a local app

`--tee <URL>` also POSTs every parsed event to a local HTTP sink (e.g. a live draft
companion), as JSON at `<URL>/<event>` — for example `http://localhost:3000/add_human_draft_pack`:

```sh
seventeenlands --tee http://localhost:3000
```

This is fire-and-forget: a missing or slow sink never affects the 17Lands upload.

## Repository layout

This is a Cargo workspace producing two artifacts from a shared core:

| Crate | Path | Artifact |
|-------|------|----------|
| `seventeenlands-core` | `crates/core` | shared library (follower, parser, REST envelope) |
| `seventeenlands-rust` | `crates/cli` | the `seventeenlands` CLI |
| `seventeenlands-desktop` | `crates/desktop` | Tauri menu-bar app (see its [README](crates/desktop/README.md)) |

Bare `cargo build`/`test` build the CLI + core; the desktop app builds with
`-p seventeenlands-desktop`.

## Development

Common tasks live in the [`justfile`](justfile) (`brew install just`):

| Command | What it does |
|---------|--------------|
| `just run [ARGS]` | Build and run the CLI against the auto-detected `Player.log` |
| `just desktop-run` | Build and run the desktop app (release; no Tauri CLI needed) |
| `just desktop-dev` | Desktop dev loop with hot reload, pointed at a local mock API |
| `just desktop-build` | Build the desktop bundle (`.app` + `.dmg`; needs the Tauri CLI) |
| `just test` | Run the test suite |
| `just lint` | Pre-commit gate: format check + clippy + tests (mirrors CI) |
| `just parity LOG` | Byte-for-byte output parity check against the Python client |
| `just replay LOG` | Replay a log offline, printing payloads without uploading |

## Credits & license

This is a Rust port of the official 17Lands client,
[`rconroy293/mtga-log-client`](https://github.com/rconroy293/mtga-log-client) (© its authors,
GPL-3.0). It deliberately mirrors that client's behavior to stay wire-compatible with the
17Lands API, so it is distributed under the same license: GPL-3.0-only. 
See [`LICENSE`](LICENSE) for the full text and [`NOTICE`](NOTICE) for the
attribution/derivation details. Release notes live in [`CHANGELOG.md`](CHANGELOG.md).

Unofficial Fan Content permitted under Wizards of the Coast's Fan Content Policy; not
approved/endorsed by Wizards. Portions © Wizards of the Coast LLC.
