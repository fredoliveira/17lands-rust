// SPDX-License-Identifier: GPL-3.0-only

//! REST client for the 17Lands API (port of `api_client.py`).
//!
//! - Every payload is wrapped by the base envelope (`_add_base_api_data`) *in the Follower*
//!   (it owns token + identity + timestamps); this module receives an already-enveloped
//!   `Value` and posts it. Fields are emitted even when null.
//! - Only `add_game` is gzipped in the kept endpoint subset.
//! - `client_version_validation`, `log_errors`, `add_event` are dropped.
//!
//! ## JSON serialization
//! Python POSTs via `requests`, whose body is `json.dumps(blob)` with the default
//! `(", ", ": ")` separators. We serialize with a matching formatter so ASCII payloads are
//! byte-identical to the Python client. (Python also defaults to `ensure_ascii=True`,
//! \u-escaping non-ASCII; we emit raw UTF-8 instead — still valid JSON the server parses
//! identically.)

#![allow(dead_code)]

use std::io::Write;

use serde::Serialize;
use serde_json::Value;

pub const DEFAULT_HOST: &str = "https://api.17lands.com";

/// Client version string sent in every payload.
///
/// Defaults to impersonating the Python client for guaranteed acceptance; revisit after
/// live testing (switch to "0.1.44.r" only if 17Lands accepts a distinct identifier).
pub const CLIENT_VERSION: &str = "0.1.44.p";

/// Endpoint paths. Kept subset only.
pub mod endpoints {
    pub const UPDATE_CARD_COLLECTION: &str = "api/client/update_card_collection";
    pub const ADD_DECK: &str = "api/client/add_deck";
    pub const ADD_PACK: &str = "api/client/add_pack";
    pub const ADD_PICK: &str = "api/client/add_pick";
    pub const UPDATE_EVENT_COURSE: &str = "api/client/update_event_course";
    pub const RECORD_EVENT_JOIN: &str = "api/client/record_event_join";
    pub const MARK_EVENT_ENDED: &str = "api/client/mark_event_ended";
    pub const ADD_GAME: &str = "api/client/add_game";
    pub const ADD_HUMAN_DRAFT_PACK: &str = "api/client/add_human_draft_pack";
    pub const ADD_HUMAN_DRAFT_PICK: &str = "api/client/add_human_draft_pick";
    pub const UPDATE_INVENTORY: &str = "api/client/update_inventory";
    pub const UPDATE_ONGOING_EVENTS: &str = "api/client/update_ongoing_events";
    pub const UPDATE_PLAYER_PROGRESS: &str = "api/client/update_player_progress";
    pub const ADD_RANK: &str = "api/client/add_rank";
    pub const ADD_MTGA_ACCOUNT: &str = "api/client/add_mtga_account";
}

/// Submits parsed events to the 17Lands REST API.
///
/// The required method is `submit(endpoint, payload, use_gzip)`; the named helpers below
/// encode the endpoint/gzip mapping so the Follower's call sites read like the
/// Python `api_client` (`submit_draft_pack`, `submit_game_result`, …). For tests, the
/// trait is implemented by [`RecordingSubmitter`], which records calls instead of sending.
pub trait Submitter {
    /// Perform (or record) a POST of an already-enveloped blob.
    fn submit(&mut self, endpoint: &str, payload: Value, use_gzip: bool);

    fn submit_collection(&mut self, blob: Value) {
        self.submit(endpoints::UPDATE_CARD_COLLECTION, blob, false);
    }
    fn submit_deck_submission(&mut self, blob: Value) {
        self.submit(endpoints::ADD_DECK, blob, false);
    }
    fn submit_draft_pack(&mut self, blob: Value) {
        self.submit(endpoints::ADD_PACK, blob, false);
    }
    fn submit_draft_pick(&mut self, blob: Value) {
        self.submit(endpoints::ADD_PICK, blob, false);
    }
    fn submit_event_course_submission(&mut self, blob: Value) {
        self.submit(endpoints::UPDATE_EVENT_COURSE, blob, false);
    }
    fn submit_joined_event(&mut self, blob: Value) {
        self.submit(endpoints::RECORD_EVENT_JOIN, blob, false);
    }
    fn submit_event_ended(&mut self, blob: Value) {
        self.submit(endpoints::MARK_EVENT_ENDED, blob, false);
    }
    /// gzipped.
    fn submit_game_result(&mut self, blob: Value) {
        self.submit(endpoints::ADD_GAME, blob, true);
    }
    fn submit_human_draft_pack(&mut self, blob: Value) {
        self.submit(endpoints::ADD_HUMAN_DRAFT_PACK, blob, false);
    }
    fn submit_human_draft_pick(&mut self, blob: Value) {
        self.submit(endpoints::ADD_HUMAN_DRAFT_PICK, blob, false);
    }
    fn submit_inventory(&mut self, blob: Value) {
        self.submit(endpoints::UPDATE_INVENTORY, blob, false);
    }
    fn submit_ongoing_events(&mut self, blob: Value) {
        self.submit(endpoints::UPDATE_ONGOING_EVENTS, blob, false);
    }
    fn submit_player_progress(&mut self, blob: Value) {
        self.submit(endpoints::UPDATE_PLAYER_PROGRESS, blob, false);
    }
    fn submit_rank(&mut self, blob: Value) {
        self.submit(endpoints::ADD_RANK, blob, false);
    }
    fn submit_user(&mut self, blob: Value) {
        self.submit(endpoints::ADD_MTGA_ACCOUNT, blob, false);
    }
}

/// The live REST client (port of `api_client.ApiClient`).
pub struct ApiClient {
    pub host: String,
}

impl ApiClient {
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into() }
    }
}

impl Submitter for ApiClient {
    fn submit(&mut self, endpoint: &str, payload: Value, use_gzip: bool) {
        let url = format!("{}/{}", self.host, endpoint);
        let json_bytes = to_python_json_vec(&payload);

        let body: Vec<u8> = if use_gzip {
            match gzip_compress(&json_bytes) {
                Ok(b) => b,
                Err(e) => {
                    log::error!("Failed to gzip payload for {endpoint}: {e}");
                    return;
                }
            }
        } else {
            json_bytes
        };

        let result = crate::retry::retry_api_call(
            || send_post(&url, &body, use_gzip),
            |resp: &ureq::Response| {
                // Retry only 5xx: valid when status is outside 500..600.
                !(500..600).contains(&resp.status())
            },
        );

        match result {
            Ok(resp) => {
                log::debug!("{endpoint} -> {}", resp.status());
            }
            Err(crate::retry::RetryError::LimitExceeded) => {
                log::error!("Giving up on {endpoint} after exhausting retries (server 5xx)");
            }
            Err(crate::retry::RetryError::Fatal(e)) => {
                log::error!("Transport error posting to {endpoint}: {e}");
            }
        }
    }
}

/// Send one POST. Non-2xx HTTP responses are normalized from `Err(Status)` back to
/// `Ok(Response)` so the retry layer's `response_validator` sees the status; only genuine
/// transport failures return `Err` (see the `retry` module's ureq normalization note).
fn send_post(
    url: &str,
    body: &[u8],
    use_gzip: bool,
) -> Result<ureq::Response, Box<ureq::Transport>> {
    let mut req = ureq::post(url).set("Content-Type", "application/json");
    if use_gzip {
        req = req.set("Content-Encoding", "gzip");
    }

    match req.send_bytes(body) {
        Ok(resp) => Ok(resp),
        // A real HTTP response (4xx/5xx) — hand it to the validator, don't treat as transport error.
        Err(ureq::Error::Status(_code, resp)) => Ok(resp),
        // Box the transport error: ureq's `Transport` is large, and it's the cold path.
        Err(ureq::Error::Transport(t)) => Err(Box::new(t)),
    }
}

/// gzip a byte slice (default compression; matches Python `gzip.compress` semantically —
/// the server decompresses, so compression level / mtime header bytes are immaterial).
fn gzip_compress(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

/// Serialize a `Value` exactly like Python `json.dumps(blob)` with its default
/// `(", ", ": ")` separators, so ASCII payload bytes match the Python client.
pub fn to_python_json_vec(value: &Value) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, PythonFormatter);
    value
        .serialize(&mut ser)
        .expect("serializing a serde_json::Value to a Vec never fails");
    buf
}

/// Convenience wrapper returning a `String`.
pub fn to_python_json_string(value: &Value) -> String {
    String::from_utf8(to_python_json_vec(value)).expect("python-json output is valid UTF-8")
}

/// A `serde_json` formatter matching Python `json.dumps`'s default separators: `", "`
/// between items and `": "` between a key and its value. All other behavior falls back to
/// the compact defaults provided by the `Formatter` trait.
struct PythonFormatter;

impl serde_json::ser::Formatter for PythonFormatter {
    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> std::io::Result<()>
    where
        W: ?Sized + std::io::Write,
    {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> std::io::Result<()>
    where
        W: ?Sized + std::io::Write,
    {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + std::io::Write,
    {
        writer.write_all(b": ")
    }
}

// ---------------------------------------------------------------------------------------
// Test support
// ---------------------------------------------------------------------------------------

/// A single recorded submission (test mock).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    pub endpoint: String,
    pub payload: Value,
    pub use_gzip: bool,
}

/// A [`Submitter`] that records `(endpoint, payload, use_gzip)` instead of sending, for the
/// fixture parity tests. Lives in `src` (not `#[cfg(test)]`) so both unit
/// tests and the `tests/parity.rs` integration harness share one definition.
#[derive(Debug, Default)]
pub struct RecordingSubmitter {
    pub calls: Vec<RecordedCall>,
}

impl RecordingSubmitter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Submitter for RecordingSubmitter {
    fn submit(&mut self, endpoint: &str, payload: Value, use_gzip: bool) {
        self.calls.push(RecordedCall {
            endpoint: endpoint.to_string(),
            payload,
            use_gzip,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn python_json_matches_default_separators() {
        let v = json!({"a": 1, "b": [1, 2, 3], "c": null, "d": {"e": true}});
        // Exactly what Python `json.dumps(...)` produces with default separators.
        let expected = r#"{"a": 1, "b": [1, 2, 3], "c": null, "d": {"e": true}}"#;
        assert_eq!(to_python_json_string(&v), expected);
    }

    #[test]
    fn python_json_empty_containers() {
        assert_eq!(to_python_json_string(&json!({})), "{}");
        assert_eq!(to_python_json_string(&json!([])), "[]");
        assert_eq!(to_python_json_string(&json!({"x": []})), r#"{"x": []}"#);
    }

    #[test]
    fn recording_submitter_routes_named_helpers_to_endpoints() {
        let mut rec = RecordingSubmitter::new();
        rec.submit_draft_pack(json!({"p": 1}));
        rec.submit_game_result(json!({"g": 2}));
        rec.submit_user(json!({"u": 3}));

        assert_eq!(rec.calls.len(), 3);
        assert_eq!(rec.calls[0].endpoint, endpoints::ADD_PACK);
        assert!(!rec.calls[0].use_gzip);
        assert_eq!(rec.calls[1].endpoint, endpoints::ADD_GAME);
        assert!(rec.calls[1].use_gzip, "add_game must be gzipped");
        assert_eq!(rec.calls[2].endpoint, endpoints::ADD_MTGA_ACCOUNT);
    }

    #[test]
    fn gzip_roundtrips() {
        use std::io::Read;
        let original = br#"{"hello": "world"}"#;
        let compressed = gzip_compress(original).unwrap();
        let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).unwrap();
        assert_eq!(out, original);
    }
}
