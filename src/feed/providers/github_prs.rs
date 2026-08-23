//! Pull requests on GitHub, by every role that puts one in front of the user
//! (§FS-001-forge-interface.1):
//!
//! - **authored** — `author:@me`
//! - **in a thread** — `commenter:@me`
//! - **cited** — `mentions:@me`
//! - **review requested** — `review-requested:@me`
//! - **assigned** — `assignee:@me`
//!
//! The last two matter most and are the easiest to miss: being asked leaves
//! nothing behind in the conversation, so a pull request waiting on the user
//! looks, to every search that reads what they have said, exactly like one
//! with nothing to do with them. All five are one request, aliased side by
//! side on the graph rather than spent on search (§FS-001-forge-interface.8).
//!
//! Like `github-issues`, the search is repository-scoped only when it is asked
//! to be: with no `repos` the whole forge is searched, and what bounds it is
//! `updated_within_days` instead. Finished pull requests come back too and
//! land under Recent (§FS-003-feed-categories.2) — a question asked of the
//! user does not stop being asked when the branch lands — but nothing more is
//! fetched about them, since a merged pull request asks nothing of anyone.
//!
//! What they *mean* is not decided here: this provider reports roles, reasons,
//! conversation, gate, and the review the user left, and `policy` turns them
//! into feed items (§FS-001-forge-interface.3).

use std::collections::BTreeMap;

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::feed::config::ActionConfig;
use crate::feed::gate::{self, Gate};
use crate::feed::model::Item;
use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::github;
use crate::feed::providers::{
    gh_command, github_login, parse_config, parse_github_time, restart_actions, show_failing_checks,
};
use crate::forge::{policy, Message, PullRequest, Reason, Review, Role, Thread};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    /// Repositories to search, as `owner/name`. Empty — the default — searches
    /// the whole forge for the user's pull requests wherever they are.
    #[serde(default)]
    repos: Vec<String>,
    /// Include pull requests the user did not open: ones they are in a thread
    /// on, cited in, asked to review, or assigned.
    #[serde(default = "crate::feed::providers::enabled")]
    reviews: bool,
    /// Record each pull request's gate status (one extra `gh pr checks` call
    /// per unfinished pull request).
    #[serde(default = "crate::feed::providers::enabled")]
    gates: bool,
    /// How far back to look. This is what keeps a forge-wide search bounded,
    /// and what decides how much finished work Recent can draw on. Zero
    /// removes the bound.
    #[serde(default = "default_window_days")]
    updated_within_days: u64,
    /// Maximum pull requests per search, per role.
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    host: Option<String>,
}

fn default_window_days() -> u64 {
    30
}

fn default_limit() -> u32 {
    30
}

pub struct GithubPrs {
    config: Config,
}

/// Head branch, conversation, review threads, and the user's own verdict in one
/// call. The review threads are here and not only in `github-threads` because a
/// citation is answered wherever the answer was written: a reply left on a line
/// of the diff is an answer, and a rule reading only the conversation tab would
/// go on reporting the citation as unanswered forever.
///
/// `latestReviews` is the last review from each reviewer, which is the only
/// form of the question worth asking (§FS-001-forge-interface.1): a reviewer's
/// history is not the fact, their current verdict is. It rides on the call that
/// was being made anyway, so reading it costs nothing.
const CONVERSATION_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){\
repository(owner:$owner,name:$repo){pullRequest(number:$number){id headRefName \
latestReviews(first:30){nodes{state author{login}}} \
comments(last:30){nodes{id author{login} body createdAt reactions(first:50){nodes{content user{login}}}}} \
reviewThreads(first:50){nodes{comments(last:20){nodes{\
id author{login} body createdAt reactions(first:50){nodes{content user{login}}}}}}}}}}";

/// What a search reports about a pull request. `state` is what tells a merged
/// pull request from an open one, and `repository` is what makes a forge-wide
/// search usable at all.
///
/// `headRefName` and `reviewDecision` are here because on this transport they
/// are free: the graph hands them over with the row, where asking per pull
/// request was a second request each, spent down the reader's longest list
/// (§FS-001-forge-interface.8.3).
const SEARCH_SELECTION: &str = "... on PullRequest{\
number title url updatedAt state headRefName reviewDecision \
repository{nameWithOwner}}";

/// The searches that put a pull request in front of the user, and the reason
/// each one reports (§FS-001-forge-interface.1). Each stays its own question —
/// they ride in one request, and a role still answers for itself
/// (§FS-001-forge-interface.8.1).
const AUTHORED: (Reason, &str) = (Reason::Authored, "author");
const REVIEWING: [(Reason, &str); 4] = [
    (Reason::InThread, "commenter"),
    (Reason::Mentioned, "mentions"),
    (Reason::ReviewRequested, "review-requested"),
    (Reason::Assigned, "assignee"),
];

impl GithubPrs {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(GithubPrs {
            config: parse_config(config)?,
        })
    }

    /// What bounds every one of this source's searches: the kind, the
    /// repositories, and the window. Repository qualifiers are OR'd by the
    /// forge, so a source watching several repositories asks about all of them
    /// in one question rather than one question each — with none configured
    /// the whole forge is searched and the window is the only bound.
    fn bounds(&self) -> String {
        let mut bounds = String::from("is:pr");
        for repo in &self.config.repos {
            bounds.push_str(&format!(" repo:{repo}"));
        }
        if self.config.updated_within_days > 0 {
            let since = Utc::now() - Duration::days(self.config.updated_within_days as i64);
            bounds.push_str(&format!(" updated:>={}", since.format("%Y-%m-%d")));
        }
        bounds.push_str(" sort:updated-desc");
        bounds
    }

    /// Every role, asked in one request (§FS-001-forge-interface.8). The
    /// answers come back per role and in the order asked, so what the caller
    /// sees is what a search per role gave it before.
    fn search(
        &self,
        ctx: &ProviderContext,
        roles: &[(Reason, &str)],
    ) -> Result<Vec<(Reason, Vec<Value>)>, ProviderError> {
        let bounds = self.bounds();
        let searches: Vec<github::Search> = roles
            .iter()
            .enumerate()
            .map(|(index, (_, qualifier))| github::Search {
                alias: format!("r{index}"),
                query: format!("{bounds} {qualifier}:@me"),
            })
            .collect();
        let mut answers = github::search(
            self.config.host.as_deref(),
            &searches,
            SEARCH_SELECTION,
            self.config.limit,
            ctx.timeout,
        )?;
        Ok(roles
            .iter()
            .enumerate()
            .map(|(index, (reason, _))| {
                (
                    *reason,
                    answers.remove(&format!("r{index}")).unwrap_or_default(),
                )
            })
            .collect())
    }

    /// The pull request's checks as a gate summary. A pull request without CI
    /// (or a failed lookup) yields an empty gate rather than an error — the
    /// gate is decoration on the row, never a reason to lose the row.
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

    /// The pull request's head branch, every thread on it — the conversation,
    /// then each review thread — as interface messages, and the review the user
    /// themselves left (§FS-001-forge-interface.1). Failure yields none: a
    /// conversation we could not read is not a reason to lose the pull request.
    fn conversation(
        &self,
        ctx: &ProviderContext,
        repo: &str,
        number: u64,
        login: &str,
    ) -> (Option<String>, Vec<Thread>, Option<Review>) {
        let Some((owner, name)) = repo.split_once('/') else {
            return (None, Vec::new(), None);
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
            return (None, Vec::new(), None);
        };
        let pull = response.pointer("/data/repository/pullRequest");
        let branch = pull
            .and_then(|pull| pull.get("headRefName"))
            .and_then(Value::as_str)
            .filter(|branch| !branch.is_empty())
            .map(String::from);
        let review = own_review(
            pull.and_then(|pull| pull.pointer("/latestReviews/nodes")),
            login,
        );

        let host = self.config.host.as_deref();
        let messages = |nodes: Option<&Value>| -> Vec<Message> {
            nodes
                .and_then(Value::as_array)
                .map(|nodes| {
                    nodes
                        .iter()
                        .map(|node| message(node, login, host))
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut threads = Vec::new();
        let conversation = messages(pull.and_then(|pull| pull.pointer("/comments/nodes")));
        if !conversation.is_empty() {
            // The conversation tab takes a comment, and the pull request is
            // what one is added to, so the channel declares reply
            // (§FS-007-matters.4). A review thread does not: a reply there is
            // part of a review, which is more than this key promises, so it
            // stays display-only and says so by declaring nothing.
            threads.push(Thread {
                messages: conversation,
                reply: pull
                    .and_then(|pull| pull.get("id"))
                    .and_then(Value::as_str)
                    .map(|id| super::github::target_json(host, id))
                    .unwrap_or(Value::Null),
            });
        }
        for thread in pull
            .and_then(|pull| pull.pointer("/reviewThreads/nodes"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let messages = messages(thread.pointer("/comments/nodes"));
            if !messages.is_empty() {
                threads.push(Thread {
                    messages,
                    ..Thread::default()
                });
            }
        }
        (branch, threads, review)
    }

    /// The handles a mention of the user can be written as: their login, and
    /// every team they are on. Teams are why this is not simply the login —
    /// `@org/reviewers` is a mention of everyone on it, and no search
    /// qualifier will ever return it. A forge that will not say (an old host,
    /// a token without the scope) yields the login alone rather than an error:
    /// finding fewer mentions is a smaller failure than losing the provider.
    fn handles(&self, ctx: &ProviderContext, login: &str) -> Vec<String> {
        let mut handles = vec![format!("@{login}")];
        let mut command = gh_command(self.config.host.as_deref());
        command.args([
            "api",
            "user/teams",
            "--paginate",
            "-q",
            ".[] | \"\\(.organization.login)/\\(.slug)\"",
        ]);
        if let Ok(out) = crate::feed::provider::run_capture(command, ctx.timeout, true) {
            for line in out.lines().map(str::trim).filter(|line| line.contains('/')) {
                handles.push(format!("@{line}"));
            }
        }
        handles
    }
}

/// The user's own verdict, out of GitHub's latest review per reviewer
/// (§FS-001-forge-interface.1). Matched on the login the rest of this provider
/// reads `mine` with, so one identity decides every "is this me" question here.
///
/// `DISMISSED` and `PENDING` are deliberately no verdict: a dismissed approval
/// has been taken back by the forge, and a pending review is a draft nobody but
/// its author can see. Reporting either as a review would tell the reader they
/// have answered when they have not.
fn own_review(nodes: Option<&Value>, login: &str) -> Option<Review> {
    nodes
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|node| node.pointer("/author/login").and_then(Value::as_str) == Some(login))
        .and_then(|node| node.get("state"))
        .and_then(Value::as_str)
        .and_then(|state| match state.to_ascii_uppercase().as_str() {
            "APPROVED" => Some(Review::Approved),
            "CHANGES_REQUESTED" => Some(Review::ChangesRequested),
            "COMMENTED" => Some(Review::Commented),
            _ => None,
        })
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
        // GitHub tracks no task on a pull request comment; a checklist there
        // is prose in the body, not something the forge holds a state for.
        task: Value::Null,
    }
}

/// Whether `body` names `handle`, as a mention and not as the start of a
/// longer name. Plain containment reports `@dev-tools` as a mention of `@dev`
/// and `@vjovanovic` as one of `@vjovanov` — a wrong flag on somebody else's
/// conversation, which is exactly the kind of noise that teaches a reader to
/// stop trusting the flag.
fn names(body: &str, handle: &str) -> bool {
    let bytes = body.as_bytes();
    let mut from = 0;
    while let Some(offset) = body[from..].find(handle) {
        let start = from + offset;
        let end = start + handle.len();
        let opens = start == 0 || !continues_a_handle(bytes[start - 1]);
        let closes = end >= bytes.len() || !continues_a_handle(bytes[end]);
        if opens && closes {
            return true;
        }
        from = end;
    }
    false
}

/// A byte that a login or team slug may go on with: GitHub allows letters,
/// digits, and hyphens, and `/` separates an organization from its team, so a
/// `/` after a login means the mention was of a team and not of the user.
fn continues_a_handle(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'/'
}

/// Whether any message names one of the user's handles. This is what catches a
/// team mention, which `--mentions` cannot: searching for a user finds the
/// pull requests that named *them*, never the ones that named a group they
/// belong to.
fn cited_in(threads: &[Thread], handles: &[String]) -> bool {
    threads.iter().any(|thread| {
        thread
            .messages
            .iter()
            .any(|message| handles.iter().any(|handle| names(&message.text, handle)))
    })
}

/// The pull request's head branch, as the search reported it.
fn head_branch(found: &Value) -> Option<String> {
    found
        .get("headRefName")
        .and_then(Value::as_str)
        .filter(|branch| !branch.is_empty())
        .map(String::from)
}

/// What the review decided, as the search reported it. It is the author's
/// question — whether they owe work — and absent where the forge has reached
/// no verdict at all.
fn review_decision(found: &Value) -> Option<String> {
    found
        .get("reviewDecision")
        .and_then(Value::as_str)
        .map(str::to_lowercase)
        .filter(|decision| !decision.is_empty())
}

/// `owner/name` for a search result: from `repository.nameWithOwner`, or from
/// the pull request url for a host that does not fill it in.
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

/// Whether the search says this pull request is over. Nothing further is
/// fetched about one that is: it settles under Recent whatever its
/// conversation looks like (§FS-003-feed-categories.2), so a gate and a comment
/// thread would be an API call spent on a row that cannot change.
fn finished(found: &Value) -> bool {
    let state = found
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    state == "closed" || state == "merged"
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
        if !command_exists("gh") {
            return Vec::new();
        }
        let red = Gate::of(item).is_some_and(|gate| gate.is_red());
        let identified = item.repo().is_some() && item.number().is_some();
        let mut entries = Vec::new();
        if red && identified {
            entries.push(show_failing_checks(self.config.host.as_deref()));
        }
        // Restarting is offered on a gate that is not red too — that is the
        // half of it the reader reaches for when the merge commit itself is
        // suspect (§FS-004-quick-actions.9).
        entries.extend(restart_actions(self.config.host.as_deref(), item));
        entries
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let login = github_login(ctx, self.config.host.as_deref())?;

        let mut searches = vec![AUTHORED];
        if self.config.reviews {
            searches.extend(REVIEWING);
        }

        // Every reason a pull request is the user's, gathered before anything
        // expensive is asked about it: the same pull request comes back from
        // several searches, and it is one row with several reasons rather than
        // several rows (§FS-003-feed-categories.5).
        let mut found: BTreeMap<String, (Value, Vec<Reason>)> = BTreeMap::new();
        for (reason, pulls) in self.search(ctx, &searches)? {
            for pull in pulls {
                let (Some(repo), Some(number)) =
                    (repo_of(&pull), pull.get("number").and_then(Value::as_u64))
                else {
                    continue;
                };
                let entry = found
                    .entry(format!("{repo}#{number}"))
                    .or_insert_with(|| (pull, Vec::new()));
                if !entry.1.contains(&reason) {
                    entry.1.push(reason);
                }
            }
        }

        // Asked once, and only if something was found that could use it.
        let handles = if self.config.reviews && !found.is_empty() {
            self.handles(ctx, &login)
        } else {
            vec![format!("@{login}")]
        };

        let mut items = Vec::new();
        for (key, (pull, mut reasons)) in found {
            let (Some(repo), Some(number)) =
                (repo_of(&pull), pull.get("number").and_then(Value::as_u64))
            else {
                continue;
            };
            let author = reasons.contains(&Reason::Authored);
            let over = finished(&pull);

            let mut branch = None;
            let mut threads = Vec::new();
            let mut review = None;
            let mut state = pull
                .get("state")
                .and_then(Value::as_str)
                .filter(|state| !state.is_empty())
                .map(str::to_lowercase);
            let mut gate = None;

            if !over {
                if author {
                    // The author's question is what the review decided, and
                    // their own review threads are `github-threads`' job. Both
                    // the decision and the branch came back with the search, so
                    // nothing is asked here at all
                    // (§FS-001-forge-interface.8.3).
                    branch = head_branch(&pull);
                    if let Some(decision) = review_decision(&pull) {
                        state = Some(match &state {
                            Some(state) => format!("{state}:{decision}"),
                            None => decision,
                        });
                    }
                } else {
                    // The reviewer's own verdict comes back with the
                    // conversation, on the call being made anyway; an author
                    // does not review their own change, so it is not asked for
                    // there (§FS-001-forge-interface.1).
                    let (head, found_threads, own) = self.conversation(ctx, &repo, number, &login);
                    branch = head;
                    threads = found_threads;
                    review = own;
                    // A team named in the conversation is a citation of the
                    // user that no search qualifier can return.
                    if !reasons.contains(&Reason::Mentioned) && cited_in(&threads, &handles) {
                        reasons.push(Reason::Mentioned);
                    }
                }
                let found_gate = self.gate(ctx, &repo, number);
                if !found_gate.is_empty() {
                    gate = Some(found_gate);
                }
            }

            let pull_request = PullRequest {
                id: key,
                repo: repo.clone(),
                number: number.to_string(),
                title: pull
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                url: pull.get("url").and_then(Value::as_str).map(String::from),
                branch,
                updated_at: parse_github_time(pull.get("updatedAt").unwrap_or(&Value::Null)),
                role: if author { Role::Author } else { Role::Reviewer },
                state,
                reasons,
                cited: false,
                review,
                threads,
                gate,
            };
            items.push(policy::pull_request_item(
                "github-prs",
                &ctx.project_id,
                &pull_request,
            ));
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::gate::RepoGate;
    use crate::feed::model::{ItemKind, ItemRole};
    use crate::feed::provider::command_exists;
    use serde_json::json;

    fn provider() -> GithubPrs {
        GithubPrs {
            config: Config {
                provider: "github-prs".to_string(),
                repos: vec!["acme/widget".to_string()],
                reviews: true,
                gates: true,
                updated_within_days: 30,
                limit: 30,
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
    /// CI item the reader has to go and find (§FS-004-quick-actions.4) — and
    /// beside it, the two restarts, offered by what each of them can do
    /// (§FS-004-quick-actions.9).
    #[test]
    fn a_pull_request_with_a_red_gate_is_offered_its_log_and_both_restarts() {
        let provider = provider();
        let described = |item: &Item| -> Vec<String> {
            provider
                .quick_actions(item)
                .into_iter()
                .map(|action| action.description)
                .collect()
        };
        // Only where `gh` is installed to answer at all: an entry that can
        // print nothing but `gh: not found` is worse than no entry
        // (§FS-004-quick-actions.2).
        if !command_exists("gh") {
            assert!(described(&pr(Some(gate(1, 2)))).is_empty());
            return;
        }
        assert_eq!(
            described(&pr(Some(gate(1, 2)))),
            [
                "see the CI failures",
                "restart what failed",
                "restart the whole gate"
            ]
        );
        // A green gate has no failures to read and nothing failing to re-run,
        // and still has the whole gate to run again — the entry for a merge
        // commit that is itself suspect (§FS-004-quick-actions.9).
        assert_eq!(described(&pr(Some(gate(3, 0)))), ["restart the whole gate"]);
        // A pull request whose gate was never recorded has nothing to restart:
        // the fact is the item's, not the project's.
        assert!(described(&pr(None)).is_empty());
    }

    /// §FS-001-forge-interface.1: the review the reader gave, picked out of
    /// everybody's by the one login this provider decides `mine` with — and
    /// nothing reported where the forge has taken the verdict back.
    #[test]
    fn the_readers_own_verdict_is_the_one_read_back() {
        let reviews = |mine: &str| {
            json!([
                { "state": "CHANGES_REQUESTED", "author": { "login": "ada" } },
                { "state": mine, "author": { "login": "me" } },
                { "state": "APPROVED", "author": { "login": "grace" } },
            ])
        };
        let read = |mine: &str| own_review(Some(&reviews(mine)), "me");

        assert_eq!(read("APPROVED"), Some(Review::Approved));
        assert_eq!(read("CHANGES_REQUESTED"), Some(Review::ChangesRequested));
        assert_eq!(read("COMMENTED"), Some(Review::Commented));

        // An approval the forge dismissed is no approval, and a draft review is
        // not one the reader has given: both would tell them they answered.
        assert_eq!(read("DISMISSED"), None);
        assert_eq!(read("PENDING"), None);
        // A state ephor has never seen is no verdict rather than a wrong one.
        assert_eq!(read("SOMETHING_ELSE"), None);

        // Somebody else's review is never the reader's, and a pull request
        // nobody reviewed says nothing.
        assert_eq!(own_review(Some(&reviews("APPROVED")), "someone"), None);
        assert_eq!(own_review(Some(&json!([])), "me"), None);
        assert_eq!(own_review(None, "me"), None);
    }

    /// Every role is searched by default, and the whole forge is searched when
    /// no repository is named (§FS-001-forge-interface.1).
    #[test]
    fn every_role_is_looked_for_and_the_default_scope_is_the_whole_forge() {
        let config = json!({ "provider": "github-prs" });
        let provider = GithubPrs::from_config(&config).unwrap();
        assert!(provider.config.repos.is_empty());
        assert!(provider.config.reviews);
        assert_eq!(provider.config.updated_within_days, 30);

        let qualifiers: Vec<&str> = std::iter::once(AUTHORED)
            .chain(REVIEWING)
            .map(|(_, qualifier)| qualifier)
            .collect();
        assert_eq!(
            qualifiers,
            [
                "author",
                "commenter",
                "mentions",
                "review-requested",
                "assignee"
            ]
        );

        // With no repository named, nothing scopes the search but the kind and
        // the window: the whole forge is asked.
        let bounds = provider.bounds();
        assert!(bounds.contains("is:pr"));
        assert!(bounds.contains("updated:>="));
        assert!(bounds.contains("sort:updated-desc"));
        assert!(!bounds.contains("repo:"));
    }

    /// Every repository a source watches is asked about in one question, not
    /// one question each: the forge reads repeated qualifiers as alternatives
    /// (§FS-001-forge-interface.8).
    #[test]
    fn every_watched_repository_rides_in_one_question() {
        let provider = GithubPrs::from_config(&json!({
            "provider": "github-prs",
            "repos": ["acme/widget", "acme/gadget"],
            "updated_within_days": 0
        }))
        .unwrap();
        let bounds = provider.bounds();
        assert!(bounds.contains("repo:acme/widget"));
        assert!(bounds.contains("repo:acme/gadget"));
        // Zero removes the window, here as everywhere else.
        assert!(!bounds.contains("updated:>="));
    }

    /// A merged pull request is news, and news is cheap: nothing further is
    /// asked about it (§FS-003-feed-categories.2).
    #[test]
    fn a_finished_pull_request_is_recognized_however_it_ended() {
        for state in ["closed", "MERGED", "Merged"] {
            assert!(finished(&json!({ "state": state })), "{state}");
        }
        assert!(!finished(&json!({ "state": "open" })));
        assert!(!finished(&json!({})));
    }

    #[test]
    fn the_repository_comes_from_the_field_or_falls_back_to_the_url() {
        let named = json!({ "repository": { "nameWithOwner": "acme/widget" } });
        assert_eq!(repo_of(&named).as_deref(), Some("acme/widget"));

        let from_url = json!({ "url": "https://github.com/acme/widget/pull/42" });
        assert_eq!(repo_of(&from_url).as_deref(), Some("acme/widget"));
        assert_eq!(repo_of(&json!({})), None);
    }

    /// A mention is a whole handle, not a prefix of somebody else's.
    #[test]
    fn a_mention_is_the_whole_handle() {
        assert!(names("ping @vjovanov please", "@vjovanov"));
        assert!(names("@vjovanov", "@vjovanov"));
        assert!(names("(@vjovanov)", "@vjovanov"));
        // A longer login that merely starts the same way is somebody else.
        assert!(!names("cc @vjovanovic", "@vjovanov"));
        assert!(!names("cc @vjovanov-bot", "@vjovanov"));
        // …and so is a team whose organization is named like the user.
        assert!(!names("cc @vjovanov/reviewers", "@vjovanov"));
        // An email address is not a mention.
        assert!(!names("write to me@vjovanov.dev", "@vjovanov"));

        // The team handle itself is matched as one.
        assert!(names("cc @acme/reviewers on this", "@acme/reviewers"));
        assert!(!names("cc @acme/reviewers-eu", "@acme/reviewers"));
    }

    /// The team mention is the case `--mentions @me` cannot return, so it is
    /// found by reading the conversation instead.
    #[test]
    fn a_team_named_in_the_conversation_cites_everyone_on_it() {
        let said = |text: &str| Thread {
            messages: vec![Message {
                author: "them".to_string(),
                text: text.to_string(),
                ..Message::default()
            }],
            ..Thread::default()
        };
        let handles = vec!["@vjovanov".to_string(), "@acme/reviewers".to_string()];

        assert!(cited_in(
            &[said("could @acme/reviewers take a look?")],
            &handles
        ));
        assert!(cited_in(&[said("@vjovanov ping")], &handles));
        assert!(!cited_in(&[said("@acme/release please cut it")], &handles));
        assert!(!cited_in(&[], &handles));
    }
}
