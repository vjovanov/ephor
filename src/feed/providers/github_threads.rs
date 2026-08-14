//! Unresolved review threads on the user's open PRs where the last word is
//! not theirs, via the GraphQL API.
//!
//! A review thread is a discussion *of* the pull request, not a subject of its
//! own (§FS-007-matters.3): this source reports the pull request carrying its
//! unresolved threads, so a change with three open threads is one row with
//! three discussions rather than three rows (§FS-003-feed-categories.5).

use serde::Deserialize;
use serde_json::{json, Value};

use crate::feed::model::{Item, ItemKind};
use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::{gh_command, github_login, parse_config, parse_github_time};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    repos: Vec<String>,
    #[serde(default)]
    host: Option<String>,
}

pub struct GithubThreads {
    config: Config,
}

const THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){\
repository(owner:$owner,name:$repo){pullRequest(number:$number){\
reviewThreads(first:50){nodes{isResolved comments(last:10){nodes{\
id author{login} body url updatedAt reactions(first:50){nodes{content user{login}}}}}}}}}}";

impl GithubThreads {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(GithubThreads {
            config: parse_config(config)?,
        })
    }
}

impl Provider for GithubThreads {
    fn name(&self) -> &'static str {
        "github-threads"
    }

    fn available(&self, _ctx: &ProviderContext) -> bool {
        command_exists("gh")
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let login = github_login(ctx, self.config.host.as_deref())?;
        let mut items = Vec::new();
        for repo in &self.config.repos {
            let (owner, name) = repo
                .split_once('/')
                .ok_or_else(|| ProviderError(format!("repo '{repo}' is not owner/name")))?;

            let mut list_cmd = gh_command(self.config.host.as_deref());
            list_cmd.args([
                "pr",
                "list",
                "--repo",
                repo,
                "--author",
                "@me",
                "--json",
                // The url too, now that the report is of the pull request:
                // the row has to be openable.
                "number,title,url",
            ]);
            let prs = run_json(list_cmd, ctx.timeout, false)?;

            for pr in prs.as_array().cloned().unwrap_or_default() {
                let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
                let mut gql = gh_command(self.config.host.as_deref());
                gql.args(["api", "graphql", "-f", &format!("query={THREADS_QUERY}")])
                    .args(["-F", &format!("owner={owner}")])
                    .args(["-F", &format!("repo={name}")])
                    .args(["-F", &format!("number={number}")]);
                let response = run_json(gql, ctx.timeout, false)?;
                let threads = response
                    .pointer("/data/repository/pullRequest/reviewThreads/nodes")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();

                let mut discussions: Vec<Value> = Vec::new();
                let mut latest: Option<Value> = None;
                for thread in threads {
                    if thread
                        .get("isResolved")
                        .and_then(Value::as_bool)
                        .unwrap_or(true)
                    {
                        continue;
                    }
                    let comments = thread
                        .pointer("/comments/nodes")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let comment = comments.last().cloned().unwrap_or(json!({}));
                    let author = comment
                        .pointer("/author/login")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    // Answered: the user wrote the last message, or reacted
                    // to it.
                    if author == login {
                        continue;
                    }
                    let reacted = comment
                        .pointer("/reactions/nodes")
                        .and_then(Value::as_array)
                        .is_some_and(|nodes| {
                            nodes.iter().any(|node| {
                                node.pointer("/user/login").and_then(Value::as_str)
                                    == Some(login.as_str())
                            })
                        });
                    if reacted {
                        continue;
                    }
                    let messages: Vec<Value> = comments
                        .iter()
                        .map(|c| {
                            let mut message = json!({
                                "author": c.pointer("/author/login").and_then(Value::as_str).unwrap_or(""),
                                "text": c.get("body").and_then(Value::as_str).unwrap_or(""),
                                "when": c.get("updatedAt").and_then(Value::as_str).unwrap_or(""),
                            });
                            let reactions = super::github::reactions_json(c.pointer("/reactions/nodes"));
                            if reactions.as_array().is_some_and(|list| !list.is_empty()) {
                                message["reactions"] = reactions;
                            }
                            if let Some(id) = c.get("id").and_then(Value::as_str) {
                                message["react"] =
                                    super::github::target_json(self.config.host.as_deref(), id);
                            }
                            message
                        })
                        .collect();
                    // The thread's own words label it where a pull request has
                    // several (§FS-007-matters.3).
                    let label: String = comment
                        .get("body")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect();
                    discussions.push(json!({
                        "label": format!("{author}: {label}"),
                        "messages": messages,
                    }));
                    let when = comment.get("updatedAt").cloned().unwrap_or(Value::Null);
                    if latest.is_none() {
                        latest = Some(when);
                    }
                }
                if discussions.is_empty() {
                    continue;
                }
                // One report of the pull request, carrying every thread of it
                // still waiting for an answer.
                items.push(Item {
                    id: format!("github-threads:{repo}#{number}"),
                    project: ctx.project_id.clone(),
                    source: "github-threads".to_string(),
                    kind: ItemKind::Pr,
                    role: None,
                    title: pr
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    url: pr.get("url").and_then(Value::as_str).map(String::from),
                    state: None,
                    needs_response: true,
                    updated_at: parse_github_time(latest.as_ref().unwrap_or(&Value::Null)),
                    raw: json!({ "repo": repo, "threads": discussions }),
                });
            }
        }
        Ok(items)
    }
}
