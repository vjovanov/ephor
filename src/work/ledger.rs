//! What ephor dispatched, and what the item looked like when it did
//! (§FS-005-dispatch.4, §FS-005-dispatch.5).
//!
//! The ledger answers one question — has this already been handed over? — and
//! holds the fingerprint that answers the next one: has the item moved since.
//! It never holds the state of the work. That belongs to the runtime, is read
//! from the plan, and a cached copy of it here would be ephor reporting on
//! itself instead of on the world.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{EphorError, Result};
use crate::feed::gate::Gate;
use crate::feed::model::Item;
use crate::paths;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default = "version")]
    pub version: u32,
    /// Keyed by feed item id — the same key unread tracking uses, so an item
    /// is the same item to both halves of ephor.
    #[serde(default)]
    pub entries: BTreeMap<String, Entry>,
}

fn version() -> u32 {
    1
}

/// One item's work: the plan it lives in, and every dispatch onto it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub project: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The rhei project directory the plan lives in.
    pub root: PathBuf,
    /// The checkout the work is about — where the runtime is run from, so a
    /// multi-repo workspace resolves to the directory holding its
    /// repositories rather than to whatever a git lookup finds
    /// (§FS-005-dispatch.3). Empty on entries written before this was
    /// recorded; the work root's parent stands in.
    #[serde(default)]
    pub checkout: PathBuf,
    /// The plan this item's work lives in — its id inside the root, which is
    /// also its file stem. Named for the plan, not for the runtime that runs
    /// it; a ledger written before that is migrated on the way in by the
    /// module whose word the old name was
    /// (§AR-007-runtime, §REQ-001-boundary.5).
    pub plan_id: String,
    pub plan: PathBuf,
    pub dispatches: Vec<Dispatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispatch {
    /// The ticket id inside the plan. Empty for a dispatch that laid down a
    /// plan of its own instead of a ticket inside one.
    pub ticket: String,
    pub recipe: String,
    pub at: DateTime<Utc>,
    /// The plan this dispatch made, where it made one of its own rather than
    /// appending to the item's — a workflow lays down its own
    /// (§FS-005-dispatch.19). An addition, so a ledger written before this
    /// field existed reads unchanged (§FS-006-project-interface.11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// The item as it was when this was asked for.
    pub snapshot: Snapshot,
}

impl Dispatch {
    /// Whether this dispatch laid down a plan of its own
    /// (§FS-005-dispatch.19).
    pub fn is_workflow(&self) -> bool {
        self.plan.is_some()
    }
}

/// The fingerprint of an item: enough of it that a change worth reopening work
/// for shows up as a difference (§FS-005-dispatch.5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default)]
    pub passed: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub running: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
    /// How much conversation there was.
    #[serde(default)]
    pub messages: usize,
}

impl Snapshot {
    pub fn of(item: &Item) -> Snapshot {
        let gate = Gate::of(item).unwrap_or_default();
        Snapshot {
            updated_at: item.updated_at,
            state: item.state.clone(),
            passed: gate.passed(),
            failed: gate.failed(),
            running: gate.running(),
            blocked: gate.blocked,
            messages: message_count(item),
        }
    }

    /// What changed between the item this was taken from and the item now, in
    /// the words the reopened ticket will use. Empty means the work still
    /// answers the item it was asked about.
    pub fn changes(&self, now: &Snapshot) -> Vec<String> {
        let mut changes = Vec::new();
        if now.messages > self.messages {
            let new = now.messages - self.messages;
            changes.push(format!(
                "{new} new message{}",
                if new == 1 { "" } else { "s" }
            ));
        }
        if now.state != self.state {
            if let Some(state) = &now.state {
                changes.push(format!(
                    "the state is now {state}{}",
                    match &self.state {
                        Some(was) => format!(" (was {was})"),
                        None => String::new(),
                    }
                ));
            }
        }
        // A gate is red for two independent reasons — jobs that failed, and a
        // forge that refuses — and they move independently. Reporting the two
        // as one number produces "still red — ✗0 where it was ✗2", which is
        // not a sentence about anything.
        let was_red = self.failed > 0 || self.blocked;
        let is_red = now.failed > 0 || now.blocked;
        match (was_red, is_red) {
            (false, true) if now.failed > 0 => {
                changes.push(format!("the gate turned red — ✗{}", now.failed))
            }
            (false, true) => changes.push("the forge now refuses the merge".to_string()),
            (true, false) => changes.push("the gate is green now".to_string()),
            (true, true) => {
                match (self.failed, now.failed) {
                    (0, now_failed) if now_failed > 0 => {
                        changes.push(format!("the gate is failing now — ✗{now_failed}"))
                    }
                    (was, 0) if was > 0 => changes.push(
                        "the jobs pass now, but the forge still refuses the merge".to_string(),
                    ),
                    (was, now_failed) if was != now_failed => changes.push(format!(
                        "the gate is still red — ✗{now_failed} where it was ✗{was}"
                    )),
                    _ => {}
                }
                if self.blocked && !now.blocked {
                    changes.push("the forge no longer refuses the merge".to_string());
                }
            }
            _ => {}
        }
        if now.running != self.running && self.running > 0 && now.running == 0 {
            changes.push("the gate finished running".to_string());
        }
        if changes.is_empty() && now.updated_at > self.updated_at {
            changes.push("there is new activity".to_string());
        }
        changes
    }
}

/// How many messages an item's conversation holds, across every thread.
fn message_count(item: &Item) -> usize {
    item.raw
        .get("threads")
        .and_then(Value::as_array)
        .map(|threads| {
            threads
                .iter()
                .filter_map(|thread| thread.get("messages").and_then(Value::as_array))
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0)
}

impl Entry {
    /// Where the runtime runs. The recorded checkout, or — for an entry from
    /// before it was recorded — the directory the work root sits in, which is
    /// what the default `{workspace}/panta` root makes it.
    pub fn checkout(&self) -> PathBuf {
        if self.checkout.as_os_str().is_empty() {
            return self
                .root
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.root.clone());
        }
        self.checkout.clone()
    }

    pub fn last(&self) -> Option<&Dispatch> {
        self.dispatches.last()
    }

    /// What changed since the last dispatch onto this item.
    pub fn changes_since(&self, item: &Item) -> Vec<String> {
        match self.last() {
            Some(dispatch) => dispatch.snapshot.changes(&Snapshot::of(item)),
            None => Vec::new(),
        }
    }
}

pub fn ledger_path() -> PathBuf {
    paths::state_dir().join("work.json")
}

pub fn load() -> Result<Ledger> {
    let path = ledger_path();
    if !path.exists() {
        return Ok(Ledger {
            version: version(),
            entries: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path)
        .map_err(|err| EphorError::Command(format!("Cannot read {}: {err}", path.display())))?;
    let corrupt = |err: serde_json::Error| {
        EphorError::Command(format!("Corrupt work ledger {}: {err}", path.display()))
    };
    let mut document: serde_json::Value = serde_json::from_str(&text).map_err(corrupt)?;
    crate::work::runtime::migrate_ledger(&mut document);
    serde_json::from_value(document).map_err(corrupt)
}

pub fn store(ledger: &Ledger) -> Result<()> {
    let path = ledger_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            EphorError::Command(format!("Cannot create {}: {err}", parent.display()))
        })?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(ledger).unwrap())
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", tmp.display())))?;
    fs::rename(&tmp, &path)
        .map_err(|err| EphorError::Command(format!("Cannot rename {}: {err}", tmp.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::gate::RepoGate;
    use crate::feed::model::ItemKind;
    use serde_json::json;

    fn item(raw: Value, state: &str, minutes_ago: i64) -> Item {
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "Retry window".to_string(),
            url: None,
            state: Some(state.to_string()),
            needs_response: false,
            updated_at: Utc::now() - chrono::Duration::minutes(minutes_ago),
            raw,
        }
    }

    fn gate(failed: u64, blocked: bool) -> Value {
        json!({ "gate": Gate {
            repos: vec![RepoGate { repo: "widget".to_string(), passed: 10, failed, running: 0 }],
            blocked,
            blockers: Vec::new(),
        }.to_value() })
    }

    fn messages(count: usize) -> Value {
        let messages: Vec<Value> = (0..count)
            .map(|i| json!({ "text": format!("{i}") }))
            .collect();
        json!({ "threads": [{ "messages": messages }] })
    }

    #[test]
    fn an_unchanged_item_has_nothing_to_reopen() {
        let before = item(gate(0, false), "open", 10);
        assert!(Snapshot::of(&before)
            .changes(&Snapshot::of(&before))
            .is_empty());
    }

    #[test]
    fn the_changes_worth_reopening_for_are_named_in_the_words_the_ticket_will_use() {
        let before = Snapshot::of(&item(gate(0, false), "open", 10));

        let red = Snapshot::of(&item(gate(2, false), "open", 0));
        assert_eq!(before.changes(&red), ["the gate turned red — ✗2"]);

        let worse = red.changes(&Snapshot::of(&item(gate(5, false), "open", 0)));
        assert_eq!(worse, ["the gate is still red — ✗5 where it was ✗2"]);

        let blocked = Snapshot::of(&item(gate(0, true), "open", 0));
        assert_eq!(
            before.changes(&blocked),
            ["the forge now refuses the merge"]
        );

        // Jobs and verdict move independently: a gate whose failures were
        // fixed while the forge still refuses is neither "green now" nor
        // "still red — ✗0".
        let failing_and_blocked = Snapshot::of(&item(gate(2, true), "open", 10));
        let fixed_but_blocked = Snapshot::of(&item(gate(0, true), "open", 0));
        assert_eq!(
            failing_and_blocked.changes(&fixed_but_blocked),
            ["the jobs pass now, but the forge still refuses the merge"]
        );
        let failing_unblocked = Snapshot::of(&item(gate(2, false), "open", 0));
        assert_eq!(
            failing_and_blocked.changes(&failing_unblocked),
            ["the forge no longer refuses the merge"]
        );

        let closed = Snapshot::of(&item(gate(0, false), "merged", 0));
        assert_eq!(
            before.changes(&closed),
            ["the state is now merged (was open)"]
        );

        // Conversation, counted across threads.
        let talked_before = Snapshot::of(&item(messages(2), "open", 10));
        let talked_now = Snapshot::of(&item(messages(5), "open", 0));
        assert_eq!(talked_before.changes(&talked_now), ["3 new messages"]);

        // Activity nothing else explains is still activity.
        let later = Snapshot::of(&item(gate(0, false), "open", 0));
        assert_eq!(before.changes(&later), ["there is new activity"]);
    }
}

#[cfg(test)]
mod compat_tests {
    use super::*;

    /// A ledger written before the runtime was carved out still reads: the
    /// field was named for the runtime and is named for the plan now, and the
    /// migration is what keeps an upgrade from losing the record of what was
    /// dispatched (§AR-007-runtime).
    #[test]
    fn a_ledger_from_before_the_carve_still_reads() {
        let mut older = serde_json::json!({
            "entries": {
                "github-prs:acme/widget#42": {
                    "project": "widget",
                    "title": "Retry window",
                    "root": "/w/panta",
                    "checkout": "/w",
                    "rhei": "github-prs-acme-widget-42",
                    "plan": "/w/panta/github-prs-acme-widget-42.rhei.md",
                    "dispatches": []
                }
            }
        });
        crate::work::runtime::migrate_ledger(&mut older);
        let ledger: Ledger = serde_json::from_value(older).expect("the migration reads it");
        let entry = &ledger.entries["github-prs:acme/widget#42"];
        assert_eq!(entry.plan_id, "github-prs-acme-widget-42");
        assert_eq!(entry.project, "widget");
    }
}
