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
# Via Homebrew (macOS/Linux):
brew install fredoliveira/tap/seventeenlands-rust

# From source (builds and installs the binary onto your PATH):
cargo install --git https://github.com/fredoliveira/17lands-rust seventeenlands-rust
```

## Running

```sh
# Assuming a normal installation, run this in your terminal
seventeenlands
```

**Detailed Logs** must be enabled in MTGA (gear → Account → "Detailed Logs") for game data
to be captured.

### Running from the source code directly

```sh
# Build a release version of the CLI
cargo build --release -p seventeenlands-rust

# Start the built artifact
./target/release/seventeenlands
```

## Repository layout

This is a Cargo workspace producing two artifacts from a shared core:

| Crate | Path | Artifact |
|-------|------|----------|
| `seventeenlands-core` | `crates/core` | shared library (follower, parser, REST envelope) |
| `seventeenlands-rust` | `crates/cli` | the `seventeenlands` CLI |
| `seventeenlands-desktop` | `crates/desktop` | Tauri menu-bar app (see its [README](crates/desktop/README.md)) |

Bare `cargo build`/`test` build the CLI + core; build the desktop app with
`-p seventeenlands-desktop` (or `just desktop-build`).

## Credits & license

This is a Rust port of the official 17Lands client,
[`rconroy293/mtga-log-client`](https://github.com/rconroy293/mtga-log-client) (© its authors,
GPL-3.0). It deliberately mirrors that client's behavior to stay wire-compatible with the
17Lands API, so it is distributed under the same license: GPL-3.0-only. 
See [`LICENSE`](LICENSE) for the full text and [`NOTICE`](NOTICE) for the
attribution/derivation details. Release notes live in [`CHANGELOG.md`](CHANGELOG.md).

Unofficial Fan Content permitted under Wizards of the Coast's Fan Content Policy; not
approved/endorsed by Wizards. Portions © Wizards of the Coast LLC.
