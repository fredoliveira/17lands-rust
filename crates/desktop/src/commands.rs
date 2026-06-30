//! `#[tauri::command]` bridge between the webview and `AppState` / core config.

use tauri::State;

use seventeenlands_core::config;

use crate::logbridge::{self, LogLine};
use crate::state::{AppState, StatusDto};

#[tauri::command]
pub fn token_present() -> bool {
    config::read_toml_token().is_some()
}

/// Validate (UUID v4) and persist the token to the same file the CLI reads, then (re)start.
#[tauri::command]
pub fn save_token(token: String, state: State<AppState>) -> Result<(), String> {
    let valid = config::validate_uuid_v4(token.trim())
        .ok_or("That doesn't look like a valid 17Lands token (expected a UUID).")?;
    config::write_toml_token(&valid);
    state.restart()
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> StatusDto {
    state.status()
}

#[tauri::command]
pub fn start_following(state: State<AppState>) -> Result<(), String> {
    state.start()
}

#[tauri::command]
pub fn stop_following(state: State<AppState>) {
    state.stop();
}

#[tauri::command]
pub fn recent_logs() -> Vec<LogLine> {
    logbridge::recent()
}

/// Point the follower at a specific log file (overrides auto-detection), then restart.
#[tauri::command]
pub fn set_log_path(path: String, state: State<AppState>) -> Result<(), String> {
    let pb = std::path::PathBuf::from(&path);
    if !pb.exists() {
        return Err(format!("No file at {path}"));
    }
    state.set_log_path(Some(pb));
    state.restart()
}
