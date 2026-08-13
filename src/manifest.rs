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

#[derive(Debug, Clone, Deserialize)]
pub struct TicketStore {
    pub kind: String,
    pub path: String,
}

/// A menu entry the project offers (§FS-006-project-interface.9): the same
/// shape a person's configured action has, selected by the same language and
/// gated by the same rungs.
#[derive(Debug, Clone, Deserialize)]
pub struct Offer {
    pub id: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub when: crate::work::recipe::Selector,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub confirm: bool,
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
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            kinds: Vec::new(),
            when: self.when.clone(),
            requires: self.requires.clone(),
            // A project cannot ask ephor to make a workspace for it; what it
            // needs on disk it says through `requires` like everything else.
            requires_checkout: false,
            confirm: self.confirm,
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
    #[serde(default)]
    pub tickets: Vec<TicketStore>,
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
    if let Some(error) = validator().iter_errors(&value).next() {
        return Err(EphorError::Registry(format!(
            "{source} does not match the manifest schema at '{}': {error}",
            error.instance_path
        )));
    }
    serde_json::from_value(value)
        .map_err(|err| EphorError::Registry(format!("{source} could not be read: {err}")))
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
