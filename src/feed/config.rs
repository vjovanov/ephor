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
/// Which sources those are is the providers' own fact, not configuration's
/// (§REQ-001-boundary.5).
pub fn is_shared_source(name: &str) -> bool {
    crate::feed::providers::is_shared(name)
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

/// One entry of the action menu, whoever it came from: ephor's own quick
/// actions, the project's offers, the person's configured commands and the
/// recipes are one shape (§FS-006-project-interface.9, §FS-005-dispatch.1).
///
/// An entry either **runs a command here** or **asks somebody for work**, and
/// exactly one of the two is written: `command` runs via `sh -c` in the
/// project's checkout with the item's context exported as `EPHOR_*`
/// environment variables; [`ActionConfig::agent`] carries a brief instead, and
/// is dispatched as the recipe it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(try_from = "RawAction")]
pub struct ActionConfig {
    /// What an entry of the same name overrides, in provenance order — the
    /// person's beats the project's beats the shipped one. Empty is anonymous:
    /// it overrides nothing and nothing overrides it.
    pub id: String,
    pub icon: String,
    pub description: String,
    /// Empty on an entry that asks for work rather than running one.
    pub command: String,
    /// The work this entry hands over, where it hands work over rather than
    /// running a command (§FS-005-dispatch.1). It is a whole recipe, not a
    /// description of one: an entry that carries a brief instead of a command
    /// *is* a recipe under another name, so the menu dispatches it through the
    /// one path the work screen uses and nothing it carries — its opening
    /// move, its own hand — is lost on the way (§FS-005-dispatch.4).
    pub agent: Option<crate::work::recipe::Recipe>,
    /// Who this work would go to, filled in when the menu opens
    /// (§FS-005-dispatch.14). Never configuration: nobody writes it, ephor
    /// resolves it so the reader sees it before pressing the key.
    pub hand: Option<Handed>,
    /// Where it runs — `workspace` (the default), `root`, or `repo:<name>`
    /// (§AR-002-summons.1).
    pub cwd: Option<String>,
    /// Restrict to item kinds (`pr`, `ci`, `message`, `status`); empty
    /// offers the action on every kind. The older spelling of `when.kinds`,
    /// kept because it is what configurations say today.
    pub kinds: Vec<String>,
    /// Which items this is offered on, in the language recipes use
    /// (§FS-006-project-interface.9).
    pub when: crate::work::recipe::Selector,
    /// Capability rungs it needs (§FS-006-project-interface.10). A missing one
    /// leaves the entry visible with its reason, never removed.
    pub requires: Vec<String>,
    /// The action needs the item's branch workspace on disk. When it is
    /// missing, the project's `checkout` command runs first — and for an entry
    /// that asks for work, it is the recipe's own `needs_checkout`, which is
    /// answered by withholding the offer rather than by checking out first
    /// (§FS-004-quick-actions.7).
    pub requires_checkout: bool,
    /// Ask before running it.
    pub confirm: bool,
    /// It runs beneath the screen as a job rather than taking the terminal
    /// (§FS-005-dispatch.17). The default is the terminal, because a menu
    /// entry has always been allowed to *be* the reader's session — `lazygit`,
    /// an editor, a pager (§FS-006-project-interface.9) — and one of those
    /// started beneath the screen is a program nobody can type into. ephor's
    /// own deterministic moves set it themselves (§FS-005-dispatch.12).
    pub background: bool,
}

/// Who a piece of agent work would go to, resolved when the menu opens
/// (§FS-005-dispatch.14): what the row says, and — where the choice cannot
/// stand — the whole reason, which is what keeps a key from being advertised
/// on an entry it cannot act on (§FS-004-quick-actions.2).
#[derive(Debug, Clone)]
pub struct Handed {
    pub says: String,
    pub refusal: Option<String>,
}

/// What an entry says when it asks for work instead of running a command
/// (§FS-005-dispatch.1): the brief the ticket carries, the state it starts in,
/// and the hand that does it. The entry's own id, icon, description and `when`
/// are the recipe's — a project writing one agent entry writes one entry,
/// not an entry and a recipe that have to agree.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentAsk {
    brief: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    hand: Option<crate::work::recipe::HandPin>,
}

/// The parse of an entry, before the one rule that cannot be expressed in the
/// shape: an entry runs a command or asks for work, never both and never
/// neither (§FS-005-dispatch.1). Refused here, where the person can still see
/// what they wrote.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAction {
    #[serde(default)]
    id: String,
    icon: String,
    description: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    agent: Option<AgentAsk>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    kinds: Vec<String>,
    #[serde(default)]
    when: crate::work::recipe::Selector,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    requires_checkout: bool,
    #[serde(default)]
    confirm: bool,
    #[serde(default)]
    background: bool,
}

impl TryFrom<RawAction> for ActionConfig {
    type Error = String;

    fn try_from(raw: RawAction) -> std::result::Result<ActionConfig, String> {
        let named = match raw.id.is_empty() {
            true => format!("'{}'", raw.description),
            false => format!("'{}'", raw.id),
        };
        let (command, ask) = match (raw.command, raw.agent) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "action {named} carries both a 'command' and an 'agent': an entry either runs \
                 something here or asks somebody for it, never both"
                ))
            }
            (None, None) => {
                return Err(format!(
                "action {named} carries neither a 'command' nor an 'agent': an entry has to say \
                 what it does"
            ))
            }
            (Some(command), None) => (command, None),
            (None, Some(ask)) => (String::new(), Some(ask)),
        };
        // The id is the ticket's name and the key a hands table answers by
        // (§FS-006-project-interface.9), so work that asks for a hand has to
        // have one.
        if ask.is_some() && raw.id.is_empty() {
            return Err(format!(
                "action {named} asks for work and has no 'id': the id names its ticket and is what \
                 a hands table answers by"
            ));
        }
        let agent = ask.map(|ask| crate::work::recipe::Recipe {
            id: raw.id.clone(),
            icon: raw.icon.clone(),
            description: raw.description.clone(),
            state: ask.state.unwrap_or_else(crate::work::recipe::default_state),
            when: raw.when.clone(),
            needs_checkout: raw.requires_checkout,
            brief: ask.brief,
            // Ephor's own deterministic moves belong to the recipes that ship
            // with them (§FS-005-dispatch.12); an entry a person wrote asks
            // for what it says and nothing before it.
            opens_with: None,
            hand: ask.hand,
            target: None,
            model: None,
        });
        Ok(ActionConfig {
            id: raw.id,
            icon: raw.icon,
            description: raw.description,
            command,
            agent,
            hand: None,
            cwd: raw.cwd,
            kinds: raw.kinds,
            when: raw.when,
            requires: raw.requires,
            requires_checkout: raw.requires_checkout,
            confirm: raw.confirm,
            background: raw.background,
        })
    }
}

impl ActionConfig {
    /// Whether this entry is offered on an item: the selector, plus the older
    /// `kinds` spelling, which is read as if it were `when.kinds`.
    pub fn matches(
        &self,
        item: &crate::feed::model::Item,
        facts: &crate::work::recipe::Facts,
    ) -> bool {
        if !self.kinds.is_empty() && self.when.kinds.is_empty() {
            let mut widened = self.when.clone();
            widened.kinds = self.kinds.clone();
            return widened.matches(item, facts);
        }
        self.when.matches(item, facts)
    }

    /// The rungs it named, and the words it named that are not rungs — one
    /// pass, because an unknown requirement has to be said out loud rather
    /// than silently satisfied.
    pub fn rungs(&self) -> (Vec<crate::capabilities::Rung>, Vec<String>) {
        let mut rungs = Vec::new();
        let mut unknown = Vec::new();
        for name in &self.requires {
            match crate::capabilities::Rung::parse(name) {
                Some(rung) => rungs.push(rung),
                None => unknown.push(name.clone()),
            }
        }
        (rungs, unknown)
    }
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

#[derive(Debug, Clone, Deserialize)]
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

// Cloned to hand a whole project's configuration to the thread a background
// refresh runs on (§FS-001-forge-interface.7).
#[derive(Debug, Clone, Deserialize)]
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

    /// An entry may carry a brief instead of a command, and it is a recipe
    /// under another name: the entry's own id, icon, description and `when`
    /// are the recipe's, so a project writing one agent entry writes one entry
    /// (§FS-005-dispatch.1).
    #[test]
    fn an_action_may_ask_for_work_instead_of_running_a_command() {
        let config: StatusConfig = serde_json::from_str(
            r#"{
                "actions": [
                    { "id": "changelog", "icon": "✎", "description": "write the changelog bullet",
                      "when": { "kinds": ["pr"] },
                      "agent": { "brief": "Write the bullet for {title}.",
                                 "state": "fix", "hand": "luna:high" } }
                ]
            }"#,
        )
        .unwrap();
        let entry = &config.actions[0];
        assert!(entry.command.is_empty());
        let recipe = entry.agent.as_ref().expect("it asks for work");
        assert_eq!(recipe.id, "changelog");
        assert_eq!(recipe.icon, "✎");
        assert_eq!(recipe.description, "write the changelog bullet");
        assert_eq!(recipe.brief, "Write the bullet for {title}.");
        assert_eq!(recipe.state, "fix");
        assert_eq!(recipe.when.kinds, ["pr"]);
        assert_eq!(
            recipe.hand,
            Some(crate::work::recipe::HandPin::Named {
                id: "luna".to_string(),
                effort: Some("high".to_string()),
            })
        );
        // Unwritten, the ticket starts where the recipes do.
        let plain: ActionConfig = serde_json::from_value(serde_json::json!({
            "id": "ask", "icon": "◆", "description": "d", "agent": { "brief": "b" }
        }))
        .unwrap();
        assert_eq!(
            plain.agent.unwrap().state,
            crate::work::recipe::default_state()
        );
    }

    /// An entry runs a command or asks for work, never both and never neither
    /// — refused where the person can still see what they wrote
    /// (§FS-005-dispatch.1). And work needs a name: the id is the ticket's and
    /// the key a hands table answers by (§FS-006-project-interface.9).
    #[test]
    fn an_entry_that_neither_runs_nor_asks_is_refused() {
        let both = serde_json::from_value::<ActionConfig>(serde_json::json!({
            "id": "x", "icon": "◆", "description": "d",
            "command": "true", "agent": { "brief": "b" }
        }))
        .unwrap_err()
        .to_string();
        assert!(both.contains("never both"), "{both}");

        let neither = serde_json::from_value::<ActionConfig>(
            serde_json::json!({ "icon": "◆", "description": "do a thing" }),
        )
        .unwrap_err()
        .to_string();
        assert!(neither.contains("has to say what it does"), "{neither}");

        let nameless = serde_json::from_value::<ActionConfig>(serde_json::json!({
            "icon": "◆", "description": "d", "agent": { "brief": "b" }
        }))
        .unwrap_err()
        .to_string();
        assert!(nameless.contains("no 'id'"), "{nameless}");

        // And a typo inside the block is caught with the rest.
        assert!(serde_json::from_value::<ActionConfig>(serde_json::json!({
            "id": "x", "icon": "◆", "description": "d", "agent": { "breif": "b" }
        }))
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
