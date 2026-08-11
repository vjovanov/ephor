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

/// One pull request's gate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gate {
    #[serde(default)]
    pub repos: Vec<RepoGate>,
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

    /// A gate with no jobs at all is not worth showing (no CI configured, the
    /// gate has not been started, or the lookup failed).
    pub fn is_empty(&self) -> bool {
        self.total() == 0
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

    /// Totals across every repo, e.g. `✓72 ✗1 ⋯3`.
    pub fn summary(&self) -> String {
        counts_text(self.passed(), self.failed(), self.running())
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
    Gate { repos: vec![entry] }
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
