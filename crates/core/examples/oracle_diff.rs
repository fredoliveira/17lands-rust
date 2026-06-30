// SPDX-License-Identifier: GPL-3.0-only

//! Compare Rust Follower output against a captured Python-oracle JSONL.
//!
//! Usage: `cargo run --example oracle_diff -- <logfile> <python_capture.jsonl>`
//!
//! Runs the Follower over the same log the oracle saw, then compares each submission's
//! `(endpoint, payload)` to the captured Python one. Payloads are compared by their
//! Python-style JSON serialization (key order + values + separators); the first differing
//! JSON path is reported per mismatch. Exits non-zero if anything differs.

use std::collections::BTreeMap;

use serde_json::Value;

use seventeenlands_core::api_client::{RecordingSubmitter, to_python_json_string};
use seventeenlands_core::follower::Follower;

fn first_diff_path(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            let keys_a: Vec<&String> = ma.keys().collect();
            let keys_b: Vec<&String> = mb.keys().collect();
            if keys_a != keys_b {
                return Some(format!(
                    "{path}: key order/set differs\n  rust: {keys_a:?}\n  py:   {keys_b:?}"
                ));
            }
            for (k, va) in ma {
                if let Some(d) = first_diff_path(va, &mb[k], &format!("{path}/{k}")) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.len() != ab.len() {
                return Some(format!("{path}: array len {} vs {}", aa.len(), ab.len()));
            }
            for (i, (va, vb)) in aa.iter().zip(ab).enumerate() {
                if let Some(d) = first_diff_path(va, vb, &format!("{path}/{i}")) {
                    return Some(d);
                }
            }
            None
        }
        _ => {
            if to_python_json_string(a) != to_python_json_string(b) {
                Some(format!(
                    "{path}: {} != {}",
                    to_python_json_string(a),
                    to_python_json_string(b)
                ))
            } else {
                None
            }
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let log_path = args
        .next()
        .expect("usage: oracle_diff <logfile> <capture.jsonl>");
    let capture_path = args
        .next()
        .expect("usage: oracle_diff <logfile> <capture.jsonl>");

    // Load the Python capture.
    let capture = std::fs::read_to_string(&capture_path).expect("read capture");
    let py: Vec<(String, Value)> = capture
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse capture line");
            (
                v["endpoint"].as_str().unwrap().to_string(),
                v["body"].clone(),
            )
        })
        .collect();

    // Run the Rust Follower over the same log.
    let mut f = Follower::with_submitter(
        "00000000-0000-4000-8000-000000000000".into(),
        "http://localhost".into(),
        RecordingSubmitter::new(),
    );
    f.parse_log(&log_path, false);
    let rust = &f.api.calls;

    println!(
        "rust submissions: {}, python submissions: {}",
        rust.len(),
        py.len()
    );

    let mut mismatches = 0usize;
    let mut by_endpoint_ok: BTreeMap<String, usize> = BTreeMap::new();

    let n = rust.len().min(py.len());
    for i in 0..n {
        let (py_ep, py_body) = &py[i];
        let rc = &rust[i];
        if &rc.endpoint != py_ep {
            println!("[{i}] ENDPOINT mismatch: rust={} py={}", rc.endpoint, py_ep);
            mismatches += 1;
            continue;
        }
        match first_diff_path(&rc.payload, py_body, "") {
            None => *by_endpoint_ok.entry(rc.endpoint.clone()).or_default() += 1,
            Some(diff) => {
                println!("[{i}] {} payload differs at {diff}", rc.endpoint);
                mismatches += 1;
            }
        }
    }

    if rust.len() != py.len() {
        println!("COUNT mismatch: rust={} py={}", rust.len(), py.len());
        mismatches += 1;
    }

    println!("\nmatching submissions by endpoint:");
    for (ep, c) in &by_endpoint_ok {
        println!("  {ep}: {c}");
    }

    if mismatches == 0 {
        println!(
            "\n✅ ALL {} submissions byte-identical to the Python oracle",
            n
        );
    } else {
        println!("\n❌ {mismatches} mismatch(es)");
        std::process::exit(1);
    }
}
