//! Provider registry. Adding a provider: create a module implementing
//! `Provider`, then add a match arm in `build_provider`.

mod custom_status;
mod discord;
mod email;
pub mod forge;
mod github_ci;
mod github_prs;
mod github_threads;
mod slack;

use std::process::Command;

use serde_json::Value;

use crate::feed::provider::{Provider, ProviderError};

pub fn build_provider(config: &Value) -> Result<Box<dyn Provider>, ProviderError> {
    let name = config
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError("provider entry is missing 'provider'".to_string()))?;
    match name {
        "github-prs" => Ok(Box::new(github_prs::GithubPrs::from_config(config)?)),
        "github-ci" => Ok(Box::new(github_ci::GithubCi::from_config(config)?)),
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

/// `serde(default)` for provider flags that are on unless switched off.
pub(crate) fn enabled() -> bool {
    true
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
