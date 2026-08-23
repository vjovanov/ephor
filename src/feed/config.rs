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

#[derive(Debug, Clone, Default, Deserialize)]
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
    /// The workflow this entry lays down, where it lays one down rather than
    /// running a command or asking for a ticket (§FS-005-dispatch.19). The
    /// workflow is the runtime's; what is here is its id and the answers this
    /// entry gives its inputs.
    pub workflow: Option<WorkflowAsk>,
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
    /// Its program runs in a window of the reader's own instead of taking the
    /// terminal, and ephor stays where it was (§FS-005-dispatch.22). Said as
    /// `background` is said, and for the entry that *is* a program somebody
    /// types into — an editor, a pager, a coding agent's own session
    /// (§FS-006-project-interface.9). Where no window can be opened the entry
    /// takes the terminal as it always did, and says so.
    pub window: bool,
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
    /// This work needs nobody to start it (§FS-005-dispatch.24). An entry
    /// that asks for work is a recipe under another name, so it says this
    /// the way a recipe does.
    #[serde(default)]
    autorun: bool,
}

/// What an entry says when it lays down a workflow the runtime offers
/// (§FS-005-dispatch.19): which workflow, what its inputs are answered with,
/// and which of them name who does the work.
#[derive(Debug, Clone, Default)]
pub struct WorkflowAsk {
    /// The workflow's id, as the runtime knows it.
    pub name: String,
    /// What this entry answers the workflow's inputs with. A string is
    /// rendered with the item's fields where it names them, exactly as a
    /// brief is; anything else is passed on as it stands, because an input
    /// wanting a number, a flag or a list of them is not served by a sentence
    /// (§FS-005-dispatch.19).
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// The inputs that name who does the work, for a workflow whose own
    /// manifest does not say so — listing one is how a person says "this
    /// input is a hand" (§DA-006-hands-fill-a-workflows-targets).
    pub hands: Vec<String>,
}

/// The parse of an entry, before the one rule that cannot be expressed in the
/// shape: an entry runs a command, asks for work, or lays down a workflow —
/// exactly one of the three (§FS-005-dispatch.1, §FS-005-dispatch.19).
/// Refused here, where the person can still see what they wrote.
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
    workflow: Option<String>,
    #[serde(default)]
    inputs: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    hands: Vec<String>,
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
    #[serde(default)]
    window: bool,
}

impl TryFrom<RawAction> for ActionConfig {
    type Error = String;

    fn try_from(raw: RawAction) -> std::result::Result<ActionConfig, String> {
        let named = match raw.id.is_empty() {
            true => format!("'{}'", raw.description),
            false => format!("'{}'", raw.id),
        };
        // The names ephor mints for its own rows are ephor's
        // (§AR-009-surfaces.1). Refused here, where the person can still see
        // what they wrote: an entry configured as `@command` would otherwise
        // stand beside the freehand row under the same name, and the command
        // that runs an entry by id — which takes the first that answers to it
        // — would run whichever of the two came first.
        if let Some(why) = crate::api::offers::reserved(&raw.id) {
            return Err(format!("action {named} is refused: {why}"));
        }
        // One entry says one thing: it runs something here, it asks somebody
        // for a ticket, or it lays down a workflow the runtime offers
        // (§FS-005-dispatch.1, §FS-005-dispatch.19). Two of them would leave
        // one silently unused.
        let said: Vec<&str> = [
            raw.command.as_ref().map(|_| "command"),
            raw.agent.as_ref().map(|_| "agent"),
            raw.workflow.as_ref().map(|_| "workflow"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if said.len() > 1 {
            return Err(format!(
                "action {named} carries {}: an entry either runs something here, asks somebody \
                 for it, or lays down a workflow — never more than one",
                said.iter()
                    .map(|word| format!("'{word}'"))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ));
        }
        if said.is_empty() {
            return Err(format!(
                "action {named} carries none of 'command', 'agent' or 'workflow': an entry has to \
                 say what it does"
            ));
        }
        // Beneath the screen and in a window of the reader's own are two
        // different places, and an entry saying both leaves one of them
        // silently unused (§FS-005-dispatch.17, §FS-005-dispatch.22).
        if raw.background && raw.window {
            return Err(format!(
                "action {named} says both 'background' and 'window': a move that needs nobody runs \
                 beneath the screen, and a program somebody types into runs in a window — never \
                 both"
            ));
        }
        let ask = raw.agent;
        let command = raw.command.unwrap_or_default();
        let workflow = raw.workflow.map(|name| WorkflowAsk {
            name,
            inputs: raw.inputs,
            hands: raw.hands,
        });
        // The id is the ticket's name and the key a hands table answers by
        // (§FS-006-project-interface.9), so work that asks for a hand has to
        // have one. A workflow entry is under the same rule for the same
        // reason: what it lays down is named after it, and a hands table
        // answers for it by id (§FS-005-dispatch.19).
        if (ask.is_some() || workflow.is_some()) && raw.id.is_empty() {
            return Err(format!(
                "action {named} hands work over and has no 'id': the id names what it writes and \
                 is what a hands table answers by"
            ));
        }
        let agent = ask.map(|ask| crate::work::recipe::Recipe {
            id: raw.id.clone(),
            icon: raw.icon.clone(),
            description: raw.description.clone(),
            state: ask.state.unwrap_or_else(crate::work::recipe::default_state),
            when: raw.when.clone(),
            needs_checkout: raw.requires_checkout,
            autorun: ask.autorun,
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
            workflow,
            hand: None,
            cwd: raw.cwd,
            kinds: raw.kinds,
            when: raw.when,
            requires: raw.requires,
            requires_checkout: raw.requires_checkout,
            confirm: raw.confirm,
            background: raw.background,
            window: raw.window,
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
    /// Which window opener fills the seam (§FS-005-dispatch.22): a shipped
    /// binding by name, or a pair of commands of the reader's own. Unset, the
    /// environment ephor was started in is recognized; where nothing is bound
    /// and nothing is recognized the terminal is handed over as it always was
    /// (§AR-002-summons.6).
    #[serde(default)]
    pub window: Option<crate::seams::window::Binding>,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            ttl_seconds: default_ttl(),
            provider_timeout_seconds: default_timeout(),
            github_user: None,
            recent_days: default_recent_days(),
            window: None,
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

    /// An entry runs a command, asks for work, or lays down a workflow —
    /// exactly one, refused where the person can still see what they wrote
    /// (§FS-005-dispatch.1, §FS-005-dispatch.19). And work needs a name: the
    /// id names what it writes and is the key a hands table answers by
    /// (§FS-006-project-interface.9).
    #[test]
    fn an_entry_that_neither_runs_nor_asks_is_refused() {
        let both = serde_json::from_value::<ActionConfig>(serde_json::json!({
            "id": "x", "icon": "◆", "description": "d",
            "command": "true", "agent": { "brief": "b" }
        }))
        .unwrap_err()
        .to_string();
        assert!(both.contains("never more than one"), "{both}");

        let with_workflow = serde_json::from_value::<ActionConfig>(serde_json::json!({
            "id": "x", "icon": "◆", "description": "d",
            "command": "true", "workflow": "changeset-review"
        }))
        .unwrap_err()
        .to_string();
        assert!(
            with_workflow.contains("never more than one"),
            "{with_workflow}"
        );

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

        // A workflow entry is under the same rule for the same reason
        // (§FS-005-dispatch.19).
        let unnamed_workflow = serde_json::from_value::<ActionConfig>(serde_json::json!({
            "icon": "◆", "description": "d", "workflow": "changeset-review"
        }))
        .unwrap_err()
        .to_string();
        assert!(unnamed_workflow.contains("no 'id'"), "{unnamed_workflow}");

        // And a typo inside the block is caught with the rest.
        assert!(serde_json::from_value::<ActionConfig>(serde_json::json!({
            "id": "x", "icon": "◆", "description": "d", "agent": { "breif": "b" }
        }))
        .is_err());
    }

    /// An id in ephor's own namespace is refused where it is written, not
    /// discovered by a reader whose `ephor actions run @command` ran their
    /// entry instead of the freehand row (§FS-005-dispatch.10).
    #[test]
    fn a_configured_id_may_not_claim_one_of_ephors_own_rows() {
        let refused = serde_json::from_value::<ActionConfig>(serde_json::json!({
            "id": "@command", "icon": "⌨", "description": "mine", "command": "true"
        }))
        .expect_err("'@command' is not a name configuration may take")
        .to_string();
        assert!(refused.contains("@command"), "{refused}");
        assert!(refused.contains("Give it a name of your own"), "{refused}");

        // A name of one's own is untouched, including one that merely contains
        // the marker.
        assert!(serde_json::from_value::<ActionConfig>(serde_json::json!({
            "id": "note@work", "icon": "✎", "description": "d", "command": "true"
        }))
        .is_ok());
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
    // A recipe is a menu entry under another name (§FS-005-dispatch.1), so it
    // is held to the same rule about ephor's own namespace as an action is —
    // and here rather than in `Recipe` itself, because the recipes a project's
    // manifest and the runtime supply do not come through this file.
    let recipes = config
        .work
        .recipes
        .iter()
        .map(|recipe| ("work.recipes".to_string(), recipe))
        .chain(config.projects.iter().flat_map(|(id, project)| {
            project
                .work
                .recipes
                .iter()
                .map(move |recipe| (format!("projects.{id}.work.recipes"), recipe))
        }));
    for (where_, recipe) in recipes {
        if let Some(why) = crate::api::offers::reserved(&recipe.id) {
            return Err(registry_error(format!(
                "Feed config {where_} has a recipe that is refused: {why}"
            )));
        }
    }
    Ok(config)
}
