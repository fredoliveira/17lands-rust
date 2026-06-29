// SPDX-License-Identifier: GPL-3.0-only

//! The stateful log follower: tailing, line accumulation, dispatch, and all handlers
//! (port of the `Follower` class in `mtga_follower.py`).
//!
//! State modeling: dynamic blobs are `serde_json::Value`; the instance-id maps
//! use `serde_json::Map` (insertion-ordered via the `preserve_order` feature) so derived
//! lists like `opponent_card_ids` keep Python dict order. `defaultdict` semantics are
//! emulated at the read sites (`.get(...).unwrap_or_default()` etc.).

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::api_client::{ApiClient, CLIENT_VERSION, Submitter};
use crate::time_parse::{epoch_zero, extract_time, isoformat, maybe_get_utc_timestamp};

const FILE_UPDATED_FORCE_REFRESH_SECONDS: u64 = 60;
const SLEEP_TIME: Duration = Duration::from_millis(500);

/// Log target marking low-signal, periodic background-sync lines (mastery/inventory/
/// collection/ongoing-event polling). The console formatter (`main::init_logging`) renders
/// these fully dimmed so they recede behind real events; the `--verbose` developer format
/// still shows them in full. Purely cosmetic — no effect on the wire payload.
pub const CHATTER: &str = "17l::chatter";

// ---------------------------------------------------------------------------------------
// Regexes (port of mtga_follower.py:132-143). Python `.match` is start-anchored; patterns
// keep the `^` / leading `.*` from the source so `captures()` reproduces it.
// ---------------------------------------------------------------------------------------

fn re_log_start_timed() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\[(UnityCrossThreadLogger|Client GRE)\](\d[\d:/ .-]+(AM|PM)?)").unwrap()
    })
}
fn re_log_start_untimed() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[(UnityCrossThreadLogger|Client GRE)\]").unwrap())
}
fn re_timestamp() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([\d/.-]+[ T][\d]+:[\d]+:[\d]+( AM| PM)?)").unwrap())
}
fn re_json_start() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[\[\{]").unwrap())
}
fn re_account_info() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r".*Updated account\. DisplayName:(.*), AccountID:(.*), Token:.*").unwrap()
    })
}
fn re_login() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r".*Logged in successfully\. Display Name:(.*)").unwrap())
}
fn re_match_account_info() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r".*: ((\w+) to Match|Match to (\w+)):").unwrap())
}

// ---------------------------------------------------------------------------------------
// Free helpers (ports of module-level functions).
// ---------------------------------------------------------------------------------------

/// `contains_log_key` (`mtga_follower.py:228`): substring with and without underscores.
fn contains_log_key(key: &str, full_log: &str) -> bool {
    full_log.contains(key) || full_log.contains(&key.replace('_', ""))
}

/// `json_value_matches` (`mtga_follower.py:189`): the value at `path` equals `expectation`.
fn json_value_matches(expectation: &Value, path: &[&str], blob: &Value) -> bool {
    let mut cur = blob;
    for p in path {
        match cur.get(*p) {
            Some(v) => cur = v,
            None => return false,
        }
    }
    cur == expectation
}

/// `int(value)`: integer numbers as-is, floats truncated, all-integer strings parsed.
fn to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_u64().map(|u| u as i64))
            .or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// `[int(x) for x in arr]` — coerce each element of a JSON array to an integer `Value`.
fn int_array(v: &Value) -> Vec<Value> {
    v.as_array()
        .map(|a| a.iter().filter_map(to_i64).map(Value::from).collect())
        .unwrap_or_default()
}

/// `Optional[str]` → JSON (`None` → `null`).
fn opt_str(v: &Option<String>) -> Value {
    match v {
        Some(s) => Value::String(s.clone()),
        None => Value::Null,
    }
}

/// `Optional[Value]` → JSON (`None` → `null`).
fn opt_val(v: &Option<Value>) -> Value {
    v.clone().unwrap_or(Value::Null)
}

/// `Optional[int]` → JSON.
fn opt_i64(v: Option<i64>) -> Value {
    v.map(Value::from).unwrap_or(Value::Null)
}

/// Python `str(x)` for the rank-string components (`None` → "None", bools title-cased,
/// floats rendered with a trailing `.0` when integral).
fn python_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".to_string(),
        Some(Value::Bool(true)) => "True".to_string(),
        Some(Value::Bool(false)) => "False".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            } else {
                n.to_string()
            }
        }
        Some(other) => other.to_string(),
    }
}

/// `__try_decode` (`mtga_follower.py:619`): if the value is a JSON-encoded string, decode
/// one value from it; on failure (or if it's already non-string) return it unchanged.
fn try_decode(v: &Value) -> Value {
    match v {
        Value::String(s) => {
            let mut it = serde_json::Deserializer::from_str(s).into_iter::<Value>();
            match it.next() {
                Some(Ok(parsed)) => parsed,
                _ => v.clone(),
            }
        }
        other => other.clone(),
    }
}

/// `__extract_payload` (`mtga_follower.py:626`): recursively unwrap `payload`/`Payload`/
/// `request`, except for `clientToMatchServiceMessageType` blobs which are returned as-is.
fn extract_payload(blob: Value) -> Value {
    if !blob.is_object() {
        return blob;
    }
    if blob.get("clientToMatchServiceMessageType").is_some() {
        return blob;
    }
    for key in ["payload", "Payload", "request"] {
        if let Some(v) = blob.get(key) {
            return extract_payload(try_decode(v));
        }
    }
    blob
}

/// `get_rank_string` (`mtga_follower.py:207`): `"-".join(str(x) for x in [...])`.
fn get_rank_string(
    rank_class: Option<&Value>,
    level: Option<&Value>,
    percentile: Option<&Value>,
    place: Option<&Value>,
    step: Option<&Value>,
) -> String {
    [rank_class, level, percentile, place, step]
        .iter()
        .map(|x| python_str(*x))
        .collect::<Vec<_>>()
        .join("-")
}

// ---------------------------------------------------------------------------------------
// Follower
// ---------------------------------------------------------------------------------------

/// Mirrors `Follower` state. Reset wholesale on each outer tail-loop pass.
pub struct Follower<S: Submitter> {
    /// The submitter (live `ApiClient`, or a recorder in tests). Public for test inspection.
    pub api: S,
    token: String,
    host: String,

    // Timing
    cur_log_time: chrono::NaiveDateTime,
    last_utc_time: chrono::NaiveDateTime,
    last_event_time: Option<Value>,
    last_raw_time: String,

    // Identity
    cur_user: Option<String>,
    user_screen_name: Option<String>,
    full_screen_name: Option<String>,
    screen_names: HashMap<i64, String>,
    disconnected_user: Option<String>,
    disconnected_screen_name: Option<String>,
    disconnected_full_screen_name: Option<String>,
    disconnected_rank: Option<Value>,

    // Draft / event
    cur_draft_event: Option<Value>,
    cur_rank_data: Option<Value>,
    cur_opponent_level: Option<String>,
    cur_opponent_match_id: Option<Value>,
    current_match_id: Option<Value>,
    current_event_id: Option<Value>,

    // Game / board
    starting_team_id: Option<i64>,
    seat_id: Option<i64>,
    turn_count: i64,
    objects_by_owner: HashMap<i64, Map<String, Value>>,
    opening_hand_count_by_seat: HashMap<i64, i64>,
    opening_hand: HashMap<i64, Vec<Value>>,
    drawn_hands: HashMap<i64, Vec<Vec<Value>>>,
    drawn_cards_by_instance_id: HashMap<i64, Map<String, Value>>,
    cards_in_hand: HashMap<i64, Vec<Value>>,
    current_game_maindeck: Option<Value>,
    current_game_sideboard: Option<Value>,
    current_game_additional_deck_info: Option<Value>,
    game_service_metadata: Option<Value>,
    game_client_metadata: Option<Value>,
    game_history_events: Vec<Value>,

    // Pending submission
    pending_game_submission: Map<String, Value>,
    pending_game_result: Map<String, Value>,
    pending_match_result: Map<String, Value>,

    // Buffering
    buffer: Vec<String>,
    last_blob: String,
    current_debug_blob: String,
}

impl Follower<ApiClient> {
    /// Build a Follower backed by the live REST client (port of `Follower.__init__`).
    pub fn new(token: String, host: String) -> Self {
        let api = ApiClient::new(host.clone());
        Self::with_submitter(token, host, api)
    }
}

impl<S: Submitter> Follower<S> {
    /// Build a Follower with an arbitrary submitter (the test seam).
    pub fn with_submitter(token: String, host: String, api: S) -> Self {
        let mut f = Self {
            api,
            token,
            host,
            cur_log_time: epoch_zero(),
            last_utc_time: epoch_zero(),
            last_event_time: None,
            last_raw_time: String::new(),
            cur_user: None,
            user_screen_name: None,
            full_screen_name: None,
            screen_names: HashMap::new(),
            disconnected_user: None,
            disconnected_screen_name: None,
            disconnected_full_screen_name: None,
            disconnected_rank: None,
            cur_draft_event: None,
            cur_rank_data: None,
            cur_opponent_level: None,
            cur_opponent_match_id: None,
            current_match_id: None,
            current_event_id: None,
            starting_team_id: None,
            seat_id: None,
            turn_count: 0,
            objects_by_owner: HashMap::new(),
            opening_hand_count_by_seat: HashMap::new(),
            opening_hand: HashMap::new(),
            drawn_hands: HashMap::new(),
            drawn_cards_by_instance_id: HashMap::new(),
            cards_in_hand: HashMap::new(),
            current_game_maindeck: None,
            current_game_sideboard: None,
            current_game_additional_deck_info: None,
            game_service_metadata: None,
            game_client_metadata: None,
            game_history_events: Vec::new(),
            pending_game_submission: Map::new(),
            pending_game_result: Map::new(),
            pending_match_result: Map::new(),
            buffer: Vec::new(),
            last_blob: String::new(),
            current_debug_blob: String::new(),
        };
        f.reinitialize();
        f
    }

    /// Port of `_reinitialize`: reset all state, ending with `__clear_match_data`.
    fn reinitialize(&mut self) {
        self.buffer.clear();
        self.cur_log_time = epoch_zero();
        self.last_utc_time = epoch_zero();
        self.last_event_time = None;
        self.last_raw_time = String::new();
        self.disconnected_user = None;
        self.disconnected_screen_name = None;
        self.disconnected_full_screen_name = None;
        self.disconnected_rank = None;
        self.cur_user = None;
        self.cur_draft_event = None;
        self.cur_rank_data = None;
        self.cur_opponent_level = None;
        self.cur_opponent_match_id = None;
        self.current_match_id = None;
        self.current_event_id = None;
        self.starting_team_id = None;
        self.seat_id = None;
        self.turn_count = 0;
        self.current_game_maindeck = None;
        self.current_game_sideboard = None;
        self.current_game_additional_deck_info = None;
        self.game_service_metadata = None;
        self.game_client_metadata = None;
        self.objects_by_owner.clear();
        self.opening_hand_count_by_seat.clear();
        self.opening_hand.clear();
        self.drawn_hands.clear();
        self.drawn_cards_by_instance_id.clear();
        self.cards_in_hand.clear();
        self.user_screen_name = None;
        self.full_screen_name = None;
        self.screen_names.clear();
        self.game_history_events.clear();
        self.pending_game_submission = Map::new();
        self.pending_game_result = Map::new();
        self.pending_match_result = Map::new();
        self.last_blob = String::new();
        self.current_debug_blob = String::new();
        self.clear_match_data(false);
    }

    /// The base envelope (`_add_base_api_data`). Fields are emitted even when null.
    fn add_base_api_data(&self, blob: Map<String, Value>) -> Value {
        let mut m = Map::new();
        m.insert("token".into(), Value::String(self.token.clone()));
        m.insert(
            "client_version".into(),
            Value::String(CLIENT_VERSION.into()),
        );
        m.insert("player_id".into(), opt_str(&self.cur_user));
        m.insert("time".into(), Value::String(isoformat(&self.cur_log_time)));
        m.insert(
            "utc_time".into(),
            Value::String(isoformat(&self.last_utc_time)),
        );
        m.insert("event_time".into(), opt_val(&self.last_event_time));
        m.insert("raw_time".into(), Value::String(self.last_raw_time.clone()));
        for (k, v) in blob {
            m.insert(k, v);
        }
        Value::Object(m)
    }

    // -----------------------------------------------------------------------------------
    // Tailing
    // -----------------------------------------------------------------------------------

    /// Tail (or read once) a log file, dispatching complete entries (port of `parse_log`).
    pub fn parse_log(&mut self, filename: &str, follow: bool) {
        // The plain entry point never cancels: a flag that is always `false` makes
        // `parse_log_cancellable` behave exactly like the original loop. Existing callers
        // and tests are unaffected.
        let never = Arc::new(AtomicBool::new(false));
        self.parse_log_cancellable(filename, follow, &never);
    }

    /// Like [`parse_log`](Self::parse_log) but honours a cooperative cancellation flag,
    /// returning within one [`SLEEP_TIME`] tick once `cancel` is set. This is additive
    /// control flow only: it changes nothing about which payloads are built or sent, so the
    /// wire contract (and parity tests) are unaffected. Used by the desktop app's start/stop.
    pub fn parse_log_cancellable(&mut self, filename: &str, follow: bool, cancel: &Arc<AtomicBool>) {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            self.reinitialize();
            let mut last_read_time = SystemTime::now();
            let mut last_file_size: u64 = 0;

            match std::fs::File::open(filename) {
                Ok(file) => {
                    let mut reader = BufReader::new(file);
                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut raw = Vec::new();
                        let read = read_until_newline(&mut reader, &mut raw);
                        let file_size = std::fs::metadata(filename).map(|m| m.len()).unwrap_or(0);

                        match read {
                            Ok(0) => {
                                // EOF.
                                self.handle_complete_log_entry();
                                let last_modified = std::fs::metadata(filename)
                                    .and_then(|m| m.modified())
                                    .unwrap_or(SystemTime::UNIX_EPOCH);

                                if file_size < last_file_size {
                                    log::info!(
                                        "Starting from beginning of file as file is smaller than before (previous = {last_file_size}; current = {file_size})"
                                    );
                                    break;
                                } else if last_modified
                                    > last_read_time
                                        + Duration::from_secs(FILE_UPDATED_FORCE_REFRESH_SECONDS)
                                {
                                    log::info!(
                                        "Starting from beginning of file as file has been updated much more recently than the last read"
                                    );
                                    break;
                                } else if follow {
                                    std::thread::sleep(SLEEP_TIME);
                                } else {
                                    break;
                                }
                            }
                            Ok(_) => {
                                let line = String::from_utf8_lossy(&raw);
                                self.append_line(&line);
                                last_read_time = SystemTime::now();
                                last_file_size = file_size;
                            }
                            Err(e) => {
                                log::error!("Error parsing log: {e}");
                                break;
                            }
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    std::thread::sleep(SLEEP_TIME);
                }
                Err(e) => {
                    log::error!("Error opening log: {e}");
                }
            }

            if !follow {
                log::info!("Done processing file.");
                break;
            }
        }
    }

    /// Process a complete in-memory log (test seam): reinitialize, feed each
    /// line, then flush the final entry. Equivalent to `parse_log(.., follow=false)`
    /// without file I/O or the tail loop.
    pub fn process_str(&mut self, data: &str) {
        self.reinitialize();
        for line in data.split_inclusive('\n') {
            self.append_line(line);
        }
        self.handle_complete_log_entry();
    }

    // -----------------------------------------------------------------------------------
    // Line accumulation & entry boundaries
    // -----------------------------------------------------------------------------------

    /// Port of `__append_line` (`recent_lines` dropped).
    fn append_line(&mut self, line: &str) {
        // __check_detailed_logs: keep the warning, drop the GUI dialog.
        if line.starts_with("DETAILED LOGS: DISABLED") {
            log::warn!("Detailed logs are disabled in MTGA.");
        } else if line.starts_with("DETAILED LOGS: ENABLED") {
            log::info!(target: CHATTER, "Detailed logs enabled in MTGA.");
        }

        self.maybe_handle_account_info(line);

        if let Some(c) = re_timestamp().captures(line) {
            self.last_raw_time = c[1].to_string();
            if let Ok(t) = extract_time(&self.last_raw_time) {
                self.cur_log_time = t;
            }
        }

        if let Some(m) = re_log_start_untimed().find(line) {
            self.handle_complete_log_entry();

            if let Some(tc) = re_log_start_timed().captures(line) {
                self.last_raw_time = tc.get(2).map(|g| g.as_str()).unwrap_or("").to_string();
                if let Ok(t) = extract_time(&self.last_raw_time) {
                    self.cur_log_time = t;
                }
                let end = tc.get(0).unwrap().end();
                self.buffer.push(line[end..].to_string());
            } else {
                self.buffer.push(line[m.end()..].to_string());
            }
        } else {
            self.buffer.push(line.to_string());
        }
    }

    /// Port of `__handle_complete_log_entry` (dedup; `cur_log_time` reset stays omitted).
    fn handle_complete_log_entry(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        // cur_log_time is never None in this port (the reset is commented out in Python).

        let full_log = self.buffer.concat();
        self.current_debug_blob = full_log.clone();

        if full_log != self.last_blob {
            self.handle_blob(&full_log);
            self.last_blob = full_log;
        } else {
            // Dedup hits are pure noise on the console (e.g. repeated "AK Message: Audio
            // thread resumed" lines); keep them at debug so `RUST_LOG=debug` can still show
            // them. Console-only — no effect on the wire (upstream logs this at info).
            log::debug!("Skipping repeated complete log entry: {full_log}");
        }

        self.buffer.clear();
    }

    // -----------------------------------------------------------------------------------
    // Blob parse + dispatch
    // -----------------------------------------------------------------------------------

    /// Port of `__handle_blob`: locate JSON, `raw_decode`, extract payload, dispatch.
    fn handle_blob(&mut self, full_log: &str) {
        let Some(m) = re_json_start().find(full_log) else {
            return;
        };
        let start = m.start();

        // raw_decode: one JSON value from `start`, ignoring trailing text.
        let mut stream =
            serde_json::Deserializer::from_str(&full_log[start..]).into_iter::<Value>();
        let json_obj = match stream.next() {
            Some(Ok(v)) => v,
            _ => {
                log::debug!(
                    "JSON decode error at {} for: {full_log}",
                    isoformat(&self.cur_log_time)
                );
                return;
            }
        };

        let json_obj = extract_payload(json_obj);
        if !json_obj.is_object() {
            return;
        }

        // utc_time / event_time (each wrapped in try/ignore in Python).
        if let Some(t) = maybe_get_utc_timestamp(&json_obj) {
            self.last_utc_time = t;
        }
        // NOTE (Python quirk, mtga_follower.py:496-505): the local `maybe_time` is reused —
        // after updating `last_utc_time` it is reassigned to `blob.get("EventTime")` before
        // being passed to the game-history handlers. So the history `_timestamp` is driven
        // by EventTime (absent in real logs → always null), NOT the utc timestamp. Mirror
        // that exactly.
        let event_time = json_obj.get("EventTime").cloned();
        if let Some(et) = &event_time {
            self.last_event_time = Some(et.clone());
        }

        self.dispatch(full_log, &json_obj, event_time);
    }

    /// The §6 dispatch table — first match wins, order is significant. `event_time` is the
    /// reused `maybe_time` (= `blob.get("EventTime")`) passed to game-history handlers.
    fn dispatch(&mut self, full_log: &str, obj: &Value, event_time: Option<Value>) {
        let has = |k: &str| obj.get(k).is_some();
        let client_connected = Value::String("Client.Connected".into());
        let c2gre = Value::String("ClientToMatchServiceMessageType_ClientToGREMessage".into());
        let c2greui = Value::String("ClientToMatchServiceMessageType_ClientToGREUIMessage".into());

        if json_value_matches(&client_connected, &["params", "messageName"], obj) {
            self.handle_login(obj);
        } else if contains_log_key("Event_Join", full_log) && has("EventName") {
            self.handle_joined_pod(obj);
        } else if contains_log_key("Event_Join", full_log) && has("Course") {
            self.handle_joined_event_response(obj);
        } else if has("DraftStatus") {
            self.handle_bot_draft_pack(obj);
        } else if contains_log_key("BotDraft_DraftPick", full_log) && has("PickInfo") {
            let pick_info = obj.get("PickInfo").cloned().unwrap_or(Value::Null);
            self.handle_bot_draft_pick(&pick_info);
        } else if contains_log_key("LogBusinessEvents", full_log) && has("PickGrpId") {
            self.handle_human_draft_combined(obj);
        } else if contains_log_key("LogBusinessEvents", full_log) && has("WinningType") {
            self.handle_log_business_game_end(obj);
        } else if full_log.contains("Draft.Notify ") && !has("method") {
            self.handle_human_draft_pack(obj);
        } else if contains_log_key("EventPlayerDraftMakePick", full_log) && has("GrpIds") {
            self.handle_player_draft_pick(obj);
        } else if contains_log_key("Event_SetDeck", full_log) && has("EventName") {
            self.handle_deck_submission(obj);
        } else if contains_log_key("Event_GetCourses", full_log) && has("Courses") {
            self.handle_ongoing_events(obj);
        } else if contains_log_key("Event_ClaimPrize", full_log) && has("EventName") {
            self.handle_claim_prize(obj);
        } else if contains_log_key("Draft_CompleteDraft", full_log) && has("DraftId") {
            self.handle_event_course(obj);
        } else if has("authenticateResponse") {
            if let Some(name) = obj
                .get("authenticateResponse")
                .and_then(|a| a.get("screenName"))
                .and_then(|n| n.as_str())
            {
                self.update_screen_name(name);
            }
        } else if has("matchGameRoomStateChangedEvent") {
            self.handle_match_state_changed(obj);
        } else if obj
            .get("greToClientEvent")
            .map(|g| g.get("greToClientMessages").is_some())
            .unwrap_or(false)
        {
            if let Some(messages) = obj["greToClientEvent"]["greToClientMessages"].as_array() {
                for message in messages.clone() {
                    self.handle_gre_to_client_message(&message, &event_time);
                }
            }
        } else if json_value_matches(&c2gre, &["clientToMatchServiceMessageType"], obj) {
            let payload = obj.get("payload").cloned().unwrap_or_else(|| json!({}));
            self.handle_client_to_gre_message(&payload, &event_time);
        } else if json_value_matches(&c2greui, &["clientToMatchServiceMessageType"], obj) {
            let payload = obj.get("payload").cloned().unwrap_or_else(|| json!({}));
            self.handle_client_to_gre_ui_message(&payload, &event_time);
        } else if contains_log_key("Rank_GetCombinedRankInfo", full_log)
            && has("limitedSeasonOrdinal")
        {
            self.handle_self_rank_info(obj);
        } else if full_log.contains(" PlayerInventory.GetPlayerCardsV3 ") && !has("method") {
            self.handle_collection(obj);
        } else if has("DTO_InventoryInfo") {
            let inv = obj.get("DTO_InventoryInfo").cloned().unwrap_or(Value::Null);
            self.handle_inventory(&inv);
        } else if obj
            .get("NodeStates")
            .map(|n| n.get("RewardTierUpgrade").is_some())
            .unwrap_or(false)
        {
            self.handle_player_progress(obj);
        } else if full_log.contains("FrontDoorConnection.Close ") {
            self.reset_current_user();
        } else if full_log.contains("Reconnect result : Connected") {
            self.handle_reconnect_result();
        }
    }

    // -----------------------------------------------------------------------------------
    // Account info
    // -----------------------------------------------------------------------------------

    fn maybe_handle_account_info(&mut self, line: &str) {
        if let Some(c) = re_account_info().captures(line) {
            let screen_name = c[1].to_string();
            self.cur_user = Some(c[2].to_string());
            self.update_screen_name(&screen_name);
            return;
        }
        if let Some(c) = re_match_account_info().captures(line) {
            self.cur_user = c
                .get(2)
                .or_else(|| c.get(3))
                .map(|m| m.as_str().to_string());
            return;
        }
        if let Some(c) = re_login().captures(line) {
            self.full_screen_name = Some(c[1].to_string());
        }
    }

    fn update_screen_name(&mut self, screen_name: &str) {
        if self.user_screen_name.as_deref() == Some(screen_name) {
            return;
        }
        self.user_screen_name = Some(screen_name.to_string());

        let mut info = Map::new();
        info.insert("player_id".into(), opt_str(&self.cur_user));
        info.insert("screen_name".into(), opt_str(&self.user_screen_name));
        info.insert("full_screen_name".into(), opt_str(&self.full_screen_name));
        log::info!(target: CHATTER, "Updating user info");
        let payload = self.add_base_api_data(info);
        self.api.submit_user(payload);
    }

    // -----------------------------------------------------------------------------------
    // Simple handlers
    // -----------------------------------------------------------------------------------

    fn handle_login(&mut self, obj: &Value) {
        self.clear_game_data(false);
        if let Some(po) = obj.get("params").and_then(|p| p.get("payloadObject")) {
            if let Some(pid) = po.get("playerId").and_then(|v| v.as_str()) {
                self.cur_user = Some(pid.to_string());
            }
            if let Some(name) = po.get("screenName").and_then(|v| v.as_str()) {
                self.update_screen_name(name);
            }
        }
    }

    fn handle_joined_pod(&mut self, obj: &Value) {
        self.clear_game_data(true);
        if let Some(name) = obj.get("EventName") {
            self.cur_draft_event = Some(name.clone());
            log::info!("Joined draft pod: {name}");
        }
    }

    fn handle_joined_event_response(&mut self, obj: &Value) {
        self.clear_game_data(true);
        let mut event = Map::new();
        event.insert("payload".into(), obj.clone());
        let payload = self.add_base_api_data(event);
        self.api.submit_joined_event(payload);
        log::info!(target: CHATTER, "Joined event successfully");
    }

    fn handle_bot_draft_pack(&mut self, obj: &Value) {
        if obj.get("DraftStatus").and_then(|v| v.as_str()) != Some("PickNext") {
            return;
        }
        self.clear_game_data(true);
        self.cur_draft_event = obj.get("EventName").cloned();

        let mut pack = Map::new();
        pack.insert("payload".into(), obj.clone());
        pack.insert("event_name".into(), opt_val(&obj.get("EventName").cloned()));
        pack.insert(
            "pack_number".into(),
            opt_i64(obj.get("PackNumber").and_then(to_i64)),
        );
        pack.insert(
            "pick_number".into(),
            opt_i64(obj.get("PickNumber").and_then(to_i64)),
        );
        pack.insert(
            "card_ids".into(),
            Value::Array(obj.get("DraftPack").map(int_array).unwrap_or_default()),
        );
        log::info!("Draft pack");
        let payload = self.add_base_api_data(pack);
        self.api.submit_draft_pack(payload);
    }

    fn handle_bot_draft_pick(&mut self, obj: &Value) {
        self.clear_game_data(true);
        self.cur_draft_event = obj.get("EventName").cloned();

        let card_id = obj.get("CardId").and_then(to_i64);
        let card_ids = obj.get("CardIds").map(int_array);

        let mut pick = Map::new();
        pick.insert("event_name".into(), opt_val(&obj.get("EventName").cloned()));
        pick.insert(
            "pack_number".into(),
            opt_i64(obj.get("PackNumber").and_then(to_i64)),
        );
        pick.insert(
            "pick_number".into(),
            opt_i64(obj.get("PickNumber").and_then(to_i64)),
        );
        pick.insert("card_id".into(), opt_i64(card_id));
        pick.insert(
            "card_ids".into(),
            card_ids.map(Value::Array).unwrap_or(Value::Null),
        );
        log::info!("Draft pick");
        let payload = self.add_base_api_data(pick);
        self.api.submit_draft_pick(payload);
    }

    fn handle_human_draft_combined(&mut self, obj: &Value) {
        self.clear_game_data(true);
        self.cur_draft_event = obj.get("EventId").cloned();

        let mut pack = Map::new();
        pack.insert("payload".into(), obj.clone());
        pack.insert("draft_id".into(), opt_val(&obj.get("DraftId").cloned()));
        pack.insert("event_name".into(), opt_val(&obj.get("EventId").cloned()));
        pack.insert(
            "pack_number".into(),
            opt_i64(obj.get("PackNumber").and_then(to_i64)),
        );
        pack.insert(
            "pick_number".into(),
            opt_i64(obj.get("PickNumber").and_then(to_i64)),
        );
        pack.insert("card_ids".into(), opt_val(&obj.get("CardsInPack").cloned()));
        pack.insert("method".into(), Value::String("LogBusiness".into()));
        log::info!("Human draft pack (combined)");
        let payload = self.add_base_api_data(pack);
        self.api.submit_human_draft_pack(payload);

        // pick_id = int(PickGrpId) or None on failure.
        let pick_id = obj.get("PickGrpId").and_then(to_i64);

        let mut pick = Map::new();
        pick.insert("payload".into(), obj.clone());
        pick.insert("draft_id".into(), opt_val(&obj.get("DraftId").cloned()));
        pick.insert("event_name".into(), opt_val(&obj.get("EventId").cloned()));
        pick.insert(
            "pack_number".into(),
            opt_i64(obj.get("PackNumber").and_then(to_i64)),
        );
        pick.insert(
            "pick_number".into(),
            opt_i64(obj.get("PickNumber").and_then(to_i64)),
        );
        pick.insert("card_id".into(), opt_i64(pick_id));
        pick.insert("auto_pick".into(), opt_val(&obj.get("AutoPick").cloned()));
        pick.insert(
            "time_remaining".into(),
            opt_val(&obj.get("TimeRemainingOnPick").cloned()),
        );
        log::info!("Human draft pick (combined)");
        let payload = self.add_base_api_data(pick);
        self.api.submit_human_draft_pick(payload);
    }

    fn handle_human_draft_pack(&mut self, obj: &Value) {
        self.clear_game_data(true);

        let card_ids: Vec<Value> = obj
            .get("PackCards")
            .and_then(|v| v.as_str())
            .map(|s| {
                s.split(',')
                    .filter_map(|x| x.trim().parse::<i64>().ok())
                    .map(Value::from)
                    .collect()
            })
            .unwrap_or_default();

        let mut pack = Map::new();
        pack.insert("payload".into(), obj.clone());
        pack.insert("draft_id".into(), opt_val(&obj.get("draftId").cloned()));
        pack.insert("event_name".into(), opt_val(&self.cur_draft_event));
        pack.insert(
            "pack_number".into(),
            opt_i64(obj.get("SelfPack").and_then(to_i64)),
        );
        pack.insert(
            "pick_number".into(),
            opt_i64(obj.get("SelfPick").and_then(to_i64)),
        );
        pack.insert("card_ids".into(), Value::Array(card_ids));
        pack.insert("method".into(), Value::String("Draft.Notify".into()));
        log::info!("Human draft pack (Draft.Notify)");
        let payload = self.add_base_api_data(pack);
        self.api.submit_human_draft_pack(payload);
    }

    fn handle_player_draft_pick(&mut self, obj: &Value) {
        self.clear_game_data(true);

        let mut pick = Map::new();
        pick.insert("payload".into(), obj.clone());
        pick.insert("draft_id".into(), opt_val(&obj.get("DraftId").cloned()));
        pick.insert("event_name".into(), opt_val(&self.cur_draft_event));
        pick.insert(
            "pack_number".into(),
            opt_i64(obj.get("Pack").and_then(to_i64)),
        );
        pick.insert(
            "pick_number".into(),
            opt_i64(obj.get("Pick").and_then(to_i64)),
        );
        pick.insert("card_ids".into(), opt_val(&obj.get("GrpIds").cloned()));
        log::info!("Human draft pick (EventPlayerDraftMakePick)");
        let payload = self.add_base_api_data(pick);
        self.api.submit_human_draft_pick(payload);
    }

    fn handle_deck_submission(&mut self, obj: &Value) {
        self.clear_game_data(true);
        let Some(decks) = obj.get("Deck") else { return };

        let expand = |key: &str| -> Vec<Value> {
            let mut out = Vec::new();
            if let Some(arr) = decks.get(key).and_then(|d| d.as_array()) {
                for d in arr {
                    let qty = d.get("quantity").and_then(to_i64).unwrap_or(0);
                    if let Some(card_id) = d.get("cardId") {
                        for _ in 0..qty {
                            out.push(card_id.clone());
                        }
                    }
                }
            }
            out
        };

        let companion = decks
            .get("Companions")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("cardId"))
            .cloned()
            .unwrap_or(Value::from(0));

        let mut deck = Map::new();
        deck.insert("payload".into(), obj.clone());
        deck.insert("event_name".into(), opt_val(&obj.get("EventName").cloned()));
        deck.insert("maindeck_card_ids".into(), Value::Array(expand("MainDeck")));
        deck.insert(
            "sideboard_card_ids".into(),
            Value::Array(expand("Sideboard")),
        );
        deck.insert("companion".into(), companion);
        deck.insert("is_during_match".into(), Value::Bool(false));
        log::info!("Deck submission (Event_SetDeck)");
        let payload = self.add_base_api_data(deck);
        self.api.submit_deck_submission(payload);
    }

    fn handle_ongoing_events(&mut self, obj: &Value) {
        let mut event = Map::new();
        event.insert("courses".into(), opt_val(&obj.get("Courses").cloned()));
        log::info!(target: CHATTER, "Updated ongoing events");
        let payload = self.add_base_api_data(event);
        self.api.submit_ongoing_events(payload);
    }

    fn handle_claim_prize(&mut self, obj: &Value) {
        let mut event = Map::new();
        event.insert("event_name".into(), opt_val(&obj.get("EventName").cloned()));
        log::info!("Event ended");
        let payload = self.add_base_api_data(event);
        self.api.submit_event_ended(payload);
    }

    fn handle_event_course(&mut self, obj: &Value) {
        let mut event = Map::new();
        event.insert("payload".into(), obj.clone());
        event.insert(
            "event_name".into(),
            opt_val(&obj.get("InternalEventName").cloned()),
        );
        event.insert("draft_id".into(), opt_val(&obj.get("DraftId").cloned()));
        event.insert("course_id".into(), opt_val(&obj.get("CourseId").cloned()));
        event.insert("card_pool".into(), opt_val(&obj.get("CardPool").cloned()));
        log::info!("Event course");
        let payload = self.add_base_api_data(event);
        self.api.submit_event_course_submission(payload);
    }

    fn handle_self_rank_info(&mut self, obj: &Value) {
        self.cur_rank_data = Some(obj.clone());
        if let Some(pid) = obj.get("playerId").and_then(|v| v.as_str()) {
            self.cur_user = Some(pid.to_string());
        }
        log::info!(target: CHATTER, "Parsed rank info");
        let mut data = Map::new();
        data.insert("rank_data".into(), opt_val(&self.cur_rank_data));
        data.insert("limited_rank".into(), Value::Null);
        data.insert("constructed_rank".into(), Value::Null);
        let payload = self.add_base_api_data(data);
        self.api.submit_rank(payload);
    }

    fn handle_collection(&mut self, obj: &Value) {
        if self.cur_user.is_none() {
            log::info!("Skipping collection submission because player id is still unknown");
            return;
        }
        let mut collection = Map::new();
        collection.insert("card_counts".into(), obj.clone());
        log::info!(target: CHATTER, "Collection submission");
        let payload = self.add_base_api_data(collection);
        self.api.submit_collection(payload);
    }

    fn handle_inventory(&mut self, obj: &Value) {
        const KEEP: &[&str] = &[
            "Gems",
            "Gold",
            "TotalVaultProgress",
            "wcTrackPosition",
            "WildCardCommons",
            "WildCardUnCommons",
            "WildCardRares",
            "WildCardMythics",
            "DraftTokens",
            "SealedTokens",
            "Boosters",
            "Changes",
        ];
        let mut filtered = Map::new();
        if let Some(map) = obj.as_object() {
            for (k, v) in map {
                if KEEP.contains(&k.as_str()) {
                    filtered.insert(k.clone(), v.clone());
                }
            }
        }
        let mut blob = Map::new();
        blob.insert("inventory".into(), Value::Object(filtered));
        log::info!(target: CHATTER, "Submitting inventory");
        let payload = self.add_base_api_data(blob);
        self.api.submit_inventory(payload);
    }

    fn handle_player_progress(&mut self, obj: &Value) {
        let mut blob = Map::new();
        blob.insert("progress".into(), obj.clone());
        log::info!(target: CHATTER, "Submitting mastery progress");
        let payload = self.add_base_api_data(blob);
        self.api.submit_player_progress(payload);
    }

    fn reset_current_user(&mut self) {
        log::info!("User logged out from MTGA");
        if self.cur_user.is_some() {
            self.disconnected_user = self.cur_user.clone();
            self.disconnected_screen_name = self.user_screen_name.clone();
            self.disconnected_full_screen_name = self.full_screen_name.clone();
            self.disconnected_rank = self.cur_rank_data.clone();
        }
        self.cur_user = None;
        self.user_screen_name = None;
        self.full_screen_name = None;
        self.cur_rank_data = None;
    }

    fn handle_reconnect_result(&mut self) {
        log::info!("Reconnected - restoring prior user info");
        self.cur_user = self.disconnected_user.clone();
        self.user_screen_name = self.disconnected_screen_name.clone();
        self.full_screen_name = self.disconnected_full_screen_name.clone();
        self.cur_rank_data = self.disconnected_rank.clone();
    }

    // -----------------------------------------------------------------------------------
    // Game-state machine
    // -----------------------------------------------------------------------------------

    fn add_to_game_history(&mut self, message_blob: &Value, event_time: &Option<Value>) {
        // Python: `None if timestamp is None else timestamp.isoformat()`. After the
        // variable-reuse quirk `timestamp` is `blob.get("EventTime")`, which is absent in
        // real logs (→ null). If it were ever present (never observed) Python would raise
        // on `.isoformat()` of a non-datetime; we carry the value through instead.
        let ts = match event_time {
            None => Value::Null,
            Some(v) => v.clone(),
        };
        let mut entry = Map::new();
        entry.insert("_timestamp".into(), ts);
        if let Some(map) = message_blob.as_object() {
            for (k, v) in map {
                entry.insert(k.clone(), v.clone());
            }
        }
        self.game_history_events.push(Value::Object(entry));
    }

    fn handle_gre_to_client_message(&mut self, message_blob: &Value, event_time: &Option<Value>) {
        let msg_type = message_blob
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Add to history before processing — we may submit the game right away.
        let is_game_state = matches!(
            msg_type,
            "GREMessageType_QueuedGameStateMessage" | "GREMessageType_GameStateMessage"
        );
        let is_ui_chat = msg_type == "GREMessageType_UIMessage"
            && message_blob
                .get("uiMessage")
                .map(|u| u.get("onChat").is_some())
                .unwrap_or(false);
        if is_game_state || is_ui_chat {
            self.add_to_game_history(message_blob, event_time);
        }

        match msg_type {
            "GREMessageType_ConnectResp" => self.handle_gre_connect_response(message_blob),
            "GREMessageType_EdictalMessage" => {
                self.handle_gre_edictal_message(message_blob, event_time)
            }
            "GREMessageType_GameStateMessage" => self.handle_game_state_message(message_blob),
            _ => {}
        }
    }

    fn handle_game_state_message(&mut self, message_blob: &Value) {
        if let Some(seat) = message_blob
            .get("systemSeatIds")
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
            .and_then(to_i64)
        {
            self.seat_id = Some(seat);
        }

        let gsm = match message_blob.get("gameStateMessage") {
            Some(v) => v.clone(),
            None => json!({}),
        };

        if let Some(game_info) = gsm.get("gameInfo") {
            // On a new matchID, switch and clear the event id.
            if let Some(match_id) = game_info.get("matchID")
                && Some(match_id) != self.current_match_id.as_ref()
            {
                self.current_match_id = Some(match_id.clone());
                self.current_event_id = None;
            }
        }

        let turn_info = gsm.get("turnInfo").cloned().unwrap_or_else(|| json!({}));
        let players = gsm
            .get("players")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        // turn_count = turnInfo.turnNumber if truthy, else max(turn_count, sum of players' turnNumber).
        match turn_info.get("turnNumber").and_then(to_i64) {
            Some(tn) if tn != 0 => self.turn_count = tn,
            _ => {
                let turns_sum: i64 = players
                    .iter()
                    .map(|p| p.get("turnNumber").and_then(to_i64).unwrap_or(0))
                    .sum();
                self.turn_count = self.turn_count.max(turns_sum);
            }
        }

        // gameObjects → objects_by_owner (Card / SplitCard only).
        if let Some(objs) = gsm.get("gameObjects").and_then(|o| o.as_array()) {
            for game_object in objs {
                let t = game_object
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if t != "GameObjectType_Card" && t != "GameObjectType_SplitCard" {
                    continue;
                }
                let (owner, instance_id, card_id) = match (
                    game_object.get("ownerSeatId").and_then(to_i64),
                    game_object.get("instanceId").and_then(to_i64),
                    game_object.get("overlayGrpId"),
                ) {
                    (Some(o), Some(i), Some(c)) => (o, i, c.clone()),
                    _ => continue,
                };
                self.objects_by_owner
                    .entry(owner)
                    .or_default()
                    .insert(instance_id.to_string(), card_id);
            }
        }

        // zones → cards_in_hand + drawn_cards_by_instance_id (ZoneType_Hand).
        if let Some(zones) = gsm.get("zones").and_then(|z| z.as_array()) {
            for zone in zones {
                if zone.get("type").and_then(|v| v.as_str()) != Some("ZoneType_Hand") {
                    continue;
                }
                let Some(owner) = zone.get("ownerSeatId").and_then(to_i64) else {
                    continue;
                };
                let hand_ids: Vec<i64> = zone
                    .get("objectInstanceIds")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(to_i64).collect())
                    .unwrap_or_default();

                let player_objects = self.objects_by_owner.entry(owner).or_default().clone();

                // cards_in_hand: skip falsy (0) instance ids; missing card -> null entry.
                let hand: Vec<Value> = hand_ids
                    .iter()
                    .filter(|&&iid| iid != 0)
                    .map(|iid| {
                        player_objects
                            .get(&iid.to_string())
                            .cloned()
                            .unwrap_or(Value::Null)
                    })
                    .collect();
                self.cards_in_hand.insert(owner, hand);

                // drawn_cards_by_instance_id: record found cards.
                let drawn = self.drawn_cards_by_instance_id.entry(owner).or_default();
                for iid in &hand_ids {
                    if let Some(card_id) = player_objects.get(&iid.to_string()) {
                        drawn.insert(iid.to_string(), card_id.clone());
                    }
                }
            }
        }

        // Mulligan / opening-hand bookkeeping.
        let mut deciding: Vec<(i64, i64)> = Vec::new();
        for p in &players {
            if p.get("pendingMessageType").and_then(|v| v.as_str())
                == Some("ClientMessageType_MulliganResp")
            {
                let seat = p.get("systemSeatNumber").and_then(to_i64);
                let mull = p.get("mulliganCount").and_then(to_i64).unwrap_or(0);
                if let Some(seat) = seat
                    && !deciding.contains(&(seat, mull))
                {
                    deciding.push((seat, mull));
                }
            }
        }
        for (player_id, mulligan_count) in deciding {
            if self.starting_team_id.is_none() {
                self.starting_team_id = turn_info.get("activePlayer").and_then(to_i64);
            }
            *self
                .opening_hand_count_by_seat
                .entry(player_id)
                .or_insert(0) += 1;

            let hands = self.drawn_hands.entry(player_id).or_default();
            if mulligan_count == hands.len() as i64 {
                let hand = self
                    .cards_in_hand
                    .get(&player_id)
                    .cloned()
                    .unwrap_or_default();
                hands.push(hand);
            }
        }

        // Capture the opening hand at (Phase_Beginning, Step_Upkeep, turn 1).
        let at_opening = turn_info.get("phase").and_then(|v| v.as_str()) == Some("Phase_Beginning")
            && turn_info.get("step").and_then(|v| v.as_str()) == Some("Step_Upkeep")
            && turn_info.get("turnNumber").and_then(to_i64) == Some(1);
        if self.opening_hand.is_empty() && at_opening {
            let snapshot: Vec<(i64, Vec<Value>)> = self
                .cards_in_hand
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect();
            for (owner, hand) in snapshot {
                self.opening_hand.insert(owner, hand);
            }
        }

        self.maybe_handle_game_over_stage(&gsm);
    }

    fn handle_gre_connect_response(&mut self, blob: &Value) {
        let mut deck_info = blob
            .get("connectResp")
            .and_then(|c| c.get("deckMessage"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.split_deck_info(&mut deck_info);
    }

    /// `.pop()`-then-store deck handling shared by connect-resp and submit-deck-resp:
    /// remove `deckCards`/`sideboardCards`, keep the rest as additional info.
    fn split_deck_info(&mut self, deck_info: &mut Value) {
        if let Some(map) = deck_info.as_object_mut() {
            // shift_remove (not remove/swap_remove) so the *remaining* keys kept as
            // additional_deck_info preserve their order, matching Python's `dict.pop`.
            self.current_game_maindeck =
                Some(map.shift_remove("deckCards").unwrap_or_else(|| json!([])));
            self.current_game_sideboard = Some(
                map.shift_remove("sideboardCards")
                    .unwrap_or_else(|| json!([])),
            );
            self.current_game_additional_deck_info = Some(Value::Object(map.clone()));
        } else {
            self.current_game_maindeck = Some(json!([]));
            self.current_game_sideboard = Some(json!([]));
            self.current_game_additional_deck_info = Some(deck_info.clone());
        }
    }

    fn handle_client_to_gre_message(&mut self, payload: &Value, event_time: &Option<Value>) {
        let ptype = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if ptype == "ClientMessageType_SelectNResp" {
            self.add_to_game_history(payload, event_time);
        }

        if ptype == "ClientMessageType_SubmitDeckResp" {
            self.clear_game_data(true);
            let mut deck_info = payload
                .get("submitDeckResp")
                .and_then(|s| s.get("deck"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            self.split_deck_info(&mut deck_info);
        }
    }

    fn handle_client_to_gre_ui_message(&mut self, payload: &Value, event_time: &Option<Value>) {
        if payload
            .get("uiMessage")
            .map(|u| u.get("onChat").is_some())
            .unwrap_or(false)
        {
            self.add_to_game_history(payload, event_time);
        }
    }

    fn handle_gre_edictal_message(&mut self, payload: &Value, event_time: &Option<Value>) {
        let edict = payload
            .get("edictalMessage")
            .and_then(|e| e.get("edictMessage"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.handle_client_to_gre_message(&edict, event_time);
    }

    fn handle_log_business_game_end(&mut self, payload: &Value) {
        if self.starting_team_id.is_none() {
            self.starting_team_id = payload.get("StartingTeamId").and_then(to_i64);
        }
        if self.enqueue_game_data() {
            let won = self.seat_id == payload.get("WinningTeamId").and_then(to_i64);
            let mut result = Map::new();
            result.insert("game_end_payload".into(), payload.clone());
            result.insert(
                "game_number".into(),
                opt_val(&payload.get("GameNumber").cloned()),
            );
            result.insert("won".into(), Value::Bool(won));
            result.insert(
                "win_type".into(),
                opt_val(&payload.get("WinningType").cloned()),
            );
            result.insert(
                "game_end_reason".into(),
                opt_val(&payload.get("WinningReason").cloned()),
            );
            self.pending_game_result = result;
            log::info!("Added pending game result via LogBusinessEvents");
        }
    }

    fn maybe_handle_game_over_stage(&mut self, gsm: &Value) {
        let game_info = gsm.get("gameInfo").cloned().unwrap_or_else(|| json!({}));
        if game_info.get("stage").and_then(|v| v.as_str()) != Some("GameStage_GameOver") {
            return;
        }
        if let Some(results) = game_info.get("results").and_then(|r| r.as_array())
            && !results.is_empty()
            && self.enqueue_game_data()
        {
            self.enqueue_game_results(results, None);
        }
    }

    fn handle_match_state_changed(&mut self, blob: &Value) {
        let game_room_info = blob
            .get("matchGameRoomStateChangedEvent")
            .and_then(|e| e.get("gameRoomInfo"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let game_room_config = game_room_info
            .get("gameRoomConfig")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let mut updated_match_id = game_room_config.get("matchId").cloned();
        let mut updated_event_id = game_room_config.get("eventId").cloned();

        if let Some(players) = game_room_config
            .get("reservedPlayers")
            .and_then(|p| p.as_array())
        {
            let mut oppo_player_id = String::new();
            for player in players {
                if let (Some(seat), Some(name)) = (
                    player.get("systemSeatId").and_then(to_i64),
                    player.get("playerName").and_then(|v| v.as_str()),
                ) {
                    self.screen_names
                        .insert(seat, name.split('#').next().unwrap_or("").to_string());
                }
                let user_id = player.get("userId").and_then(|v| v.as_str());
                if user_id.is_some() && user_id.map(|s| s.to_string()) == self.cur_user {
                    if let Some(name) = player.get("playerName").and_then(|v| v.as_str()) {
                        self.update_screen_name(name);
                    }
                    if let Some(eid) = player.get("eventId") {
                        updated_event_id = Some(eid.clone());
                    }
                } else if let Some(uid) = user_id {
                    oppo_player_id = uid.to_string();
                }
            }

            if !oppo_player_id.is_empty()
                && let Some(metadata) = game_room_config.get("clientMetadata")
            {
                self.cur_opponent_level = Some(get_rank_string(
                    metadata.get(format!("{oppo_player_id}_RankClass")),
                    metadata.get(format!("{oppo_player_id}_RankTier")),
                    metadata.get(format!("{oppo_player_id}_LeaderboardPercentile")),
                    metadata.get(format!("{oppo_player_id}_LeaderboardPlacement")),
                    None,
                ));
                self.cur_opponent_match_id = game_room_config.get("matchId").cloned();
                log::info!("Parsed opponent rank info");
            }
        }

        let match_truthy = updated_match_id.as_ref().is_some_and(is_truthy);
        let event_truthy = updated_event_id.as_ref().is_some_and(is_truthy);
        if match_truthy && event_truthy {
            self.current_match_id = updated_match_id.take();
            self.current_event_id = updated_event_id.take();
        }

        if let Some(sm) = game_room_config.get("serviceMetadata") {
            self.game_service_metadata = Some(sm.clone());
        }
        if let Some(cm) = game_room_config.get("clientMetadata") {
            self.game_client_metadata = Some(cm.clone());
        }

        if let Some(final_result) = game_room_info.get("finalMatchResult") {
            let results = final_result
                .get("resultList")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();
            if !results.is_empty() && self.enqueue_game_data() {
                self.enqueue_game_results(&results, Some(blob));
            }
            self.clear_match_data(true);
        }
    }

    fn has_pending_game_data(&self) -> bool {
        !self.drawn_cards_by_instance_id.is_empty() && self.game_history_events.len() > 5
    }

    fn enqueue_game_results(&mut self, results: &[Value], match_obj: Option<&Value>) {
        let game_results: Vec<&Value> = results
            .iter()
            .filter(|r| r.get("scope").and_then(|v| v.as_str()) == Some("MatchScope_Game"))
            .collect();
        if let Some(this) = game_results.last() {
            let won = self.seat_id == this.get("winningTeamId").and_then(to_i64);
            let mut result = Map::new();
            result.insert(
                "game_number".into(),
                Value::from(1.max(game_results.len() as i64)),
            );
            result.insert("won".into(), Value::Bool(won));
            result.insert("win_type".into(), opt_val(&this.get("result").cloned()));
            result.insert(
                "game_end_reason".into(),
                opt_val(&this.get("reason").cloned()),
            );
            self.pending_game_result = result;
            log::info!("Added pending game result");
        }

        if let Some(match_result) = results
            .iter()
            .find(|r| r.get("scope").and_then(|v| v.as_str()) == Some("MatchScope_Match"))
        {
            let won_match = self.seat_id == match_result.get("winningTeamId").and_then(to_i64);
            let mut mr = Map::new();
            mr.insert("won_match".into(), Value::Bool(won_match));
            mr.insert(
                "match_result_type".into(),
                opt_val(&match_result.get("result").cloned()),
            );
            mr.insert(
                "match_end_reason".into(),
                opt_val(&match_result.get("reason").cloned()),
            );
            if let Some(obj) = match_obj {
                mr.insert("match_result_payload".into(), obj.clone());
            }
            self.pending_match_result = mr;
            log::info!("Added pending match result");
        }
    }

    fn enqueue_game_data(&mut self) -> bool {
        if !self.has_pending_game_data() {
            return false;
        }

        let opponent_id = if self.seat_id == Some(1) { 2 } else { 1 };
        let opponent_card_ids: Vec<Value> = self
            .objects_by_owner
            .get(&opponent_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();

        if self.current_match_id != self.cur_opponent_match_id {
            self.cur_opponent_level = None;
        }

        let seat = self.seat_id;
        let seat_hand = |map: &HashMap<i64, Vec<Value>>| -> Vec<Value> {
            seat.and_then(|s| map.get(&s)).cloned().unwrap_or_default()
        };
        let opening_hand = seat_hand(&self.opening_hand);
        let drawn_hands: Vec<Vec<Value>> = seat
            .and_then(|s| self.drawn_hands.get(&s))
            .cloned()
            .unwrap_or_default();
        let mulligans: Vec<Vec<Value>> = if drawn_hands.is_empty() {
            Vec::new()
        } else {
            drawn_hands[..drawn_hands.len() - 1].to_vec()
        };
        let drawn_cards: Vec<Value> = seat
            .and_then(|s| self.drawn_cards_by_instance_id.get(&s))
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        let mulligan_count = seat
            .and_then(|s| self.opening_hand_count_by_seat.get(&s))
            .copied()
            .unwrap_or(0)
            - 1;
        let opponent_mulligan_count = self
            .opening_hand_count_by_seat
            .get(&opponent_id)
            .copied()
            .unwrap_or(0)
            - 1;

        let on_play = self.seat_id == self.starting_team_id;

        let to_array = |v: Vec<Vec<Value>>| Value::Array(v.into_iter().map(Value::Array).collect());

        let mut game = Map::new();
        game.insert("event_name".into(), opt_val(&self.current_event_id));
        game.insert("match_id".into(), opt_val(&self.current_match_id));
        game.insert("on_play".into(), Value::Bool(on_play));
        game.insert("opening_hand".into(), Value::Array(opening_hand));
        game.insert("mulligans".into(), to_array(mulligans));
        game.insert("drawn_hands".into(), to_array(drawn_hands));
        game.insert("drawn_cards".into(), Value::Array(drawn_cards));
        game.insert("mulligan_count".into(), Value::from(mulligan_count));
        game.insert(
            "opponent_mulligan_count".into(),
            Value::from(opponent_mulligan_count),
        );
        game.insert("turns".into(), Value::from(self.turn_count));
        game.insert("duration".into(), Value::from(-1));
        game.insert("opponent_card_ids".into(), Value::Array(opponent_card_ids));
        game.insert("rank_data".into(), opt_val(&self.cur_rank_data));
        game.insert("opponent_rank".into(), opt_str(&self.cur_opponent_level));
        game.insert(
            "maindeck_card_ids".into(),
            opt_val(&self.current_game_maindeck),
        );
        game.insert(
            "sideboard_card_ids".into(),
            opt_val(&self.current_game_sideboard),
        );
        game.insert(
            "additional_deck_info".into(),
            opt_val(&self.current_game_additional_deck_info),
        );
        game.insert(
            "service_metadata".into(),
            opt_val(&self.game_service_metadata),
        );
        game.insert(
            "client_metadata".into(),
            opt_val(&self.game_client_metadata),
        );
        log::info!("Completed game");

        let mut history = Map::new();
        history.insert("seat_id".into(), opt_i64(self.seat_id));
        history.insert("opponent_seat_id".into(), Value::from(opponent_id));
        history.insert(
            "screen_name".into(),
            Value::String(
                seat.and_then(|s| self.screen_names.get(&s))
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        history.insert(
            "opponent_screen_name".into(),
            Value::String(
                self.screen_names
                    .get(&opponent_id)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        history.insert(
            "events".into(),
            Value::Array(self.game_history_events.clone()),
        );
        game.insert("history".into(), Value::Object(history));
        log::info!(
            "Adding game history ({} events)",
            self.game_history_events.len()
        );

        // copy.deepcopy(game): Value clone is a deep copy, so later mutation can't leak.
        self.pending_game_submission = game;
        true
    }

    fn maybe_submit_pending_game(&mut self) {
        if self.pending_game_submission.is_empty() || self.pending_game_result.is_empty() {
            return;
        }
        let mut full = self.pending_game_result.clone();
        for (k, v) in &self.pending_match_result {
            full.insert(k.clone(), v.clone());
        }
        for (k, v) in &self.pending_game_submission {
            full.insert(k.clone(), v.clone());
        }
        log::info!("Submitting queued game result");
        let payload = self.add_base_api_data(full);
        self.api.submit_game_result(payload);
        self.pending_game_submission = Map::new();
        self.clear_game_data(true);
    }

    fn clear_game_data(&mut self, submit_pending_game: bool) {
        if submit_pending_game {
            self.maybe_submit_pending_game();
        }
        self.turn_count = 0;
        self.objects_by_owner.clear();
        self.opening_hand_count_by_seat.clear();
        self.opening_hand.clear();
        self.drawn_hands.clear();
        self.drawn_cards_by_instance_id.clear();
        self.starting_team_id = None;
        self.game_history_events.clear();
        self.current_game_maindeck = None;
        self.current_game_sideboard = None;
        self.current_game_additional_deck_info = None;
        self.game_service_metadata = None;
        self.game_client_metadata = None;
        self.pending_game_result = Map::new();
        self.pending_match_result = Map::new();
    }

    fn clear_match_data(&mut self, submit_pending_game: bool) {
        self.screen_names.clear();
        self.current_match_id = None;
        self.current_event_id = None;
        self.seat_id = None;
        self.clear_game_data(submit_pending_game);
    }
}

/// Read one line including its trailing `\n` (mirrors Python `readline` with
/// `errors="replace"` handled by the lossy decode at the call site). Returns bytes read.
fn read_until_newline<R: Read>(
    reader: &mut BufReader<R>,
    buf: &mut Vec<u8>,
) -> std::io::Result<usize> {
    use std::io::BufRead;
    reader.read_until(b'\n', buf)
}

/// Python truthiness for the `if updated_match_id and updated_event_id` guard: non-null,
/// non-empty-string, non-zero.
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
