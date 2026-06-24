//! 17Lands MTGA log client — Rust port.
//!
//! Drop-in replacement for the Python `seventeenlands` client. See `SPEC.md` for the
//! full specification; section numbers in module docs refer to it.
//!
//! Build order (SPEC §13):
//!   1. plumbing (this file, config, paths, logging)
//!   2. api_client + retry
//!   3. tailing + accumulation
//!   4. blob parse + dispatch
//!   5. simple handlers
//!   6. game-state machine
//!   7. end-to-end

mod api_client;
mod config;
mod follower;
mod paths;
mod retry;
mod time_parse;

use clap::Parser;

/// CLI flags, mirroring the Python argparse interface (SPEC §2).
#[derive(Parser, Debug)]
#[command(about = "MTGA log follower — uploads MTG Arena data to 17lands.com")]
struct Args {
    /// Log filename to process. If unset, tries the known Player.log locations.
    #[arg(short = 'l', long = "log-file")]
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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let _args = Args::parse();

    // TODO(SPEC §5.3): resolve token (config::resolve_token), build Follower, run the
    // processing loop (previous-log catch-up in normal mode, then follow current log).
    todo!("processing loop — SPEC §5.3");
}
