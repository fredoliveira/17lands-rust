//! Exponential-backoff retry (SPEC §10, port of `retry_utils.py`).
//!
//! - initial 1s, ×2 each attempt, capped at 10min, max total 24h.
//! - response valid when `status < 500 || status >= 600` (retry only 5xx).
//! - retry on transport/connection errors only; re-raise others.
//! - ⚠️ `ureq` maps non-2xx to `Err(Status)` — normalize so HTTP responses (incl. 4xx/5xx)
//!   reach the response-validator instead of being treated as transport errors.

#![allow(dead_code)]

use std::time::Duration;

pub const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(1);
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(10 * 60);
pub const MAX_TOTAL_RETRY_DURATION: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug)]
pub struct RetryLimitExceeded;

/// Retry `callback` until `response_validator` accepts its result, with backoff (SPEC §10).
pub fn retry_until_successful<T, F, V, E>(
    _callback: F,
    _response_validator: V,
    _error_validator: E,
) -> Result<T, RetryLimitExceeded>
where
    F: FnMut() -> T,
    V: Fn(&T) -> bool,
    E: Fn() -> bool,
{
    todo!("SPEC §10")
}
