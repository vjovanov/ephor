//! Reading the other agent tool's sessions: `~/.codex/sessions/**/*.jsonl`
//! (§FS-013-burn.3).
//!
//! One JSON record per line again, but counted the other way round. The
//! `token_count` event does not report what the last call spent — it restates
//! **the session's running total**, from the first turn to this one. Summing
//! the events therefore sums every prefix of the session and inflates it by
//! orders of magnitude, which is why this reader diffs consecutive events and
//! the test below is a session whose events would inflate.
//!
//! Two more things this log makes non-trivial, both handled here:
//!
//! - **The model changes mid-session.** `turn_context` names the model in
//!   force from that turn on, and each delta is attributed to the one in force
//!   when it was reported — never to one model for the whole session.
//! - **Its input counter includes the cached part.** The other log reports the
//!   cache counters beside the input; this one reports them inside it, so what
//!   is left after taking them out is the input that was actually paid for at
//!   the input rate.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::cursors::Carry;
use super::{Sample, Tokens};

/// What this tool is called on a bucket key.
pub const SOURCE: &str = "codex";

/// Where a session that never named its provider is filed. The tool routes to
/// more than one, so this is a fallback, not the answer (§FS-013-burn.3).
const UNKNOWN: &str = "unknown";

/// Every sample in `text`, with `carry` moved on to what the last record left.
pub fn read(text: &str, carry: &mut Carry) -> Vec<Sample> {
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(when) = string(&record, "timestamp") {
            carry.at = Some(when);
        }
        let payload = record.get("payload");
        match record.get("type").and_then(Value::as_str) {
            // The session's own facts, named once at the top.
            Some("session_meta") => {
                if let Some(payload) = payload {
                    carry.session = string(payload, "session_id").or(carry.session.take());
                    carry.cwd = string(payload, "cwd").or(carry.cwd.take());
                    carry.provider = string(payload, "model_provider").or(carry.provider.take());
                }
            }
            // The model in force from this turn on (§FS-013-burn.3).
            Some("turn_context") => {
                if let Some(payload) = payload {
                    if let Some(model) = string(payload, "model") {
                        carry.model = Some(model);
                    }
                    if let Some(cwd) = string(payload, "cwd") {
                        carry.cwd = Some(cwd);
                    }
                }
            }
            Some("event_msg") => {
                let Some(payload) = payload else { continue };
                if payload.get("type").and_then(Value::as_str) == Some("token_count") {
                    if let Some(sample) = spent(&record, payload, carry) {
                        found.push(sample);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// What one `token_count` event says was spent *since the last one*.
fn spent(record: &Value, payload: &Value, carry: &mut Carry) -> Option<Sample> {
    let total = payload.get("info")?.get("total_token_usage")?;
    let cache_read = counter(total, "cached_input_tokens");
    let cache_write = counter(total, "cache_write_input_tokens");
    let running = Tokens {
        // The reported input holds the cached part; what is left is what was
        // paid for at the input rate.
        input: counter(total, "input_tokens")
            .saturating_sub(cache_read)
            .saturating_sub(cache_write),
        output: counter(total, "output_tokens"),
        cache_read,
        cache_write,
    };
    let seen = carry.totals.unwrap_or_default();
    // A running total that went backwards is a session restarted or compacted
    // under the same name: take it as it stands rather than as nothing.
    let tokens = if running.behind(&seen) {
        running
    } else {
        running.since(&seen)
    };
    carry.totals = Some(running);
    if tokens.is_empty() {
        return None;
    }
    let at = when(record.get("timestamp").and_then(Value::as_str))
        .or_else(|| when(carry.at.as_deref()))?;
    Some(Sample {
        at,
        cwd: carry.cwd.clone(),
        source: SOURCE,
        provider: carry
            .provider
            .clone()
            .unwrap_or_else(|| UNKNOWN.to_string()),
        model: carry.model.clone().unwrap_or_else(|| UNKNOWN.to_string()),
        session: carry.session.clone().unwrap_or_default(),
        subagent: false,
        tokens,
        // Nothing in this log carries a price (§FS-013-burn.7).
        cost_usd: None,
    })
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

    /// Three events restating one running total. Summing them gives 600
    /// output tokens where 300 were spent — and a session with a hundred
    /// events would be out by a hundredfold, which is the failure this pins
    /// (§FS-013-burn.3).
    const CUMULATIVE: &str = r#"
{"type":"session_meta","timestamp":"2026-09-03T10:00:00Z","payload":{"session_id":"c1","cwd":"/w/app","model_provider":"openai"}}
{"type":"turn_context","timestamp":"2026-09-03T10:00:01Z","payload":{"model":"gpt-5.6-sol"}}
{"type":"event_msg","timestamp":"2026-09-03T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":100}}}}
{"type":"event_msg","timestamp":"2026-09-03T10:02:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":200}}}}
{"type":"event_msg","timestamp":"2026-09-03T10:03:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":300}}}}
"#;

    #[test]
    fn the_running_total_is_diffed_and_never_summed() {
        let mut carry = Carry::default();
        let found = read(CUMULATIVE, &mut carry);
        assert_eq!(found.len(), 3);
        let output: u64 = found.iter().map(|sample| sample.tokens.output).sum();
        assert_eq!(output, 300, "the events were summed rather than diffed");
        for sample in &found {
            assert_eq!(sample.tokens.output, 100);
            assert_eq!(sample.cwd.as_deref(), Some("/w/app"));
            assert_eq!(sample.provider, "openai");
            assert_eq!(sample.session, "c1");
        }
    }

    /// A session that changes model mid-way splits, and each delta lands on
    /// the model in force when it was reported — never on one model for the
    /// session (§FS-013-burn.3).
    #[test]
    fn each_delta_lands_on_the_model_in_force() {
        let text = r#"
{"type":"session_meta","timestamp":"2026-09-03T10:00:00Z","payload":{"session_id":"c1","cwd":"/w/app","model_provider":"openai"}}
{"type":"turn_context","timestamp":"2026-09-03T10:00:01Z","payload":{"model":"gpt-5.6-sol"}}
{"type":"event_msg","timestamp":"2026-09-03T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":100}}}}
{"type":"turn_context","timestamp":"2026-09-03T10:02:00Z","payload":{"model":"gpt-5.6-terra"}}
{"type":"event_msg","timestamp":"2026-09-03T10:03:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":250}}}}
"#;
        let mut carry = Carry::default();
        let found = read(text, &mut carry);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].model, "gpt-5.6-sol");
        assert_eq!(found[0].tokens.output, 100);
        assert_eq!(found[1].model, "gpt-5.6-terra");
        assert_eq!(found[1].tokens.output, 150);
    }

    /// The cached part sits inside the reported input, so taking it out is
    /// what keeps a cache read from being counted as an input token twice.
    #[test]
    fn the_cached_part_is_taken_out_of_the_input() {
        let text = r#"
{"type":"event_msg","timestamp":"2026-09-03T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":800,"cache_write_input_tokens":50,"output_tokens":7}}}}
"#;
        let mut carry = Carry::default();
        let found = read(text, &mut carry);
        assert_eq!(
            found[0].tokens,
            Tokens {
                input: 150,
                output: 7,
                cache_read: 800,
                cache_write: 50
            }
        );
        assert_eq!(found[0].tokens.total(), 1007, "the input was counted twice");
    }

    /// A running total that went backwards is a restart, not a refund: the
    /// new total stands rather than a saturated zero swallowing the session.
    #[test]
    fn a_reset_running_total_starts_again_rather_than_vanishing() {
        let text = r#"
{"type":"event_msg","timestamp":"2026-09-03T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":500}}}}
{"type":"event_msg","timestamp":"2026-09-03T10:02:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":30}}}}
"#;
        let mut carry = Carry::default();
        let found = read(text, &mut carry);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].tokens.output, 30);
    }

    /// The carry is what a later pass reads the session's facts from: a tail
    /// read on its own still knows the model and the directory
    /// (§FS-013-burn.5).
    #[test]
    fn the_carry_answers_for_a_tail_read_on_its_own() {
        let mut carry = Carry::default();
        read(
            r#"{"type":"session_meta","timestamp":"2026-09-03T10:00:00Z","payload":{"session_id":"c1","cwd":"/w/app","model_provider":"openrouter"}}
{"type":"turn_context","timestamp":"2026-09-03T10:00:01Z","payload":{"model":"z-ai/glm-5.2"}}"#,
            &mut carry,
        );
        let found = read(
            r#"{"type":"event_msg","timestamp":"2026-09-03T10:05:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":9}}}}"#,
            &mut carry,
        );
        assert_eq!(found[0].model, "z-ai/glm-5.2");
        assert_eq!(found[0].provider, "openrouter");
        assert_eq!(found[0].cwd.as_deref(), Some("/w/app"));
    }
}
