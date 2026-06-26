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
cargo install --git https://github.com/fredoliveira/17lands-rust
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
# Build a release version
cargo build --release

# Start the built artifact
./target/release/seventeenlands
```

## Card names in draft logs (optional)

By default the draft-pick log lines show the bare Arena card ID that was picked. To see card
**names** instead, build a one-off Arena-ID → name map from
[MTGJSON](https://mtgjson.com/data-models/identifiers/):

```sh
just card-db                                    # downloads MTGJSON, writes to the config dir
# or directly (the underlying cargo example):
cargo run --example build_card_db -- [--source AllIdentifiers.json] [--out PATH]
```

This writes `arena_names.json` next to the token config (`<config>/17l/`), which the client
loads automatically on the next run. It is **console-only** — purely cosmetic logging that
never affects the uploaded payloads, so it stays clear of the wire-compatibility contract.
Override the lookup path with `SEVENTEENLANDS_CARD_DB`; if the file is absent, logs simply
fall back to raw IDs.

## Credits & license

This is a Rust port of the official 17Lands client,
[`rconroy293/mtga-log-client`](https://github.com/rconroy293/mtga-log-client) (© its authors,
GPL-3.0). It deliberately mirrors that client's behavior to stay wire-compatible with the
17Lands API, so it is distributed under the same license: GPL-3.0-only. 
See [`LICENSE`](LICENSE) for the full text and [`NOTICE`](NOTICE) for the
attribution/derivation details. Release notes live in [`CHANGELOG.md`](CHANGELOG.md).

Unofficial Fan Content permitted under Wizards of the Coast's Fan Content Policy; not
approved/endorsed by Wizards. Portions © Wizards of the Coast LLC.
