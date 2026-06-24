//! Token resolution & config file (SPEC §5.1).
//!
//! Resolution order (first valid UUID-v4 wins):
//!   1. `--token` flag
//!   2. `~/.config/17l/config.toml` (`token` key)
//!   3. legacy `~/.mtga_follower.ini` `[client] token` — migrate to TOML if found
//!   4. interactive stdin prompt; on success, write to the TOML config
//!
//! No GUI prompts, no env var (SPEC §14 #3).

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Contents of `~/.config/17l/config.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub token: Option<String>,
}

/// Validate a string is an acceptable UUID v4 token.
///
/// NOTE (SPEC §11.6): Python's `uuid.UUID(s, version=4)` is lenient — match its
/// *acceptance set*, not strict RFC v4, so tokens the Python client accepts aren't rejected.
pub fn validate_uuid_v4(_maybe: &str) -> bool {
    todo!("SPEC §5.1 / §11.6")
}

/// Resolve the token from flag → TOML → legacy ini (migrate) → stdin prompt.
pub fn resolve_token(_flag: Option<&str>) -> String {
    todo!("SPEC §5.1")
}
