//! Provider registry. Adding a provider: create a module implementing
//! `Provider`, then add a match arm in `build_provider`.

mod custom_status;
mod discord;
mod email;
pub mod forge;
mod github_ci;
mod github_issues;
mod github_prs;
mod github_threads;
mod slack;

use std::process::Command;

use serde_json::Value;

use crate::feed::config::ActionConfig;
use crate::feed::model::Item;
use crate::feed::provider::{Provider, ProviderError};

pub fn build_provider(config: &Value) -> Result<Box<dyn Provider>, ProviderError> {
    let name = config
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError("provider entry is missing 'provider'".to_string()))?;
    match name {
        "github-prs" => Ok(Box::new(github_prs::GithubPrs::from_config(config)?)),
        "github-ci" => Ok(Box::new(github_ci::GithubCi::from_config(config)?)),
        "github-issues" => Ok(Box::new(github_issues::GithubIssues::from_config(config)?)),
        "github-threads" => Ok(Box::new(github_threads::GithubThreads::from_config(
            config,
        )?)),
        "custom-status" => Ok(Box::new(custom_status::CustomStatus::from_config(config)?)),
        "slack" => Ok(Box::new(slack::Slack::from_config(config)?)),
        "discord" => Ok(Box::new(discord::Discord::from_config(config)?)),
        "email" => Ok(Box::new(email::Email::from_config(config)?)),
        // Anything else names a forge rather than a built-in provider: reach
        // it out of process (§FS-001-forge-interface.2). `ephor-forge-<name>`
        // on PATH, or an explicit "command".
        _ => Ok(Box::new(forge::ForgeProvider::external(config)?)),
    }
}

/// The quick actions a project's sources offer on one item
/// (§FS-004-quick-actions.1). Only the source that produced the item is
/// asked — it is the one that knows what the item means — and a provider
/// block that no longer builds simply offers nothing, since a menu is not the
/// place to report a broken configuration.
pub fn quick_actions(provider_blocks: &[Value], item: &Item) -> Vec<ActionConfig> {
    provider_blocks
        .iter()
        .filter(|block| block.get("provider").and_then(Value::as_str) == Some(item.source.as_str()))
        .filter_map(|block| build_provider(block).ok())
        .flat_map(|provider| provider.quick_actions(item))
        .collect()
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
        icon: "✗".to_string(),
        description: "see the CI failures".to_string(),
        command,
        kinds: Vec::new(),
        requires_checkout: false,
    }
}

/// `serde(default)` for provider flags that are on unless switched off.
pub(crate) fn enabled() -> bool {
    true
}

/// A configured value as one `sh` word. Single quotes take everything
/// literally, so only the single quote itself has to be broken out.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub(crate) fn parse_config<T: serde::de::DeserializeOwned>(
    config: &Value,
) -> Result<T, ProviderError> {
    serde_json::from_value(config.clone())
        .map_err(|err| ProviderError(format!("invalid provider config: {err}")))
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
    ctx: &crate::feed::provider::ProviderContext,
    host: Option<&str>,
) -> Result<String, ProviderError> {
    if let Some(user) = &ctx.github_user {
        return Ok(user.clone());
    }
    let mut command = gh_command(host);
    command.args(["api", "user", "-q", ".login"]);
    let out = crate::feed::provider::run_capture(command, ctx.timeout, false)?;
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
    use crate::feed::model::ItemKind;
    use crate::feed::provider::command_exists;
    use serde_json::json;

    fn failing_ci_item(source: &str) -> Item {
        Item {
            id: "github-ci:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: source.to_string(),
            kind: ItemKind::Ci,
            role: None,
            title: "#42 Retry window: 1/3 checks passing, 2 failing".to_string(),
            url: None,
            state: Some("failing".to_string()),
            needs_response: true,
            updated_at: chrono::Utc::now(),
            raw: Value::Null,
        }
    }

    #[test]
    fn only_the_source_that_produced_the_item_is_asked() {
        let blocks = vec![
            json!({ "provider": "custom-status", "command": "true" }),
            json!({ "provider": "github-ci", "repos": ["acme/widget"] }),
        ];
        // The github-ci block answers for its own item — where `gh` is
        // installed to answer at all (§FS-004-quick-actions.2).
        let offered = quick_actions(&blocks, &failing_ci_item("github-ci"));
        assert_eq!(offered.len(), usize::from(command_exists("gh")));
        assert!(offered
            .iter()
            .all(|action| action.description == "see the CI failures"));
        // The same item attributed to another source asks that source, which
        // knows nothing about it.
        assert!(quick_actions(&blocks, &failing_ci_item("custom-status")).is_empty());
    }

    #[test]
    fn a_configured_value_survives_becoming_a_shell_word() {
        assert_eq!(shell_quote("ghe.example.com"), "'ghe.example.com'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
