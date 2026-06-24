//! The stateful log follower: tailing, line accumulation, dispatch, and all handlers
//! (SPEC §5.4–5.8, §6, §7, §8 — port of the `Follower` class in `mtga_follower.py`).
//!
//! Highest-risk area is the game-state reconstruction (SPEC §8): defaultdict semantics,
//! the `.pop()`-mutates-then-store deck handling, and the `copy.deepcopy` of the queued
//! game. Gate that work behind the oracle/parity tests (SPEC §12).

#![allow(dead_code)]

/// Mirrors `Follower` state (SPEC §7). Reset wholesale on each outer tail-loop pass.
pub struct Follower {
    // TODO(SPEC §7): timing, identity, draft/event, game/board, pending submission, buffering.
    // (recent_lines is intentionally dropped — SPEC §14 #4.)
}

impl Follower {
    pub fn new(_token: String, _host: String) -> Self {
        todo!("SPEC §7 — _reinitialize()")
    }

    /// Tail (or read once) a log file, dispatching complete entries (SPEC §5.4).
    pub fn parse_log(&mut self, _filename: &str, _follow: bool) {
        todo!("SPEC §5.4")
    }

    // Internals to port:
    //   append_line / handle_complete_log_entry          (SPEC §5.5)
    //   handle_blob / extract_payload                     (SPEC §5.6, §5.6.1)
    //   maybe_handle_account_info                         (SPEC §5.7)
    //   the dispatch table                                (SPEC §6)
    //   ~30 handle_* methods + game-state machine         (SPEC §8)
}
