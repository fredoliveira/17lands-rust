//! Fixture-based parity tests (SPEC §12).
//!
//! Each fixture under `tests/fixtures/` is raw `Player.log` lines plus the expected
//! sequence of `(endpoint, payload)` submissions. The harness feeds lines through
//! `Follower` with a mock `Submitter` that records calls instead of sending.
//!
//! The Python client serves as the oracle: run it against the same fixtures pointed at a
//! local mock server, capture its payloads, and assert the Rust output is byte-identical.
//! This is what settles the deferred decisions — time serialization (§11.3) and
//! `client_version` (§11.1).
//!
//! Coverage target (SPEC §12): bot draft pack+pick; human draft (combined / Draft.Notify /
//! EventPlayerDraftMakePick); deck submission; a full game (opening hand, mulligans,
//! game-over, match result → add_game); rank; account/screen-name; inventory; collection;
//! ongoing events; claim prize; event course; reconnect.

#[test]
#[ignore = "scaffold — implement once Follower + fixtures exist (SPEC §12)"]
fn parity_placeholder() {}
