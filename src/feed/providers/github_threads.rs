//! Unresolved review threads on the user's open PRs where the last word is
//! not theirs, via the GraphQL API.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::feed::model::{Item, ItemKind};
use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::{gh_command, github_login, parse_config, parse_github_time};
use crate::feed::react;

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
                "number,title",
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
                    let url = comment.get("url").and_then(Value::as_str).unwrap_or("");
                    let body = comment.get("body").and_then(Value::as_str).unwrap_or("");
                    let first_line: String = body
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect();
                    let messages: Vec<Value> = comments
                        .iter()
                        .map(|c| {
                            let mut message = json!({
                                "author": c.pointer("/author/login").and_then(Value::as_str).unwrap_or(""),
                                "text": c.get("body").and_then(Value::as_str).unwrap_or(""),
                                "when": c.get("updatedAt").and_then(Value::as_str).unwrap_or(""),
                            });
                            let reactions = react::github_reactions_json(c.pointer("/reactions/nodes"));
                            if reactions.as_array().is_some_and(|list| !list.is_empty()) {
                                message["reactions"] = reactions;
                            }
                            if let Some(id) = c.get("id").and_then(Value::as_str) {
                                message["react"] =
                                    react::github_target_json(self.config.host.as_deref(), id);
                            }
                            message
                        })
                        .collect();
                    items.push(Item {
                        id: format!("github-threads:{url}"),
                        project: ctx.project_id.clone(),
                        source: "github-threads".to_string(),
                        kind: ItemKind::Message,
                        role: None,
                        title: format!("#{number} {author}: {first_line}"),
                        url: Some(url.to_string()),
                        state: Some("unresolved".to_string()),
                        needs_response: true,
                        updated_at: parse_github_time(
                            comment.get("updatedAt").unwrap_or(&Value::Null),
                        ),
                        raw: json!({ "threads": [{ "messages": messages }] }),
                    });
                }
            }
        }
        Ok(items)
    }
}
