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
}
