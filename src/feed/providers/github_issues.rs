//! Issues on GitHub, split by role (§FS-001-forge-interface.1):
//! - author: issues the user opened (`gh search issues --author @me`)
//! - participant: issues the user is otherwise involved in — commented on,
//!   assigned, or mentioned in (`gh search issues --involves @me`)
//!
//! A source may also **follow a label**: `gh search issues --label <name>
//! --state open` reports the open issues carrying it whoever is in them, which
//! is the one question the two role searches cannot ask — being nobody in an
//! issue is exactly what they filter out. Such an issue is reported under the
//! role its `author.login` says the reader holds.
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
    /// Include issues the user opened. On unless switched off — it is the
    /// search this source began as; off is for a block that follows labels
    /// alone (§FS-001-forge-interface.1).
    #[serde(default = "crate::feed::providers::enabled")]
    authored: bool,
    /// Include issues the user takes part in but did not open.
    #[serde(default = "crate::feed::providers::enabled")]
    participating: bool,
    /// Labels to follow: the open issues carrying any of them are reported
    /// whoever is in them (§FS-001-forge-interface.1). One search per label,
    /// and a search that comes back full fails rather than answering in part.
    #[serde(default)]
    labels: Vec<String>,
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
    /// Count an issue nobody has taken as work awaiting somebody
    /// (§FS-003-feed-categories.4). Off unless asked for: it is the right
    /// reading of a backlog you are answerable for, and the wrong reading of
    /// every repository you have ever commented in.
    #[serde(default)]
    unclaimed: bool,
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
///
/// `author` is what a label search reads the role off: it asked about the
/// work, not about the reader, so who opened the issue is the only thing that
/// says whose it is (§FS-001-forge-interface.1).
const SEARCH_FIELDS: &str =
    "number,title,url,updatedAt,state,repository,commentsCount,assignees,author";

/// One question a search asks of the forge. The two role questions know the
/// reader's role by construction; the label question does not.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Question<'a> {
    /// Issues the user opened.
    Authored,
    /// Issues the user takes part in but did not open.
    Involves,
    /// Open issues carrying this label, whoever is in them.
    Labelled(&'a str),
}

impl Question<'_> {
    /// The role the reader holds on an issue this question found. A label
    /// search reads it from who opened the issue, so a followed issue lands
    /// where its kind lands (§FS-003-feed-categories.1).
    fn role(&self, found: &Value, login: &str) -> Role {
        match self {
            Question::Authored => Role::Author,
            Question::Involves => Role::Reviewer,
            Question::Labelled(_) => {
                let author = found.pointer("/author/login").and_then(Value::as_str);
                if author == Some(login) {
                    Role::Author
                } else {
                    Role::Reviewer
                }
            }
        }
    }
}

/// A label search that returned as many issues as it was allowed to has not
/// answered: it delivered a prefix nobody can size, and a queue shown as a
/// fraction of itself reads as the whole queue (§FS-001-forge-interface.6).
fn label_search_is_full(label: &str, found: usize, limit: u32) -> Result<(), ProviderError> {
    if found as u64 >= u64::from(limit) {
        return Err(ProviderError(format!(
            "{found} open issues carry the label `{label}`, which is this source's `limit` — \
             raise `limit` or narrow `labels`, rather than be shown an unknown fraction of them"
        )));
    }
    Ok(())
}

impl GithubIssues {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        let config: Config = parse_config(config)?;
        // A source that asks nothing would answer "nothing" forever, and an
        // empty section has to mean there is nothing waiting
        // (§FS-001-forge-interface.6).
        if !config.authored && !config.participating && config.labels.is_empty() {
            return Err(ProviderError(
                "github-issues asks nothing: `authored` and `participating` are both off and \
                 `labels` is empty — turn one on, name a label to follow, or drop the block"
                    .to_string(),
            ));
        }
        Ok(GithubIssues { config })
    }

    /// What this source asked ephor to make of an issue nobody has taken.
    fn unclaimed(&self) -> policy::Unclaimed {
        if self.config.unclaimed {
            policy::Unclaimed::Awaits
        } else {
            policy::Unclaimed::Ignored
        }
    }

    /// The `gh` arguments one question builds. Kept apart from running it so
    /// a test can read the question off the command line. The label is passed
    /// as its own argument, so `gh` receives it whatever is in it and no shell
    /// ever sees it.
    fn search_args(&self, question: Question<'_>) -> Vec<String> {
        let mut args = vec!["search".to_string(), "issues".to_string()];
        match question {
            Question::Authored => args.extend(["--author".to_string(), "@me".to_string()]),
            Question::Involves => args.extend(["--involves".to_string(), "@me".to_string()]),
            // Following a label is following work, so only what is open is
            // asked for: the closed would spend the limit on history
            // (§FS-001-forge-interface.1).
            Question::Labelled(label) => args.extend([
                "--label".to_string(),
                label.to_string(),
                "--state".to_string(),
                "open".to_string(),
            ]),
        }
        for repo in &self.config.repos {
            args.extend(["--repo".to_string(), repo.clone()]);
        }
        if self.config.updated_within_days > 0 {
            let since = Utc::now() - Duration::days(self.config.updated_within_days as i64);
            args.extend([
                "--updated".to_string(),
                format!(">={}", since.format("%Y-%m-%d")),
            ]);
        }
        args.extend(["--sort".to_string(), "updated".to_string()]);
        args.extend(["--order".to_string(), "desc".to_string()]);
        args.extend(["--limit".to_string(), self.config.limit.to_string()]);
        args.extend(["--json".to_string(), SEARCH_FIELDS.to_string()]);
        args
    }

    /// One search for one question.
    fn search(
        &self,
        ctx: &ProviderContext,
        question: Question<'_>,
    ) -> Result<Vec<Value>, ProviderError> {
        let mut command = gh_command(self.config.host.as_deref());
        command.args(self.search_args(question));
        let result = run_json(command, ctx.timeout, false)?;
        let found = result.as_array().cloned().unwrap_or_default();
        // Only the label searches are held to this. The role searches have
        // always truncated silently and are left as they are.
        if let Question::Labelled(label) = question {
            label_search_is_full(label, found.len(), self.config.limit)?;
        }
        Ok(found)
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
            // Reported as a fact whatever the source asked for; what it means
            // is the policy's to decide (§FS-001-forge-interface.3). Absent
            // from the search result is not "nobody has it" — it is a field
            // that did not come back, so it stays unsaid.
            assigned: found
                .get("assignees")
                .and_then(Value::as_array)
                .map(|assignees| !assignees.is_empty()),
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
    let reactions = super::github::reactions_json(node.pointer("/reactions/nodes"));
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
            .map(|id| super::github::target_json(host, id))
            .unwrap_or(Value::Null),
        // GitHub tracks no task on an issue comment; a checklist there is
        // prose in the body, not something the forge holds a state for.
        task: Value::Null,
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
        // involves search — and a label search — return it too.
        let mut questions: Vec<Question> = Vec::new();
        if self.config.authored {
            questions.push(Question::Authored);
        }
        if self.config.participating {
            questions.push(Question::Involves);
        }
        questions.extend(self.config.labels.iter().map(|l| Question::Labelled(l)));
        for question in questions {
            for found in self.search(ctx, question)? {
                let role = question.role(&found, &login);
                let Some(issue) = self.issue(ctx, &found, role, &login) else {
                    continue;
                };
                if !seen.insert(issue.key.clone()) {
                    continue;
                }
                items.push(policy::issue_item(
                    "github-issues",
                    &ctx.project_id,
                    &issue,
                    self.unclaimed(),
                ));
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
            assigned: None,
            messages: Vec::new(),
        };
        let item = policy::issue_item("github-issues", "hub", &issue, policy::Unclaimed::Ignored);
        assert_eq!(item.id, "github-issues:earendil-works/pi#7951");
        assert!(item.is_finished());
        assert!(item.within_recent_window(Utc::now(), 7));
    }

    /// The search that had no switch gets one, and it is on
    /// (§FS-001-forge-interface.1): a block written before labels existed asks
    /// exactly what it asked before.
    #[test]
    fn a_block_that_names_no_label_still_asks_both_role_questions() {
        let provider = GithubIssues::from_config(&json!({ "provider": "github-issues" })).unwrap();
        assert!(provider.config.authored);
        assert!(provider.config.participating);
        assert!(provider.config.labels.is_empty());
    }

    /// A source that asks nothing would answer "nothing" forever, and an empty
    /// section has to mean there is nothing waiting (§FS-001-forge-interface.6).
    #[test]
    fn a_source_that_asks_nothing_is_refused_rather_than_run() {
        let err = GithubIssues::from_config(&json!({
            "provider": "github-issues",
            "authored": false,
            "participating": false
        }))
        .err()
        .expect("a block asking nothing is refused");
        assert!(err.0.contains("asks nothing"), "{}", err.0);
        assert!(err.0.contains("labels"), "{}", err.0);

        // One label is enough to be a question.
        assert!(GithubIssues::from_config(&json!({
            "provider": "github-issues",
            "authored": false,
            "participating": false,
            "labels": ["priority"]
        }))
        .is_ok());
    }

    #[test]
    fn each_question_puts_its_own_flags_on_the_command_line() {
        let provider = GithubIssues::from_config(&json!({
            "provider": "github-issues",
            "repos": ["oracle/graalvm-reachability-metadata"],
            "updated_within_days": 0,
            "limit": 40,
            "labels": ["high priority"]
        }))
        .unwrap();

        let authored = provider.search_args(Question::Authored);
        assert_eq!(&authored[..4], &["search", "issues", "--author", "@me"]);
        let involves = provider.search_args(Question::Involves);
        assert_eq!(&involves[..4], &["search", "issues", "--involves", "@me"]);

        // A label search names the label and asks for the open only; no role
        // flag, since it asks about the work rather than about the reader
        // (§FS-001-forge-interface.1). The label is one argument, spaces and
        // all — no shell ever sees it.
        let labelled = provider.search_args(Question::Labelled("high priority"));
        assert_eq!(
            &labelled[..6],
            &[
                "search",
                "issues",
                "--label",
                "high priority",
                "--state",
                "open"
            ]
        );
        assert!(!labelled
            .iter()
            .any(|arg| arg == "--author" || arg == "--involves"));

        // The bounds every question carries are the same ones.
        for args in [&authored, &involves, &labelled] {
            assert!(args
                .windows(2)
                .any(|pair| pair == ["--repo", "oracle/graalvm-reachability-metadata"]));
            assert!(args.windows(2).any(|pair| pair == ["--sort", "updated"]));
            assert!(args.windows(2).any(|pair| pair == ["--limit", "40"]));
            assert!(args
                .windows(2)
                .any(|pair| pair == ["--json", SEARCH_FIELDS]));
            // `updated_within_days: 0` removes the bound for all of them.
            assert!(!args.iter().any(|arg| arg == "--updated"));
        }

        // The window, where it is asked for, is on every question too.
        let bounded = GithubIssues::from_config(&json!({
            "provider": "github-issues",
            "labels": ["priority"]
        }))
        .unwrap();
        assert!(bounded
            .search_args(Question::Labelled("priority"))
            .iter()
            .any(|arg| arg == "--updated"));
    }

    /// A label search asked about the work, so whose the issue is comes from
    /// who opened it (§FS-001-forge-interface.1).
    #[test]
    fn a_followed_issue_takes_the_role_of_whoever_opened_it() {
        let mine = json!({ "author": { "login": "me" } });
        let theirs = json!({ "author": { "login": "them" } });
        assert_eq!(
            Question::Labelled("priority").role(&mine, "me"),
            Role::Author
        );
        assert_eq!(
            Question::Labelled("priority").role(&theirs, "me"),
            Role::Reviewer
        );
        // An issue whose author did not come back is nobody's but the
        // participant's — never silently the reader's.
        assert_eq!(
            Question::Labelled("priority").role(&json!({}), "me"),
            Role::Reviewer
        );
        // The role questions still know their answer by construction.
        assert_eq!(Question::Authored.role(&theirs, "me"), Role::Author);
        assert_eq!(Question::Involves.role(&mine, "me"), Role::Reviewer);
    }

    /// A label search as full as it was allowed to be has not answered
    /// (§FS-001-forge-interface.6).
    #[test]
    fn a_label_search_that_fills_its_limit_fails_rather_than_answers_in_part() {
        assert!(label_search_is_full("priority", 16, 30).is_ok());
        let err = label_search_is_full("priority", 30, 30)
            .err()
            .expect("a full page is not an answer");
        assert!(err.0.contains("30 open issues"), "{}", err.0);
        assert!(err.0.contains("priority"), "{}", err.0);
        assert!(err.0.contains("raise `limit`"), "{}", err.0);
        assert!(err.0.contains("narrow `labels`"), "{}", err.0);
    }
}
