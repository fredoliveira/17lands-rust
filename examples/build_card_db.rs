//! Build the Arena-ID → name map for the client's console logging (see `src/card_db.rs`).
//!
//! Reads MTGJSON's `AllIdentifiers.json` (downloaded, or a local `.json` / `.json.gz`) and
//! writes a compact `{ "<mtgArenaId>": "<name>" }` file that the client loads at runtime to
//! show card names in draft-pick logs. This is **console-only** data — it never affects the
//! uploaded payloads, so it stays clear of the wire-compatibility contract (CLAUDE.md).
//!
//! Usage:
//!   cargo run --example build_card_db                        # download MTGJSON → config dir
//!   cargo run --example build_card_db -- --source FILE       # local AllIdentifiers.json(.gz)
//!   cargo run --example build_card_db -- --out PATH          # custom output location
//!
//! By default it writes to `card_db::default_path()` (`<config>/17l/arena_names.json`), the
//! exact location the runtime loader reads. Override the runtime lookup with
//! `$SEVENTEENLANDS_CARD_DB`.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

use serde_json::Value;
use seventeenlands_rust::card_db;

const MTGJSON_URL: &str = "https://mtgjson.com/api/v5/AllIdentifiers.json";
const USAGE: &str = "usage: build_card_db [--source URL|PATH] [--out PATH]";

fn main() {
    let mut source = MTGJSON_URL.to_string();
    let mut out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--source" => {
                source = args
                    .next()
                    .unwrap_or_else(|| panic!("--source needs a value; {USAGE}"))
            }
            "--out" => {
                out = Some(PathBuf::from(
                    args.next()
                        .unwrap_or_else(|| panic!("--out needs a value; {USAGE}")),
                ))
            }
            other => panic!("unknown argument {other:?}; {USAGE}"),
        }
    }
    let out = out
        .or_else(card_db::default_path)
        .expect("could not resolve the config directory; pass --out");

    let bytes = load_source(&source);
    let doc: Value = serde_json::from_slice(&bytes).expect("source is not valid JSON");

    // AllIdentifiers nests the cards under "data"; fall back to the root object otherwise.
    let cards = doc
        .get("data")
        .unwrap_or(&doc)
        .as_object()
        .expect("expected a JSON object of cards");

    // BTreeMap keeps the output key-sorted and deterministic across runs.
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for card in cards.values() {
        let arena = card.get("identifiers").and_then(|i| i.get("mtgArenaId"));
        let name = card.get("name").and_then(|n| n.as_str());
        let (Some(arena), Some(name)) = (arena, name) else {
            continue;
        };
        // mtgArenaId is a string in MTGJSON; accept a number too, defensively. Multiple
        // printings can share an ID with the same name, so last-wins is harmless.
        let id = match arena {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => continue,
        };
        names.insert(id, name.to_string());
    }

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let json = serde_json::to_string(&names).expect("serialize id→name map");
    std::fs::write(&out, json).expect("write output file");
    eprintln!(
        "Wrote {} Arena card names to {}",
        names.len(),
        out.display()
    );
}

/// Fetch the source bytes (URL or local path), transparently gunzipping a gzip payload.
fn load_source(source: &str) -> Vec<u8> {
    let raw = if source.starts_with("http://") || source.starts_with("https://") {
        eprintln!("Downloading {source} ...");
        let resp = ureq::get(source)
            .call()
            .expect("download AllIdentifiers.json");
        let mut buf = Vec::new();
        resp.into_reader()
            .read_to_end(&mut buf)
            .expect("read response body");
        buf
    } else {
        eprintln!("Reading {source} ...");
        std::fs::read(source).expect("read source file")
    };
    // Gunzip a `.json.gz` source. (ureq already decodes Content-Encoding for URLs, but a
    // local gz or a server using Content-Type gzip still arrives compressed.)
    if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_end(&mut out)
            .expect("gunzip source");
        out
    } else {
        raw
    }
}
