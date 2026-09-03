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
//!
//! **And one call is written as several records.** A response carrying more
//! than one content block is one `assistant` record per block, every one of
//! them naming the same `requestId` and repeating that request's identical
//! `usage`. Each reads as a perfectly ordinary call, so charging them all
//! bills one response two or three times and nothing downstream can tell —
//! on this machine's own transcripts that is most of a doubling. So the
//! request last charged is remembered, in the carry, and the records
//! repeating it are read for everything except their counters.

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
                // One response, several records: a reply that says something
                // and then calls two tools is written as one record per
                // content block, all naming the same request and all
                // restating that request's identical `usage`. The first is
                // the call; the rest are the same call again, and charging
                // them bills it two or three times (§FS-013-burn.3).
                let response = response(&record);
                if response.is_some() && response == carry.charged {
                    continue;
                }
                if let Some(sample) = call(&record, carry, subagent(file, carry)) {
                    found.push(sample);
                    carry.charged = response;
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

/// Which response a record is part of: the request the tool made, or — where
/// a record names no request — the message that request answered with. Two
/// records naming the same one are one call written twice (§FS-013-burn.3).
fn response(record: &Value) -> Option<String> {
    string(record, "requestId").or_else(|| string(record.get("message")?, "id"))
}

/// One paid call: the outer counters, and nothing from `iterations`.
fn call(record: &Value, carry: &mut Carry, subagent: bool) -> Option<Sample> {
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
    let model = string(message, "model").unwrap_or_else(|| "unknown".to_string());
    // What the rollup below joins its dollars back to (§FS-013-burn.3).
    carry.models.insert(model.clone());
    Some(Sample {
        at,
        cwd: carry.cwd.clone(),
        source: SOURCE,
        provider: PROVIDER.to_string(),
        model,
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
            model: joined(model, carry),
            session: carry.session.clone().unwrap_or_default(),
            subagent,
            tokens: Tokens::default(),
            cost_usd: Some(added),
        });
    }
    found
}

/// The model a rollup's dollars belong on (§FS-013-burn.3).
///
/// The two halves of this log spell one model two ways: a call names the
/// alias (`claude-opus-5`) and the rollup names the billing variant
/// (`claude-opus-5[1m]`). Left alone that is two rows for one model, one
/// holding every token and no price and the other a price and no tokens —
/// and a reader cannot add them back up, because they look like two models.
///
/// So a variant whose alias this session actually called is filed under that
/// alias. A model the rollup names and the calls never did — the tool's own
/// background work is billed but never appears as a call — keeps the only
/// spelling anything knows it by, and is a row of its own on purpose.
fn joined(model: &str, carry: &Carry) -> String {
    match model.split_once('[') {
        Some((alias, _)) if carry.models.contains(alias) => alias.to_string(),
        _ => model.to_string(),
    }
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
        assert_eq!(
            found[0].tokens.output, 1000,
            "the breakdown was counted too"
        );
        assert_eq!(found[0].tokens.total(), 1002);
        assert_eq!(found[0].model, "claude-opus-5");
        assert_eq!(found[0].cwd.as_deref(), Some("/w/app"));
        assert_eq!(found[0].session, "s1");
        assert!(!found[0].subagent);
    }

    /// One response, three records: text and then two tool calls, all under
    /// one `requestId` and all restating the one `usage` that request was
    /// billed at. This is the shape most of the machine's transcripts have,
    /// and a reader that charged every record would report 95 502 tokens
    /// where 31 834 were spent (§FS-013-burn.3).
    const BLOCKS: &str = r#"
{"type":"assistant","requestId":"req_1","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:00Z","message":{"id":"msg_1","model":"claude-opus-5","content":[{"type":"text"}],"usage":{"input_tokens":2,"output_tokens":338,"cache_read_input_tokens":10005,"cache_creation_input_tokens":21489}}}
{"type":"assistant","requestId":"req_1","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:01Z","message":{"id":"msg_1","model":"claude-opus-5","content":[{"type":"tool_use"}],"usage":{"input_tokens":2,"output_tokens":338,"cache_read_input_tokens":10005,"cache_creation_input_tokens":21489}}}
{"type":"assistant","requestId":"req_1","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:02Z","message":{"id":"msg_1","model":"claude-opus-5","content":[{"type":"tool_use"}],"usage":{"input_tokens":2,"output_tokens":338,"cache_read_input_tokens":10005,"cache_creation_input_tokens":21489}}}
{"type":"assistant","requestId":"req_2","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:02:00Z","message":{"id":"msg_2","model":"claude-opus-5","content":[{"type":"text"}],"usage":{"input_tokens":1,"output_tokens":9,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}
"#;

    #[test]
    fn a_response_is_charged_once_however_many_records_it_was_written_as() {
        let mut carry = Carry::default();
        let found = read(Path::new("/x/s1.jsonl"), BLOCKS, &mut carry);
        assert_eq!(found.len(), 2, "one response was charged more than once");
        assert_eq!(found[0].tokens.total(), 31834);
        assert_eq!(found[1].tokens.total(), 10);
        // The record charged is the first of the response, not the last, so
        // the call lands at the time it was made.
        assert_eq!(found[0].at.to_rfc3339(), "2026-09-03T10:01:00+00:00");
    }

    /// The trap the carry closes: a scan that stops between two records of
    /// one response must not charge the rest of it on the next pass
    /// (§FS-013-burn.5).
    #[test]
    fn a_scan_resuming_mid_response_does_not_charge_it_again() {
        let mut lines = BLOCKS.trim().lines();
        let first = lines.next().expect("the first block");
        let rest: String = lines.collect::<Vec<_>>().join("\n");
        let mut carry = Carry::default();
        let head = read(Path::new("/x/s1.jsonl"), first, &mut carry);
        assert_eq!(head.len(), 1);
        assert_eq!(carry.charged.as_deref(), Some("req_1"));
        let tail = read(Path::new("/x/s1.jsonl"), &rest, &mut carry);
        assert_eq!(tail.len(), 1, "the response was charged twice: {tail:?}");
        assert_eq!(tail[0].tokens.total(), 10);
    }

    /// A record naming no request falls back to the message it answered
    /// with, and one naming neither is charged rather than dropped: an
    /// unidentifiable call is still spend.
    #[test]
    fn a_response_with_no_request_id_is_still_told_apart() {
        let text = r#"
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:00Z","message":{"id":"msg_1","model":"m","usage":{"output_tokens":5}}}
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:01Z","message":{"id":"msg_1","model":"m","usage":{"output_tokens":5}}}
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:02:00Z","message":{"model":"m","usage":{"output_tokens":7}}}
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:03:00Z","message":{"model":"m","usage":{"output_tokens":7}}}
"#;
        let mut carry = Carry::default();
        let found = read(Path::new("/x/s1.jsonl"), text, &mut carry);
        let spent: u64 = found.iter().map(|sample| sample.tokens.total()).sum();
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(spent, 19, "an unidentifiable call was dropped");
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

    /// The two halves of this log spell one model two ways, and a reading
    /// that kept them apart would show every token under a row with no price
    /// and every dollar under a row with no tokens (§FS-013-burn.3).
    ///
    /// This is the shape the real transcripts have: calls name
    /// `claude-opus-5`, the rollup names `claude-opus-5[1m]`, and it also
    /// bills a model no call ever named.
    #[test]
    fn a_rollups_dollars_land_on_the_model_the_calls_named() {
        let text = r#"
{"type":"assistant","cwd":"/w/app","sessionId":"s1","timestamp":"2026-09-03T10:01:00Z","message":{"model":"claude-opus-5","usage":{"output_tokens":5}}}
{"type":"cost-state","sessionId":"s1","modelUsage":{"claude-opus-5[1m]":{"costUSD":4.0},"claude-haiku-4-5-20251001":{"costUSD":0.5}}}
"#;
        let mut carry = Carry::default();
        let found = read(Path::new("/x/s1.jsonl"), text, &mut carry);

        let priced: Vec<(&str, f64)> = found
            .iter()
            .filter_map(|sample| Some((sample.model.as_str(), sample.cost_usd?)))
            .collect();
        // The variant is filed under the alias its own session called, so the
        // tokens and the dollars are one row.
        assert!(
            priced.contains(&("claude-opus-5", 4.0)),
            "the variant did not join the alias: {priced:?}"
        );
        assert!(
            !priced.iter().any(|(model, _)| model.contains('[')),
            "a billing variant reached a bucket key: {priced:?}"
        );
        // And a model only the rollup knows about keeps its own spelling
        // rather than being folded into whichever alias looks nearest.
        assert!(
            priced.contains(&("claude-haiku-4-5-20251001", 0.5)),
            "a model no call named lost its row: {priced:?}"
        );
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
