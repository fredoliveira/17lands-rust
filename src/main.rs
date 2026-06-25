//! 17Lands MTGA log client — Rust port.
//!
//! Drop-in replacement for the Python `seventeenlands` client. See `SPEC.md` for the
//! full specification; section numbers in module docs refer to it.
//!
//! Build order (SPEC §13):
//!   1. plumbing (this file, config, paths, logging)   <- DONE (milestone 1)
//!   2. api_client + retry
//!   3. tailing + accumulation
//!   4. blob parse + dispatch
//!   5. simple handlers
//!   6. game-state machine
//!   7. end-to-end

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;

use seventeenlands_rust::api_client;
use seventeenlands_rust::config;
use seventeenlands_rust::follower::Follower;
use seventeenlands_rust::paths;

/// CLI flags, mirroring the Python argparse interface (SPEC §2).
#[derive(Parser, Debug)]
#[command(about = "MTGA log follower — uploads MTG Arena data to 17lands.com")]
struct Args {
    /// Log filename to process. If unset, tries the known Player.log locations.
    #[arg(short = 'l', long = "log-file", alias = "log_file")]
    log_file: Option<String>,

    /// Host to submit requests to.
    #[arg(long, default_value = api_client::DEFAULT_HOST)]
    host: String,

    /// Client token (UUID v4). If unset, resolved from config / stdin (SPEC §5.1).
    #[arg(long)]
    token: Option<String>,

    /// Parse the file once and exit instead of following it.
    #[arg(long)]
    once: bool,
}

fn main() {
    init_logging();

    let args = Args::parse();

    // Resolve the token (flag → TOML → legacy-ini migration → stdin prompt; SPEC §5.1).
    let token = config::resolve_token(args.token.as_deref());
    log::info!(
        "Using token {}...{}",
        &token[..token.len().min(4)],
        &token[token.len().saturating_sub(4)..]
    );

    // SPEC §2: the startup version check is intentionally dropped. The client just runs.
    processing_loop(&args, token);
}

/// stdout/stderr logging only — no rotating file handler (SPEC §2). Format mirrors the
/// Python `logging_utils` layout (`<date> <time>.<ms>,<level>,<target>,<message>`) for
/// familiarity; it is not part of the wire contract.
fn init_logging() {
    use env_logger::{Builder, Env};

    Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let now = chrono::Local::now();
            writeln!(
                buf,
                "{}.{:03},{},{},{}",
                now.format("%Y%m%d %H%M%S"),
                now.timestamp_subsec_millis(),
                record.level(),
                record.target(),
                record.args(),
            )
        })
        .init();
}

/// Port of Python `processing_loop` (SPEC §5.3).
fn processing_loop(args: &Args, token: String) {
    let filepaths: Vec<PathBuf> = match &args.log_file {
        Some(path) => vec![PathBuf::from(path)],
        None => paths::possible_current_filepaths(),
    };

    let follow = !args.once;

    let mut follower = Follower::new(token, args.host.clone());

    // "Normal mode": no explicit log file, default host, and following. Parse the first
    // existing previous-log once at startup to catch up on missed events (SPEC §5.3).
    if args.log_file.is_none() && args.host == api_client::DEFAULT_HOST && follow {
        for filename in paths::possible_previous_filepaths() {
            if filename.exists() {
                log::info!("Parsing the previous log {} once", filename.display());
                follower.parse_log(&filename.to_string_lossy(), false);
                break;
            }
        }
    }

    // Tail and parse the current log file to handle ongoing events.
    let mut any_found = false;
    for filename in &filepaths {
        if Path::new(filename).exists() {
            any_found = true;
            log::info!("Following along {}", filename.display());
            follower.parse_log(&filename.to_string_lossy(), follow);
        }
    }

    if !any_found {
        log::warn!(
            "Found no files to parse. Try to find Arena's Player.log file and pass it as an \
             argument with -l"
        );
    }

    log::info!("Exiting");
}
