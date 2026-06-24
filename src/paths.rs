//! MTGA `Player.log` discovery (SPEC §5.2).
//!
//! Port `POSSIBLE_ROOTS` × {`Player.log`, `Player-prev.log`} verbatim from
//! `mtga_follower.py:54-128`: OSX `~/Library/Logs`, Steam Proton compatdata `2141910`,
//! Lutris, Wine (`$WINEPREFIX`), Windows `C:/`+`D:/` `users/<user>/AppData/LocalLow`.

#![allow(dead_code)]

use std::path::PathBuf;

/// Candidate current-log paths (`Player.log`), in priority order.
pub fn possible_current_filepaths() -> Vec<PathBuf> {
    todo!("SPEC §5.2")
}

/// Candidate previous-log paths (`Player-prev.log`), in priority order.
pub fn possible_previous_filepaths() -> Vec<PathBuf> {
    todo!("SPEC §5.2")
}
