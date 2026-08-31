//! The published schema for every `--json` shape (§AR-009-surfaces.3).
//!
//! What `--json` prints is a declared shape, not whatever a struct happened to
//! serialize to: what a release may change is answerable by diffing this
//! document (§REQ-002-parity.4). `ephor schema views` prints it verbatim,
//! beside the manifest, answer, registry and forge schemas
//! (§FS-006-project-interface.11).
//!
//! [`SHAPES`] says which command prints which shape, and it is held from both
//! ends by the tests below. One walks the actual command tree and fails on a
//! `--json` that names no shape; the others hold every name here to the
//! published document and back. A command that gains a machine form without a
//! schema therefore fails the build rather than shipping undocumented — the
//! list is checked, not remembered (§REQ-002-parity.5).

/// The published views schema, verbatim.
pub const VIEWS_SCHEMA: &str = include_str!("../../assets/ephor-views.schema.json");

/// Every `--json` a command takes, and the shape it prints, by the name the
/// schema files it under.
///
/// A command is named by its path, as the parity list names one. Several
/// commands share a shape and that is the point: `ephor actions run`,
/// `ephor react`, `ephor tick` and `ephor reply` all print an `outcome`
/// because they are all one move returning what it changed
/// (§AR-009-surfaces.1). The moves that are sweeps or replays rather than one
/// entry running print shapes of their own.
///
/// A path may carry the flag that *selects* the shape, as `check
/// --list-features` does: one command answering two different questions prints
/// two different documents, and saying so is more honest than one entry
/// describing whichever of them somebody thought of first. The flag is not part
/// of the command path — [`command_path`] drops it — so the walk of the command
/// tree still resolves it.
///
/// A refusal is not on this list. Under `--json` every command that fails
/// prints an `outcome` with `ok` false, wherever the refusal came from
/// (`src/main.rs`), because a program that reads only standard output has to
/// learn that the thing did not happen (§REQ-002-parity.3).
pub const SHAPES: &[(&str, &str)] = &[
    ("actions", "actions"),
    ("actions list", "actions"),
    ("actions run", "outcome"),
    ("actions open", "outcome"),
    ("branches", "branches"),
    ("operations", "operations"),
    ("operations attach", "outcome"),
    ("thread", "thread"),
    ("react", "outcome"),
    ("tick", "outcome"),
    ("reply", "outcome"),
    ("refresh", "refresh"),
    ("mark-read", "mark-read"),
    ("failures", "failures"),
    ("restart", "restart"),
    ("rebase", "rebase"),
    ("checkout", "checkout"),
    ("validate", "validate"),
    ("list", "list"),
    ("job list", "job"),
    ("job log", "job-log"),
    ("capabilities", "capabilities"),
    ("doctor", "doctor"),
    ("feed", "feed"),
    ("status", "status"),
    ("check", "check"),
    ("check --list-features", "features"),
    ("work list", "work-list"),
    ("work offers", "work"),
    ("work dispatch", "work-dispatch"),
    ("work ask", "work-ask"),
    ("work sync", "work-sync"),
    ("work cancel", "work-cancel"),
    ("work run", "work-run"),
    ("work lay", "work-lay"),
    ("work workflows", "work-workflows"),
    ("work forget", "work-forget"),
    ("work states", "work-states"),
    ("update", "update"),
    ("ensure-agents", "ensure-agents"),
];

/// Every shape this API publishes, by the name the schema files it under. The
/// tests below hold it to the document in both directions, so it is a
/// spelling of the document rather than a second list to keep true.
pub const NAMES: [&str; 34] = [
    "actions",
    "branches",
    "operations",
    "thread",
    "work",
    "outcome",
    "refresh",
    "mark-read",
    "failures",
    "restart",
    "checkout",
    "rebase",
    "validate",
    "list",
    "job",
    "job-log",
    "capabilities",
    "doctor",
    "feed",
    "status",
    "features",
    "check",
    "work-list",
    "work-dispatch",
    "work-ask",
    "work-sync",
    "work-cancel",
    "work-run",
    "work-lay",
    "work-workflows",
    "work-forget",
    "work-states",
    "update",
    "ensure-agents",
];

/// The command path of a [`SHAPES`] entry, without the flag that selects the
/// shape. `check --list-features` is one command answering a second question,
/// not a second command.
pub fn command_path(named: &str) -> String {
    named
        .split_whitespace()
        .filter(|word| !word.starts_with('-'))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Hold one command's actual output to the shape it publishes
/// (§REQ-002-parity.4), and return every way it does not.
///
/// This is the half the lists could never check. `every_shape_publishes_a_schema`
/// asks whether a *name* appears in the document, which a schema describing the
/// wrong thing entirely passes: `branches` was declared an object while the
/// command printed an array, and half a dozen fields were declared `string`
/// where the code emits `null`. A declared shape nobody validates against is a
/// declaration, not a contract — so `tests/e2e/cases/E2E-012-command-line-parity.rs`
/// runs the real commands and puts what they print through here.
///
/// The named shape's subschema is validated as a document of its own with the
/// shared `$defs` carried along, so `#/$defs/offer` still resolves.
pub fn holds(shape: &str, value: &serde_json::Value) -> Vec<String> {
    let document: serde_json::Value =
        serde_json::from_str(VIEWS_SCHEMA).expect("the views schema parses");
    let Some(mut subschema) = document
        .get("properties")
        .and_then(|shapes| shapes.get(shape))
        .cloned()
    else {
        return vec![format!("no published shape is called '{shape}'")];
    };
    if let (Some(object), Some(defs)) = (subschema.as_object_mut(), document.get("$defs")) {
        object.insert("$defs".to_string(), defs.clone());
    }
    let validator = match jsonschema::validator_for(&subschema) {
        Ok(validator) => validator,
        Err(err) => {
            return vec![format!(
                "the published '{shape}' shape is not a schema: {err}"
            )]
        }
    };
    validator
        .iter_errors(value)
        .map(|err| format!("{shape}{}: {err}", err.instance_path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Command, CommandFactory};

    fn document() -> serde_json::Value {
        serde_json::from_str(VIEWS_SCHEMA).expect("the views schema parses")
    }

    fn properties() -> serde_json::Map<String, serde_json::Value> {
        document()
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("the schema files shapes under `properties`")
            .clone()
    }

    /// Every path in the command tree, deepest first, that takes `--json`.
    /// Read off the tree rather than off anybody's memory: this is what makes
    /// the list below a check rather than a convention (§REQ-002-parity.5).
    fn machine_forms(at: &Command, path: &mut Vec<String>, found: &mut Vec<String>) {
        if !path.is_empty() && at.get_arguments().any(|arg| arg.get_long() == Some("json")) {
            found.push(path.join(" "));
        }
        for child in at.get_subcommands() {
            // A hidden command is not a surface: `job run` is the supervisor
            // the interface starts (§AR-002-summons.5), not something a reader
            // asks a reading of.
            if child.is_hide_set() {
                continue;
            }
            path.push(child.get_name().to_string());
            machine_forms(child, path, found);
            path.pop();
        }
    }

    /// A `--json` with no published shape is the thing §REQ-002-parity.4
    /// forbids: a surface printing its internals, where every refactor is a
    /// breaking change for whoever automated against it. Caught here, off the
    /// real command tree, rather than by the reader who parsed it.
    #[test]
    fn every_machine_form_names_a_shape() {
        let cli = crate::cli::Cli::command();
        let mut found = Vec::new();
        machine_forms(&cli, &mut Vec::new(), &mut found);
        for command in &found {
            assert!(
                SHAPES
                    .iter()
                    .any(|(named, _)| &command_path(named) == command),
                "`ephor {command} --json` prints a shape nothing declares — add it to \
                 SHAPES in src/api/schema.rs and to assets/ephor-views.schema.json"
            );
        }
        // And back: a command renamed out from under the list would leave the
        // list pointing at nothing, which is how a check quietly stops
        // checking.
        for (command, _) in SHAPES {
            let path = command_path(command);
            assert!(
                found.iter().any(|named| named == &path),
                "SHAPES names `ephor {command} --json`, which the command tree does not have"
            );
        }
    }

    /// The lists say a command prints a named shape; nothing here says the
    /// shape is *right*. That is the E2E case's, where real commands run and
    /// what they print goes through [`holds`] — so this only pins down that
    /// the machinery works and that a wrong shape is actually caught.
    #[test]
    fn a_published_shape_holds_what_matches_it_and_refuses_what_does_not() {
        let outcome = serde_json::json!({ "ok": true, "says": "done" });
        assert!(holds("outcome", &outcome).is_empty());
        // A missing required field, a field of the wrong type, and a shape
        // nobody publishes are each reported rather than passed.
        assert!(!holds("outcome", &serde_json::json!({ "ok": true })).is_empty());
        assert!(!holds("outcome", &serde_json::json!({ "ok": "yes", "says": "x" })).is_empty());
        assert!(!holds("no-such-shape", &outcome).is_empty());
        // An array shape is an array: the `branches` entry declared an object
        // while the command printed a list, which every name-membership check
        // in the tree passed.
        assert!(holds("branches", &serde_json::json!([])).is_empty());
        assert!(!holds("branches", &serde_json::json!({})).is_empty());
    }

    /// A shape without a schema is a `--json` nobody can rely on
    /// (§REQ-002-parity.4), so the three lists are held together here rather
    /// than by anyone remembering.
    #[test]
    fn every_shape_publishes_a_schema() {
        let properties = properties();
        for name in NAMES {
            assert!(
                properties.contains_key(name),
                "shape '{name}' publishes no schema — add it to assets/ephor-views.schema.json"
            );
        }
        for name in properties.keys() {
            assert!(
                NAMES.contains(&name.as_str()),
                "the schema documents '{name}', which is not a shape ephor prints"
            );
        }
        for (command, shape) in SHAPES {
            assert!(
                NAMES.contains(shape),
                "`ephor {command} --json` prints '{shape}', which is on no list"
            );
        }
    }

    /// Every published shape says what it is for. A schema entry with no
    /// description is one a reader has to guess at, which is the thing
    /// publishing it was meant to stop.
    #[test]
    fn every_published_shape_says_what_it_is() {
        for (name, shape) in properties() {
            assert!(
                shape.get("description").is_some(),
                "shape '{name}' publishes no description"
            );
        }
    }
}
