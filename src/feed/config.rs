//! Feed configuration: which providers watch which project, loaded from
//! `$EPHOR_HOME/config/status.json` (override with `EPHOR_STATUS_CONFIG`).
//!
//! Provider blocks are loosely typed here; each provider deserializes its own
//! block with `deny_unknown_fields`, so typos fail loudly at refresh time.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{registry_error, Result};
use crate::paths;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusConfig {
    #[serde(default)]
    pub defaults: Defaults,
    /// Item actions offered on every project (see [`ActionConfig`]).
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
    /// Which items deserve work and what the ticket asks for
    /// (§FS-005-dispatch.1).
    #[serde(default)]
    pub work: crate::work::recipe::WorkConfig,
    /// Sources shared across every project — a notification stream, a
    /// mailbox — fetched once per site rather than once per project
    /// (§AR-008-pipeline.1). What they report is placed by the attribution
    /// engine (§AR-003-attribution), never by the source itself, which is
    /// what lets one of them be exhaustive without being told in advance
    /// where to look (§DA-002-fetch-attribution-split).
    #[serde(default)]
    pub sources: Vec<serde_json::Value>,
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectFeedConfig>,
}

/// Whether a source fetches unscoped — asking nothing about any one project,
/// and answering about all of them. These belong at site level; declared under
/// a project they still work, and say so once (§DA-002-fetch-attribution-split).
pub fn is_shared_source(name: &str) -> bool {
    matches!(name, "github-notifications" | "slack" | "discord" | "email")
}

impl StatusConfig {
    /// Shared sources still declared under a project. They keep working; the
    /// note says where they belong now, once per run rather than per project.
    pub fn misplaced_shared_sources(&self) -> Vec<(String, String)> {
        let mut found = Vec::new();
        for (project, config) in &self.projects {
            for block in &config.providers {
                let Some(name) = block.get("provider").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if is_shared_source(name) {
                    found.push((project.clone(), name.to_string()));
                }
            }
        }
        found
    }
}

/// A user-defined action summoned on a feed item in the TUI (`x`). The
/// command runs via `sh -c` in the project's checkout, with the item's
/// context exported as `EPHOR_*` environment variables.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConfig {
    pub icon: String,
    pub description: String,
    pub command: String,
    /// Restrict to item kinds (`pr`, `ci`, `message`, `status`); empty
    /// offers the action on every kind.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// The action needs the item's branch workspace on disk. When it is
    /// missing, the project's `checkout` command runs first.
    #[serde(default)]
    pub requires_checkout: bool,
}

/// The per-project command that materializes a branch workspace. Contract:
/// it runs in the project root with the item's `EPHOR_*` environment and must
/// make `$EPHOR_WORKSPACE` exist — ephor verifies the directory afterwards.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutConfig {
    #[serde(default = "default_checkout_icon")]
    pub icon: String,
    #[serde(default = "default_checkout_description")]
    pub description: String,
    pub command: String,
}

fn default_checkout_icon() -> String {
    "⇣".to_string()
}

fn default_checkout_description() -> String {
    "check out branch workspace".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default = "default_ttl")]
    pub ttl_seconds: u64,
    #[serde(default = "default_timeout")]
    pub provider_timeout_seconds: u64,
    #[serde(default)]
    pub github_user: Option<String>,
    /// How long finished work stays under Recent, in days
    /// (§FS-003-feed-categories.3). Zero drops it as soon as it finishes.
    #[serde(default = "default_recent_days")]
    pub recent_days: u64,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            ttl_seconds: default_ttl(),
            provider_timeout_seconds: default_timeout(),
            github_user: None,
            recent_days: default_recent_days(),
        }
    }
}

fn default_ttl() -> u64 {
    600
}

fn default_timeout() -> u64 {
    30
}

fn default_recent_days() -> u64 {
    7
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFeedConfig {
    pub providers: Vec<Value>,
    /// Extra actions for this project, offered after the global ones.
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
    /// How to materialize a missing branch workspace (see [`CheckoutConfig`]).
    #[serde(default)]
    pub checkout: Option<CheckoutConfig>,
    /// This project's own recipes and work root (§FS-005-dispatch.1).
    #[serde(default)]
    pub work: crate::work::recipe::ProjectWorkConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_global_and_project_actions() {
        let config: StatusConfig = serde_json::from_str(
            r#"{
                "actions": [
                    { "icon": "⎇", "description": "check out", "command": "gh pr checkout $EPHOR_NUMBER", "kinds": ["pr"] }
                ],
                "projects": {
                    "demo": {
                        "providers": [{ "provider": "custom-status", "command": "true" }],
                        "actions": [{ "icon": "🧪", "description": "gate", "command": "just gate", "requires_checkout": true }],
                        "checkout": { "command": "git worktree add \"$EPHOR_WORKSPACE\" \"$EPHOR_BRANCH\"" }
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(config.actions.len(), 1);
        assert_eq!(config.actions[0].kinds, ["pr"]);
        assert!(!config.actions[0].requires_checkout);
        assert_eq!(config.projects["demo"].actions[0].description, "gate");
        assert!(config.projects["demo"].actions[0].requires_checkout);
        let checkout = config.projects["demo"].checkout.as_ref().unwrap();
        assert_eq!(checkout.icon, "⇣");
        assert_eq!(checkout.description, "check out branch workspace");
        // An action typo fails loudly.
        assert!(serde_json::from_str::<StatusConfig>(
            r#"{ "actions": [{ "icon": "x", "description": "d", "cmd": "true" }] }"#
        )
        .is_err());
    }
}

pub fn config_path() -> PathBuf {
    std::env::var_os("EPHOR_STATUS_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths::resolve_config("status.json"))
}

pub fn load_config() -> Result<StatusConfig> {
    let path = config_path();
    let text = fs::read_to_string(&path).map_err(|err| {
        registry_error(format!(
            "Cannot read feed config {}: {err}. Copy config/status.example.json there and edit it.",
            path.display()
        ))
    })?;
    let config: StatusConfig = serde_json::from_str(&text)
        .map_err(|err| registry_error(format!("Invalid feed config {}: {err}", path.display())))?;
    for (project_id, project) in &config.projects {
        for provider in &project.providers {
            if provider.get("provider").and_then(Value::as_str).is_none() {
                return Err(registry_error(format!(
                    "Feed config project '{project_id}' has a provider entry without a 'provider' name."
                )));
            }
        }
    }
    Ok(config)
}
