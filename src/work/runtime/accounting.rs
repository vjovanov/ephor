//! The work lens: what a piece of work cost, read out of the runtime's own
//! accounting records (§FS-013-burn.1).
//!
//! The runtime writes one record per agent invocation under each work root,
//! attributed to the task, the state and the visit that spent it. That is the
//! half the machine lens cannot answer: a transcript knows the directory an
//! agent ran in and can never know which ticket it was working.
//!
//! It is read the way everything else of the runtime's is read — files under
//! the work root, by where they sit and for what they say (§AR-007-runtime.1).
//! Nothing here writes work state, and nothing here is added into the machine
//! lens: a run measured by both appears in both, and summing them would
//! double-count every run the runtime started (§FS-013-burn.1).
//!
//! **What it did not measure is part of the reading.** A record exists whether
//! or not the agent tool behind it reported any usage, and some report none.
//! Dropping those quietly would show a plan costing a fraction of what it cost,
//! so they are counted and said out loud (§FS-013-burn.2).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::burn::Tokens;

/// Where the runtime keeps one record per invocation, relative to the work
/// root. Read and never written here.
const INVOCATIONS: &str = "runtime/accounting/invocations";

/// One agent invocation the runtime paid for.
#[derive(Clone, Debug, PartialEq)]
pub struct Invocation {
    /// The work root it was found under — how a matter is reached, since
    /// ephor's ledger keys its dispatches by root and plan
    /// (§FS-005-dispatch.4).
    pub root: PathBuf,
    /// The plan inside that root: the part of the task id before its first
    /// separator, which is how the runtime qualifies a ticket.
    pub plan: String,
    pub task: String,
    pub state: String,
    /// Which agent tool ran, and what it ran as. The tier rides on the
    /// target where the record names one — the runtime knows it and the
    /// transcripts do not (§FS-013-burn.1).
    pub agent: String,
    pub provider: String,
    pub model: String,
    pub target: Option<String>,
    /// When it ended, where the record says; the start otherwise.
    pub at: Option<DateTime<Utc>>,
    /// Whether the agent behind it reported any usage at all
    /// (§FS-013-burn.2).
    pub measured: bool,
    pub tokens: Tokens,
    /// Dollars only where the runtime priced it (§FS-013-burn.7).
    pub cost_usd: Option<f64>,
}

impl Invocation {
    /// How a reading names the identity that ran this: the target where the
    /// runtime pinned one, else the provider and model it recorded.
    pub fn identity(&self) -> String {
        match &self.target {
            Some(target) => target.clone(),
            None => format!("{}:{}", self.provider, self.model),
        }
    }
}

/// Every invocation recorded under one work root, oldest first.
///
/// A root with no accounting directory is not a failure: the runtime writes
/// one when it has metered something, and a root it has not run yet has
/// nothing to say.
pub fn under(root: &Path) -> Vec<Invocation> {
    let Ok(entries) = fs::read_dir(root.join(INVOCATIONS)) else {
        return Vec::new();
    };
    let mut found: Vec<Invocation> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        .filter_map(|record| invocation(root, &record))
        .collect();
    found.sort_by(|left, right| left.at.cmp(&right.at).then_with(|| left.task.cmp(&right.task)));
    found
}

fn invocation(root: &Path, record: &Value) -> Option<Invocation> {
    let task = string(record, "task_id")?;
    let plan = task
        .split_once('.')
        .map(|(plan, _)| plan.to_string())
        .unwrap_or_else(|| task.clone());
    let tokens = Tokens {
        input: measure(record, &["tokens", "input", "total"]),
        output: measure(record, &["tokens", "output", "total"]),
        cache_read: measure(record, &["tokens", "input", "cached_read"]),
        cache_write: measure(record, &["tokens", "input", "cache_write"]),
    };
    Some(Invocation {
        root: root.to_path_buf(),
        plan,
        task,
        state: string(record, "state").unwrap_or_default(),
        agent: string(record, "agent").unwrap_or_else(|| "unknown".to_string()),
        provider: string(record, "provider").unwrap_or_else(|| "unknown".to_string()),
        model: string(record, "model").unwrap_or_else(|| "unknown".to_string()),
        target: string(record, "target_slug"),
        at: when(record, "ended_at").or_else(|| when(record, "started_at")),
        // The record's own word for it, so ephor is not second-guessing the
        // runtime about whether something was measured (§FS-013-burn.2).
        measured: string(record, "extraction_status").as_deref() == Some("measured"),
        tokens,
        cost_usd: priced(record),
    })
}

/// A counter the runtime reports as `{ "value": n }` beside a source, or as a
/// status where the agent could not report it. A status is not a zero.
fn measure(record: &Value, path: &[&str]) -> u64 {
    let mut at = record;
    for step in path {
        let Some(next) = at.get(step) else {
            return 0;
        };
        at = next;
    }
    at.get("value").and_then(Value::as_u64).unwrap_or(0)
}

/// What the runtime priced this at, where it priced it. Recorded in
/// millionths, and absent — not zero — where there was no price for the model
/// (§FS-013-burn.7).
fn priced(record: &Value) -> Option<f64> {
    let pricing = record.get("pricing")?;
    if pricing.get("status").and_then(Value::as_str) == Some("unpriced") {
        return None;
    }
    let micro = pricing
        .get("cost_micro")
        .and_then(Value::as_f64)
        .or_else(|| record.get("cost_micro").and_then(Value::as_f64))?;
    Some(micro / 1_000_000.0)
}

fn string(record: &Value, field: &str) -> Option<String> {
    record
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn when(record: &Value, field: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(record.get(field)?.as_str()?)
        .ok()
        .map(|when| when.with_timezone(&Utc))
}

/// What a set of invocations did *not* measure, as a sentence (§FS-013-burn.2).
///
/// Named from the records rather than from anything compiled in: the agents
/// that reported are the ones a reader can believe the numbers for, and the
/// count of those that reported nothing is what the numbers are missing. An
/// absent sentence therefore means every record was measured, which is a fact
/// rather than an oversight.
pub fn unmeasured(invocations: &[Invocation]) -> Option<String> {
    let missing = invocations.iter().filter(|one| !one.measured).count();
    if missing == 0 {
        return None;
    }
    let measured: BTreeSet<&str> = invocations
        .iter()
        .filter(|one| one.measured)
        .map(|one| one.agent.as_str())
        .collect();
    let reported = if measured.is_empty() {
        "nothing reported usage".to_string()
    } else {
        format!(
            "measured: {}",
            measured.into_iter().collect::<Vec<_>>().join(", ")
        )
    };
    let quiet: BTreeSet<&str> = invocations
        .iter()
        .filter(|one| !one.measured)
        .map(|one| one.agent.as_str())
        .collect();
    Some(format!(
        "{reported} — {missing} invocation{} by {} reported none, and {} not counted here",
        if missing == 1 { "" } else { "s" },
        quiet.into_iter().collect::<Vec<_>>().join(", "),
        if missing == 1 { "is" } else { "are" },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEASURED: &str = r#"{
      "schema": "accounting.invocation.v1",
      "task_id": "tickets.3", "state": "assessing", "visit": 1,
      "target_slug": "codex-xhigh-openai-gpt-5.6-luna",
      "agent": "codex", "provider": "openai", "model": "gpt-5.6-luna",
      "started_at": "2026-09-03T10:00:00Z", "ended_at": "2026-09-03T10:01:00Z",
      "extraction_status": "measured",
      "tokens": {
        "total": { "value": 78625 },
        "input": { "total": { "value": 77147 },
                   "cached_read": { "value": 62464 },
                   "cache_write": { "value": 0 } },
        "output": { "total": { "value": 1478 },
                    "cached_read": { "status": "unsupported" },
                    "cache_write": { "status": "unsupported" } } },
      "pricing": { "status": "unpriced", "currency": "USD" }
    }"#;

    const QUIET: &str = r#"{
      "task_id": "tickets.4", "state": "implement",
      "agent": "claude", "provider": "anthropic", "model": "opus",
      "started_at": "2026-09-03T10:02:00Z",
      "extraction_status": "no-usage-emitted",
      "tokens": { "total": { "status": "missing" },
                  "input": { "total": { "status": "missing" } },
                  "output": { "total": { "status": "missing" } } },
      "pricing": { "status": "unpriced" }
    }"#;

    fn root(records: &[&str]) -> tempfile::TempDir {
        let home = tempfile::tempdir().expect("a temporary world");
        let dir = home.path().join(INVOCATIONS);
        fs::create_dir_all(&dir).expect("the accounting directory");
        for (index, record) in records.iter().enumerate() {
            fs::write(dir.join(format!("{index}.json")), record).expect("a record");
        }
        home
    }

    /// The counters are read apart, the plan comes off the qualified task id,
    /// and a status where a number was expected is not a zero.
    #[test]
    fn a_record_is_read_into_the_four_counters() {
        let home = root(&[MEASURED]);
        let found = under(home.path());
        assert_eq!(found.len(), 1);
        let one = &found[0];
        assert_eq!(one.plan, "tickets");
        assert_eq!(one.task, "tickets.3");
        assert_eq!(one.state, "assessing");
        assert_eq!(
            one.tokens,
            Tokens {
                input: 77147,
                output: 1478,
                cache_read: 62464,
                cache_write: 0
            }
        );
        assert!(one.measured);
        assert_eq!(one.cost_usd, None, "unpriced must not become a zero");
        assert_eq!(one.identity(), "codex-xhigh-openai-gpt-5.6-luna");
        assert_eq!(
            one.at.map(|at| at.to_rfc3339()),
            Some("2026-09-03T10:01:00+00:00".to_string())
        );
    }

    /// An invocation whose agent reported nothing is kept, counted, and said
    /// out loud — never dropped into a number that merely looks low
    /// (§FS-013-burn.2).
    #[test]
    fn what_reported_no_usage_is_named_rather_than_dropped() {
        let home = root(&[MEASURED, QUIET]);
        let found = under(home.path());
        assert_eq!(found.len(), 2);
        let says = unmeasured(&found).expect("it says what it did not measure");
        assert!(says.contains("measured: codex"), "{says}");
        assert!(says.contains("1 invocation"), "{says}");
        assert!(says.contains("claude"), "{says}");
        // Everything measured: nothing to say, and the absence means that.
        assert_eq!(unmeasured(&found[..1]), None);
    }

    /// A root the runtime has not metered is empty, not an error.
    #[test]
    fn a_root_with_no_records_is_empty() {
        let home = tempfile::tempdir().expect("a temporary world");
        assert!(under(home.path()).is_empty());
    }
}
