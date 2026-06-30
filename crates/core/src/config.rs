// SPDX-License-Identifier: GPL-3.0-only

//! Token resolution & config file.
//!
//! Resolution order (first valid UUID-v4 wins):
//!   1. `--token` flag
//!   2. `~/.config/17l/config.toml` (`token` key)
//!   3. legacy `~/.mtga_follower.ini` `[client] token` — migrate to TOML if found
//!   4. interactive stdin prompt; on success, write to the TOML config
//!
//! No GUI prompts, no env var.

use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Contents of `~/.config/17l/config.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub token: Option<String>,
}

const TOKEN_ENTRY_MESSAGE: &str = "Please enter your client token from 17lands.com/account: ";
const TOKEN_INVALID_MESSAGE: &str = "That token is invalid. Please specify a valid client token. See 17lands.com/getting_started for more details.";

/// New primary config location: `<config_dir>/17l/config.toml`.
///
/// Uses `dirs::config_dir()` (respects `$XDG_CONFIG_HOME`).
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("17l").join("config.toml"))
}

/// Legacy Python config: `~/.mtga_follower.ini`.
pub fn legacy_ini_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".mtga_follower.ini"))
}

/// Validate a string is an acceptable UUID v4 token.
///
/// Mirrors Python's `uuid.UUID(s, version=4)` *acceptance set*, which is
/// lenient: it strips `urn:` / `uuid:` prefixes and surrounding braces, removes all
/// hyphens, then requires exactly 32 hexadecimal digits. The `version=4` argument only
/// rewrites version/variant bits afterward, so any well-formed UUID string is accepted
/// regardless of its actual version. Returns the *original* string on success (like the
/// Python helper, which returns `maybe_uuid` unchanged).
pub fn validate_uuid_v4(maybe: &str) -> Option<String> {
    // Python: hex = s.replace('urn:', '').replace('uuid:', '')
    let mut hex = maybe.replace("urn:", "").replace("uuid:", "");
    // Python: hex = hex.strip('{}')  — strip leading/trailing '{' and '}' chars.
    hex = hex
        .trim_start_matches(['{', '}'])
        .trim_end_matches(['{', '}'])
        .to_string();
    // Python: hex = hex.replace('-', '')
    hex = hex.replace('-', "");

    if hex.len() != 32 {
        return None;
    }
    // Python: int(hex, 16) — must be all hex digits.
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(maybe.to_string())
}

/// Resolve the token from flag → TOML → legacy ini (migrate) → stdin prompt.
///
/// Returns the validated token, or exits the process on an unrecoverable error (e.g. no
/// stdin available for the interactive prompt), mirroring the Python client's behavior.
pub fn resolve_token(flag: Option<&str>) -> String {
    // 1. --token flag.
    if let Some(t) = flag {
        if let Some(valid) = validate_uuid_v4(t) {
            return valid;
        }
        log::warn!("Ignoring --token flag: not a valid UUID");
    }

    // 2. ~/.config/17l/config.toml.
    if let Some(t) = read_toml_token() {
        return t;
    }

    // 3. Legacy ~/.mtga_follower.ini migration.
    if let Some(t) = read_legacy_ini_token() {
        log::info!("Migrating token from legacy ~/.mtga_follower.ini to the new config");
        write_toml_token(&t);
        return t;
    }

    // 4. Interactive stdin prompt; persist on success.
    let t = prompt_token_cli();
    write_toml_token(&t);
    t
}

/// Read + validate the `token` key from `<config_dir>/17l/config.toml`.
///
/// Public so the desktop GUI can check for / load the saved token without going through
/// [`resolve_token`], which falls back to a blocking stdin prompt unsuitable for a GUI.
pub fn read_toml_token() -> Option<String> {
    let path = config_path()?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let config: Config = toml::from_str(&contents).ok()?;
    let token = config.token?;
    validate_uuid_v4(&token)
}

/// Minimal manual parse of the legacy ini's `[client] token` (no configparser dep).
///
/// Only the one `token` key under the `[client]` section is read; the file is otherwise
/// ignored and not kept in sync afterward.
fn read_legacy_ini_token() -> Option<String> {
    let path = legacy_ini_path()?;
    let contents = std::fs::read_to_string(&path).ok()?;

    let mut in_client = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_client = trimmed[1..trimmed.len() - 1]
                .trim()
                .eq_ignore_ascii_case("client");
            continue;
        }
        if in_client
            && let Some((key, value)) = trimmed.split_once('=')
            && key.trim().eq_ignore_ascii_case("token")
        {
            return validate_uuid_v4(value.trim());
        }
    }
    None
}

/// Write the token to `<config_dir>/17l/config.toml`, creating the directory.
///
/// Public so the desktop GUI can persist a token captured in its settings dialog, writing
/// to the same file the CLI reads.
pub fn write_toml_token(token: &str) {
    let Some(path) = config_path() else {
        log::error!("Could not determine config directory; token not persisted");
        return;
    };
    let config = Config {
        token: Some(token.to_string()),
    };
    let body = match toml::to_string(&config) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to serialize config: {e}");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        log::error!(
            "Failed to create config directory {}: {e}",
            parent.display()
        );
        return;
    }
    if let Err(e) = std::fs::write(&path, body) {
        log::error!("Failed to write config {}: {e}", path.display());
        return;
    }
    log::info!("Saved token to {}", path.display());
}

/// Interactive stdin token prompt, re-prompting on invalid UUID (port of
/// `get_client_token_cli`).
fn prompt_token_cli() -> String {
    let mut message = TOKEN_ENTRY_MESSAGE.to_string();
    loop {
        print!("{message}");
        let _ = io::stdout().flush();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                // EOF — no token can be obtained. Python's input() raises EOFError; we exit.
                eprintln!(
                    "Error: The program cannot continue without specifying a client token. Exiting."
                );
                std::process::exit(1);
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading token: {e}");
                std::process::exit(1);
            }
        }

        let token = input.trim();
        match validate_uuid_v4(token) {
            Some(valid) => return valid,
            None => message = format!("{TOKEN_INVALID_MESSAGE} Token: "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_standard_hyphenated_v4() {
        let t = "12345678-1234-4234-8234-123456789abc";
        assert_eq!(validate_uuid_v4(t).as_deref(), Some(t));
    }

    #[test]
    fn accepts_non_v4_uuid_like_python() {
        // Version nibble '1' (not 4) — Python overwrites it, so it's accepted.
        let t = "12345678-1234-1234-1234-123456789abc";
        assert_eq!(validate_uuid_v4(t).as_deref(), Some(t));
    }

    #[test]
    fn accepts_simple_32_hex_and_braces_and_urn() {
        assert!(validate_uuid_v4("123456781234423482341234567890ab").is_some());
        assert!(validate_uuid_v4("{12345678-1234-4234-8234-123456789abc}").is_some());
        assert!(validate_uuid_v4("urn:uuid:12345678-1234-4234-8234-123456789abc").is_some());
    }

    #[test]
    fn rejects_wrong_length_and_non_hex() {
        assert!(validate_uuid_v4("not-a-uuid").is_none());
        assert!(validate_uuid_v4("12345678-1234-4234-8234-123456789ab").is_none()); // 31 hex
        assert!(validate_uuid_v4("g2345678-1234-4234-8234-123456789abc").is_none()); // 'g'
        assert!(validate_uuid_v4("").is_none());
    }

    #[test]
    fn returns_original_unmodified_string() {
        // Preserves exact input (incl. case) like the Python helper.
        let t = "ABCDEF78-1234-4234-8234-123456789ABC";
        assert_eq!(validate_uuid_v4(t).as_deref(), Some(t));
    }
}
