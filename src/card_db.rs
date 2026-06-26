//! Arena card-ID → name lookup for human-readable console logging.
//!
//! This is a **console-only** convenience: it lets the draft-pick log lines show card
//! names instead of bare numeric Arena IDs (`GrpIds`). It never touches the wire payload,
//! so it sits entirely outside the upstream compatibility contract (CLAUDE.md) — the
//! uploads still carry numeric IDs exactly as the Python client sends them.
//!
//! The map is built offline by `tools/cards/build_card_db.py` from MTGJSON's
//! `AllIdentifiers.json`, which emits a compact `{ "<mtgArenaId>": "<name>" }` file. At
//! runtime we load it lazily and degrade gracefully: if the file is missing or unreadable
//! we keep an empty table and simply render raw IDs.

use std::collections::HashMap;
use std::path::PathBuf;

/// Environment override for the card-db path. When unset, the platform config dir is used.
const ENV_OVERRIDE: &str = "SEVENTEENLANDS_CARD_DB";

/// `<config_dir>/17l/arena_names.json` — alongside the token config (see `config.rs`).
/// Public so the `build_card_db` example writes to exactly where the loader reads.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("17l").join("arena_names.json"))
}

fn resolve_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(ENV_OVERRIDE) {
        return Some(PathBuf::from(p));
    }
    default_path()
}

/// Arena-ID → card-name table. Always usable: a missing/invalid source yields an empty
/// table, and lookups fall back to the raw ID.
#[derive(Default)]
pub struct CardDb {
    names: HashMap<i64, String>,
}

impl CardDb {
    /// Load from the resolved path (`$SEVENTEENLANDS_CARD_DB` or the config dir). On any
    /// failure this logs once and returns an empty table, so callers always get raw IDs.
    pub fn load() -> Self {
        let Some(path) = resolve_path() else {
            return Self::default();
        };
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log::debug!(
                    "No Arena card-name DB at {} ({e}); draft logs will show raw IDs",
                    path.display()
                );
                return Self::default();
            }
        };
        let raw: HashMap<String, String> = match serde_json::from_str(&contents) {
            Ok(m) => m,
            Err(e) => {
                log::warn!(
                    "Arena card-name DB at {} is not valid id→name JSON ({e}); using raw IDs",
                    path.display()
                );
                return Self::default();
            }
        };
        let names: HashMap<i64, String> = raw
            .into_iter()
            .filter_map(|(k, v)| k.parse::<i64>().ok().map(|id| (id, v)))
            .collect();
        log::info!(
            "Loaded {} Arena card names from {}",
            names.len(),
            path.display()
        );
        Self { names }
    }

    /// Look up a single Arena ID's card name, if known.
    pub fn name(&self, id: i64) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    /// Render a list of Arena IDs as `"Name, Other Name, 12345"`, substituting the bare ID
    /// for any card not in the table. Returns `None` when there are no IDs to show, so the
    /// caller can fall back to the plain (suffix-less) log line.
    pub fn label(&self, ids: &[i64]) -> Option<String> {
        if ids.is_empty() {
            return None;
        }
        let parts: Vec<String> = ids
            .iter()
            .map(|id| match self.name(*id) {
                Some(n) => n.to_string(),
                None => id.to_string(),
            })
            .collect();
        Some(parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(pairs: &[(i64, &str)]) -> CardDb {
        CardDb {
            names: pairs.iter().map(|(id, n)| (*id, n.to_string())).collect(),
        }
    }

    #[test]
    fn label_resolves_known_and_falls_back_to_id() {
        let d = db(&[(90234, "Lightning Bolt")]);
        assert_eq!(d.label(&[90234]).as_deref(), Some("Lightning Bolt"));
        assert_eq!(
            d.label(&[90234, 11111]).as_deref(),
            Some("Lightning Bolt, 11111")
        );
    }

    #[test]
    fn label_empty_is_none() {
        assert_eq!(db(&[]).label(&[]), None);
    }

    #[test]
    fn empty_db_renders_raw_ids() {
        // Graceful degradation: no data file → bare IDs, never a panic.
        assert_eq!(CardDb::default().label(&[42, 7]).as_deref(), Some("42, 7"));
    }
}
