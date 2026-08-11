//! Open PRs on GitHub, split by role:
//! - author: PRs the user opened (`gh search prs --author @me`)
//! - reviewer: PRs the user is engaged with — participating in a thread
//!   (`--commenter @me`) or cited (`--mentions @me`). Merely being
//!   review-requested does not qualify.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::feed::config::ActionConfig;
use crate::feed::gate::{self, Gate};
use crate::feed::model::{Item, ItemKind, ItemRole};
use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::{
    gh_command, github_login, parse_config, parse_github_time, show_failing_checks,
};
use crate::feed::react;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    repos: Vec<String>,
    /// Include PRs the user reviews (is in a thread on, or is cited in).
    #[serde(default)]
    reviews: bool,
    /// Record each PR's gate status (one extra `gh pr checks` call per PR).
    #[serde(default = "crate::feed::providers::enabled")]
    gates: bool,
    #[serde(default)]
    host: Option<String>,
}

pub struct GithubPrs {
    config: Config,
}

/// PR head branch and conversation comments with reactions, for
/// answered-mention detection and thread display (including reaction emoji
/// and posting targets).
const CONVERSATION_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){\
repository(owner:$owner,name:$repo){pullRequest(number:$number){headRefName \
comments(last:30){nodes{id author{login} body createdAt reactions(first:50){nodes{content user{login}}}}}}}}";

impl GithubPrs {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(GithubPrs {
            config: parse_config(config)?,
        })
    }

    fn search(
        &self,
        ctx: &ProviderContext,
        repo: &str,
        role_flag: &str,
    ) -> Result<Vec<Value>, ProviderError> {
        let mut command = gh_command(self.config.host.as_deref());
        command.args([
            "search",
            "prs",
            "--repo",
            repo,
            role_flag,
            "@me",
            "--state",
            "open",
            "--json",
            "number,title,url,updatedAt,isDraft",
        ]);
        let result = run_json(command, ctx.timeout, false)?;
        Ok(result.as_array().cloned().unwrap_or_default())
    }

    /// The PR's review decision and head branch (None on any failure).
    fn pr_details(
        &self,
        ctx: &ProviderContext,
        repo: &str,
        number: u64,
    ) -> (Option<String>, Option<String>) {
        let mut command = gh_command(self.config.host.as_deref());
        command.args([
            "pr",
            "view",
            &number.to_string(),
            "--repo",
            repo,
            "--json",
            "reviewDecision,headRefName",
        ]);
        let Ok(value) = run_json(command, ctx.timeout, true) else {
            return (None, None);
        };
        let decision = value
            .get("reviewDecision")
            .and_then(Value::as_str)
            .map(|decision| decision.to_lowercase())
            .filter(|decision| !decision.is_empty());
        let branch = value
            .get("headRefName")
            .and_then(Value::as_str)
            .filter(|branch| !branch.is_empty())
            .map(String::from);
        (decision, branch)
    }

    /// The PR's checks as a gate summary. A PR without CI (or a failed
    /// lookup) yields an empty gate rather than an error — the gate is
    /// decoration on the PR row, never a reason to lose the PR.
    fn gate(&self, ctx: &ProviderContext, repo: &str, number: u64) -> Gate {
        if !self.config.gates {
            return Gate::default();
        }
        let mut command = gh_command(self.config.host.as_deref());
        command.args([
            "pr",
            "checks",
            &number.to_string(),
            "--repo",
            repo,
            "--json",
            "name,state",
        ]);
        // `gh pr checks` reports check state through its exit code, so accept
        // non-zero exits and read stdout regardless.
        let Ok(checks) = run_json(command, ctx.timeout, true) else {
            return Gate::default();
        };
        gate::from_check_states(
            repo,
            checks.as_array().map(Vec::as_slice).unwrap_or_default(),
        )
    }

    /// The PR's head branch and conversation comments (empty on failure).
    fn conversation(
        &self,
        ctx: &ProviderContext,
        repo: &str,
        number: u64,
    ) -> (Option<String>, Vec<Value>) {
        let Some((owner, name)) = repo.split_once('/') else {
            return (None, Vec::new());
        };
        let mut gql = gh_command(self.config.host.as_deref());
        gql.args([
            "api",
            "graphql",
            "-f",
            &format!("query={CONVERSATION_QUERY}"),
        ])
        .args(["-F", &format!("owner={owner}")])
        .args(["-F", &format!("repo={name}")])
        .args(["-F", &format!("number={number}")]);
        let Ok(response) = run_json(gql, ctx.timeout, false) else {
            return (None, Vec::new());
        };
        let branch = response
            .pointer("/data/repository/pullRequest/headRefName")
            .and_then(Value::as_str)
            .filter(|branch| !branch.is_empty())
            .map(String::from);
        let comments = response
            .pointer("/data/repository/pullRequest/comments/nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        (branch, comments)
    }

    fn base_item(&self, ctx: &ProviderContext, repo: &str, pr: &Value) -> Item {
        let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
        Item {
            id: format!("github-prs:{repo}#{number}"),
            project: ctx.project_id.clone(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: pr
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            url: pr.get("url").and_then(Value::as_str).map(String::from),
            state: None,
            needs_response: false,
            updated_at: parse_github_time(pr.get("updatedAt").unwrap_or(&Value::Null)),
            raw: Value::Null,
        }
    }
}

/// A mention counts as answered when the user commented after the last
/// citing comment, or reacted to it. A mention living only in the PR body
/// counts as answered once the user commented at all.
fn mention_answered(comments: &[Value], login: &str) -> bool {
    let needle = format!("@{login}");
    let mut last_citation: Option<usize> = None;
    for (index, comment) in comments.iter().enumerate() {
        let body = comment.get("body").and_then(Value::as_str).unwrap_or("");
        if body.contains(&needle) {
            last_citation = Some(index);
        }
    }
    let my_comment =
        |comment: &Value| comment.pointer("/author/login").and_then(Value::as_str) == Some(login);
    match last_citation {
        Some(index) => {
            let citing = &comments[index];
            let reacted = citing
                .pointer("/reactions/nodes")
                .and_then(Value::as_array)
                .is_some_and(|nodes| {
                    nodes.iter().any(|node| {
                        node.pointer("/user/login").and_then(Value::as_str) == Some(login)
                    })
                });
            reacted || comments.iter().skip(index + 1).any(my_comment)
        }
        // Mention is in the PR body or a review comment: any comment of
        // ours in the conversation counts as having engaged.
        None => comments.iter().any(my_comment),
    }
}

/// Normalize the conversation into the shared thread display shape.
fn conversation_threads(comments: &[Value], host: Option<&str>) -> Value {
    let messages: Vec<Value> = comments
        .iter()
        .map(|comment| {
            let mut message = json!({
                "author": comment.pointer("/author/login").and_then(Value::as_str).unwrap_or(""),
                "text": comment.get("body").and_then(Value::as_str).unwrap_or(""),
                "when": comment.get("createdAt").and_then(Value::as_str).unwrap_or(""),
            });
            let reactions = react::github_reactions_json(comment.pointer("/reactions/nodes"));
            if reactions.as_array().is_some_and(|list| !list.is_empty()) {
                message["reactions"] = reactions;
            }
            if let Some(id) = comment.get("id").and_then(Value::as_str) {
                message["react"] = react::github_target_json(host, id);
            }
            message
        })
        .collect();
    if messages.is_empty() {
        json!([])
    } else {
        json!([{ "messages": messages }])
    }
}

impl Provider for GithubPrs {
    fn name(&self) -> &'static str {
        "github-prs"
    }

    fn available(&self, _ctx: &ProviderContext) -> bool {
        command_exists("gh")
    }

    /// A pull request whose gate is red is offered its log here, on the row
    /// that shows the red count (§FS-004-quick-actions.4). The condition is
    /// the gate rather than the item's kind: on GitHub the same failure is
    /// reachable through the `github-ci` item as well, and the reader should
    /// not have to know which of the two rows carries the action.
    fn quick_actions(&self, item: &Item) -> Vec<ActionConfig> {
        let red = Gate::of(item).is_some_and(|gate| gate.is_red());
        let identified = item.repo().is_some() && item.number().is_some();
        if !red || !identified || !command_exists("gh") {
            return Vec::new();
        }
        vec![show_failing_checks(self.config.host.as_deref())]
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let mut items: Vec<Item> = Vec::new();
        for repo in &self.config.repos {
            for pr in self.search(ctx, repo, "--author")? {
                let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
                let (decision, branch) = self.pr_details(ctx, repo, number);
                let changes_requested = decision.as_deref() == Some("changes_requested");
                let mut item = self.base_item(ctx, repo, &pr);
                let mut raw = serde_json::Map::new();
                if let Some(branch) = branch {
                    raw.insert("branch".to_string(), json!(branch));
                }
                let gate = self.gate(ctx, repo, number);
                if !gate.is_empty() {
                    raw.insert("gate".to_string(), gate.to_value());
                }
                if !raw.is_empty() {
                    item.raw = Value::Object(raw);
                }
                item.role = Some(ItemRole::Author);
                item.state = Some(match &decision {
                    Some(decision) => format!("open:{decision}"),
                    None => "open".to_string(),
                });
                item.needs_response = changes_requested;
                items.push(item);
            }

            if !self.config.reviews {
                continue;
            }
            let authored: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
            let mentions = self.search(ctx, repo, "--mentions")?;
            let mentioned: HashSet<u64> = mentions
                .iter()
                .filter_map(|pr| pr.get("number").and_then(Value::as_u64))
                .collect();
            let mut review_prs = self.search(ctx, repo, "--commenter")?;
            for pr in mentions {
                let number = pr.get("number").and_then(Value::as_u64);
                let already = review_prs
                    .iter()
                    .any(|other| other.get("number").and_then(Value::as_u64) == number);
                if !already {
                    review_prs.push(pr);
                }
            }
            let login = if review_prs.is_empty() {
                None
            } else {
                github_login(ctx, self.config.host.as_deref()).ok()
            };
            for pr in review_prs {
                let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
                let mut item = self.base_item(ctx, repo, &pr);
                if authored.contains(&item.id) {
                    continue;
                }
                let (branch, comments) = self.conversation(ctx, repo, number);
                let cited = mentioned.contains(&number);
                let answered = cited
                    && login
                        .as_deref()
                        .is_some_and(|login| mention_answered(&comments, login));
                item.raw = json!({
                    "branch": branch,
                    "threads": conversation_threads(&comments, self.config.host.as_deref())
                });
                let gate = self.gate(ctx, repo, number);
                if !gate.is_empty() {
                    item.raw["gate"] = gate.to_value();
                }
                item.role = Some(ItemRole::Reviewer);
                item.state = Some(
                    if cited {
                        "open:mentioned"
                    } else {
                        "open:in-thread"
                    }
                    .to_string(),
                );
                item.needs_response = cited && !answered;
                items.push(item);
            }
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::gate::RepoGate;
    use crate::feed::provider::command_exists;

    fn provider() -> GithubPrs {
        GithubPrs {
            config: Config {
                provider: "github-prs".to_string(),
                repos: vec!["acme/widget".to_string()],
                reviews: true,
                gates: true,
                host: None,
            },
        }
    }

    fn pr(gate: Option<Gate>) -> Item {
        let raw = match gate {
            Some(gate) => json!({ "gate": gate.to_value() }),
            None => json!({}),
        };
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: Some(ItemRole::Author),
            title: "Widen the retry window".to_string(),
            url: None,
            state: Some("open".to_string()),
            needs_response: false,
            updated_at: parse_github_time(&Value::Null),
            raw,
        }
    }

    fn gate(passed: u64, failed: u64) -> Gate {
        Gate {
            repos: vec![RepoGate {
                repo: "acme/widget".to_string(),
                passed,
                failed,
                running: 0,
            }],
            ..Gate::default()
        }
    }

    /// The action belongs to the row showing the red count, not to a separate
    /// CI item the reader has to go and find (§FS-004-quick-actions.4).
    #[test]
    fn a_pull_request_with_a_red_gate_is_offered_its_log() {
        let provider = provider();
        // Only where `gh` is installed to answer at all — the same condition
        // the github-ci item's action is under (§FS-004-quick-actions.2).
        let expected = usize::from(command_exists("gh"));
        assert_eq!(
            provider.quick_actions(&pr(Some(gate(1, 2)))).len(),
            expected
        );
        if expected == 1 {
            assert_eq!(
                provider.quick_actions(&pr(Some(gate(1, 2))))[0].description,
                "see the CI failures"
            );
        }

        // A green gate, and a pull request whose gate was never recorded, have
        // nothing to show.
        assert!(provider.quick_actions(&pr(Some(gate(3, 0)))).is_empty());
        assert!(provider.quick_actions(&pr(None)).is_empty());
    }
}
