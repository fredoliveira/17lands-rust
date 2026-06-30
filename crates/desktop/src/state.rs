//! App state: owns the follower thread and its cancellation, the resolved log path, and the
//! shared upload status. Managed by Tauri (`app.manage`) so commands and the tray can reach it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::Serialize;

use seventeenlands_core::api_client::ApiClient;
use seventeenlands_core::follower::Follower;
use seventeenlands_core::{config, paths};

use crate::observer::{ObservingSubmitter, UploadStatus};

pub struct AppState {
    host: String,
    cancel: Arc<AtomicBool>,
    following: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
    log_path: Mutex<Option<PathBuf>>,
    pub upload: Arc<Mutex<UploadStatus>>,
}

/// Status snapshot serialized to the UI and used to render tray text.
#[derive(Serialize)]
pub struct StatusDto {
    pub following: bool,
    pub token_present: bool,
    pub log_path: Option<String>,
    pub host: String,
    pub upload_count: u64,
    pub last_endpoint: Option<String>,
    pub last_time: Option<String>,
}

impl AppState {
    pub fn new(host: String) -> Self {
        // Dev/test override (parallels SEVENTEENLANDS_HOST): pin the followed log file so the
        // app can be exercised headlessly against a fixture log.
        let log_path = std::env::var("SEVENTEENLANDS_LOG").ok().map(PathBuf::from);
        Self {
            host,
            cancel: Arc::new(AtomicBool::new(false)),
            following: Arc::new(AtomicBool::new(false)),
            thread: Mutex::new(None),
            log_path: Mutex::new(log_path),
            upload: Arc::new(Mutex::new(UploadStatus::default())),
        }
    }

    /// The user-chosen path if set, else the first auto-detected `Player.log` that exists.
    pub fn resolve_log_path(&self) -> Option<PathBuf> {
        if let Some(p) = self.log_path.lock().unwrap().clone() {
            return Some(p);
        }
        paths::possible_current_filepaths()
            .into_iter()
            .find(|p| p.exists())
    }

    pub fn set_log_path(&self, p: Option<PathBuf>) {
        *self.log_path.lock().unwrap() = p;
    }

    pub fn is_following(&self) -> bool {
        self.following.load(Ordering::Relaxed)
    }

    /// Spawn the follower on a dedicated thread. No-op if already running.
    pub fn start(&self) -> Result<(), String> {
        if self.is_following() {
            return Ok(());
        }
        let token = config::read_toml_token().ok_or("No valid token configured")?;
        let path = self
            .resolve_log_path()
            .ok_or("No Player.log found — set a log path in Settings")?;
        let path_str = path.to_string_lossy().to_string();

        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        let following = self.following.clone();
        let host = self.host.clone();
        let upload = self.upload.clone();

        let handle = std::thread::Builder::new()
            .name("17l-follower".into())
            .spawn(move || {
                following.store(true, Ordering::Relaxed);
                let api = ObservingSubmitter::new(ApiClient::new(host.clone()), upload);
                let mut follower = Follower::with_submitter(token, host, api);
                follower.parse_log_cancellable(&path_str, true, &cancel);
                following.store(false, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string())?;

        *self.thread.lock().unwrap() = Some(handle);
        *self.log_path.lock().unwrap() = Some(path);
        Ok(())
    }

    /// Signal cancellation and join the follower thread (returns within ~one 500ms tick).
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.lock().unwrap().take() {
            let _ = handle.join();
        }
        self.following.store(false, Ordering::Relaxed);
    }

    pub fn restart(&self) -> Result<(), String> {
        self.stop();
        self.start()
    }

    pub fn status(&self) -> StatusDto {
        let u = self.upload.lock().unwrap().clone();
        StatusDto {
            following: self.is_following(),
            token_present: config::read_toml_token().is_some(),
            log_path: self
                .resolve_log_path()
                .map(|p| p.to_string_lossy().to_string()),
            host: self.host.clone(),
            upload_count: u.count,
            last_endpoint: u.last_endpoint,
            last_time: u.last_time,
        }
    }
}
