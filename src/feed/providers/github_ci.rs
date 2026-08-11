//! CI check status for the user's open PRs via `gh pr checks`. Emits one
//! item per PR that has failing or pending checks.

use serde::Deserialize;
use serde_json::Value;

use crate::feed::gate;
use crate::feed::model::{Item, ItemKind};
use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::{gh_command, parse_config, parse_github_time};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    repos: Vec<String>,
    #[serde(default)]
    host: Option<String>,
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
}

impl Provider for GithubCi {
    fn name(&self) -> &'static str {
        "github-ci"
    }

    fn available(&self, _ctx: &ProviderContext) -> bool {
        command_exists("gh")
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
                    state: Some(if failing > 0 { "failing" } else { "pending" }.to_string()),
                    needs_response: failing > 0,
                    updated_at: parse_github_time(pr.get("updatedAt").unwrap_or(&Value::Null)),
                    raw: Value::Null,
                });
            }
        }
        Ok(items)
    }
}
