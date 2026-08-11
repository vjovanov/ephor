//! CI check status for the user's open PRs via `gh pr checks`. Emits one
//! item per PR that has failing or pending checks, and offers the failing one
//! its log as a quick action (§FS-004-quick-actions.4).

use serde::Deserialize;
use serde_json::Value;

use crate::feed::config::ActionConfig;
use crate::feed::gate;
use crate::feed::model::{Item, ItemKind};
use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::{gh_command, parse_config, parse_github_time, shell_quote};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    repos: Vec<String>,
    #[serde(default)]
    host: Option<String>,
}

/// The item state that says the gate is red — the condition the failing-CI
/// quick action exists for — and its counterpart, still in flight.
const FAILING: &str = "failing";
const PENDING: &str = "pending";

/// What a red gate is asking, answered in one screen (§FS-004-quick-actions.4):
/// the check list as GitHub reports it, then the log of every failed job, one
/// copy per underlying run. `gh pr checks` signals check state through its
/// exit code, so a non-zero status here is the failure being reported and not
/// a broken command; the run id is read back out of each failing check's link,
/// which is the only place `gh` hands it over.
const SHOW_FAILING_CHECKS: &str = r##"{
  gh pr checks "$EPHOR_NUMBER" --repo "$EPHOR_REPO" || true
  gh pr checks "$EPHOR_NUMBER" --repo "$EPHOR_REPO" --json state,link --jq '
        .[] | select(.state as $state
          | ["FAILURE", "FAILED", "ERROR", "TIMED_OUT", "CANCELLED", "CANCELED"]
          | index($state)) | .link' \
    | sed -n 's#.*/runs/\([0-9][0-9]*\)/.*#\1#p' \
    | sort -u \
    | while read -r run; do
        printf '\n===== failed jobs of run %s =====\n\n' "$run"
        gh run view "$run" --repo "$EPHOR_REPO" --log-failed
      done
} 2>&1 | ${PAGER:-less -R}
"##;

/// The failing-CI quick action, pointed at the host the checks live on: the
/// enterprise host is configuration, so it is exported rather than named in
/// the script.
fn show_failing_checks(host: Option<&str>) -> ActionConfig {
    let command = match host {
        Some(host) => format!(
            "export GH_HOST={}\n{SHOW_FAILING_CHECKS}",
            shell_quote(host)
        ),
        None => SHOW_FAILING_CHECKS.to_string(),
    };
    ActionConfig {
        icon: "✗".to_string(),
        description: "see the CI failures".to_string(),
        command,
        kinds: Vec::new(),
        requires_checkout: false,
    }
}

pub struct GithubCi {
    config: Config,
}

impl GithubCi {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(GithubCi {
            config: parse_config(config)?,
        })
    }

    /// The quick action for one item: offered only on a gate that is actually
    /// red, and only while the item still names the pull request whose log is
    /// being asked for (§FS-004-quick-actions.2).
    fn failing_check_actions(&self, item: &Item) -> Vec<ActionConfig> {
        let identified = item.repo().is_some() && item.number().is_some();
        if item.state.as_deref() != Some(FAILING) || !identified {
            return Vec::new();
        }
        vec![show_failing_checks(self.config.host.as_deref())]
    }
}

impl Provider for GithubCi {
    fn name(&self) -> &'static str {
        "github-ci"
    }

    fn available(&self, _ctx: &ProviderContext) -> bool {
        command_exists("gh")
    }

    /// Nothing is offered where the CLI that would answer is not installed:
    /// a menu entry that can only print `gh: not found` is worse than no
    /// entry (§FS-004-quick-actions.2).
    fn quick_actions(&self, item: &Item) -> Vec<ActionConfig> {
        if !command_exists("gh") {
            return Vec::new();
        }
        self.failing_check_actions(item)
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let mut items = Vec::new();
        for repo in &self.config.repos {
            let mut command = gh_command(self.config.host.as_deref());
            command.args([
                "pr",
                "list",
                "--repo",
                repo,
                "--author",
                "@me",
                "--json",
                "number,title,url,updatedAt",
            ]);
            let prs = run_json(command, ctx.timeout, false)?;
            for pr in prs.as_array().cloned().unwrap_or_default() {
                let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
                // `gh pr checks` uses its exit code for check state, so accept
                // non-zero exits and parse stdout regardless.
                let mut checks_cmd = gh_command(self.config.host.as_deref());
                checks_cmd.args([
                    "pr",
                    "checks",
                    &number.to_string(),
                    "--repo",
                    repo,
                    "--json",
                    "name,state,link",
                ]);
                let checks = match run_json(checks_cmd, ctx.timeout, true) {
                    Ok(checks) => checks.as_array().cloned().unwrap_or_default(),
                    // No checks reported (e.g. no CI configured) is not an error.
                    Err(_) => Vec::new(),
                };
                if checks.is_empty() {
                    continue;
                }
                let gate = gate::from_check_states(repo, &checks);
                let (total, passing, failing, pending) =
                    (gate.total(), gate.passed(), gate.failed(), gate.running());
                if failing == 0 && pending == 0 {
                    continue;
                }

                let mut summary = format!("{passing}/{total} checks passing");
                if failing > 0 {
                    summary.push_str(&format!(", {failing} failing"));
                }
                if pending > 0 {
                    summary.push_str(&format!(", {pending} pending"));
                }
                items.push(Item {
                    id: format!("github-ci:{repo}#{number}"),
                    project: ctx.project_id.clone(),
                    source: "github-ci".to_string(),
                    kind: ItemKind::Ci,
                    role: None,
                    title: format!(
                        "#{number} {}: {summary}",
                        pr.get("title").and_then(Value::as_str).unwrap_or("")
                    ),
                    url: pr.get("url").and_then(Value::as_str).map(String::from),
                    state: Some(if failing > 0 { FAILING } else { PENDING }.to_string()),
                    needs_response: failing > 0,
                    updated_at: parse_github_time(pr.get("updatedAt").unwrap_or(&Value::Null)),
                    raw: Value::Null,
                });
            }
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn provider(host: Option<&str>) -> GithubCi {
        GithubCi {
            config: Config {
                provider: "github-ci".to_string(),
                repos: vec!["acme/widget".to_string()],
                host: host.map(String::from),
            },
        }
    }

    fn ci_item(state: &str) -> Item {
        Item {
            id: "github-ci:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-ci".to_string(),
            kind: ItemKind::Ci,
            role: None,
            title: "#42 Retry window: 1/3 checks passing, 2 failing".to_string(),
            url: None,
            state: Some(state.to_string()),
            needs_response: true,
            updated_at: Utc::now(),
            raw: Value::Null,
        }
    }

    #[test]
    fn only_a_red_gate_is_offered_its_log() {
        let provider = provider(None);
        assert_eq!(provider.failing_check_actions(&ci_item(FAILING)).len(), 1);
        // Checks still running have no failure to show yet.
        assert!(provider.failing_check_actions(&ci_item(PENDING)).is_empty());
        // An item that no longer names its pull request cannot be asked about.
        let mut anonymous = ci_item(FAILING);
        anonymous.id = "github-ci:widget".to_string();
        assert!(provider.failing_check_actions(&anonymous).is_empty());
    }

    #[test]
    fn the_enterprise_host_is_exported_rather_than_written_into_the_script() {
        let command = show_failing_checks(Some("ghe.example.com")).command;
        assert!(command.starts_with("export GH_HOST='ghe.example.com'\n"));
        assert!(command.contains("gh run view \"$run\" --repo \"$EPHOR_REPO\" --log-failed"));
        assert!(!show_failing_checks(None).command.contains("GH_HOST"));
    }
}
