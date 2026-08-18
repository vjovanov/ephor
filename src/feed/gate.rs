//! Gate status for a pull request: how many CI jobs passed, failed, and are
//! still in flight — per repository the gate covers.
//!
//! A change often spans several repositories: an internal PR gates over its
//! whole PR tree (app + plugins + docs-site), so a gate keeps one entry
//! per repo and callers show the total plus, when there is more than one, the
//! per-repo breakdown. Providers record it in the item's `raw.gate` during
//! refresh, so reading it is instant and works offline.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::feed::model::Item;

/// How much of a gate a restart asks for (§FS-004-quick-actions.9).
///
/// The caller's word, never the forge's guess: the cheap re-run and the
/// expensive one are different questions, and one of them spends an hour of a
/// shared machine pool.
/// Not serde's: the word is spelled once, by [`Scope::name`], because it
/// crosses three seams — the forge protocol, a bound command's environment,
/// and the command line — and a derived spelling beside a hand-written one is
/// two sources of truth for the same word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// Only what is not green — the failing gate and everything downstream of
    /// it (§FS-005-dispatch.11). The ordinary case: a job died on
    /// infrastructure and what it needs is that job back.
    #[default]
    Failed,
    /// Everything the gate covers, the green included. For when the merge
    /// commit itself is suspect and the passes are as untrustworthy as the
    /// failures.
    All,
}

impl Scope {
    /// The word that crosses every seam: the forge protocol, a bound command's
    /// environment, and the command line all spell it this way.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Failed => "failed",
            Scope::All => "all",
        }
    }

    /// Read the word back. `None` where it is not one of the two — a scope
    /// nobody recognizes is refused rather than defaulted, because the two
    /// differ by an hour of somebody else's machines.
    pub fn parse(word: &str) -> Option<Scope> {
        match word.trim().to_ascii_lowercase().as_str() {
            "failed" => Some(Scope::Failed),
            "all" => Some(Scope::All),
            _ => None,
        }
    }
}

/// What a job's state string means. Anything that is neither a finished pass
/// nor a finished failure is still in flight (queued, running, waiting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Passed,
    Failed,
    Running,
}

/// Classify a GitHub check state (`SUCCESS`, `TIMED_OUT`, …) or a Bitbucket
/// build state (`SUCCESSFUL`, `INPROGRESS`, …).
pub fn classify(state: &str) -> JobState {
    match state.trim().to_uppercase().as_str() {
        "SUCCESS" | "SUCCESSFUL" | "NEUTRAL" | "SKIPPED" => JobState::Passed,
        "FAILURE" | "FAILED" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "CANCELED" => JobState::Failed,
        _ => JobState::Running,
    }
}

/// Job counts for one repository of the gate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoGate {
    pub repo: String,
    #[serde(default)]
    pub passed: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub running: u64,
}

impl RepoGate {
    pub fn new(repo: impl Into<String>) -> Self {
        RepoGate {
            repo: repo.into(),
            ..RepoGate::default()
        }
    }

    pub fn add(&mut self, state: &str, count: u64) {
        match classify(state) {
            JobState::Passed => self.passed += count,
            JobState::Failed => self.failed += count,
            JobState::Running => self.running += count,
        }
    }

    pub fn total(&self) -> u64 {
        self.passed + self.failed + self.running
    }
}

/// One pull request's gate: what its jobs did, and — where the forge reaches a
/// verdict of its own — whether it will let the change merge.
///
/// The verdict is not derivable from the counts. A gate whose every job is
/// green may still be blocked on an approval, on a downstream repository, or
/// on jobs it has not started, and a row that shows only what passed reads as
/// finished work (§FS-001-forge-interface.1). A forge with no verdict reports
/// none, and the counts speak for themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gate {
    #[serde(default)]
    pub repos: Vec<RepoGate>,
    /// The forge says this gate blocks the merge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
    /// Why, in the forge's own words, one reason per entry. Shown verbatim:
    /// the reasons are the forge's vocabulary, and rewording them costs the
    /// reader the ability to match what ephor says against the forge itself.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
}

impl Gate {
    pub fn passed(&self) -> u64 {
        self.repos.iter().map(|repo| repo.passed).sum()
    }

    pub fn failed(&self) -> u64 {
        self.repos.iter().map(|repo| repo.failed).sum()
    }

    pub fn running(&self) -> u64 {
        self.repos.iter().map(|repo| repo.running).sum()
    }

    pub fn total(&self) -> u64 {
        self.repos.iter().map(RepoGate::total).sum()
    }

    /// A gate that says nothing is not worth showing: no jobs, and no verdict
    /// either (no CI configured, the gate has not been started, or the lookup
    /// failed). A gate with no jobs that nonetheless blocks the merge is
    /// saying the most important thing it has to say.
    pub fn is_empty(&self) -> bool {
        self.total() == 0 && !self.blocked && self.blockers.is_empty()
    }

    /// The gate is red: something ran and did not pass, or the forge refuses
    /// the merge. This is the condition the failures action is offered on
    /// (§FS-004-quick-actions.4).
    pub fn is_red(&self) -> bool {
        self.failed() > 0 || self.blocked
    }

    /// The gate a provider recorded on an item, if any jobs ran.
    pub fn of(item: &Item) -> Option<Gate> {
        let gate: Gate = serde_json::from_value(item.raw.get("gate")?.clone()).ok()?;
        if gate.is_empty() {
            return None;
        }
        Some(gate)
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Totals across every repo, e.g. `✓72 ✗1 ⋯3`, with the forge's verdict
    /// appended when it refuses the merge — `✓118 ⊘ blocked` is a gate whose
    /// jobs all passed and which still will not go in.
    pub fn summary(&self) -> String {
        let counts = counts_text(self.passed(), self.failed(), self.running());
        match (self.blocked, counts.is_empty()) {
            (false, _) => counts,
            (true, true) => BLOCKED.to_string(),
            (true, false) => format!("{counts} {BLOCKED}"),
        }
    }

    /// Per-repo counts, e.g. `widget ✓42 ✗1 · plugins ✓30 ⋯3`. Empty
    /// for a gate covering a single repository — [`Gate::summary`] already
    /// says everything then.
    pub fn breakdown(&self) -> String {
        if self.repos.len() < 2 {
            return String::new();
        }
        self.repos
            .iter()
            .map(|repo| {
                format!(
                    "{} {}",
                    repo.repo,
                    counts_text(repo.passed, repo.failed, repo.running)
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// The forge's refusal, in the same shorthand as the counts beside it.
pub const BLOCKED: &str = "⊘ blocked";

/// One thing that went wrong under a red gate (§FS-001-forge-interface.1).
///
/// `job` names it as the forge does and `url` reaches its log; `trace` is the
/// error itself, where the forge can extract one. A forge that can only link
/// is a complete implementation — the reader still gets a list of what failed
/// and one keystroke to each log, which is the errand they would otherwise run
/// by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// The job as the forge names it. Empty where the forge reports a failure
    /// without attributing it to a named job.
    #[serde(default)]
    pub job: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The error text, as close to the compiler or test runner as the forge
    /// can get it.
    #[serde(default)]
    pub trace: String,
}

impl Failure {
    /// The line a reader scans for: the first non-empty line of the trace, or
    /// the job name when there is no trace to show.
    pub fn headline(&self) -> String {
        self.trace
            .lines()
            .map(str::trim_end)
            .find(|line| !line.trim().is_empty())
            .map(String::from)
            .unwrap_or_else(|| self.job.clone())
    }
}

/// Failures that are the same error seen from different jobs, collapsed to one
/// entry each and counted (§FS-004-quick-actions.4). A gate fans one compile
/// error across every job that built the file; six copies of it is a worse
/// answer than one copy that says six.
///
/// Sameness is the trace, verbatim — two jobs that printed the same error are
/// reporting one problem, whatever they are called. Failures with no trace
/// group by job name instead, which is all that distinguishes them.
pub fn group(failures: Vec<Failure>) -> Vec<(Failure, usize)> {
    fn key(failure: &Failure) -> &str {
        if failure.trace.trim().is_empty() {
            &failure.job
        } else {
            &failure.trace
        }
    }
    let mut grouped: Vec<(Failure, usize)> = Vec::new();
    for failure in failures {
        match grouped
            .iter()
            .position(|(seen, _)| key(seen) == key(&failure))
        {
            Some(index) => grouped[index].1 += 1,
            None => grouped.push((failure, 1)),
        }
    }
    grouped
}

/// `✓N ✗N ⋯N`, dropping the counts that are zero.
fn counts_text(passed: u64, failed: u64, running: u64) -> String {
    let mut parts = Vec::new();
    if passed > 0 {
        parts.push(format!("✓{passed}"));
    }
    if failed > 0 {
        parts.push(format!("✗{failed}"));
    }
    if running > 0 {
        parts.push(format!("⋯{running}"));
    }
    parts.join(" ")
}

/// Count `gh pr checks --json state` results into a single-repo gate.
pub fn from_check_states(repo: &str, checks: &[Value]) -> Gate {
    let mut entry = RepoGate::new(repo);
    for check in checks {
        entry.add(check.get("state").and_then(Value::as_str).unwrap_or(""), 1);
    }
    if entry.total() == 0 {
        return Gate::default();
    }
    Gate {
        repos: vec![entry],
        ..Gate::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::ItemKind;
    use serde_json::json;

    fn item_with(raw: Value) -> Item {
        Item {
            id: "bitbucket-prs:app/23562".to_string(),
            project: "widget".to_string(),
            source: "bitbucket-prs".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "#23562 work".to_string(),
            url: None,
            state: None,
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw,
        }
    }

    #[test]
    fn states_classify_across_both_forges() {
        assert_eq!(classify("SUCCESS"), JobState::Passed);
        assert_eq!(classify("successful"), JobState::Passed);
        assert_eq!(classify("SKIPPED"), JobState::Passed);
        assert_eq!(classify("FAILURE"), JobState::Failed);
        assert_eq!(classify("FAILED"), JobState::Failed);
        assert_eq!(classify("TIMED_OUT"), JobState::Failed);
        assert_eq!(classify("INPROGRESS"), JobState::Running);
        assert_eq!(classify("QUEUED"), JobState::Running);
        assert_eq!(classify("whatever"), JobState::Running);
    }

    #[test]
    fn check_states_become_a_single_repo_gate() {
        let checks = vec![
            json!({ "name": "gate", "state": "FAILURE" }),
            json!({ "name": "style", "state": "SUCCESS" }),
            json!({ "name": "build", "state": "IN_PROGRESS" }),
        ];
        let gate = from_check_states("acme/widget", &checks);
        assert_eq!(gate.summary(), "✓1 ✗1 ⋯1");
        assert!(from_check_states("acme/widget", &[]).is_empty());
    }

    #[test]
    fn a_verdict_is_part_of_the_gate_even_with_nothing_red_in_the_counts() {
        let gate = Gate {
            repos: vec![RepoGate {
                repo: "app".to_string(),
                passed: 118,
                failed: 0,
                running: 0,
            }],
            blocked: true,
            blockers: vec!["The gate app has 122 jobs not yet run.".to_string()],
        };
        // All green and still red: this is the row a reader would skip.
        assert_eq!(gate.failed(), 0);
        assert!(gate.is_red());
        assert_eq!(gate.summary(), "✓118 ⊘ blocked");

        // A gate that never started but refuses the merge says the one thing
        // it knows, rather than disappearing.
        let approvals = Gate {
            blocked: true,
            blockers: vec!["Requires approvals".to_string()],
            ..Gate::default()
        };
        assert!(!approvals.is_empty());
        assert_eq!(approvals.summary(), "⊘ blocked");

        // And a gate with counts and no verdict reads exactly as before.
        let plain = from_check_states("acme/widget", &[json!({ "state": "SUCCESS" })]);
        assert_eq!(plain.summary(), "✓1");
        assert!(!plain.is_red());
    }

    #[test]
    fn identical_failures_collapse_and_are_counted() {
        let failure = |job: &str, trace: &str| Failure {
            job: job.to_string(),
            url: Some(format!("https://ci.example/{job}")),
            trace: trace.to_string(),
        };
        let grouped = group(vec![
            failure("build 1", "error: already defined"),
            failure("build 2", "error: cannot find symbol"),
            failure("build 3", "error: already defined"),
            failure("build 4", "error: already defined"),
        ]);
        // Four jobs, two problems — and the first job of each group keeps its
        // link, so the reader still has a log to open.
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].1, 3);
        assert_eq!(grouped[0].0.job, "build 1");
        assert_eq!(grouped[1].1, 1);

        // Nothing to compare but the job name: those group by name instead of
        // collapsing into one nameless entry.
        let linked = group(vec![failure("build 5", ""), failure("build 6", "")]);
        assert_eq!(linked.len(), 2);
        assert_eq!(linked[0].0.headline(), "build 5");
    }

    #[test]
    fn a_headline_is_the_first_line_a_reader_would_read() {
        let failure = Failure {
            job: "build 7".to_string(),
            url: None,
            trace: "\n  \nerror: cannot find symbol\n  location: class Foo\n".to_string(),
        };
        assert_eq!(failure.headline(), "error: cannot find symbol");
    }

    #[test]
    fn item_gate_roundtrips_through_raw() {
        let gate = from_check_states("acme/widget", &[json!({ "state": "SUCCESS" })]);
        let item = item_with(json!({ "branch": "vj/work", "gate": gate.to_value() }));
        assert_eq!(Gate::of(&item), Some(gate));

        // No gate recorded, or one without any job: nothing to show.
        assert_eq!(Gate::of(&item_with(json!({ "branch": "vj/work" }))), None);
        assert_eq!(
            Gate::of(&item_with(json!({ "gate": { "repos": [] } }))),
            None
        );
    }
}
