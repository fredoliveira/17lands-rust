# tests/fixtures

Sanitized MTGA `Player.log` snippets + expected payloads, consumed by `tests/parity.rs`
(SPEC §12).

## Raw source logs (gitignored)

Two real logs from this machine live under `local/logs/` (gitignored — they contain
account id / screen name / token, **never commit them**):

- `local/logs/Player.log`       — ~55k lines
- `local/logs/Player-prev.log`  — ~57k lines

Derive sanitized fixtures from these. Sanitize at minimum: the `Updated account.
DisplayName:…, AccountID:…, Token:…` line, screen names in `reservedPlayers`, and the
token field. Use the Python client as the oracle (point it at a local mock server, capture
payloads) and apply the same sanitization to both sides before diffing.

## Coverage map (what the real logs contain)

Confirmed present across the two logs — extract real fixtures for these:

| Path | Handler / branch (SPEC §6) | Notes |
|---|---|---|
| Human draft packs | `Draft.Notify` (#8) | 42 occurrences |
| Human draft picks | `EventPlayerDraftMakePick` (#9) | 84 occurrences |
| **Full games + match results** | `greToClientEvent` (#16), `matchGameRoomStateChangedEvent` (#15) | **~6k GRE msgs, 11 match-state changes → several complete `add_game` submissions.** This exercises the §8 game-state machine — the priority for real-data parity. |
| Client→GRE (deck submit, etc.) | `ClientToGREMessage` (#17) | ~3.4k |
| Account / screen name | `authenticateResponse` (#14) | |
| Mastery progress | `NodeStates`/`RewardTierUpgrade` (#22) | |
| Logout | `FrontDoorConnection.Close` (#23) | reconnect (#24) not present |

## Gaps (need synthetic fixtures built from the Python source)

Not present in either log — author hand-built blobs from `mtga_follower.py`:

- Bot draft: `DraftStatus` (#4), `BotDraft_DraftPick` (#5)
- `LogBusinessEvents` variants: combined human draft (#6), game-end (#7)
- Deck submission `Event_SetDeck` (#10)
- Ongoing events `Event_GetCourses` (#11)
- Claim prize `Event_ClaimPrize` (#12)
- Event course `Draft_CompleteDraft` (#13)
- Rank `Rank_GetCombinedRankInfo` (#19)
- Inventory `DTO_InventoryInfo` (#21)
- Legacy: `Client.Connected` (#1), `GetPlayerCardsV3` (#20) — may be obsolete in current MTGA.
