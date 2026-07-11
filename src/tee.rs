// SPDX-License-Identifier: GPL-3.0-only

//! Optional local tee for parsed events (`--tee <URL>`).
//!
//! [`Tee`] wraps the real 17Lands submitter and additionally forwards every blob to a
//! local HTTP sink — e.g. a companion app rendering the draft live. The local leg is
//! fire-and-forget by design: posts run on a background thread behind a bounded queue,
//! never retry, and failures cannot slow down or break the 17Lands upload path.
//!
//! Endpoint mapping: the 17Lands path's last segment is appended to the tee base URL,
//! so with `--tee http://localhost:5910/api/live` a blob bound for
//! `api/client/add_human_draft_pack` also lands on
//! `POST http://localhost:5910/api/live/add_human_draft_pack`. Local posts are never
//! gzipped — the payload JSON is sent as-is.

use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

use crate::api_client::{Submitter, to_python_json_vec};

/// How many pending local posts to buffer before dropping new ones. Draft events arrive
/// at human pace; the queue only fills if the sink is down or wedged, and then dropping
/// is exactly what we want.
const QUEUE_CAP: usize = 256;

/// Per-request timeout for local posts. The sink is on localhost; anything slower than
/// this is effectively down.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(2);

/// A [`Submitter`] that forwards every call to `primary` and then to `local`.
pub struct Tee<P: Submitter> {
    primary: P,
    local: LocalSink,
}

impl<P: Submitter> Tee<P> {
    pub fn new(primary: P, local: LocalSink) -> Self {
        Self { primary, local }
    }
}

impl<P: Submitter> Submitter for Tee<P> {
    fn submit(&mut self, endpoint: &str, payload: Value, use_gzip: bool) {
        // The local sink only borrows the payload; the primary keeps ownership semantics.
        self.local.enqueue(endpoint, &payload);
        self.primary.submit(endpoint, payload, use_gzip);
    }
}

/// Background poster for the local sink.
///
/// Owns a bounded channel and a worker thread. `enqueue` never blocks: when the queue is
/// full the event is dropped (with a warning), keeping the log-tailing loop responsive no
/// matter what the sink does. Dropping the sink closes the channel and joins the worker,
/// flushing whatever is queued — so `--once` runs deliver everything before exit.
pub struct LocalSink {
    tx: Option<SyncSender<(String, Vec<u8>)>>,
    worker: Option<JoinHandle<()>>,
}

impl LocalSink {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let (tx, rx) = sync_channel::<(String, Vec<u8>)>(QUEUE_CAP);
        let worker = std::thread::Builder::new()
            .name("local-tee".into())
            .spawn(move || drain(base_url, rx))
            .expect("spawning the local tee thread never fails");
        Self {
            tx: Some(tx),
            worker: Some(worker),
        }
    }

    fn enqueue(&mut self, endpoint: &str, payload: &Value) {
        // "api/client/add_pick" -> "add_pick"
        let name = endpoint.rsplit('/').next().unwrap_or(endpoint).to_string();
        let body = to_python_json_vec(payload);
        let tx = self.tx.as_ref().expect("tx lives until drop");
        match tx.try_send((name, body)) {
            Ok(()) => {}
            Err(TrySendError::Full((name, _))) => {
                log::warn!("Local tee queue full; dropping {name}");
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

impl Drop for LocalSink {
    fn drop(&mut self) {
        drop(self.tx.take()); // close the channel so the worker drains and exits
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Worker loop: post each queued blob once, with a short timeout and no retries.
/// The first failure logs at WARN so a missing sink is visible; repeats stay at DEBUG
/// to keep the feed calm while the companion app is closed.
fn drain(base_url: String, rx: Receiver<(String, Vec<u8>)>) {
    let agent = ureq::AgentBuilder::new()
        .timeout(LOCAL_TIMEOUT)
        .build();
    let mut warned = false;

    for (name, body) in rx {
        let url = format!("{base_url}/{name}");
        match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_bytes(&body)
        {
            Ok(_) => {
                log::debug!("tee {name} -> ok");
            }
            Err(e) => {
                if warned {
                    log::debug!("tee {name} failed: {e}");
                } else {
                    log::warn!("Local tee unreachable ({url}): {e}");
                    warned = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_client::{RecordingSubmitter, endpoints};
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// A one-request HTTP sink: accepts a single POST, returns its (path, body).
    fn tiny_sink(listener: TcpListener) -> std::thread::JoinHandle<(String, String)> {
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream);

            let mut request_line = String::new();
            reader.read_line(&mut request_line).expect("request line");
            let path = request_line
                .split_whitespace()
                .nth(1)
                .expect("path")
                .to_string();

            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("header");
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = v.trim().parse().expect("length");
                }
            }

            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).expect("body");
            reader
                .into_inner()
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                .expect("response");

            (path, String::from_utf8(body).expect("utf8 body"))
        })
    }

    #[test]
    fn tee_posts_to_both_destinations() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let sink = tiny_sink(listener);

        let mut tee = Tee::new(
            RecordingSubmitter::new(),
            LocalSink::new(format!("http://{addr}/api/live")),
        );
        tee.submit_human_draft_pack(json!({"card_ids": [1, 2, 3]}));

        // Primary saw the call unchanged.
        assert_eq!(tee.primary.calls.len(), 1);
        assert_eq!(tee.primary.calls[0].endpoint, endpoints::ADD_HUMAN_DRAFT_PACK);

        // Drop flushes the queue; then the sink must have the local copy.
        drop(tee);
        let (path, body) = sink.join().expect("sink thread");
        assert_eq!(path, "/api/live/add_human_draft_pack");
        assert_eq!(body, r#"{"card_ids": [1, 2, 3]}"#);
    }

    #[test]
    fn unreachable_sink_does_not_break_primary() {
        // A port from the ephemeral range with nothing listening.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        drop(listener);

        let mut tee = Tee::new(
            RecordingSubmitter::new(),
            LocalSink::new(format!("http://{addr}")),
        );
        tee.submit_draft_pick(json!({"pick": 1}));
        drop(tee); // joins the worker; must not hang or panic

        // (tee was consumed; the assertion is that we got here without blocking.)
    }
}
