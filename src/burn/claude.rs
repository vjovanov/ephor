//! Reading one agent tool's transcripts: `~/.claude/projects/**/*.jsonl`
//! (§FS-013-burn.3).
//!
//! One JSON record per line. An `assistant` record carries the call that was
//! just paid for: its four counters under `message.usage`, the model under
//! `message.model`, and — the thing that makes attribution possible at all —
//! the directory it ran in, on every record.
//!
//! **The outer counters are the whole of the call.** The same record also
//! carries `usage.iterations`, a per-iteration breakdown of exactly those
//! tokens. Reading both counts every token twice, and the doubled number is
//! plausible enough that nothing downstream would catch it — so this reader
//! never looks inside `iterations`, and the test below is a record whose
//! breakdown would double it.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::cursors::Carry;
use super::{Sample, Tokens};

/// What this tool is called on a bucket key.
pub const SOURCE: &str = "claude";

/// Who serves it. One vendor, so it is not read off the record.
const PROVIDER: &str = "anthropic";

/// Every sample in `text`, with `carry` moved on to what the last record left.
///
/// `file` is only read for its name: this tool files a sub-agent's transcript
/// beside the session's, and a sub-agent's spend is real spend that has to be
/// tellable apart (§FS-013-burn.3).
pub fn read(file: &Path, text: &str, carry: &mut Carry) -> Vec<Sample> {
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        // Every record that names one moves the session's own facts on, not
        // only the ones that spend: a `cost-state` carries no directory and no
        // time of its own and is attributed to what came before it.
        if let Some(cwd) = string(&record, "cwd") {
            carry.cwd = Some(cwd);
        }
        if let Some(session) = string(&record, "sessionId") {
            carry.session = Some(session);
        }
        if let Some(when) = string(&record, "timestamp") {
            carry.at = Some(when);
        }
        if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            carry.subagent = true;
        }
        match record.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                if let Some(sample) = call(&record, carry, subagent(file, carry)) {
                    found.push(sample);
                }
            }
            // The tool's own dollar rollup, which is the only place dollars
            // come from in this version (§FS-013-burn.7). It restates the
            // session's running cost, so what it adds is the difference.
            Some("cost-state") => found.extend(priced(&record, carry, subagent(file, carry))),
            _ => {}
        }
    }
    found
}

/// Whether this transcript is a sub-agent's. Either the records said so, or
/// the tool filed it under the directory it keeps them in.
fn subagent(file: &Path, carry: &Carry) -> bool {
    carry.subagent
        || file
            .components()
            .any(|part| part.as_os_str() == "subagents" || part.as_os_str() == "sidechains")
}

/// One paid call: the outer counters, and nothing from `iterations`.
fn call(record: &Value, carry: &Carry, subagent: bool) -> Option<Sample> {
    let message = record.get("message")?;
    let usage = message.get("usage")?;
    let tokens = Tokens {
        input: counter(usage, "input_tokens"),
        output: counter(usage, "output_tokens"),
        cache_read: counter(usage, "cache_read_input_tokens"),
        cache_write: counter(usage, "cache_creation_input_tokens"),
    };
    if tokens.is_empty() {
        return None;
    }
    let at = when(record.get("timestamp").and_then(Value::as_str))?;
    Some(Sample {
        at,
        cwd: carry.cwd.clone(),
        source: SOURCE,
        provider: PROVIDER.to_string(),
        model: string(message, "model").unwrap_or_else(|| "unknown".to_string()),
        session: carry.session.clone().unwrap_or_default(),
        subagent,
        tokens,
        // A call carries no price of its own; the rollup below is where a
        // dollar figure comes from.
        cost_usd: None,
    })
}

/// What the session's cost rollup has added since it was last seen.
///
/// The rollup restates the whole session's dollars per model, so the tokens
/// are already counted by the calls above and only the money is taken. It
/// carries no time of its own, so it lands where the last record it followed
/// did — it was written after everything before it.
fn priced(record: &Value, carry: &mut Carry, subagent: bool) -> Vec<Sample> {
    let Some(usage) = record.get("modelUsage").and_then(Value::as_object) else {
        return Vec::new();
    };
    let Some(at) = when(carry.at.as_deref()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (model, spent) in usage {
        let Some(total) = spent.get("costUSD").and_then(Value::as_f64) else {
            continue;
        };
        let seen = carry.costs.get(model).copied().unwrap_or(0.0);
        // A rollup that went backwards is a session restarted under the same
        // name: take it as it stands rather than as a negative.
        let added = if total < seen { total } else { total - seen };
        carry.costs.insert(model.clone(), total);
        if added == 0.0 {
            continue;
        }
        found.push(Sample {
            at,
            cwd: carry.cwd.clone(),
            source: SOURCE,
            provider: PROVIDER.to_string(),
            model: model.clone(),
            session: carry.session.clone().unwrap_or_default(),
            subagent,
            tokens: Tokens::default(),
            cost_usd: Some(added),
        });
    }
    found
}

fn counter(usage: &Value, field: &str) -> u64 {
    usage.get(field).and_then(Value::as_u64).unwrap_or(0)
}

fn string(record: &Value, field: &str) -> Option<String> {
    record
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn when(text: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text?)
        .ok()
        .map(|when| when.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record whose `iterations` restate the very counters above them. A
    /// reader that summed both would report 2 004 tokens where 1 002 were
    /// spent, and everything downstream would agree with it — so the fixture
    /// is built to fail on the naive reading rather than to pass on the right
    /// one (§FS-013-burn.3).
    const DOUBLE: &str = r#"
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:00Z","message":{"model":"claude-opus-5","usage":{"input_tokens":2,"output_tokens":1000,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"iterations":[{"input_tokens":2,"output_tokens":600},{"input_tokens":0,"output_tokens":400}]}}}
"#;

    #[test]
    fn the_iteration_breakdown_is_never_added_to_the_call() {
        let mut carry = Carry::default();
        let found = read(Path::new("/x/s1.jsonl"), DOUBLE, &mut carry);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tokens.output, 1000, "the breakdown was counted too");
        assert_eq!(found[0].tokens.total(), 1002);
        assert_eq!(found[0].model, "claude-opus-5");
        assert_eq!(found[0].cwd.as_deref(), Some("/w/app"));
        assert_eq!(found[0].session, "s1");
        assert!(!found[0].subagent);
    }

    /// The four counters are read apart, and a user record spends nothing.
    #[test]
    fn the_four_counters_stay_apart_and_only_calls_spend() {
        let text = r#"
{"type":"user","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:00:00Z","message":{"role":"user"}}
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:00Z","message":{"model":"m","usage":{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":4}}}
"#;
        let mut carry = Carry::default();
        let found = read(Path::new("/x/s1.jsonl"), text, &mut carry);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].tokens,
            Tokens {
                input: 1,
                output: 2,
                cache_read: 3,
                cache_write: 4
            }
        );
    }

    /// A sub-agent's spend is counted and tagged, so a reading can tell it
    /// from the session that spawned it (§FS-013-burn.3).
    #[test]
    fn a_sub_agents_spend_is_counted_and_tagged() {
        let text = r#"
{"type":"assistant","isSidechain":true,"cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:00Z","message":{"model":"m","usage":{"output_tokens":5}}}
"#;
        let mut carry = Carry::default();
        let found = read(Path::new("/x/s1.jsonl"), text, &mut carry);
        assert_eq!(found.len(), 1);
        assert!(found[0].subagent);
        assert_eq!(found[0].tokens.output, 5);
    }

    /// The dollar rollup is cumulative, so a second one adds the difference
    /// and not the total again — and it adds no tokens, which the calls have
    /// already counted (§FS-013-burn.7).
    #[test]
    fn the_cost_rollup_adds_its_difference_and_no_tokens() {
        let text = r#"
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:00Z","message":{"model":"m","usage":{"output_tokens":5}}}
{"type":"cost-state","sessionId":"s1","totalCostUSD":1.5,"modelUsage":{"m":{"costUSD":1.5}}}
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:07:00Z","message":{"model":"m","usage":{"output_tokens":5}}}
{"type":"cost-state","sessionId":"s1","totalCostUSD":2.0,"modelUsage":{"m":{"costUSD":2.0}}}
"#;
        let mut carry = Carry::default();
        let found = read(Path::new("/x/s1.jsonl"), text, &mut carry);
        let dollars: f64 = found.iter().filter_map(|sample| sample.cost_usd).sum();
        assert!((dollars - 2.0).abs() < 1e-9, "{dollars}");
        let tokens: u64 = found.iter().map(|sample| sample.tokens.total()).sum();
        assert_eq!(tokens, 10, "the rollup added tokens the calls already had");
        // It lands where the record before it did, since it carries no time.
        let priced = found
            .iter()
            .find(|sample| sample.cost_usd.is_some())
            .expect("a priced sample");
        assert_eq!(priced.at.to_rfc3339(), "2026-09-03T10:01:00+00:00");
    }

    /// A carry is what makes the scan incremental: the second half of a file
    /// read on its own still knows whose session it is (§FS-013-burn.5).
    #[test]
    fn the_carry_answers_for_records_read_in_a_later_pass() {
        let mut carry = Carry::default();
        read(
            Path::new("/x/s1.jsonl"),
            r#"{"type":"user","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:00:00Z"}"#,
            &mut carry,
        );
        let found = read(
            Path::new("/x/s1.jsonl"),
            r#"{"type":"assistant","timestamp":"2026-09-03T10:01:00Z","message":{"model":"m","usage":{"output_tokens":5}}}"#,
            &mut carry,
        );
        assert_eq!(found[0].cwd.as_deref(), Some("/w/app"));
        assert_eq!(found[0].session, "s1");
    }
}
