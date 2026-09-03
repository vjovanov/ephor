//! One machine, two organizations, and a maintenance loop that hands work
//! over across every project it reaches (§FS-011-command-line.10).
//!
//! Once a selector actually scopes a verb, `ephor work dispatch --org
//! foundation` writes a ticket in every project the organization holds, and
//! the only thing between that width and the act is the caller remembering to
//! type `--dry-run`. Reading at any width is free; writing at a width above
//! one checkout is a different kind of act, and the first sweep over a whole
//! organization is not the moment to discover it. The scenario is that
//! reproduction from the outside: the wide sweep must report and write
//! nothing, `--act` must do exactly what the sweep did before, and a
//! single-project turn must be untouched.

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::json;
use support::*;

/// The second project, in the other organization, watched beside the first.
const OTHER: &str = "far";

/// A world watching two organizations, each with one project reporting one
/// status item that names itself — so a sweep that reaches both says out loud
/// that it did.
fn two_organizations() -> World {
    let world = World::new();
    world.organize("foundation", "Foundation");
    world.file("status.txt", "demo is well\n");

    let far = world.path().join(OTHER);
    std::fs::create_dir_all(&far).expect("the other forest");
    std::fs::write(far.join("status.txt"), "far is well\n").expect("its status");

    let mut registry = world.registry_doc();
    registry["organizations"]
        .as_array_mut()
        .expect("organizations")
        .push(json!({ "id": "elsewhere", "name": "Elsewhere" }));
    let mut row = registry["projects"][0].clone();
    row["id"] = json!(OTHER);
    row["display_name"] = json!("Far");
    row["organization"] = json!("elsewhere");
    row["root"] = json!(far.to_string_lossy());
    registry["projects"]
        .as_array_mut()
        .expect("projects")
        .push(row);
    write_json(&world.registry_path(), &registry);

    let watching =
        json!({ "providers": [{ "provider": "custom-status", "command": "cat status.txt" }] });
    world.configure(json!({
        "work": {
            "recipes": [{
                "id": "look",
                "description": "look at it",
                "brief": "look at {title}",
                "when": { "kinds": ["status"] }
            }]
        },
        "projects": { PROJECT: watching.clone(), OTHER: watching }
    }));
    world.ephor().args(["refresh"]).assert().success();
    world
}

/// Ephor's own record of what it handed over. Nothing here means nothing was
/// handed over, which is the whole claim a report makes.
fn ledger(world: &World) -> std::path::PathBuf {
    world.path().join("state/ephor/work.json")
}

/// The reproducer: a bare sweep at a site watching two projects used to hand
/// work over across both. It now says what it would do, writes nothing, and
/// names the word that does it — and under that word it acts as it always did.
#[test]
fn a_sweep_above_one_project_reports_and_acts_only_when_told_to() {
    let world = two_organizations();

    world
        .ephor()
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("would open"))
        .stdout(predicate::str::contains("Pass --act to do it."));
    assert!(
        !ledger(&world).exists(),
        "a sweep that reported still wrote {}",
        ledger(&world).display()
    );

    let held = world
        .ephor_raw()
        .args(["work", "dispatch", "--json"])
        .output()
        .expect("ran");
    let reading = json_of(&held);
    assert_eq!(reading["gated"], json!(true), "{reading:#}");
    assert_eq!(reading["dry_run"], json!(true), "{reading:#}");
    assert!(
        reading["says"]
            .as_str()
            .unwrap_or_default()
            .contains("--act"),
        "a gated report must say how to act: {reading:#}"
    );

    // And with the word said, the sweep this command line always made.
    let acted = world
        .ephor_raw()
        .args(["work", "dispatch", "--act", "--json"])
        .output()
        .expect("ran");
    let acted = json_of(&acted);
    assert_eq!(acted["dry_run"], json!(false), "{acted:#}");
    assert!(acted["gated"].is_null(), "{acted:#}");
    assert!(
        acted["opened"].as_u64().unwrap_or_default() > 0,
        "--act opened nothing: {acted:#}"
    );
    assert!(ledger(&world).exists(), "--act wrote nothing");
}

/// A turn that reaches one project is what this command line did before the
/// rule, byte for byte — whether the selector narrowed it or `--project` did.
#[test]
fn one_project_is_untouched() {
    let world = two_organizations();

    let one = world
        .ephor_raw()
        .args(["work", "dispatch", "--org", "elsewhere", "--json"])
        .output()
        .expect("ran");
    let one = json_of(&one);
    assert!(one["gated"].is_null(), "{one:#}");
    assert_eq!(one["dry_run"], json!(false), "{one:#}");
    assert!(
        one["opened"].as_u64().unwrap_or_default() > 0,
        "a single-project sweep opened nothing: {one:#}"
    );
    assert!(
        ledger(&world).exists(),
        "a single-project sweep wrote nothing"
    );
}

/// `--act` is taken where the gate can fire and refused by name everywhere
/// else, exit 2 — the code every scope refusal takes — and under `--json` an
/// outcome on standard output like any other answer. `update` sweeps every
/// managed workspace and is deliberately outside the gate; its refusal says
/// that rather than pretending it sweeps nothing.
#[test]
fn the_flag_is_refused_where_it_would_change_nothing() {
    let world = two_organizations();

    world
        .ephor()
        .args(["rebase", "--act"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("rebase does not take --act"));

    world
        .ephor()
        .args(["update", "--act"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "it acts at every width, as it always has",
        ));

    let refused = world
        .ephor_raw()
        .args(["status", "--cached", "--act", "--json"])
        .output()
        .expect("ran");
    assert_eq!(refused.status.code(), Some(2));
    let outcome = json_of(&refused);
    assert_eq!(outcome["ok"], json!(false));
    assert!(
        outcome["says"]
            .as_str()
            .unwrap_or_default()
            .contains("status does not take --act"),
        "{outcome:#}"
    );

    // And it is advertised once, beside the selectors it belongs with.
    let help = world.ephor_raw().args(["--help"]).output().expect("ran");
    let help = String::from_utf8(help.stdout).expect("utf-8");
    assert!(help.contains("--act"), "{help}");
}
