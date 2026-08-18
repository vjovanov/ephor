//! Provider registry. Adding a provider: create a module implementing
//! `Provider`, then add a match arm in `build_provider`.

mod custom_status;
mod discord;
mod email;
pub mod forge;
pub(crate) mod github;
mod github_ci;
mod github_issues;
mod github_notifications;
mod github_prs;
mod github_threads;
mod slack;

use serde_json::Value;

use crate::feed::config::ActionConfig;
use crate::feed::model::Item;
use crate::feed::provider::{Provider, ProviderError};
use crate::forge::Forge;

pub fn build_provider(config: &Value) -> Result<Box<dyn Provider>, ProviderError> {
    let name = config
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError("provider entry is missing 'provider'".to_string()))?;
    match name {
        "github-prs" => Ok(Box::new(github_prs::GithubPrs::from_config(config)?)),
        "github-ci" => Ok(Box::new(github_ci::GithubCi::from_config(config)?)),
        "github-issues" => Ok(Box::new(github_issues::GithubIssues::from_config(config)?)),
        "github-notifications" => Ok(Box::new(
            github_notifications::GithubNotifications::from_config(config)?,
        )),
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

/// A write one of ephor's own providers performs itself rather than through
/// the forge interface (§FS-001-forge-interface.1). Which provider it is
/// belongs down here with the providers; above this module a write is a
/// write, and the descriptor it came from is opaque (§REQ-001-boundary.5).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeWrite(github::Target);

/// Whether a `react` or `reply` descriptor is one of ephor's own providers' to
/// carry out. A descriptor that is claimed and unusable is no target at all,
/// which is why claiming and reading are two questions.
pub fn claims_write(descriptor: &Value) -> bool {
    github::claims(descriptor)
}

/// The descriptor read back into a write, where it carries what one needs.
pub fn native_write(descriptor: &Value) -> Option<NativeWrite> {
    github::parse_target(descriptor).map(NativeWrite)
}

/// Post a reaction through the provider that claimed the descriptor.
pub fn post_reaction(write: &NativeWrite, content: &str) -> crate::error::Result<()> {
    github::react(&write.0, content)
}

/// Send a reply through the provider that claimed the descriptor.
pub fn post_reply(write: &NativeWrite, text: &str) -> crate::error::Result<()> {
    github::reply(&write.0, text)
}

/// Whether a provider fetches unscoped — asking nothing about any one project
/// and answering about all of them (§DA-002-fetch-attribution-split). A
/// property of the provider, so it is answered where the providers are.
pub fn is_shared(name: &str) -> bool {
    matches!(name, "github-notifications" | "slack" | "discord" | "email")
}

/// Whether a provider name is one ephor implements itself. The complement of
/// this is the forge case in `build_provider`, kept in one place because a
/// write has to make the same distinction and two copies of the list would
/// drift into a source that fetches one way and writes another.
fn built_in(name: &str) -> bool {
    matches!(
        name,
        "github-prs"
            | "github-ci"
            | "github-issues"
            | "github-notifications"
            | "github-threads"
            | "custom-status"
            | "slack"
            | "discord"
            | "email"
    )
}

/// The forge behind a source and the request to call it with, for the writes
/// that go back to it — a reaction, a ticked task. Fails where the source is
/// one of ephor's own providers: those reach their host directly, and a caller
/// that lands here with one has a descriptor it should have handled itself.
///
/// The block's own timeout is honored the way a fetch honors it, since a forge
/// that needs a minute to be reached at all needs it for a write too.
pub fn forge_call(
    blocks: &[Value],
    source: &str,
    project: &str,
    defaults: &crate::feed::config::Defaults,
) -> Result<(Box<dyn Forge>, crate::forge::Request), ProviderError> {
    if built_in(source) {
        return Err(ProviderError(format!(
            "'{source}' is not reached through the forge interface"
        )));
    }
    let block = blocks
        .iter()
        .find(|block| block.get("provider").and_then(Value::as_str) == Some(source))
        .ok_or_else(|| {
            ProviderError(format!(
                "'{project}' has no source named '{source}' anymore"
            ))
        })?;
    let timeout = crate::feed::refresh::provider_timeout(block)
        .map(|timeout| timeout.as_secs())
        .unwrap_or(defaults.provider_timeout_seconds);
    let request = crate::forge::Request {
        config: block.clone(),
        project: project.to_string(),
        tickets: Vec::new(),
        user: defaults.github_user.clone(),
        timeout_seconds: timeout,
    };
    Ok((forge::ForgeProvider::external(block)?.into_forge(), request))
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

/// `serde(default)` for provider flags that are on unless switched off.
pub(crate) fn enabled() -> bool {
    true
}

pub(crate) use crate::seams::summons::quote as shell_quote;

pub(crate) use github::{
    gh_command, github_login, parse_github_time, restart_actions, show_failing_checks,
};

pub(crate) fn parse_config<T: serde::de::DeserializeOwned>(
    config: &Value,
) -> Result<T, ProviderError> {
    serde_json::from_value(config.clone())
        .map_err(|err| ProviderError(format!("invalid provider config: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::ItemKind;
    use crate::feed::provider::command_exists;
    use serde_json::json;

    /// A pull request whose gate is red — one row carrying its gate, which is
    /// what the CI source reports now (§FS-007-matters.5).
    fn failing_ci_item(source: &str) -> Item {
        Item {
            id: "github-ci:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: source.to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "Retry window".to_string(),
            url: None,
            state: None,
            needs_response: true,
            updated_at: chrono::Utc::now(),
            raw: json!({
                "repo": "acme/widget",
                "gate": { "repos": [{
                    "repo": "acme/widget", "passed": 1, "failed": 2, "running": 0
                }] }
            }),
        }
    }

    #[test]
    fn only_the_source_that_produced_the_item_is_asked() {
        let blocks = vec![
            json!({ "provider": "custom-status", "command": "true" }),
            json!({ "provider": "github-ci", "repos": ["acme/widget"] }),
        ];
        // The github-ci block answers for its own item — where `gh` is
        // installed to answer at all (§FS-004-quick-actions.2): the failures
        // and both restarts (§FS-004-quick-actions.9).
        let offered = quick_actions(&blocks, &failing_ci_item("github-ci"));
        if command_exists("gh") {
            assert_eq!(
                offered
                    .iter()
                    .map(|action| action.description.as_str())
                    .collect::<Vec<_>>(),
                [
                    "see the CI failures",
                    "restart what failed",
                    "restart the whole gate"
                ]
            );
        } else {
            assert!(offered.is_empty());
        }
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
