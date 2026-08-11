//! Issues on GitHub, split by role (§FS-001-forge-interface.1):
//! - author: issues the user opened (`gh search issues --author @me`)
//! - participant: issues the user is otherwise involved in — commented on,
//!   assigned, or mentioned in (`gh search issues --involves @me`)
//!
//! Unlike the pull request providers, the search is not repository-scoped
//! unless it is asked to be: with no `repos` the whole forge is searched, so an
//! issue filed against a stranger's project is followed like any other. Closed
//! issues come back too — a closing is usually the activity worth seeing — and
//! land under Recent (§FS-003-feed-categories.2). What bounds the search
//! instead of a repository list is time: `updated_within_days`.

use std::collections::HashSet;

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::feed::model::Item;
use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::{gh_command, github_login, parse_config, parse_github_time};
use crate::feed::react;
use crate::forge::{policy, Issue, Message, Role};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    /// Repositories to search, as `owner/name`. Empty — the default — searches
    /// the whole forge for the user's issues wherever they are.
    #[serde(default)]
    repos: Vec<String>,
    /// Include issues the user takes part in but did not open.
    #[serde(default = "crate::feed::providers::enabled")]
    participating: bool,
    /// How far back to look. Older issues are not fetched at all, whatever
    /// their state; this is what keeps a forge-wide search bounded. Zero
    /// removes the bound.
    #[serde(default = "default_window_days")]
    updated_within_days: u64,
    /// Maximum issues per search, per role.
    #[serde(default = "default_limit")]
    limit: u32,
    /// Fetch each issue's comments, for the thread view and for deciding
    /// whether the issue awaits a reply. One API call per issue that has any.
    #[serde(default = "crate::feed::providers::enabled")]
    comments: bool,
    #[serde(default)]
    host: Option<String>,
}

fn default_window_days() -> u64 {
    30
}

fn default_limit() -> u32 {
    30
}

pub struct GithubIssues {
    config: Config,
}

const COMMENTS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){\
repository(owner:$owner,name:$repo){issue(number:$number){\
comments(last:30){nodes{id author{login} body createdAt \
reactions(first:50){nodes{content user{login}}}}}}}}";

/// Search fields. `commentsCount` is what lets us skip the comment fetch for
/// the many issues that have none — the difference between one API call and
/// sixty on a forge-wide refresh.
const SEARCH_FIELDS: &str = "number,title,url,updatedAt,state,repository,commentsCount";

impl GithubIssues {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(GithubIssues {
            config: parse_config(config)?,
        })
    }

    /// One search for one role. `role_flag` is `--author` or `--involves`.
    fn search(&self, ctx: &ProviderContext, role_flag: &str) -> Result<Vec<Value>, ProviderError> {
        let mut command = gh_command(self.config.host.as_deref());
        command.args(["search", "issues", role_flag, "@me"]);
        for repo in &self.config.repos {
            command.args(["--repo", repo]);
        }
        if self.config.updated_within_days > 0 {
            let since = Utc::now() - Duration::days(self.config.updated_within_days as i64);
            command.args(["--updated", &format!(">={}", since.format("%Y-%m-%d"))]);
        }
        command
            .args(["--sort", "updated", "--order", "desc"])
            .args(["--limit", &self.config.limit.to_string()])
            .args(["--json", SEARCH_FIELDS]);
        let result = run_json(command, ctx.timeout, false)?;
        Ok(result.as_array().cloned().unwrap_or_default())
    }

    /// An issue's comments as interface messages. Failure yields none: a
    /// conversation we could not read is not a reason to lose the issue.
    fn messages(
        &self,
        ctx: &ProviderContext,
        repo: &str,
        number: u64,
        login: &str,
    ) -> Vec<Message> {
        let Some((owner, name)) = repo.split_once('/') else {
            return Vec::new();
        };
        let mut gql = gh_command(self.config.host.as_deref());
        gql.args(["api", "graphql", "-f", &format!("query={COMMENTS_QUERY}")])
            .args(["-F", &format!("owner={owner}")])
            .args(["-F", &format!("repo={name}")])
            .args(["-F", &format!("number={number}")]);
        let Ok(response) = run_json(gql, ctx.timeout, false) else {
            return Vec::new();
        };
        response
            .pointer("/data/repository/issue/comments/nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|node| message(node, login, self.config.host.as_deref()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One search result as an interface issue, comments included when it has
    /// any and we were asked to read them.
    fn issue(
        &self,
        ctx: &ProviderContext,
        found: &Value,
        role: Role,
        login: &str,
    ) -> Option<Issue> {
        let number = found.get("number").and_then(Value::as_u64)?;
        let url = found.get("url").and_then(Value::as_str).map(String::from);
        let repo = repo_of(found)?;
        let has_comments = found
            .get("commentsCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0;
        let messages = if self.config.comments && has_comments {
            self.messages(ctx, &repo, number, login)
        } else {
            Vec::new()
        };
        Some(Issue {
            key: format!("{repo}#{number}"),
            title: found
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            status: found
                .get("state")
                .and_then(Value::as_str)
                .filter(|state| !state.is_empty())
                .map(|state| state.to_lowercase()),
            url,
            updated_at: parse_github_time(found.get("updatedAt").unwrap_or(&Value::Null)),
            role,
            messages,
        })
    }
}

/// `owner/name` for a search result: from `repository.nameWithOwner`, or from
/// the issue url for a host that does not fill it in.
fn repo_of(found: &Value) -> Option<String> {
    if let Some(name) = found
        .pointer("/repository/nameWithOwner")
        .and_then(Value::as_str)
        .filter(|name| name.contains('/'))
    {
        return Some(name.to_string());
    }
    let url = found.get("url").and_then(Value::as_str)?;
    let path = url.split("://").nth(1)?;
    let mut segments = path.split('/').skip(1);
    let owner = segments.next().filter(|part| !part.is_empty())?;
    let name = segments.next().filter(|part| !part.is_empty())?;
    Some(format!("{owner}/{name}"))
}

/// One GraphQL comment node as an interface message. Whether it is the user's
/// own is decided here — policy stays identity-agnostic
/// (§FS-001-forge-interface.3).
fn message(node: &Value, login: &str, host: Option<&str>) -> Message {
    let author = node
        .pointer("/author/login")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let reactions = react::github_reactions_json(node.pointer("/reactions/nodes"));
    Message {
        mine: author == login,
        author,
        text: node
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        when: node
            .get("createdAt")
            .and_then(Value::as_str)
            .and_then(|when| chrono::DateTime::parse_from_rfc3339(when).ok())
            .map(|when| when.with_timezone(&Utc)),
        reactions: serde_json::from_value(reactions)
            .map(|list: Vec<crate::forge::Reaction>| {
                list.into_iter()
                    .map(|mut reaction| {
                        reaction.mine = reaction.users.iter().any(|user| user == login);
                        reaction
                    })
                    .collect()
            })
            .unwrap_or_default(),
        react: node
            .get("id")
            .and_then(Value::as_str)
            .map(|id| react::github_target_json(host, id))
            .unwrap_or(Value::Null),
    }
}

impl Provider for GithubIssues {
    fn name(&self) -> &'static str {
        "github-issues"
    }

    fn available(&self, _ctx: &ProviderContext) -> bool {
        command_exists("gh")
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let login = github_login(ctx, self.config.host.as_deref())?;
        let mut items: Vec<Item> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Author first, so an issue the user opened is theirs even though the
        // involves search returns it too.
        let mut searches = vec![(Role::Author, "--author")];
        if self.config.participating {
            searches.push((Role::Reviewer, "--involves"));
        }
        for (role, role_flag) in searches {
            for found in self.search(ctx, role_flag)? {
                let Some(issue) = self.issue(ctx, &found, role, &login) else {
                    continue;
                };
                if !seen.insert(issue.key.clone()) {
                    continue;
                }
                items.push(policy::issue_item("github-issues", &ctx.project_id, &issue));
            }
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_repository_comes_from_the_field_or_falls_back_to_the_url() {
        let named = json!({ "repository": { "nameWithOwner": "acme/widget" } });
        assert_eq!(repo_of(&named).as_deref(), Some("acme/widget"));

        let from_url = json!({ "url": "https://github.com/earendil-works/pi/issues/7951" });
        assert_eq!(repo_of(&from_url).as_deref(), Some("earendil-works/pi"));

        // An enterprise host has extra path depth in the origin, not the path.
        let enterprise = json!({ "url": "https://git.example.com/team/app/issues/3" });
        assert_eq!(repo_of(&enterprise).as_deref(), Some("team/app"));

        assert_eq!(repo_of(&json!({})), None);
    }

    #[test]
    fn a_comment_node_becomes_a_message_that_knows_whose_it_is() {
        let node = json!({
            "id": "IC_1",
            "author": { "login": "me" },
            "body": "on it",
            "createdAt": "2026-08-01T10:00:00Z",
            "reactions": { "nodes": [
                { "content": "THUMBS_UP", "user": { "login": "me" } },
                { "content": "ROCKET", "user": { "login": "them" } }
            ] }
        });
        let message = message(&node, "me", None);
        assert!(message.mine);
        assert_eq!(message.text, "on it");
        assert_eq!(message.reactions[0].emoji, "👍");
        assert!(message.reactions[0].mine);
        assert!(!message.reactions[1].mine);
        assert_eq!(message.react["subject_id"], "IC_1");

        let theirs = message_of("them");
        assert!(!theirs.mine);
    }

    fn message_of(author: &str) -> Message {
        message(
            &json!({ "author": { "login": author }, "body": "" }),
            "me",
            None,
        )
    }

    #[test]
    fn a_closed_issue_is_still_reported_and_lands_under_recent() {
        let config = json!({ "provider": "github-issues" });
        let provider = GithubIssues::from_config(&config).unwrap();
        // No repos: the whole forge. No state filter: closed included.
        assert!(provider.config.repos.is_empty());
        assert_eq!(provider.config.updated_within_days, 30);
        assert!(provider.config.participating);

        let issue = Issue {
            key: "earendil-works/pi#7951".to_string(),
            title: "OSC 8 hyperlinks disabled on VTE terminals".to_string(),
            status: Some("closed".to_string()),
            url: None,
            updated_at: Utc::now(),
            role: Role::Author,
            messages: Vec::new(),
        };
        let item = policy::issue_item("github-issues", "hub", &issue);
        assert_eq!(item.id, "github-issues:earendil-works/pi#7951");
        assert!(item.is_finished());
        assert!(item.within_recent_window(Utc::now(), 7));
    }
}
