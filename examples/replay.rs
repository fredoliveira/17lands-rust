//! Replay a Player.log through the Follower with a recording submitter, printing a summary
//! of what would be submitted. Dev tooling for validating the parser on real logs without
//! posting anywhere. Usage: `cargo run --example replay -- <logfile>`
//!
//! Prints only endpoint counts and top-level key names (never payload values) to avoid
//! echoing account ids / screen names from a real log.

use std::collections::BTreeMap;

use seventeenlands_rust::api_client::RecordingSubmitter;
use seventeenlands_rust::follower::Follower;

fn main() {
    let path = std::env::args().nth(1).expect("usage: replay <logfile>");
    let rec = RecordingSubmitter::new();
    let mut f = Follower::with_submitter(
        "00000000-0000-4000-8000-000000000000".into(),
        "http://localhost".into(),
        rec,
    );
    f.parse_log(&path, false);

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in &f.api.calls {
        *counts.entry(c.endpoint.clone()).or_default() += 1;
    }
    eprintln!("total submissions: {}", f.api.calls.len());
    for (ep, n) in &counts {
        eprintln!("  {ep}: {n}");
    }

    if let Some(g) = f.api.calls.iter().find(|c| c.endpoint.ends_with("add_game")) {
        if let Some(obj) = g.payload.as_object() {
            eprintln!("add_game top-level keys: {:?}", obj.keys().collect::<Vec<_>>());
            if let Some(hist) = obj.get("history").and_then(|h| h.as_object()) {
                eprintln!(
                    "add_game.history keys: {:?}; events={}",
                    hist.keys().collect::<Vec<_>>(),
                    hist.get("events").and_then(|e| e.as_array()).map(|a| a.len()).unwrap_or(0),
                );
            }
        }
    }
}
