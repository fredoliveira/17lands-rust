//! Timestamp parsing & serialization (SPEC §5.8, §11.3).
//!
//! - `extract_time`: try the full `TIME_FORMATS` list (mtga_follower.py:146-158).
//! - `maybe_get_utc_timestamp`: ms-since-epoch / .NET-ticks / ISO-8601 branches.
//! - Output must match Python `datetime.isoformat()` byte-for-byte — RESOLVED EMPIRICALLY
//!   against the Python oracle (SPEC §11.3), not hard-specified here.

#![allow(dead_code)]

use chrono::NaiveDateTime;

/// Parse a raw log timestamp string by trying each known format (SPEC §5.8).
pub fn extract_time(_time_str: &str) -> Result<NaiveDateTime, String> {
    todo!("SPEC §5.8 — port TIME_FORMATS")
}

/// Serialize a datetime the way Python `datetime.isoformat()` does (SPEC §11.3).
pub fn isoformat(_dt: &NaiveDateTime) -> String {
    todo!("SPEC §11.3 — verify against oracle")
}
