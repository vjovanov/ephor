//! What every GitHub source shares: the vendor CLI it is reached through, the
//! login it is reached as, the timestamps it speaks, and the two writes ephor
//! performs against it directly rather than through the forge interface.
//!
//! The five `github_*` providers each fetch one kind of thing; this module is
//! the half none of them owns alone. It is also where the word `github` and
//! the command `gh` are allowed to appear at all — a reaction posted from the
//! thread screen and a reply sent from it are GitHub work, so the GraphQL that
//! does them lives beside the rest of the adapter and the engine above asks
//! for "a write this source performs itself" (§REQ-001-boundary.5,
//! §AR-001-layers.2).

use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{EphorError, Result};
use crate::feed::config::ActionConfig;
use crate::feed::gate::Scope;
use crate::feed::model::Item;
use crate::feed::provider::{run_capture, run_json, ProviderContext, ProviderError};
use crate::feed::react::emoji_for_content;

use super::shell_quote;

/// The `provider` a descriptor carries when the write is this adapter's.
const DESCRIPTOR: &str = "github";

/// Where a write this adapter performs goes: a node in the graph, on the host
/// the source was configured with.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    host: Option<String>,
    subject_id: String,
}

/// The `react`/`reply` descriptor for a node this adapter can write to.
pub fn target_json(host: Option<&str>, subject_id: &str) -> Value {
    json!({ "provider": DESCRIPTOR, "host": host, "subject_id": subject_id })
}

/// Whether this descriptor is this adapter's to write through. Separate from
/// reading it: a descriptor that claims this adapter and is unusable is no
/// target at all, rather than one handed to a forge that never wrote it.
pub fn claims(descriptor: &Value) -> bool {
    descriptor.get("provider").and_then(Value::as_str) == Some(DESCRIPTOR)
}

/// The descriptor read back into a target, where it carries what a write needs.
pub fn parse_target(descriptor: &Value) -> Option<Target> {
    Some(Target {
        host: descriptor
            .get("host")
            .and_then(Value::as_str)
            .map(String::from),
        subject_id: descriptor
            .get("subject_id")
            .and_then(Value::as_str)?
            .to_string(),
    })
}

const ADD_REACTION: &str = "mutation($subject:ID!,$content:ReactionContent!){\
addReaction(input:{subjectId:$subject,content:$content}){reaction{content}}}";

const ADD_COMMENT: &str = "mutation($subject:ID!,$body:String!){\
addComment(input:{subjectId:$subject,body:$body}){clientMutationId}}";

/// Post a reaction. `content` is the palette content name (e.g. THUMBS_UP).
pub fn react(target: &Target, content: &str) -> Result<()> {
    let mut command = gh_command(target.host.as_deref());
    command
        .args(["api", "graphql", "-f", &format!("query={ADD_REACTION}")])
        .args(["-f", &format!("subject={}", target.subject_id)])
        .args(["-f", &format!("content={content}")]);
    run_json(command, Duration::from_secs(15), false)
        .map_err(|err| EphorError::Command(format!("reaction failed: {err}")))?;
    Ok(())
}

/// Send a reply. The text is the reader's and goes out exactly as it stands
/// (§FS-005-dispatch.13).
pub fn reply(target: &Target, text: &str) -> Result<()> {
    let mut command = gh_command(target.host.as_deref());
    command
        .args(["api", "graphql", "-f", &format!("query={ADD_COMMENT}")])
        .args(["-f", &format!("subject={}", target.subject_id)])
        .args(["-f", &format!("body={text}")]);
    run_json(command, Duration::from_secs(30), false)
        .map_err(|err| EphorError::Command(format!("reply failed: {err}")))?;
    Ok(())
}

/// Group GraphQL reaction nodes (`[{content, user{login}}]`) into the thread
/// display shape `[{"emoji": "👍", "users": ["alice", ...]}]`.
pub fn reactions_json(nodes: Option<&Value>) -> Value {
    let mut grouped: Vec<(String, Vec<Value>)> = Vec::new();
    for node in nodes.and_then(Value::as_array).into_iter().flatten() {
        let Some(content) = node.get("content").and_then(Value::as_str) else {
            continue;
        };
        let emoji = emoji_for_content(content);
        let user = node
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("");
        match grouped.iter_mut().find(|(existing, _)| *existing == emoji) {
            Some((_, users)) => users.push(json!(user)),
            None => grouped.push((emoji, vec![json!(user)])),
        }
    }
    Value::Array(
        grouped
            .into_iter()
            .map(|(emoji, users)| json!({ "emoji": emoji, "users": users }))
            .collect(),
    )
}

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

/// Run a pull request's checks again (§FS-004-quick-actions.9).
///
/// Checks on a pull request are workflow runs against its head commit, so the
/// head is resolved first and the runs are asked for by it — `gh run rerun`
/// takes a run, and nothing on the pull request itself names one. `--failed`
/// is GitHub's own `rerun-failed-jobs`, which is why *restart what failed*
/// asks for exactly the jobs that failed rather than for something ephor
/// reconstructed.
///
/// Only completed runs are asked: one still in flight is already running, and
/// re-running it is refused by the forge rather than by a guess here. And a
/// check that is **not** a workflow run — a status somebody else's system
/// wrote onto the commit — cannot be re-run through this API at all, so it is
/// named rather than silently left out: a key that quietly did three quarters
/// of the job is worse than one that says which quarter it skipped.
const RESTART_CHECKS: &str = r##"{
  sha="$(gh pr view "$EPHOR_NUMBER" --repo "$EPHOR_REPO" --json headRefOid -q .headRefOid)" || exit 1
  [ -n "$sha" ] || { echo "no head commit for $EPHOR_REPO#$EPHOR_NUMBER"; exit 1; }
  printf 'restarting @WHAT@ on %s#%s (head %s)\n\n' "$EPHOR_REPO" "$EPHOR_NUMBER" "$sha"

  asked=0
  refused=0
  # Tab-separated and split on the tab alone: a workflow run's name has spaces
  # in it, and the default IFS would eat the leading ones.
  while IFS="$(printf '\t')" read -r id name; do
    [ -n "$id" ] || continue
    printf '\342\237\263 %s (run %s)\n' "$name" "$id"
    if gh run rerun "$id" --repo "$EPHOR_REPO"@FLAG@; then
      asked=$((asked + 1))
    else
      refused=$((refused + 1))
    fi
  done <<EOF
$(gh api --paginate "repos/$EPHOR_REPO/actions/runs?head_sha=$sha" \
    -q '.workflow_runs[] | select(@FILTER@) | "\(.id)\t\(.name)"')
EOF

  gh pr checks "$EPHOR_NUMBER" --repo "$EPHOR_REPO" --json name,link \
     --jq '.[] | select((.link // "") | contains("/actions/runs/") | not) | .name' 2>/dev/null \
    | while read -r name; do
        [ -n "$name" ] || continue
        printf '\302\267 %s is not a workflow run \342\200\224 it is restarted where it is published\n' "$name"
      done

  printf '\nasked %s run(s) to run again' "$asked"
  if [ "$refused" -eq 0 ]; then printf '\n'; else printf ', %s refused\n' "$refused"; fi
  [ "$asked" -gt 0 ] || echo "nothing here needed restarting"
} 2>&1
"##;

/// The two restart entries for a GitHub-hosted gate, offered by both GitHub
/// sources for the same reason the failing-checks entry is: the same gate is
/// reachable through the pull request and through its checks, and the reader
/// should not have to know which row carries the key.
///
/// A red gate gets both. A gate that is not red gets *restart the whole gate*
/// alone — the entry that still has something to do there — and not *restart
/// what failed*, which would report that there was nothing to restart
/// (§FS-004-quick-actions.2, §FS-004-quick-actions.9).
pub(crate) fn restart_actions(host: Option<&str>, item: &Item) -> Vec<ActionConfig> {
    let Some(gate) = crate::feed::gate::Gate::of(item) else {
        return Vec::new();
    };
    if item.repo().is_none() || item.number().is_none() {
        return Vec::new();
    }
    let mut entries = Vec::new();
    if gate.is_red() {
        entries.push(restart_checks(host, Scope::Failed));
    }
    entries.push(restart_checks(host, Scope::All));
    entries
}

/// One restart entry. It runs beneath the screen: the gate answers minutes
/// later and asks nothing meanwhile (§FS-005-dispatch.17).
fn restart_checks(host: Option<&str>, scope: Scope) -> ActionConfig {
    let (what, flag, filter) = match scope {
        Scope::Failed => (
            "what failed",
            " --failed",
            r#".status == "completed" and .conclusion != "success""#,
        ),
        Scope::All => ("every check", "", r#".status == "completed""#),
    };
    let script = RESTART_CHECKS
        .replace("@WHAT@", what)
        .replace("@FLAG@", flag)
        .replace("@FILTER@", filter);
    let command = match host {
        Some(host) => format!("export GH_HOST={}\n{script}", shell_quote(host)),
        None => script,
    };
    ActionConfig {
        id: match scope {
            Scope::Failed => "restart-failed".to_string(),
            Scope::All => "restart-all".to_string(),
        },
        icon: "⟳".to_string(),
        description: match scope {
            Scope::Failed => "restart what failed".to_string(),
            Scope::All => "restart the whole gate".to_string(),
        },
        command,
        background: true,
        // Re-running a whole gate spends an hour of a shared machine pool, and
        // a keystroke away from a cursor is not a decision
        // (§FS-004-quick-actions.9).
        confirm: matches!(scope, Scope::All),
        ..ActionConfig::default()
    }
}

/// The failing-CI quick action, pointed at the host the checks live on: the
/// enterprise host is configuration, so it is exported rather than named in
/// the script. Shared by both GitHub sources — the same red gate reached
/// through the pull request or through its checks asks the same question, and
/// two copies of this script would drift.
pub(crate) fn show_failing_checks(host: Option<&str>) -> ActionConfig {
    let command = match host {
        Some(host) => format!(
            "export GH_HOST={}\n{SHOW_FAILING_CHECKS}",
            shell_quote(host)
        ),
        None => SHOW_FAILING_CHECKS.to_string(),
    };
    ActionConfig {
        id: "ci-failures".to_string(),
        icon: "✗".to_string(),
        description: "see the CI failures".to_string(),
        command,
        ..ActionConfig::default()
    }
}

/// One question put to the forge's search, and the name its answer comes back
/// under.
pub(crate) struct Search {
    /// A GraphQL alias: letters, digits and underscores, not starting with a
    /// digit. Generated by the caller rather than derived from the query, so
    /// that a label with a space or a quote in it cannot produce a malformed
    /// request.
    pub alias: String,
    /// The query in GitHub's own search syntax, `is:pr` and `repo:` qualifiers
    /// and all — exactly what the search box would take.
    pub query: String,
}

/// The most one GraphQL connection returns in a page, whatever `first` asks
/// for.
const PAGE: u32 = 100;

/// Every search in one request (§FS-001-forge-interface.8).
///
/// GitHub meters its search endpoint at thirty requests a minute and its graph
/// at five thousand points an hour, and the same question can be asked of
/// either. Asked of the graph, and asked as one aliased field per question,
/// every search a provider needs costs **one point of the generous meter and
/// nothing at all of the scarce one** — where the REST search spent one of
/// thirty per role, per repository, per project, and a registry of a handful
/// of projects crossed the ceiling on every refresh.
///
/// Each search keeps its own alias, so a role is still asked as its own
/// question and the reasons stay exactly as separable as they were
/// (§FS-001-forge-interface.8.1) — what collapsed is the number of requests,
/// never the number of answers.
///
/// `selection` is the node body, spliced in once per search; `limit` is the
/// most each search returns, paged where it is more than a connection gives at
/// once. Answers come back per alias, in the order the forge returned them.
pub(crate) fn search(
    host: Option<&str>,
    searches: &[Search],
    selection: &str,
    limit: u32,
    timeout: Duration,
) -> std::result::Result<HashMap<String, Vec<Value>>, ProviderError> {
    let mut collected: HashMap<String, Vec<Value>> = searches
        .iter()
        .map(|search| (search.alias.clone(), Vec::new()))
        .collect();
    if searches.is_empty() || limit == 0 {
        return Ok(collected);
    }

    // Alias -> where its next page starts. Present means "still wanted": a
    // search drops out of the round once it is full or the forge says there is
    // no more of it.
    let mut cursors: HashMap<&str, Option<String>> = searches
        .iter()
        .map(|search| (search.alias.as_str(), None))
        .collect();

    // One round per page. Nearly every refresh takes exactly one: a page holds
    // a hundred and the configured limit is typically thirty.
    while !cursors.is_empty() {
        let mut fields = String::new();
        for search in searches
            .iter()
            .filter(|s| cursors.contains_key(s.alias.as_str()))
        {
            let want = limit.saturating_sub(collected[&search.alias].len() as u32);
            let after: Option<String> = cursors
                .get(search.alias.as_str())
                .and_then(Option::as_ref)
                .cloned();
            fields.push_str(&search_field(search, selection, want.min(PAGE), &after));
        }

        let mut command = gh_command(host);
        command.args(["api", "graphql", "-f", &format!("query={{{fields}}}")]);
        let response = run_json(command, timeout, false)?;
        if let Some(message) = graphql_error(&response) {
            return Err(ProviderError(format!("search failed: {message}")));
        }

        let asked: Vec<&str> = cursors.keys().copied().collect();
        for alias in asked {
            let Some(connection) = response.pointer(&format!("/data/{alias}")) else {
                cursors.remove(alias);
                continue;
            };
            let nodes = connection
                .pointer("/nodes")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let entry = collected.get_mut(alias).expect("alias was registered");
            // A search over `type:ISSUE` returns issues and pull requests
            // alike, and an inline fragment for the other kind yields an empty
            // node rather than nothing at all. Dropping them here keeps every
            // caller from having to know that.
            entry.extend(nodes.iter().filter(|node| !is_empty_node(node)).cloned());

            let more = connection
                .pointer("/pageInfo/hasNextPage")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let next = connection
                .pointer("/pageInfo/endCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            match (more, next) {
                (true, Some(next)) if (entry.len() as u32) < limit => {
                    cursors.insert(alias, Some(next));
                }
                _ => {
                    cursors.remove(alias);
                }
            }
        }
    }

    Ok(collected)
}

/// One aliased search as a GraphQL field. Everything a caller controls — the
/// query and the cursor — goes through `graphql_string`, so a label with a
/// quote in it cannot close the literal and change the request around it.
fn search_field(search: &Search, selection: &str, first: u32, after: &Option<String>) -> String {
    let after = match after {
        Some(cursor) => format!(",after:{}", graphql_string(cursor)),
        None => String::new(),
    };
    format!(
        "{alias}:search(query:{query},type:ISSUE,first:{first}{after}){{\
pageInfo{{hasNextPage endCursor}} nodes{{{selection}}}}} ",
        alias = search.alias,
        query = graphql_string(&search.query),
    )
}

/// A node the search matched but this query did not ask about — the other kind
/// of thing under `type:ISSUE`, whose inline fragment selected nothing.
fn is_empty_node(node: &Value) -> bool {
    node.as_object().is_some_and(serde_json::Map::is_empty)
}

/// A GraphQL string literal. JSON's own escaping is a subset of GraphQL's, so
/// serializing is the escape: a label carrying a quote, a backslash, or a
/// newline cannot end the literal and change the query around it.
fn graphql_string(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

/// The first error a GraphQL response carries, if it carries any. A response
/// may be a partial success — some aliases answered, one refused — and the
/// refusal is reported rather than read as an empty answer
/// (§FS-001-forge-interface.6).
fn graphql_error(response: &Value) -> Option<String> {
    let errors = response.get("errors")?.as_array()?;
    let first = errors.first()?;
    Some(
        first
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the forge reported an error with no message")
            .to_string(),
    )
}

/// `gh` invocation with optional GitHub Enterprise host.
pub(crate) fn gh_command(host: Option<&str>) -> Command {
    let mut command = Command::new("gh");
    command
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1");
    if let Some(host) = host {
        command.env("GH_HOST", host);
    }
    command
}

/// The authenticated GitHub login: config override or `gh api user`.
pub(crate) fn github_login(
    ctx: &ProviderContext,
    host: Option<&str>,
) -> std::result::Result<String, ProviderError> {
    if let Some(user) = &ctx.github_user {
        return Ok(user.clone());
    }
    let mut command = gh_command(host);
    command.args(["api", "user", "-q", ".login"]);
    let out = run_capture(command, ctx.timeout, false)?;
    let login = out.trim().to_string();
    if login.is_empty() {
        return Err(ProviderError(
            "could not determine GitHub login".to_string(),
        ));
    }
    Ok(login)
}

pub(crate) fn parse_github_time(value: &Value) -> chrono::DateTime<chrono::Utc> {
    value
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_group_by_emoji() {
        let nodes = json!([
            { "content": "THUMBS_UP", "user": { "login": "alice" } },
            { "content": "THUMBS_UP", "user": { "login": "bob" } },
            { "content": "ROCKET", "user": { "login": "carol" } },
        ]);
        assert_eq!(
            reactions_json(Some(&nodes)),
            json!([
                { "emoji": "👍", "users": ["alice", "bob"] },
                { "emoji": "🚀", "users": ["carol"] },
            ])
        );
    }

    /// A descriptor this adapter wrote is claimed and read back; one it did
    /// not write is neither.
    #[test]
    fn a_descriptor_is_claimed_only_where_this_adapter_wrote_it() {
        let mine = target_json(Some("github.example.com"), "MDEy");
        assert!(claims(&mine));
        assert_eq!(
            parse_target(&mine),
            Some(Target {
                host: Some("github.example.com".to_string()),
                subject_id: "MDEy".to_string(),
            })
        );
        assert!(!claims(&json!({ "kind": "comment", "id": "c-7" })));
    }

    /// Claimed and unusable: a target that cannot be written to is no target,
    /// rather than one handed to a forge that never wrote the descriptor.
    #[test]
    fn a_claimed_descriptor_without_a_subject_is_no_target() {
        let broken = json!({ "provider": "github", "host": null });
        assert!(claims(&broken));
        assert_eq!(parse_target(&broken), None);
    }

    /// Every question is one field of one request, and each keeps its own
    /// alias so a role still answers for itself (§FS-001-forge-interface.8).
    #[test]
    fn each_search_is_its_own_aliased_field_of_the_one_request() {
        let search = Search {
            alias: "r0".to_string(),
            query: "is:pr repo:acme/widget author:@me".to_string(),
        };
        let field = search_field(&search, "... on PullRequest{number}", 30, &None);
        assert!(field.starts_with("r0:search(query:"));
        assert!(field.contains(r#""is:pr repo:acme/widget author:@me""#));
        assert!(field.contains("type:ISSUE,first:30"));
        assert!(field.contains("nodes{... on PullRequest{number}}"));
        // A first page asks for no cursor at all.
        assert!(!field.contains("after:"));

        // A later page carries the cursor the forge handed back.
        let paged = search_field(&search, "x", 30, &Some("Y3Vyc29y".to_string()));
        assert!(paged.contains(r#",after:"Y3Vyc29y""#));
    }

    /// A label is the reader's text, and it reaches the forge inside a string
    /// literal: one carrying a quote must not be able to end that literal and
    /// write the rest of the request itself.
    #[test]
    fn a_query_carrying_a_quote_stays_inside_its_literal() {
        let search = Search {
            alias: "q0".to_string(),
            query: r#"label:"needs "review"" state:open"#.to_string(),
        };
        let field = search_field(&search, "x", 10, &None);
        // Every quote from the query is escaped, so the literal opens and
        // closes exactly once.
        let unescaped = field
            .match_indices('"')
            .filter(|(at, _)| *at == 0 || field.as_bytes()[at - 1] != b'\\')
            .count();
        assert_eq!(unescaped, 2, "{field}");
        assert!(serde_json::from_str::<Value>(
            field
                .split_once("query:")
                .and_then(|(_, rest)| rest.split_once(",type:ISSUE"))
                .map(|(literal, _)| literal)
                .unwrap()
        )
        .is_ok());
    }

    /// One `type:ISSUE` search returns both kinds, and the fragment for the
    /// kind this query did not ask about selects nothing at all. An empty node
    /// is not a result.
    #[test]
    fn the_other_kind_of_thing_is_not_a_result() {
        assert!(is_empty_node(&json!({})));
        assert!(!is_empty_node(&json!({ "number": 42 })));
        assert!(!is_empty_node(&Value::Null));
    }

    /// A refusal is reported rather than read as an empty answer
    /// (§FS-001-forge-interface.6): a search that came back empty because the
    /// forge said no is a source that did not answer.
    #[test]
    fn a_refused_search_is_an_error_and_not_an_empty_answer() {
        let refused = json!({
            "data": { "r0": null },
            "errors": [{ "message": "API rate limit exceeded" }]
        });
        assert_eq!(
            graphql_error(&refused).as_deref(),
            Some("API rate limit exceeded")
        );
        assert_eq!(graphql_error(&json!({ "data": { "r0": {} } })), None);
        // An error with nothing to say is still an error.
        assert!(graphql_error(&json!({ "errors": [{}] })).is_some());
    }
}
