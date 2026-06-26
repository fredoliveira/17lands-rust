# 17Lands, but in rust!

[![CI](https://github.com/fredoliveira/17lands-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/fredoliveira/17lands-rust/actions/workflows/ci.yml)

A Rust port of the [17Lands](https://www.17lands.com) MTG Arena log client. It tails
Arena's `Player.log`, parses gameplay events, and uploads them to the 17Lands REST API.

This is a drop-in replacement for the official Python client
([`mtga-log-client`](https://github.com/rconroy293/mtga-log-client)), and as such it aims
for byte-for-byte compatibility with the original client. The motivation to build this stems
from wanting a single binary distribution that requires no old python dependencies.

## Run

```sh
cargo build --release

# Tail the auto-discovered Player.log and upload to 17lands.com:
./target/release/seventeenlands-rust

# Single catch-up pass over a specific log, then exit:
./target/release/seventeenlands-rust --log-file /path/to/Player.log --once
```

Flags: `--log-file <path>`, `--host <url>`, `--token <uuid>`, `--once`.

**Token:** resolved as `--token` → `~/.config/17l/config.toml`¹ → legacy
`~/.mtga_follower.ini` (migrated on first run) → interactive prompt. Get yours at
[17lands.com/account](https://www.17lands.com/account).

> ¹ Platform config dir (`dirs::config_dir()`): `~/.config/17l/` on Linux,
> `~/Library/Application Support/17l/` on macOS, `%APPDATA%\17l\` on Windows.

**Detailed Logs** must be enabled in MTGA (gear → Account → "Detailed Logs") for game data
to be captured.

## Distribute

The release build is a single self-contained binary (TLS via rustls; no OpenSSL or other
system dependency beyond libc):

```sh
cargo build --release          # → target/release/seventeenlands-rust
# install onto this machine:
cargo install --path .
# or just copy the binary to any same-OS/arch machine and run it.
```

For other targets, cross-compile, e.g.:

```sh
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

## Test

```sh
cargo test                                                   # unit + integration + parity
cargo run --example replay -- /path/to/Player.log            # show submissions, send nothing
tools/oracle/run_oracle.sh /path/to/Player.log out.jsonl     # capture the Python client
cargo run --example oracle_diff -- /path/to/Player.log out.jsonl   # diff vs Python (byte-exact)
```

`CLAUDE.md` explains the upstream relationship and how to keep this client compatible.

## Credits & license

This is a Rust port of the official 17Lands client,
[`rconroy293/mtga-log-client`](https://github.com/rconroy293/mtga-log-client) (© its authors,
GPL-3.0). It deliberately mirrors that client's behavior to stay wire-compatible with the
17Lands API, so it is distributed under the same license: GPL-3.0-only. 
See [`LICENSE`](LICENSE) for the full text and [`NOTICE`](NOTICE) for the
attribution/derivation details. Release notes live in [`CHANGELOG.md`](CHANGELOG.md).

Unofficial Fan Content permitted under Wizards of the Coast's Fan Content Policy; not
approved/endorsed by Wizards. Portions © Wizards of the Coast LLC.
