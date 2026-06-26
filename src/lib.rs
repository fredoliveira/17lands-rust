//! 17Lands MTGA log client — Rust port (library crate).
//!
//! Drop-in replacement for the Python `seventeenlands` client. The `main` binary is a thin
//! wrapper over this crate; the modules are public so the integration tests in `tests/`
//! (fixture/oracle parity, HTTP) can drive them directly.

pub mod api_client;
pub mod config;
pub mod follower;
pub mod paths;
pub mod retry;
pub mod time_parse;
