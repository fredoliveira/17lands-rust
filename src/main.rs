// SPDX-License-Identifier: GPL-3.0-only
//
// seventeenlands-rust — a Rust port of the 17Lands MTG Arena log client.
// Copyright (C) 2026 Fred Oliveira <fred@helloform.com>
//
// This program is a derivative work of mtga-log-client
// (https://github.com/rconroy293/mtga-log-client), Copyright (C) its authors
// (rconroy293), licensed under the GNU General Public License v3.0.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License, version 3, as published by the
// Free Software Foundation. This program is distributed in the hope that it will
// be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public
// License for more details. You should have received a copy of the GNU General
// Public License along with this program. If not, see
// <https://www.gnu.org/licenses/>.

//! 17Lands MTGA log client — Rust port.
//!
//! Drop-in replacement for the Python `seventeenlands` client. This is the thin CLI
//! wrapper: it resolves the token, sets up logging, and runs the processing loop.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;

use seventeenlands_rust::api_client;
use seventeenlands_rust::config;
use seventeenlands_rust::follower::{self, Follower};
use seventeenlands_rust::paths;

/// CLI flags, mirroring the Python argparse interface.
#[derive(Parser, Debug)]
#[command(about = "MTGA log follower — uploads MTG Arena data to 17lands.com")]
struct Args {
    /// Log filename to process. If unset, tries the known Player.log locations.
    #[arg(short = 'l', long = "log-file", alias = "log_file")]
    log_file: Option<String>,

    /// Host to submit requests to.
    #[arg(long, default_value = api_client::DEFAULT_HOST)]
    host: String,

    /// Client token (UUID v4). If unset, resolved from config / stdin.
    #[arg(long)]
    token: Option<String>,

    /// Parse the file once and exit instead of following it.
    #[arg(long)]
    once: bool,

    /// Use the detailed developer log format (full date, milliseconds, level, module
    /// target) instead of the clean default. Handy for debugging / parity work.
    #[arg(short = 'v', long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    init_logging(args.verbose);

    // Resolve the token (flag → TOML → legacy-ini migration → stdin prompt).
    let token = config::resolve_token(args.token.as_deref());
    log::info!(
        target: follower::CHATTER,
        "Using token {}...{}",
        &token[..token.len().min(4)],
        &token[token.len().saturating_sub(4)..]
    );

    // The startup version check is intentionally dropped. The client just runs.
    processing_loop(&args, token);
}

/// stdout/stderr logging only — no rotating file handler. The console output is
/// purely cosmetic; it is **not** part of the wire contract (that lives in `api_client`).
///
/// Default ("clean") format: `HH:MM:SS  message`, with the time dimmed and only WARN/ERROR
/// carrying a colored level tag — a calm status feed. `--verbose` switches to the detailed
/// developer layout that mirrors the Python `logging_utils` line
/// (`<date> <time>.<ms>,<level>,<target>,<message>`), useful for debugging / parity work.
///
/// Color is emitted unconditionally; env_logger's anstream-backed buffer strips the ANSI
/// codes automatically when the output is not a terminal or `NO_COLOR` is set.
fn init_logging(verbose: bool) {
    use env_logger::{Builder, Env};

    let mut builder = Builder::from_env(Env::default().default_filter_or("info"));

    if verbose {
        builder.format(|buf, record| {
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
        });
    } else {
        use anstyle::{AnsiColor, Style};

        let dim = Style::new().dimmed();
        let yellow = Style::new().fg_color(Some(AnsiColor::Yellow.into())).bold();
        let red = Style::new().fg_color(Some(AnsiColor::Red.into())).bold();

        builder.format(move |buf, record| {
            let ts = chrono::Local::now().format("%H:%M:%S");
            match record.level() {
                log::Level::Warn => writeln!(
                    buf,
                    "{dim}{ts}{dim:#}  {yellow}WARN{yellow:#}  {yellow}{}{yellow:#}",
                    record.args()
                ),
                log::Level::Error => writeln!(
                    buf,
                    "{dim}{ts}{dim:#}  {red}ERROR{red:#}  {red}{}{red:#}",
                    record.args()
                ),
                // Background-sync chatter: dim the whole line so it recedes behind events.
                _ if record.target() == follower::CHATTER => {
                    writeln!(buf, "{dim}{ts}  {}{dim:#}", record.args())
                }
                // INFO/DEBUG/TRACE: dim time, message normal — keep the feed readable.
                _ => writeln!(buf, "{dim}{ts}{dim:#}  {}", record.args()),
            }
        });
    }

    builder.init();
}

/// Port of Python `processing_loop`.
fn processing_loop(args: &Args, token: String) {
    let filepaths: Vec<PathBuf> = match &args.log_file {
        Some(path) => vec![PathBuf::from(path)],
        None => paths::possible_current_filepaths(),
    };

    let follow = !args.once;

    let mut follower = Follower::new(token, args.host.clone());

    // "Normal mode": no explicit log file, default host, and following. Parse the first
    // existing previous-log once at startup to catch up on missed events.
    if args.log_file.is_none() && args.host == api_client::DEFAULT_HOST && follow {
        for filename in paths::possible_previous_filepaths() {
            if filename.exists() {
                log::info!(target: follower::CHATTER, "Parsing the previous log {} once", filename.display());
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
            log::info!(target: follower::CHATTER, "Following along {}", filename.display());
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
