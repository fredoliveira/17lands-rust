# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-06-27

### Changed
- The installed executable is now named `seventeenlands` (was `seventeenlands-rust`),
  matching the official Python client it replaces. The crate, repository, and release
  artifacts keep the `seventeenlands-rust` name.

## [0.1.0] - 2026-06-26

Initial release. A drop-in, wire-compatible Rust port of the official 17Lands MTG Arena log
client ([`mtga-log-client`](https://github.com/rconroy293/mtga-log-client)), targeting
upstream `CLIENT_VERSION = "0.1.44.p"`.

### Added
- Tails Arena's `Player.log`, parses gameplay events, and uploads them to the 17Lands REST
  API with byte-for-byte payload parity against the Python client.
- CLI flags: `--log-file`, `--host`, `--token`, `--once`.
- Token resolution chain: flag → platform config dir → legacy `~/.mtga_follower.ini`
  (migrated on first run) → interactive prompt.
- Oracle parity tooling (`tools/oracle/`, `examples/oracle_diff.rs`) and an offline
  `replay` example.

[Unreleased]: https://github.com/fredoliveira/17lands-rust/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/fredoliveira/17lands-rust/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/fredoliveira/17lands-rust/releases/tag/v0.1.0
