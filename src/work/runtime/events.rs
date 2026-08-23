//! The stream a run writes about itself (§FS-005-dispatch.15.2,
//! §AR-007-runtime.1).
//!
//! The journal beside it (`watch::holding`) is append-only across every run a
//! root ever had, so an assignment it never released has to be argued down
//! from evidence elsewhere — the ticket's own state, the age of a log against
//! the birth of the lock. This file is about one run: the runtime truncates it
//! when a run starts and appends a record per structural move, so an
//! assignment still open in it belongs to the run that wrote it and to no
//! other. That is the whole reason to prefer it.
//!
//! It is the binding's own artifact grammar and is spelled here and nowhere
//! else (§REQ-001-boundary.5). Everything is tolerant of what it does not
//! recognize: the binding may add record kinds and fields, so an unknown kind
//! is skipped rather than failing the read, and a stream that is absent is not
//! an error but the floor answering alone (§AR-007-runtime.3).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Where a run writes the record of itself: one JSON object per line,
/// truncated at run start and flushed per line, so a reader following a live
/// run sees each record as it lands.
pub const STREAM: &str = "runtime/events.jsonl";

/// The record that opens a stream, carrying the run's own id.
const STARTED: &str = "run_started";
/// A worker was spawned on a ticket.
const ASSIGNED: &str = "slot_assigned";
/// That worker exited, however it exited.
const RELEASED: &str = "slot_released";
/// The run loop ended. Closing diagnostics may follow it; no slot record
/// does, so a reader that has seen this has seen every slot the run took.
const FINISHED: &str = "run_finished";

/// The record layout this reader was written against
/// (`run_started.schema`). The binding moves it only when a field named in
/// its own contract is removed or changes meaning; adding kinds and fields
/// does not. So a stream that says more than this is read as far as it is
/// understood, and one that says *less* — a number below what we know — is
/// read too: neither is a reason to fall back to a worse witness.
const SCHEMA: u64 = 1;

/// One slot the run took up and has not let go of, as the run itself says.
///
/// No inference is involved: this is an assignment with no matching release
/// in a file that covers exactly one run. Whether that means *running* or
/// *dropped* is the lock's answer, not this one (§FS-005-dispatch.15.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The ticket, as the runtime spells it — plan-qualified or bare,
    /// exactly as the journal's task field is.
    pub task: String,
    /// The state the worker was moving the ticket to, where the record said
    /// one, and the state it came from where the two differ — the same pair
    /// the journal's held entry carries, so a caller matches either.
    pub states: Vec<String>,
    /// The invocation's log, as the record spelled it.
    pub log: Option<PathBuf>,
}

impl Slot {
    /// Whether the ticket's current state is still one this assignment
    /// names. The same question [`super::watch::Held::still_at`] answers: a
    /// ticket that moved on since was released by whatever moved it.
    pub fn still_at(&self, state: &str) -> bool {
        self.states.iter().any(|known| known == state)
    }
}

/// What one run's stream says about that run (§FS-005-dispatch.15.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Progress {
    /// What the run calls itself, from its opening record. The descriptor
    /// beside the lock says this too (§FS-005-dispatch.20); read here it
    /// costs nothing extra and lets a caller tell a stream that belongs to
    /// the run it is asking about from one left by an earlier run.
    pub id: Option<String>,
    /// The last structural record's number — how much of the stream this
    /// reading has seen. 0 for a stream with no numbered record in it.
    pub seq: u64,
    /// Slots assigned and not released, in the order the run took them.
    pub holding: Vec<Slot>,
    /// The run loop ended and said so. A stream without this says the run
    /// did not reach its own end — interrupted, failed, or the process
    /// died — which is information, not damage.
    pub finished: bool,
    /// The record layout the run declared, where it declared one.
    pub schema: Option<u64>,
}

impl Progress {
    /// Whether this reading understands the stream it read. A schema the
    /// binding has moved past means fields this reader knows may no longer
    /// mean what it thinks; the honest move is to say so and let the floor
    /// answer, rather than to report a confident reading of a document in a
    /// language that changed (§AR-007-runtime.3).
    fn legible(&self) -> bool {
        self.schema.map(|found| found <= SCHEMA).unwrap_or(true)
    }

    /// Whether the run's stream still holds this ticket: an assignment with
    /// no release, on a state the ticket is still in.
    ///
    /// The state check is the one inference kept from the journal reading,
    /// and it is kept for a different reason: not because the record might
    /// belong to another run — it cannot — but because a ticket the reader
    /// moved by hand mid-run left the run's slot behind, and the plan is
    /// the authority on where a ticket is (§FS-005-dispatch.4).
    pub fn holds(&self, plan_id: &str, ticket: &str, state: Option<&str>) -> bool {
        self.holding.iter().any(|slot| {
            (slot.task == ticket || slot.task == format!("{plan_id}.{ticket}"))
                && state.map(|state| slot.still_at(state)).unwrap_or(false)
        })
    }
}

/// The stream a run left on this root, where it left one.
///
/// `None` where the binding writes none, where the file cannot be read, or
/// where it declares a layout this reader does not understand — in every one
/// of those the journal is the floor and answers alone
/// (§FS-005-dispatch.15.2). A file that exists but parses to nothing is a run
/// that has written no structural record yet, which is a legitimate reading
/// and is returned as an empty [`Progress`], not as absence: the caller can
/// then tell "this run holds nothing" from "there is no stream here".
pub fn progress(root: &Path) -> Option<Progress> {
    read(&fs::read_to_string(root.join(STREAM)).ok()?)
}

/// The reading itself, over the stream's text. Split out because it is the
/// whole of the grammar and is what the tests exercise.
fn read(text: &str) -> Option<Progress> {
    let mut progress = Progress::default();
    // Slots are keyed on (task, log) exactly as the journal's are: under
    // fanout one task runs as several invocations at once, and a release of
    // the first must not free the second (§FS-005-dispatch.15).
    let mut open: Vec<(String, Option<PathBuf>, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // A partial last line is what following a live run looks like: the
        // run flushes per record, but a reader may still arrive mid-write.
        // Skipping it is right — the next read gets it whole.
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let word = |key: &str| {
            record
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|found| !found.is_empty())
                .map(String::from)
        };
        if let Some(seq) = record.get("seq").and_then(serde_json::Value::as_u64) {
            progress.seq = progress.seq.max(seq);
        }
        match word("event").as_deref() {
            Some(STARTED) => {
                progress.id = word("run_id");
                progress.schema = record.get("schema").and_then(serde_json::Value::as_u64);
            }
            Some(ASSIGNED) => {
                let Some(task) = word("task") else { continue };
                let log = word("log_path").map(PathBuf::from);
                let states = states(&word("from"), &word("to"));
                match open
                    .iter_mut()
                    .find(|(known, path, _)| known == &task && path == &log)
                {
                    Some(slot) => slot.2 = states,
                    None => open.push((task, log, states)),
                }
            }
            Some(RELEASED) => {
                let Some(task) = word("task") else { continue };
                let log = word("log_path").map(PathBuf::from);
                open.retain(|(known, path, _)| !(known == &task && path == &log));
            }
            // The run loop is over. Closing notes may follow and are read
            // like any other record; no slot record ever does.
            Some(FINISHED) => progress.finished = true,
            _ => {}
        }
    }
    progress.holding = open
        .into_iter()
        .map(|(task, log, states)| Slot { task, states, log })
        .collect();
    progress.legible().then_some(progress)
}

/// The state or states a slot record names: the target, and the origin where
/// the two differ — one assignment can be matched against a ticket sitting at
/// either end of the move, which is the same latitude the journal's reading
/// takes.
fn states(from: &Option<String>, to: &Option<String>) -> Vec<String> {
    let mut states: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for state in [to, from].into_iter().flatten() {
        if seen.insert(state.clone()) {
            states.push(state.clone());
        }
    }
    states
}

/// When the run's stream was last written, for the change gate
/// (§FS-005-dispatch.15.1). One more name in the fixed handful, never a
/// sweep: a live run touches this on every structural move.
pub fn wrote_at(root: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(root.join(STREAM))
        .and_then(|meta| meta.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assigned(seq: u64, task: &str, to: &str, log: &str) -> String {
        format!(
            r#"{{"seq":{seq},"ts":"2026-08-23T10:00:00Z","event":"slot_assigned","slot":0,"task":"{task}","from":"collect","to":"{to}","agent":"claude","log_path":"{log}"}}"#
        )
    }

    fn released(seq: u64, task: &str, to: &str, log: &str, outcome: &str) -> String {
        format!(
            r#"{{"seq":{seq},"ts":"2026-08-23T10:01:00Z","event":"slot_released","slot":0,"task":"{task}","from":"collect","to":"{to}","log_path":"{log}","outcome":"{outcome}","exit_code":0,"duration_ms":60000}}"#
        )
    }

    fn started(id: &str) -> String {
        format!(
            r#"{{"seq":1,"ts":"2026-08-23T09:59:00Z","event":"run_started","schema":1,"run_id":"{id}","workspace":"/w","parallel":1,"total_tasks":2}}"#
        )
    }

    #[test]
    fn a_run_that_took_a_slot_and_has_not_released_it_is_holding_it() {
        let text = [
            started("3f9a2c"),
            assigned(2, "plan.fix-gate-1", "fix", "runtime/logs/a.log"),
        ]
        .join("\n");
        let progress = read(&text).expect("a legible stream");
        assert_eq!(progress.id.as_deref(), Some("3f9a2c"));
        assert_eq!(progress.seq, 2);
        assert!(!progress.finished);
        assert!(progress.holds("plan", "fix-gate-1", Some("fix")));
        // The origin state matches too: a ticket the run has picked up but
        // not yet moved is still where it was.
        assert!(progress.holds("plan", "fix-gate-1", Some("collect")));
        assert!(!progress.holds("plan", "fix-gate-1", Some("review")));
    }

    #[test]
    fn a_released_slot_is_no_longer_held() {
        let text = [
            started("3f9a2c"),
            assigned(2, "plan.fix-gate-1", "fix", "runtime/logs/a.log"),
            released(
                3,
                "plan.fix-gate-1",
                "fix",
                "runtime/logs/a.log",
                "completed",
            ),
        ]
        .join("\n");
        let progress = read(&text).expect("a legible stream");
        assert!(progress.holding.is_empty());
        assert!(!progress.holds("plan", "fix-gate-1", Some("fix")));
    }

    /// The reason slots are keyed on the log and not on the task alone: one
    /// task runs as several invocations under fanout, and releasing the
    /// first must leave the second held (§FS-005-dispatch.15).
    #[test]
    fn one_task_in_two_slots_stays_held_when_one_is_released() {
        let text = [
            started("3f9a2c"),
            assigned(2, "plan.fan", "work", "runtime/logs/a.log"),
            assigned(3, "plan.fan", "work", "runtime/logs/b.log"),
            released(4, "plan.fan", "work", "runtime/logs/a.log", "completed"),
        ]
        .join("\n");
        let progress = read(&text).expect("a legible stream");
        assert_eq!(progress.holding.len(), 1);
        assert!(progress.holds("plan", "fan", Some("work")));
    }

    #[test]
    fn a_finished_run_says_so() {
        let text = [
            started("3f9a2c"),
            assigned(2, "plan.fix-gate-1", "fix", "runtime/logs/a.log"),
            released(3, "plan.fix-gate-1", "fix", "runtime/logs/a.log", "completed"),
            r#"{"seq":4,"ts":"2026-08-23T10:02:00Z","event":"run_finished","summary":{"completed":1}}"#.to_string(),
        ]
        .join("\n");
        let progress = read(&text).expect("a legible stream");
        assert!(progress.finished);
        assert_eq!(progress.seq, 4);
    }

    /// A run that died mid-slot left its assignment open and never said it
    /// finished — which is exactly the evidence a dropped ticket needs, and
    /// here it is one run's own word rather than an inference over a journal
    /// that outlives every run (§FS-005-dispatch.15.2).
    #[test]
    fn a_run_that_died_leaves_its_slot_open_and_never_says_it_finished() {
        let text = [
            started("3f9a2c"),
            assigned(2, "plan.fix-gate-1", "fix", "runtime/logs/a.log"),
        ]
        .join("\n");
        let progress = read(&text).expect("a legible stream");
        assert!(!progress.finished);
        assert_eq!(progress.holding.len(), 1);
    }

    #[test]
    fn unknown_records_and_a_partial_last_line_are_skipped() {
        let text = [
            started("3f9a2c"),
            r#"{"seq":2,"ts":"2026-08-23T10:00:00Z","event":"pass_started","pass":1,"ready":["plan.fix-gate-1"]}"#.to_string(),
            r#"{"seq":3,"ts":"2026-08-23T10:00:00Z","event":"something_new_the_binding_added","what":1}"#.to_string(),
            assigned(4, "plan.fix-gate-1", "fix", "runtime/logs/a.log"),
            r#"{"seq":5,"ts":"2026-08-23T10:0"#.to_string(),
        ]
        .join("\n");
        let progress = read(&text).expect("a legible stream");
        assert_eq!(progress.seq, 4, "the partial line is not counted as seen");
        assert!(progress.holds("plan", "fix-gate-1", Some("fix")));
    }

    /// A layout this reader was not written against: the floor answers alone
    /// rather than this reporting a confident reading of a document whose
    /// language moved (§AR-007-runtime.3).
    #[test]
    fn a_stream_from_a_later_layout_is_not_read() {
        let text = r#"{"seq":1,"ts":"2026-08-23T09:59:00Z","event":"run_started","schema":9,"run_id":"3f9a2c"}"#;
        assert_eq!(read(text), None);
    }

    #[test]
    fn a_stream_with_nothing_in_it_yet_is_a_reading_not_an_absence() {
        let progress = read("").expect("an empty stream still reads");
        assert_eq!(progress.seq, 0);
        assert!(progress.holding.is_empty());
        assert!(!progress.finished);
    }

    #[test]
    fn a_root_with_no_stream_has_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(progress(dir.path()), None);
    }

    #[test]
    fn a_bare_task_id_matches_the_plan_qualified_question() {
        let text = [
            started("3f9a2c"),
            assigned(2, "fix-gate-1", "fix", "runtime/logs/a.log"),
        ]
        .join("\n");
        let progress = read(&text).expect("a legible stream");
        assert!(progress.holds("plan", "fix-gate-1", Some("fix")));
    }
}
