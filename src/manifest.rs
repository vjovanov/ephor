//! What a project says about itself: `ephor.json` at the forest root
//! (§FS-006-project-interface.2).
//!
//! Offered, never required (§DF-001-manifest-offered): every field is
//! optional, an empty manifest is valid, and nothing in it gates a capability
//! that probing or site configuration could not establish alone. Identity
//! fields are hints the registry row adopts unless it overrides — the row is
//! authoritative, because attribution keys must not be forgeable by a
//! checkout.
//!
//! Manifest commands run with exactly the trust a person extends to running
//! the project's own build, and the row can narrow that (§Trust).

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{EphorError, Result};

/// The published manifest schema (§FS-006-project-interface.11), embedded so a
/// project can be validated with no network and `ephor schema` can print it.
pub const MANIFEST_SCHEMA: &str = include_str!("../assets/ephor-manifest.schema.json");

/// The file a project places at its forest root to speak.
pub const FILE: &str = "ephor.json";

/// How much of a manifest the registry row is willing to believe
/// (§FS-006-project-interface.2). A checkout trusted less still describes
/// itself; it just does not get to run anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trust {
    /// Honor it fully: its commands run with the trust a person extends to
    /// the project's own build.
    #[default]
    Full,
    /// Read only what it says about itself — identity, layout, descriptions —
    /// and run none of it.
    Descriptions,
    /// Do not read it at all.
    Ignore,
}

impl Trust {
    /// What a row's `manifest_trust` field says. An unknown word is not a
    /// silent downgrade: it is a configuration error the caller reports.
    pub fn parse(value: &str) -> Result<Trust> {
        match value {
            "full" => Ok(Trust::Full),
            "descriptions" => Ok(Trust::Descriptions),
            "ignore" => Ok(Trust::Ignore),
            other => Err(EphorError::Registry(format!(
                "unknown manifest_trust '{other}': expected 'full', 'descriptions', or 'ignore'"
            ))),
        }
    }
}

/// A command the manifest binds, in either of the schema's two spellings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Binding {
    Line(String),
    Placed {
        command: String,
        #[serde(default)]
        cwd: Option<String>,
    },
}

impl Binding {
    pub fn command(&self) -> &str {
        match self {
            Binding::Line(command) => command,
            Binding::Placed { command, .. } => command,
        }
    }

    /// Where it runs, as the binding spells it — `root` or `repo:<name>`.
    pub fn cwd(&self) -> Option<&str> {
        match self {
            Binding::Line(_) => None,
            Binding::Placed { cwd, .. } => cwd.as_deref(),
        }
    }
}

/// Identity hints. The row adopts these where it says nothing of its own and
/// overrides them where it does (§FS-008-attribution.1).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Identity {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub ticket_patterns: Vec<String>,
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub territory: Vec<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
}

/// One repository of the forest as the project declares it
/// (§AR-004-forest). The row overrides paths and remotes, which are where
/// things sit on a particular machine.
#[derive(Debug, Clone, Deserialize)]
pub struct Repo {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Checks {
    #[serde(default)]
    pub check: Option<Binding>,
    #[serde(default)]
    pub style: Option<Binding>,
    #[serde(default)]
    pub smoke: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Gate {
    #[serde(default)]
    pub status: Option<Binding>,
    #[serde(default)]
    pub failures: Option<Binding>,
    #[serde(default)]
    pub restart: Option<Binding>,
}

/// A task store the project keeps somewhere other than a probed name
/// (§FS-006-project-interface.7).
#[derive(Debug, Clone, Deserialize)]
pub struct TaskStore {
    pub kind: String,
    pub path: String,
}

/// A menu entry the project offers (§FS-006-project-interface.9): the same
/// shape a person's configured action has, selected by the same language and
/// gated by the same rungs. It runs a command here, or lays down one of the
/// runtime's workflows (§FS-005-dispatch.19) — the schema refuses an offer
/// that says both or neither, so nothing here has to.
#[derive(Debug, Clone, Deserialize)]
pub struct Offer {
    pub id: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub description: String,
    #[serde(default)]
    pub command: Option<String>,
    /// The workflow this offer lays down, where it lays one down rather than
    /// running a command (§FS-005-dispatch.19).
    #[serde(default)]
    pub workflow: Option<String>,
    /// What it answers that workflow's inputs with.
    #[serde(default)]
    pub inputs: std::collections::BTreeMap<String, serde_json::Value>,
    /// Which of those inputs name who does the work
    /// (§DA-006-hands-fill-a-workflows-targets).
    #[serde(default)]
    pub hands: Vec<String>,
    /// The work this offer lays down needs nobody to start it
    /// (§FS-005-dispatch.28). Only on an offer that names a workflow: an
    /// offer that runs a command here has no run to start, and the schema
    /// refuses it there.
    #[serde(default)]
    pub autorun: bool,
    /// Which branch the work this offer lays down belongs on, where the matter
    /// has none of its own (§FS-005-dispatch.25). A template rendered from the
    /// matter's fields, and the one thing a project may say about where a
    /// workspace goes: what it names is its own branch, made under the
    /// registry's own template for them.
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub when: crate::work::recipe::Selector,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub confirm: bool,
    /// It runs beneath the screen as a job rather than taking the terminal
    /// (§FS-005-dispatch.17). An offer that needs no reader says so; the
    /// default is the terminal, because an offer may be a pager or an editor
    /// (§FS-006-project-interface.9).
    #[serde(default)]
    pub background: bool,
    /// It is such a program and should not take the terminal: it runs in a
    /// window of the reader's own where one is bound, and ephor stays beside it
    /// (§FS-005-dispatch.22, §FS-006-project-interface.9).
    #[serde(default)]
    pub window: bool,
}

/// What a project offers, as a menu entry. The icon is the only thing ephor
/// fills in: an offer that named none still has to look like the entries
/// beside it.
const OFFER_ICON: &str = "▸";

impl Offer {
    pub fn action(&self) -> crate::feed::config::ActionConfig {
        crate::feed::config::ActionConfig {
            id: self.id.clone(),
            icon: self.icon.clone().unwrap_or_else(|| OFFER_ICON.to_string()),
            description: self.description.clone(),
            command: self.command.clone().unwrap_or_default(),
            // A manifest offer runs a command or lays down a workflow; a
            // brief a project wants handed to an agent is a recipe of its own
            // (§FS-005-dispatch.1).
            agent: None,
            workflow: self
                .workflow
                .clone()
                .map(|name| crate::feed::config::WorkflowAsk {
                    name,
                    inputs: self.inputs.clone(),
                    hands: self.hands.clone(),
                    autorun: self.autorun,
                }),
            hand: None,
            cwd: self.cwd.clone(),
            kinds: Vec::new(),
            when: self.when.clone(),
            requires: self.requires.clone(),
            // A project cannot ask ephor to run a checkout before a command
            // it offers; what such an offer needs on disk it says through
            // `requires` like everything else. Work it lays down is the other
            // case: `branch` names the branch that work belongs on, and the
            // workspace made for it is that branch's own
            // (§FS-005-dispatch.25).
            requires_checkout: false,
            branch: self.branch.clone(),
            minted: None,
            confirm: self.confirm,
            background: self.background,
            window: self.window,
        }
    }
}

/// Everything a project chose to say.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub identity: Identity,
    #[serde(default)]
    pub forest: Vec<Repo>,
    #[serde(default)]
    pub checks: Checks,
    #[serde(default, rename = "ci")]
    pub gate: Gate,
    /// `tickets` is the older spelling of this key and is still read: the
    /// interface evolves by addition (§FS-006-project-interface.11), so a
    /// manifest somebody already wrote goes on meaning what it meant.
    #[serde(default, alias = "tickets")]
    pub tasks: Vec<TaskStore>,
    #[serde(default, rename = "actions")]
    pub offers: Vec<Offer>,
}

impl Manifest {
    /// Read the manifest a project placed at `root`, under the trust the row
    /// extends to it. None where there is none — which is most projects, and
    /// is a complete answer (§DF-001-manifest-offered).
    pub fn read(root: &Path, trust: Trust) -> Result<Option<Manifest>> {
        if trust == Trust::Ignore {
            return Ok(None);
        }
        let path = root.join(FILE);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| EphorError::Command(format!("Cannot read {}: {err}", path.display())))?;
        let mut manifest = parse(&text, &path.display().to_string())?;
        if trust == Trust::Descriptions {
            // Read what it says about itself; run none of it.
            manifest.checks = Checks::default();
            manifest.gate = Gate::default();
            manifest.offers.clear();
        }
        Ok(Some(manifest))
    }
}

/// Parse and validate one manifest against the published schema. Whatever
/// crosses this interface in structure is validated
/// (§FS-006-project-interface.11): a project that mistyped a field learns so
/// here rather than by a verb quietly never running.
pub fn parse(text: &str, source: &str) -> Result<Manifest> {
    let value: Value = serde_json::from_str(text)
        .map_err(|err| EphorError::Registry(format!("{source} is not JSON: {err}")))?;
    // Ahead of the schema, which says this too — a `not`, so that a manifest
    // validated by anything but ephor hears it as well — but says it as a
    // shape. The reader gets the sentence, and it is the one a person's own
    // configuration is refused in (§AR-005-capabilities.2).
    if let Some(why) = says_two_places(&value) {
        return Err(EphorError::Registry(format!("{source}: {why}")));
    }
    if let Some(why) = starts_a_command(&value) {
        return Err(EphorError::Registry(format!("{source}: {why}")));
    }
    if let Some(error) = validator().iter_errors(&value).next() {
        return Err(EphorError::Registry(format!(
            "{source} does not match the manifest schema at '{}': {error}",
            error.instance_path
        )));
    }
    serde_json::from_value(value)
        .map_err(|err| EphorError::Registry(format!("{source} could not be read: {err}")))
}

/// Why an offer here names two places to run in, or None where none does.
///
/// Beneath the screen and in a window of the reader's own are two different
/// places (§FS-005-dispatch.17, §FS-005-dispatch.22), and an offer saying both
/// leaves one of them silently unused: [`Session::how`] answered *beneath* and
/// the start opened a window anyway.
fn says_two_places(value: &Value) -> Option<String> {
    let flag = |offer: &Value, key: &str| offer.get(key).and_then(Value::as_bool).unwrap_or(false);
    value
        .get("actions")?
        .as_array()?
        .iter()
        .find(|offer| flag(offer, "background") && flag(offer, "window"))
        .map(|offer| {
            format!(
                "offer '{}' says both 'background' and 'window': a move that needs nobody runs \
                 beneath the screen, and a program somebody types into runs in a window — never \
                 both",
                offer
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("(unnamed)")
            )
        })
}

/// Why an offer here asks to run work it does not hand over, or None where
/// none does.
///
/// Work nobody has to start is said on the thing that hands work over
/// (§FS-005-dispatch.28), and an offer that runs a command here has no run to
/// start. The schema says it too — as a `dependentSchemas`, so a manifest
/// validated by anything but ephor hears it — but says it as a shape; the
/// reader gets the sentence, and it is the one a person's own configuration is
/// refused in (§AR-005-capabilities.2).
fn starts_a_command(value: &Value) -> Option<String> {
    value
        .get("actions")?
        .as_array()?
        .iter()
        .find(|offer| {
            offer
                .get("autorun")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && offer.get("workflow").is_none()
        })
        .map(|offer| {
            format!(
                "offer '{}' says 'autorun' and runs a command here: work nobody has to start is \
                 said on the thing that hands it over, and a command runs here — there is no run \
                 to start",
                offer
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("(unnamed)")
            )
        })
}

fn validator() -> &'static jsonschema::Validator {
    static VALIDATOR: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(MANIFEST_SCHEMA).expect("embedded manifest schema is valid JSON");
        jsonschema::validator_for(&schema).expect("embedded manifest schema is a valid schema")
    })
}

/// Site configuration over manifest over probe (§REQ-001-boundary.2,
/// §FS-006-project-interface.1) — one lookup, so that "who wins" is answered
/// the same way for every verb rather than per caller. Probing is defaulting,
/// a manifest is the project declaring what probing would have guessed, and
/// site configuration is the person overriding both.
pub fn resolve<T>(site: Option<T>, manifest: Option<T>, probe: Option<T>) -> Option<T> {
    site.or(manifest).or(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_schema_compiles() {
        let _ = validator();
    }

    #[test]
    fn an_empty_manifest_is_valid_because_the_file_is_an_offer() {
        let manifest = parse("{}", "ephor.json").unwrap();
        assert!(manifest.forest.is_empty());
        assert!(manifest.offers.is_empty());
        assert!(manifest.checks.check.is_none());
    }

    /// A project's own offer that lays a workflow down may say the work needs
    /// nobody to start it, and an offer that runs a command may not — there
    /// is no run to start (§FS-005-dispatch.28). The schema says it too, so a
    /// manifest validated by anything but ephor hears the same refusal.
    #[test]
    fn an_offer_that_lays_a_workflow_down_may_ask_to_run_itself() {
        let manifest = parse(
            r#"{"actions": [{"id": "fix-issue", "description": "fix it",
                             "workflow": "supervised-fix", "autorun": true}]}"#,
            "ephor.json",
        )
        .unwrap();
        let ask = manifest.offers[0]
            .action()
            .workflow
            .expect("it lays a workflow down");
        assert!(ask.autorun);

        let refused = parse(
            r#"{"actions": [{"id": "gate", "description": "gate",
                             "command": "just gate", "autorun": true}]}"#,
            "ephor.json",
        )
        .unwrap_err()
        .to_string();
        assert!(refused.contains("no run to start"), "{refused}");
    }

    #[test]
    fn a_manifest_reads_its_bindings_in_either_spelling() {
        let manifest = parse(
            r#"{"checks": {"check": "./check.sh",
                           "style": {"command": "./style.sh", "cwd": "repo:ce"}}}"#,
            "ephor.json",
        )
        .unwrap();
        assert_eq!(
            manifest.checks.check.as_ref().unwrap().command(),
            "./check.sh"
        );
        assert_eq!(manifest.checks.check.as_ref().unwrap().cwd(), None);
        let style = manifest.checks.style.as_ref().unwrap();
        assert_eq!(style.command(), "./style.sh");
        assert_eq!(style.cwd(), Some("repo:ce"));
    }

    /// Beneath the screen and in a window of the reader's own are two different
    /// places, and an offer saying both leaves one of them silently unused
    /// (§FS-005-dispatch.17, §FS-005-dispatch.22). Refused where it is written,
    /// in the same words a person's own configuration is refused in — the two
    /// paths used to disagree, one deciding *beneath* and the other opening a
    /// window anyway.
    #[test]
    fn an_offer_saying_both_background_and_window_is_refused() {
        let both = r#"{"actions": [
            {"id": "edit", "description": "open it", "command": "true",
             "background": true, "window": true}
        ]}"#;
        let err = parse(both, "ephor.json").expect_err("it is refused");
        assert!(err.to_string().contains("edit"), "{err}");
        assert!(err.to_string().contains("never both"), "{err}");

        // Either alone is an offer.
        for one in ["\"background\": true", "\"window\": true"] {
            let text = format!(
                r#"{{"actions": [{{"id": "edit", "description": "open it",
                    "command": "true", {one}}}]}}"#
            );
            parse(&text, "ephor.json").expect("one place is a place");
        }
    }

    #[test]
    fn a_mistyped_field_is_refused_where_it_is_written_rather_than_ignored() {
        // `cwd` is 'root' or 'repo:<name>' and nothing else.
        let err = parse(
            r#"{"checks": {"check": {"command": "./check.sh", "cwd": "somewhere"}}}"#,
            "ephor.json",
        )
        .unwrap_err();
        assert!(err.to_string().contains("manifest schema"), "{err}");

        let err = parse("not json", "ephor.json").unwrap_err();
        assert!(err.to_string().contains("not JSON"), "{err}");
    }

    #[test]
    fn an_offer_carries_the_rungs_it_needs() {
        let manifest = parse(
            r#"{"actions": [{"id": "rebuild", "description": "rebuild it",
                             "command": "./build.sh", "requires": ["checkout-able"]}]}"#,
            "ephor.json",
        )
        .unwrap();
        assert_eq!(manifest.offers[0].id, "rebuild");
        assert_eq!(manifest.offers[0].requires, vec!["checkout-able"]);
        assert!(!manifest.offers[0].confirm);
    }

    /// An offer is a menu entry in the shape a person's action has, selected
    /// by the same language and gated by the same rungs
    /// (§FS-006-project-interface.9) — one shape, so the menu cannot tell
    /// them apart except by where they came from.
    #[test]
    fn an_offer_becomes_a_menu_entry_of_the_same_shape() {
        let manifest = parse(
            r#"{"actions": [{"id": "bench", "description": "run the benchmarks",
                             "command": "./bench.sh", "cwd": "repo:ce",
                             "when": {"kinds": ["pr"], "gate": "green"},
                             "requires": ["checkable"], "confirm": true}]}"#,
            "ephor.json",
        )
        .unwrap();
        let action = manifest.offers[0].action();
        assert_eq!(action.id, "bench");
        assert_eq!(action.command, "./bench.sh");
        assert_eq!(action.cwd.as_deref(), Some("repo:ce"));
        assert_eq!(action.when.kinds, vec!["pr"]);
        assert_eq!(action.when.gate.as_deref(), Some("green"));
        assert!(action.confirm);
        assert_eq!(action.rungs().0, vec![crate::capabilities::Rung::Checkable]);
        // An offer that named no icon still looks like the entries beside it,
        // and a project cannot ask ephor to make it a workspace.
        assert_eq!(action.icon, OFFER_ICON);
        assert!(!action.requires_checkout);
    }

    /// An offer that lays a workflow down may say which branch that work
    /// belongs on, for the matter that has none (§FS-005-dispatch.25). An
    /// offer that runs a command may not: it runs in the workspace the project
    /// already has.
    #[test]
    fn an_offer_that_lays_work_down_may_say_the_branch_it_belongs_on() {
        let manifest = parse(
            r#"{"actions": [{"id": "do-issue", "description": "do the issue",
                             "workflow": "supervised-ticket-fix",
                             "branch": "fix/issue-{number}",
                             "when": {"kinds": ["issue"]}}]}"#,
            "ephor.json",
        )
        .unwrap();
        assert_eq!(
            manifest.offers[0].action().branch.as_deref(),
            Some("fix/issue-{number}")
        );

        let err = parse(
            r#"{"actions": [{"id": "bench", "description": "d", "command": "./bench.sh",
                             "branch": "fix/issue-{number}"}]}"#,
            "ephor.json",
        )
        .unwrap_err();
        assert!(err.to_string().contains("manifest schema"), "{err}");
    }

    /// The selector is the recipes' language, so a field neither of them has
    /// is refused where it is written rather than ignored
    /// (§FS-006-project-interface.11).
    #[test]
    fn an_offer_selecting_on_something_nobody_selects_on_is_refused() {
        let err = parse(
            r#"{"actions": [{"id": "x", "description": "d", "command": "c",
                             "when": {"phase": "nightly"}}]}"#,
            "ephor.json",
        )
        .unwrap_err();
        assert!(err.to_string().contains("manifest schema"), "{err}");
    }

    #[test]
    fn a_checkout_trusted_less_still_describes_itself() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(FILE),
            r#"{"identity": {"aliases": ["the widget"]},
                "checks": {"check": "./check.sh"},
                "actions": [{"id": "x", "description": "d", "command": "c"}]}"#,
        )
        .unwrap();

        let full = Manifest::read(tmp.path(), Trust::Full).unwrap().unwrap();
        assert_eq!(full.identity.aliases, vec!["the widget"]);
        assert!(full.checks.check.is_some());
        assert_eq!(full.offers.len(), 1);

        // Descriptions only: what it says about itself survives, what it would
        // run does not.
        let narrowed = Manifest::read(tmp.path(), Trust::Descriptions)
            .unwrap()
            .unwrap();
        assert_eq!(narrowed.identity.aliases, vec!["the widget"]);
        assert!(narrowed.checks.check.is_none());
        assert!(narrowed.offers.is_empty());

        // Ignored: not read at all.
        assert!(Manifest::read(tmp.path(), Trust::Ignore).unwrap().is_none());
    }

    #[test]
    fn a_project_that_placed_no_manifest_is_a_complete_answer() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(Manifest::read(tmp.path(), Trust::Full).unwrap().is_none());
    }

    #[test]
    fn the_person_outranks_the_project_which_outranks_the_probe() {
        assert_eq!(
            resolve(Some("site"), Some("manifest"), Some("probe")),
            Some("site")
        );
        assert_eq!(
            resolve(None, Some("manifest"), Some("probe")),
            Some("manifest")
        );
        assert_eq!(resolve(None, None, Some("probe")), Some("probe"));
        assert_eq!(resolve::<&str>(None, None, None), None);
    }

    #[test]
    fn a_trust_word_nobody_recognizes_is_an_error_not_a_downgrade() {
        assert_eq!(Trust::parse("full").unwrap(), Trust::Full);
        assert_eq!(Trust::parse("descriptions").unwrap(), Trust::Descriptions);
        assert_eq!(Trust::parse("ignore").unwrap(), Trust::Ignore);
        assert!(Trust::parse("mostly").is_err());
    }
}

#[cfg(test)]
mod row_tests {
    use super::*;
    use crate::branches::Placement;

    fn placement(root: &std::path::Path, trust: Trust) -> Placement {
        Placement {
            project: "widget".to_string(),
            root: root.to_path_buf(),
            template: None,
            branches: Vec::new(),
            main_branch: Some("main".to_string()),
            repos: Vec::new(),
            aliases: Vec::new(),
            territory: Vec::new(),
            trust,
        }
    }

    /// The row adopts what the project says where it says nothing itself, and
    /// overrides it where it does — the row is authoritative, because
    /// attribution keys must not be forgeable by a checkout
    /// (§FS-008-attribution.1).
    #[test]
    fn the_row_adopts_the_projects_hints_and_overrides_them() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(FILE),
            r#"{"identity": {"aliases": ["from the manifest"],
                             "territory": ["acme-labs"],
                             "addresses": ["widget@acme.example"]}}"#,
        )
        .unwrap();

        // Row silent: the hints are adopted.
        let adopted = placement(tmp.path(), Trust::Full).identity();
        assert_eq!(adopted.aliases, vec!["from the manifest"]);
        assert_eq!(adopted.territory, vec!["acme-labs"]);
        assert_eq!(adopted.addresses, vec!["widget@acme.example"]);

        // Row speaks: it wins, and the hint does not creep in beside it.
        let mut row = placement(tmp.path(), Trust::Full);
        row.aliases = vec!["from the row".to_string()];
        row.territory = vec!["acme".to_string()];
        let overridden = row.identity();
        assert_eq!(overridden.aliases, vec!["from the row"]);
        assert_eq!(overridden.territory, vec!["acme"]);

        // A checkout trusted less still describes itself.
        let narrowed = placement(tmp.path(), Trust::Descriptions).identity();
        assert_eq!(narrowed.aliases, vec!["from the manifest"]);

        // And one nobody reads claims nothing at all.
        let ignored = placement(tmp.path(), Trust::Ignore).identity();
        assert!(ignored.aliases.is_empty());
        assert!(ignored.territory.is_empty());
    }

    /// The manifest's layout is used where the row declares none — a project
    /// that knows its own shape says so once (§AR-004-forest).
    #[test]
    fn the_forest_layout_comes_from_the_manifest_where_the_row_declares_none() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["ce", "ee"] {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            assert!(std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&path)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(
            tmp.path().join(FILE),
            r#"{"forest": [{"name": "ce", "path": "ce", "role": "community"},
                           {"name": "ee", "path": "ee"}]}"#,
        )
        .unwrap();

        let forest = placement(tmp.path(), Trust::Full).forest(tmp.path());
        assert_eq!(forest.names(), vec!["ce", "ee"]);
        assert_eq!(forest.repos[0].role.as_deref(), Some("community"));

        // Ignored, there is nothing to adopt, so it is probed as before.
        let probed = placement(tmp.path(), Trust::Ignore).forest(tmp.path());
        assert_eq!(
            probed.names(),
            vec!["ce", "ee"],
            "probing finds them anyway"
        );
    }
}
