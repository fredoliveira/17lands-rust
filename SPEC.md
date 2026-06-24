# 17Lands MTGA Log Client — Rust Port Specification

## 1. Goal

Reimplement the Python `seventeenlands` client (`src/python/seventeenlands/`) as a
**single, simple Rust binary** that is a **drop-in replacement** against the live
17Lands service (`https://api.17lands.com`).

"Drop-in replacement" means:

- The Rust client tails the same MTG Arena `Player.log`, parses the same messages,
  and POSTs the **same payloads** to the **same endpoints** as the Python client.
- The 17Lands server must accept its uploads exactly as it accepts the Python
  client's. Payload **field names, shapes, value types, and null-vs-absent semantics
  must match** (see §11, Compatibility).
- **All ~20 message handlers** are ported (full parity), not a subset.

It is *not* a clean-room redesign. When in doubt, mirror the Python behavior exactly,
even where it looks odd — the oddities (dispatch order, defaultdict semantics, repeated
branches, numeric coercions) are part of the wire contract.

## 2. Scope

### In scope
- The entire log-parsing + submission pipeline under `src/python/seventeenlands/`:
  - `mtga_follower.py` — the `Follower` state machine, log discovery, tailing, dispatch, all handlers.
  - `api_client.py` — REST client, endpoints, gzip, base-data envelope.
  - `retry_utils.py` — exponential-backoff retry.
  - Token handling via **CLI flag + TOML config (`~/.config/17l/config.toml`) + stdin
    prompt**, with one-time migration from the legacy `~/.mtga_follower.ini` (see §5.1).
- A `--log-file`, `--host`, `--token`, `--once` CLI matching the Python flags.

### Out of scope (explicitly dropped)
- The C# / ClickOnce GUI under `src/cs/` — ignore entirely.
- **GUI token prompts** (tkinter / `osascript` dialogs) → CLI flag / TOML config / stdin only.
- **Server-side error reporting** (`log_errors` endpoint, `submit_error_info`,
  `recent_lines`, stacktraces). Log parse errors locally and continue.
- **Startup version check** (`client_version_validation` GET, the blocking update loop,
  `show_update_message`). The client just runs.
  - ⚠️ We still send a `client_version` field in every payload (see §11).
- **Rotating file logs** (`logging_utils.py`'s `TimedRotatingFileHandler` +
  `~/.seventeenlands/seventeenlands.log`) → log to **stderr/stdout only**.
- `submit_event_submission` / `add_event` — defined in Python but never called; skip.

## 3. Source → module mapping

| Python | Rust module | Notes |
|---|---|---|
| `mtga_follower.py` (constants, paths) | `paths.rs` | Log-file discovery across OS/Steam/Lutris/Wine. |
| `mtga_follower.py` (`extract_time`, time fmts) | `time_parse.rs` | Multi-format timestamp parsing + serialization. |
| `mtga_follower.py` (`Follower`) | `follower.rs` | The stateful parser/dispatcher + all handlers. |
| `mtga_follower.py` (token/config/main) | `config.rs` + `main.rs` | TOML/CLI token, arg parsing, processing loop. |
| `api_client.py` | `api_client.rs` | Endpoints, gzip, base-data envelope. |
| `retry_utils.py` | `retry.rs` | Exponential backoff. |
| `logging_utils.py` | (use `env_logger`/`log` in `main.rs`) | Stdout only; no rotation. |

**Location:** a **separate, new repository** containing only the Rust client (this spec
file should be copied in as the build reference). Keep `src/python/` available as the
read-only reference/oracle during development.

Suggested layout:

```
<new-repo>/
  Cargo.toml
  src/
    main.rs                 # CLI, processing loop
    config.rs               # token: --token > ~/.config/17l/config.toml > legacy .ini (migrate) > stdin
    paths.rs                # candidate Player.log paths
    time_parse.rs           # extract_time + isoformat-compatible output
    follower.rs             # Follower struct: state + parse_log + dispatch + handlers
    api_client.rs           # ApiClient: endpoints + gzip + envelope
    retry.rs                # retry_until_successful / retry_api_call
  tests/
    fixtures/               # sample Player.log snippets (user-provided)
    parity.rs               # fixture → expected-payload assertions
```

## 4. Dependencies (proposed)

| Concern | Crate | Rationale |
|---|---|---|
| HTTP | `ureq` | Blocking + lightweight; fits the synchronous tail loop. No async runtime needed. |
| JSON | `serde_json` with **`preserve_order`** | Mirror Python dict insertion order; eases payload diffing. Use `Value`/`Map` for dynamic blobs. |
| gzip | `flate2` | For `add_game` + (dropped) error endpoints. |
| Time | `chrono` | strptime-style parsing + ISO output. |
| Regex | `regex` | Direct port of the line/timestamp regexes. |
| CLI | `clap` | `--log-file/--host/--token/--once`. |
| config | `toml` + `serde` | Read/write `~/.config/17l/config.toml` (new primary location). |
| legacy ini | minimal manual parse | One-time read of legacy `~/.mtga_follower.ini` `[client] token` for migration (no full configparser dep needed). |
| UUID | `uuid` | Validate token is UUID v4. |
| Logging | `log` + `env_logger` | Stdout, level-filtered. |
| Home dir | `dirs` | `~` expansion / `expanduser`. |

> Decision note: `ureq` over `reqwest` keeps the binary small and avoids tokio. The
> Python client is fully synchronous and single-threaded; we preserve that.

## 5. Behavioral model (the pipeline)

### 5.1 Token resolution (`config.rs`)
**Deviation from Python:** the token now lives at **`~/.config/17l/config.toml`**
(respecting `$XDG_CONFIG_HOME`; use `dirs::config_dir()`), format:

```toml
token = "xxxxxxxx-xxxx-4xxx-xxxx-xxxxxxxxxxxx"
```

Order (first valid UUID-v4 wins), dropping all GUI paths:
1. `--token` flag.
2. `~/.config/17l/config.toml`, `token` key.
3. **Legacy migration:** `~/.mtga_follower.ini`, `[client] token` — if found and valid,
   use it **and write it to the new TOML location** (one-time migration so existing
   Python users keep working). Minimal manual parse; do not keep the legacy file in sync
   afterward.
4. Interactive stdin prompt (`get_client_token_cli`), re-prompting on invalid UUID.
   On success, **write the token to `~/.config/17l/config.toml`** (creating the dir).

`validate_uuid_v4`: parse as UUID v4; return `None`/`Err` if invalid. (Python uses
`uuid.UUID(s, version=4)` which is lenient about the variant — match its acceptance set;
see §10.)

### 5.2 Log file discovery (`paths.rs`)
Port `POSSIBLE_ROOTS` × `{Player.log, Player-prev.log}` verbatim
(`mtga_follower.py:54-128`): OSX `~/Library/Logs`, Steam Proton compatdata `2141910`,
Lutris, Wine (`$WINEPREFIX`), Windows `C:/`+`D:/` `users/<user>/AppData/LocalLow`.
Build `POSSIBLE_CURRENT_FILEPATHS` and `POSSIBLE_PREVIOUS_FILEPATHS`.

### 5.3 Processing loop (`main.rs`, port of `processing_loop`)
1. Resolve token.
2. Build `Follower`.
3. If "normal mode" (`--log-file` not set, host == default, following): parse the
   **first existing previous-log** once with `follow=false` to catch up.
4. Parse each existing current-log path with `follow = !--once`.
5. If no files found, warn.

### 5.4 Tailing (`follower.parse_log`, port of `:309`)
Outer loop re-opens the file forever (when following). Inner loop reads lines:
- On a line: append to buffer pipeline (§5.5).
- On EOF: flush the current entry (`__handle_complete_log_entry`), then check:
  - file size **shrank** vs last seen → log + break (restart from top — rotation).
  - file mtime is **> last_read_time + 60s** (`FILE_UPDATED_FORCE_REFRESH_SECONDS`)
    → log + break (restart from top — big external update).
  - else if following → `sleep(0.5s)` and continue.
  - else → break.
- `FileNotFoundError` → sleep 0.5s and retry the outer loop.
- Any other error → log locally and continue (Python also POSTs to `log_errors`; we
  **drop that POST**, log only).
- After the loop, if not following → "Done processing file." and return.

Preserve `_reinitialize()` resetting **all** state at the start of each outer iteration.

### 5.5 Line accumulation & entry boundaries (`__append_line`, `:389`)
Per line, in order:
1. (Dropped) `__check_detailed_logs` "DETAILED LOGS: DISABLED/ENABLED" — the disabled
   branch shows a GUI message. **Keep the log warning, drop the dialog.**
2. `__maybe_handle_account_info(line)` (§5.7).
3. `TIMESTAMP_REGEX` match → update `last_raw_time` + `cur_log_time` (via `extract_time`).
4. `LOG_START_REGEX_UNTIMED` match (`[UnityCrossThreadLogger]` or `[Client GRE]`):
   - flush the in-progress entry first;
   - if `LOG_START_REGEX_TIMED` also matches, capture its timestamp and push the
     remainder of the line after the match into the buffer;
   - else push the remainder after the untimed match.
   - else (no marker) push the whole line into the buffer.

An "entry" is the accumulated `buffer` joined with `""`. `__handle_complete_log_entry`
(`:418`):
- skip if buffer empty or `cur_log_time` is None;
- join → `full_log`; set `current_debug_blob`;
- **de-dup**: if `full_log == last_blob`, skip (log "Skipping repeated…"); else dispatch
  via `__handle_blob`, then set `last_blob`;
- clear buffer. (Note: the `cur_log_time = None` reset is commented out in Python — keep
  it commented/omitted to match.)

### 5.6 Blob parse + payload extraction (`__handle_blob`, `:477`)
1. `JSON_START_REGEX` (`[\[\{]`) — find first `[` or `{`. No match → return.
2. **`raw_decode` from that offset**: decode exactly one JSON value, ignoring trailing
   text. In Rust: `serde_json::Deserializer::from_str(&full_log[start..])` →
   `.into_iter::<Value>().next()`; on `Err`, log at debug and return (Python catches
   `JSONDecodeError`). The consumed length isn't needed downstream (Python ignores `end`).
3. `__extract_payload` (§5.6.1) → if not an object, return.
4. Compute `utc_time` (§5.8) and `event_time` (`blob["EventTime"]`), updating
   `last_utc_time` / `last_event_time` when present (each wrapped in try/ignore).
5. **Dispatch table** (§6) — first match wins; order is significant.

#### 5.6.1 Recursive payload extraction (`__extract_payload` / `__try_decode`, :619-637)
- Non-dict → return as-is.
- If `clientToMatchServiceMessageType` key present → return blob unchanged.
- Else for key in `("payload","Payload","request")`: if present, the value may itself be
  a JSON-encoded string → try to `raw_decode` it (`__try_decode`: on failure return the
  raw value), then **recurse** on the result.
- Else return blob.

### 5.7 Account info (`__maybe_handle_account_info`, :986)
Run on **every raw line** (not just JSON):
- `ACCOUNT_INFO_REGEX` `…Updated account. DisplayName:(.*), AccountID:(.*), Token:…`
  → `cur_user = group(2)`, `__update_screen_name(group(1))`, return.
- `MATCH_ACCOUNT_INFO_REGEX` `…: ((\w+) to Match|Match to (\w+)):` →
  `cur_user = group(2) or group(3)`.
- `LOGIN_REGEX` `…Logged in successfully. Display Name:(.*)` → `full_screen_name = group(1)`.

### 5.8 Timestamps (`time_parse.rs` + `__maybe_get_utc_timestamp`, :445)
- `extract_time` (`:165`): strip trailing `: / ` junk via `STRIPPED_TIMESTAMP_REGEX`,
  cut at first `": "`, then try each format in `TIME_FORMATS` (port the full list,
  `:146-158`); raise/Err if none match.
- `__maybe_get_utc_timestamp`: pull `timestamp` from `blob`, else `blob.payloadObject`,
  else `blob.params.payloadObject`. Then:
  - integer & `< MAX_MILLISECONDS_SINCE_EPOCH` (ms since epoch for year < 3000) →
    `fromtimestamp(ms/1000)`.
  - integer & larger → **.NET ticks**: `seconds = value / 10_000_000`, base =
    `0001-01-01` + `seconds` (Python `datetime.fromordinal(1) + timedelta`).
  - non-integer → ISO-8601 parse (Python `dateutil.parser.isoparse`).
- Output serialization must match Python `datetime.isoformat()` (see §11.3).

## 6. Dispatch table (port verbatim, first match wins)

`contains(key)` = `contains_log_key`: substring search for `key` **and** for
`key` with underscores removed (`:228`). `has(obj, k)` = key present in the decoded
object. Order below is the exact `if/elif` order of `__handle_blob` (`:509-617`).

| # | Condition | Handler | Endpoint |
|---|---|---|---|
| 1 | `params.messageName == "Client.Connected"` | `handle_login` (legacy) | `add_mtga_account` (via screen name) |
| 2 | `contains("Event_Join")` & `has("EventName")` | `handle_joined_pod` | — (local only) |
| 3 | `contains("Event_Join")` & `has("Course")` | `handle_joined_event_response` | `record_event_join` |
| 4 | `has("DraftStatus")` | `handle_bot_draft_pack` | `add_pack` (when `PickNext`) |
| 5 | `contains("BotDraft_DraftPick")` & `has("PickInfo")` | `handle_bot_draft_pick(obj.PickInfo)` | `add_pick` |
| 6 | `contains("LogBusinessEvents")` & `has("PickGrpId")` | `handle_human_draft_combined` | `add_human_draft_pack` + `add_human_draft_pick` |
| 7 | `contains("LogBusinessEvents")` & `has("WinningType")` | `handle_log_business_game_end` | (queues game result) |
| 8 | `"Draft.Notify " in full_log` & `!has("method")` | `handle_human_draft_pack` | `add_human_draft_pack` |
| 9 | `contains("EventPlayerDraftMakePick")` & `has("GrpIds")` | `handle_player_draft_pick` | `add_human_draft_pick` |
| 10 | `contains("Event_SetDeck")` & `has("EventName")` | `handle_deck_submission` | `add_deck` |
| 11 | `contains("Event_GetCourses")` & `has("Courses")` | `handle_ongoing_events` | `update_ongoing_events` |
| 12 | `contains("Event_ClaimPrize")` & `has("EventName")` | `handle_claim_prize` | `mark_event_ended` |
| 13 | `contains("Draft_CompleteDraft")` & `has("DraftId")` | `handle_event_course` | `update_event_course` |
| 14 | `has("authenticateResponse")` | `update_screen_name(obj.authenticateResponse.screenName)` | `add_mtga_account` |
| 15 | `has("matchGameRoomStateChangedEvent")` | `handle_match_state_changed` | `add_game` (on final result) |
| 16 | `has("greToClientEvent.greToClientMessages")` | loop `handle_gre_to_client_message` | `add_game` (on game over) |
| 17 | `clientToMatchServiceMessageType == ClientToGREMessage` | `handle_client_to_gre_message(obj.payload)` | — |
| 18 | `clientToMatchServiceMessageType == ClientToGREUIMessage` | `handle_client_to_gre_ui_message(obj.payload)` | — |
| 19 | `contains("Rank_GetCombinedRankInfo")` & `has("limitedSeasonOrdinal")` | `handle_self_rank_info` | `add_rank` |
| 20 | `" PlayerInventory.GetPlayerCardsV3 " in full_log` & `!has("method")` | `handle_collection` (legacy) | `update_card_collection` |
| 21 | `has("DTO_InventoryInfo")` | `handle_inventory(obj.DTO_InventoryInfo)` | `update_inventory` |
| 22 | `has("NodeStates")` & `has(NodeStates,"RewardTierUpgrade")` | `handle_player_progress` | `update_player_progress` |
| 23 | `"FrontDoorConnection.Close " in full_log` | `reset_current_user` | — |
| 24 | `"Reconnect result : Connected" in full_log` | `handle_reconnect_result` | — |

> Note: branch 24's condition is duplicated in the Python source (`:614` and `:616`); the
> second is dead. Port a single branch.

## 7. Follower state (port of `_reinitialize`, :251)

Mirror every field. Key groups:

- **Timing**: `cur_log_time`, `last_utc_time` (both init `fromtimestamp(0)`),
  `last_event_time: Option`, `last_raw_time: String`.
- **Identity**: `cur_user`, `user_screen_name`, `full_screen_name`,
  `screen_names: HashMap<seat, String>` (**defaults to `""`** — replicate defaultdict),
  plus the `disconnected_*` snapshot set for reconnect.
- **Draft/event**: `cur_draft_event`, `current_match_id`, `current_event_id`,
  `cur_rank_data`, `cur_opponent_level`, `cur_opponent_match_id`.
- **Game/board** (the hard part): `seat_id`, `starting_team_id`, `turn_count`,
  `objects_by_owner: HashMap<owner, HashMap<instanceId, cardId>>`,
  `opening_hand_count_by_seat` (defaultdict int),
  `opening_hand` / `drawn_hands` / `cards_in_hand` (defaultdict list),
  `drawn_cards_by_instance_id` (defaultdict dict),
  `current_game_maindeck` / `current_game_sideboard` / `current_game_additional_deck_info`,
  `game_service_metadata`, `game_client_metadata`, `game_history_events: Vec`.
- **Pending submission**: `pending_game_submission`, `pending_game_result`,
  `pending_match_result` (dicts).
- **Buffering**: `buffer: Vec<String>`, `last_blob`, `current_debug_blob`.
  (**`recent_lines` dropped** — it only fed the removed `log_errors` endpoint.)

Replicate `__clear_game_data` / `__clear_match_data` semantics exactly, **including the
order**: `__clear_game_data(submit_pending_game)` first calls
`__maybe_submit_pending_game()` when requested, then resets game fields.
`__clear_match_data` clears screen names / ids / seat then calls `__clear_game_data`.

> **Decision (state modeling):** decide per-field during the build. **Default to
> `serde_json::Value`** for fidelity and minimal divergence; promote a field to a typed
> struct only when it demonstrably helps (e.g. a stable, frequently-accessed shape) and
> doesn't risk drifting from the Python payload. No blanket typing up front.

## 8. The game-state reconstruction (highest-risk area)

Port these with care; they assemble the `add_game` payload:

- **`handle_gre_to_client_message` (`:725`)** — appends game-state / UI-chat messages to
  `game_history_events` (with `_timestamp` = `timestamp.isoformat()` or null), tracks
  `seat_id`, switches `current_match_id` on new `matchID` (clearing `current_event_id`),
  updates `turn_count` (max of `turnInfo.turnNumber` or sum of players' turn numbers),
  populates `objects_by_owner` from `gameObjects` (Card/SplitCard only, keyed by
  `instanceId` → `overlayGrpId`), rebuilds `cards_in_hand` + `drawn_cards_by_instance_id`
  from `ZoneType_Hand`, records mulligan/opening-hand bookkeeping, captures the opening
  hand at `(Phase_Beginning, Step_Upkeep, turn 1)`, and calls
  `__maybe_handle_game_over_stage`.
- **`handle_gre_connect_response` (`:838`)** + **`ClientMessageType_SubmitDeckResp`**
  (in `handle_client_to_gre_message`, `:852`) — set `current_game_maindeck` /
  `sideboard` / `additional_deck_info`. **Note the Python `.pop()` mutates** the decoded
  object before storing the remainder as `additional_deck_info`; replicate (remove
  `deckCards`/`sideboardCards`, keep the rest).
- **`handle_gre_edictal_message`** — unwrap `edictalMessage.edictMessage` then reuse
  `handle_client_to_gre_message`.
- **`handle_log_business_game_end` (`:911`)** — alternate game-end path; sets
  `starting_team_id` if unset, builds `pending_game_result` (won = `seat_id ==
  WinningTeamId`).
- **`maybe_handle_game_over_stage` / `enqueue_game_results` / `enqueue_game_data`** —
  `__has_pending_game_data` gate: `len(drawn_cards_by_instance_id) > 0 and
  len(game_history_events) > 5`. `enqueue_game_data` builds the full game dict
  (opponent card ids, on_play, opening hand, mulligans `drawn_hands[:-1]`, drawn cards,
  mulligan counts, turns, `duration: -1`, rank/opponent rank, deck ids, metadata, and a
  deep-copied `history` block). Replicate the **deep copy** (`copy.deepcopy`) so later
  mutation can't leak into the queued submission.
- **`handle_match_state_changed` (`:660`)** — sets screen names from `reservedPlayers`,
  backfills self screen name, parses opponent rank via `get_rank_string`, captures
  service/client metadata, and on `finalMatchResult` enqueues game + results then clears
  match data (submitting pending game).
- **`maybe_submit_pending_game` (`:947`)** — merges `pending_game_result` +
  `pending_match_result` + `pending_game_submission` and POSTs `add_game` (**gzip**),
  then clears.

`get_rank_string` (`:207`): `"-".join(str(x) for x in [class, level, percentile, place, step])`
— must reproduce Python `str()` formatting of `None` → `"None"`, floats, etc.

## 9. API client (`api_client.rs`, port of `api_client.py`)

- `DEFAULT_HOST = "https://api.17lands.com"`.
- **Envelope** (`_add_base_api_data`, `:297`): every payload is
  `{ token, client_version, player_id: cur_user, time: cur_log_time.isoformat(),
  utc_time: last_utc_time.isoformat(), event_time: last_event_time, raw_time:
  last_raw_time, ...blob }`. Field order and inclusion (even when null) must match §11.
- **POST**: `application/json`. Endpoints (kept subset):

  | Method | Endpoint | gzip |
  |---|---|---|
  | `submit_collection` | `api/client/update_card_collection` | no |
  | `submit_deck_submission` | `api/client/add_deck` | no |
  | `submit_draft_pack` | `api/client/add_pack` | no |
  | `submit_draft_pick` | `api/client/add_pick` | no |
  | `submit_event_course_submission` | `api/client/update_event_course` | no |
  | `submit_joined_event` | `api/client/record_event_join` | no |
  | `submit_event_ended` | `api/client/mark_event_ended` | no |
  | `submit_game_result` | `api/client/add_game` | **yes** |
  | `submit_human_draft_pack` | `api/client/add_human_draft_pack` | no |
  | `submit_human_draft_pick` | `api/client/add_human_draft_pick` | no |
  | `submit_inventory` | `api/client/update_inventory` | no |
  | `submit_ongoing_events` | `api/client/update_ongoing_events` | no |
  | `submit_player_progress` | `api/client/update_player_progress` | no |
  | `submit_rank` | `api/client/add_rank` | no |
  | `submit_user` | `api/client/add_mtga_account` | no |

  (`client_version_validation`, `log_errors`, `add_event` are dropped per §2.)
- **gzip**: body = `gzip(json_bytes)` with headers `content-type: application/json`,
  `content-encoding: gzip` (match Python exactly).

## 10. Retry (`retry.rs`, port of `retry_utils.py`)
- `retry_api_call`: wraps `retry_until_successful` with initial 1s, max 10min, max total
  24h.
- **Response valid** when `status < 500 || status >= 600` (i.e. retry only 5xx).
- **Retry on error** only for connection errors (`reqwest`/`ureq` transport errors);
  re-raise others. Exponential doubling, capped at max delay; on exceeding total duration,
  return `RetryLimitExceededError` / `Err`.
- ⚠️ `ureq` maps non-2xx into `Err(Status)` by default — normalize so HTTP responses reach
  the response-validator (don't treat 4xx/5xx as transport errors). Use the response API
  that yields the status code regardless.

## 11. Compatibility requirements (the wire contract)

These are the make-or-break details for "drop-in":

### 11.1 `client_version`
Python sends `CLIENT_VERSION = "0.1.44.p"` (the `.p` = python; the version-check endpoint
strips the last 2 chars). We **drop the check** but still send the field.

**Decision:** define `CLIENT_VERSION` as a single constant, **default to `"0.1.44.p"`**
(impersonate the trusted Python client so uploads are guaranteed accepted today), and
**revisit after live testing** — if 17Lands accepts a distinct identifier, switch to a
Rust suffix (`"0.1.44.r"`). Keep it a one-line constant so the switch is trivial.

### 11.2 Null vs absent
Python builds dicts that include explicit `None` (→ JSON `null`) for many fields
(e.g. draft `card_id`, rank fields). Use a `Value`-based builder so `null` is **emitted**,
not omitted. Do **not** use `#[serde(skip_serializing_if)]` on these.

### 11.3 Time serialization
Match Python `datetime.isoformat()` byte-for-byte:
- No microseconds when zero (`1970-01-01T00:00:00`), `.ffffff` when present.
- **Naive (no timezone offset)** — `fromtimestamp` returns local-naive; reproduce local
  time without a `+00:00` suffix.

**Decision:** exact time-serialization format is **resolved empirically** by diffing
Rust output against the Python oracle (§12) rather than specified up front. Treat any
remaining diff (TZ suffix, microsecond presence, local vs UTC) as a bug to fix against the
captured Python payloads.

### 11.4 Numeric coercion
Reproduce Python `int(...)` casts (draft pack/pick numbers, card ids) and the
string-split parsing (`PackCards.split(",")`). Keep ids as integers where Python does.

### 11.5 JSON object key order
Enable `serde_json/preserve_order` so emitted objects follow insertion order like Python
dicts. (Semantically unordered, but keeps payloads diff-identical to the oracle.)

### 11.6 UUID acceptance
Python `uuid.UUID(s, version=4)` overwrites the variant/version bits and does **not**
strictly reject all non-v4 UUIDs. Match its *acceptance set* so a token the Python client
accepts isn't rejected here (test with the user's real token format).

## 12. Testing strategy

Primary: **fixture-based parity tests** using **sample `Player.log` data the user will
provide** (real, sanitized as needed).

- Place snippets under `tests/fixtures/`. Each fixture = raw log lines + the expected
  sequence of `(endpoint, payload)` submissions.
- Test harness: feed lines through `Follower` with a **mock `ApiClient`** that records
  `(endpoint, json)` instead of sending; assert against expected payloads.
- Cover at minimum: bot draft pack+pick, human draft (all 3 variants: combined/Notify/
  PlayerDraftMakePick), deck submission, a full game (opening hand, mulligans, game-over,
  match result → `add_game`), rank, account/screen-name, inventory, collection, ongoing
  events, claim prize, event course, reconnect.
- **Oracle harness (in scope, required):** run the Python client against the same
  fixtures pointed at a local mock server, capture its payloads, and assert the Rust
  output is byte-identical. This is the mechanism that **resolves the deferred decisions**
  — time serialization (§11.3) and `client_version` are settled by diffing against the
  oracle, not by spec. Build this early enough to validate the game-state path against it.
- Unit tests for `extract_time` (every format), `__maybe_get_utc_timestamp` (ms / .NET
  ticks / ISO branches), `extract_payload` recursion, and `contains_log_key`.

## 13. Suggested implementation order

1. **Skeleton + plumbing**: Cargo project, CLI args, config/token (TOML + legacy-ini
   migration + stdin), logging, `paths.rs`. Runs and resolves a token.
2. **HTTP + retry**: `api_client.rs` (envelope, endpoints, gzip) + `retry.rs`, against a
   mock server.
3. **Tailing + accumulation**: `parse_log`, `__append_line`, entry boundaries, dedup,
   rotation/truncation handling. Verify line→blob segmentation on a real log.
4. **Blob parse + dispatch**: `raw_decode`, `extract_payload`, the §6 table, `time_parse`.
5. **Simple handlers**: drafts, decks, rank, account, inventory, collection, events,
   reconnect — each with a fixture test.
6. **Game-state machine**: §8. The largest unit; gate behind the oracle/parity tests.
7. **End-to-end** against a captured real log + (optionally) the live API with the user's
   token, on a throwaway/replayed log.

## 14. Open questions (to resolve before/while coding)

1. ~~`client_version` suffix~~ **RESOLVED (revisit after testing)**: ship a `CLIENT_VERSION`
   constant defaulting to `"0.1.44.p"`; switch to `"0.1.44.r"` only if live testing shows
   17Lands accepts it.
2. ~~Project location~~ **RESOLVED**: a separate, new repository (Rust client only); this
   spec copied in as the build reference.
3. ~~Env-var token~~ **RESOLVED**: token stored at `~/.config/17l/config.toml` (TOML),
   with one-time migration from legacy `~/.mtga_follower.ini`. No env var.
4. ~~`recent_lines` buffer~~ **RESOLVED**: dropped (only fed the removed `log_errors`).
5. ~~`--once` + non-default host gating~~ **RESOLVED (confirmed)**: replicate Python's exact
   "normal mode" condition (no `--log-file` && host == default && following) for parsing the
   previous log once at startup.
6. ~~Detailed-logs-disabled notice~~ **RESOLVED (confirmed)**: log a warning only; no dialog,
   no louder treatment.
```
