//! REST client for the 17Lands API (SPEC §9, port of `api_client.py`).
//!
//! - Every payload is wrapped by the base envelope (`add_base_api_data`, SPEC §9):
//!   token, client_version, player_id, time, utc_time, event_time, raw_time, ...blob.
//!   Fields are emitted even when null (SPEC §11.2).
//! - Only `add_game` is gzipped in the kept endpoint subset.
//! - `client_version_validation`, `log_errors`, `add_event` are dropped (SPEC §2).

#![allow(dead_code)]

use serde_json::Value;

pub const DEFAULT_HOST: &str = "https://api.17lands.com";

/// Client version string sent in every payload (SPEC §11.1).
///
/// Defaults to impersonating the Python client for guaranteed acceptance; revisit after
/// live testing (switch to "0.1.44.r" only if 17Lands accepts a distinct identifier).
pub const CLIENT_VERSION: &str = "0.1.44.p";

/// Submits parsed events to the 17Lands REST API.
///
/// For tests, this is mocked to record `(endpoint, payload)` instead of sending (SPEC §12).
pub trait Submitter {
    fn submit(&mut self, endpoint: &str, payload: Value, gzip: bool);
}

pub struct ApiClient {
    pub host: String,
}

impl ApiClient {
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into() }
    }

    // Endpoint methods (SPEC §9 table) — e.g. submit_draft_pack -> "api/client/add_pack",
    // submit_game_result -> "api/client/add_game" (gzip), etc. TODO milestone 2.
}
