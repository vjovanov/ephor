//! One machine, two organizations, and a person who scopes a turn to one of
//! them (§FS-011-command-line.9).
//!
//! A maintenance loop passes `--org foundation` to `refresh` and `work
//! dispatch` believing it scopes the turn. Before this rule the flag parsed
//! everywhere and was read by three commands, so the turn was site-wide and
//! nothing in what it printed said so — the loop dispatched the other
//! organization's work and the only way to find out was to run the same
//! command against both organizations and diff. The scenario is that
//! reproduction, from the outside: the two organizations must answer
//! differently, and a verb that will not scope must say so rather than
//! pretend.

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::json;
use support::*;

/// The second project, in the other organization, watched beside the first.
const OTHER: &str = "far";

/// A world watching two organizations, each with one project reporting one
/// status item that names itself.
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

/// The ticket's first reproducer: the two organizations printed the same
/// table. Now each prints its own, and neither is the site's.
#[test]
fn the_two_organizations_answer_differently() {
    let world = two_organizations();

    let mine = world
        .ephor()
        .args(["status", "--cached", "--org", "foundation"])
        .output();
    let theirs = world
        .ephor()
        .args(["status", "--cached", "--org", "elsewhere"])
        .output();
    let mine = String::from_utf8(mine.expect("ran").stdout).expect("utf-8");
    let theirs = String::from_utf8(theirs.expect("ran").stdout).expect("utf-8");
    assert_ne!(
        mine, theirs,
        "both organizations printed one table:\n{mine}"
    );
    assert!(mine.contains(PROJECT) && !mine.contains(OTHER), "{mine}");
    assert!(
        theirs.contains(OTHER) && !theirs.contains(PROJECT),
        "{theirs}"
    );

    // The feed is the same reading in one stream, and scopes the same way.
    world
        .ephor()
        .args(["feed", "--org", "elsewhere"])
        .assert()
        .success()
        .stdout(predicate::str::contains("far is well"))
        .stdout(predicate::str::contains("demo is well").not());
}

/// The ticket's third reproducer: a sweep scoped to one organization proposed
/// the other's work. Now it proposes that organization's and nothing else.
#[test]
fn a_scoped_dispatch_stays_inside_its_organization() {
    let world = two_organizations();

    let proposed = world
        .ephor_raw()
        .args([
            "work",
            "dispatch",
            "--org",
            "elsewhere",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("ran");
    let reading = json_of(&proposed);
    let items: Vec<&str> = reading["items"]
        .as_array()
        .expect("the items it would open")
        .iter()
        .map(|row| row["item"].as_str().unwrap_or_default())
        .collect();
    assert!(
        !items.is_empty(),
        "nothing was proposed at all: {reading:#}"
    );
    assert!(
        items.iter().all(|item| item.contains(OTHER)),
        "a project of the other organization was proposed: {items:?}"
    );
}

/// The ticket's fourth reproducer: `rebase` advertised the selectors and read
/// none of them. It now refuses by name, exits 2 — the code an empty selection
/// and every other usage-shaped refusal takes, so "the scope was refused" is
/// one comparison — and answers a program the way every other refusal does: an
/// outcome on standard output (§FS-011-command-line.7).
#[test]
fn a_verb_that_will_not_scope_says_so() {
    let world = two_organizations();

    world
        .ephor()
        .args(["rebase", "--org", "foundation"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("rebase does not take --org"));

    let refused = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--json"])
        .output()
        .expect("ran");
    assert_eq!(refused.status.code(), Some(2));
    let outcome = json_of(&refused);
    assert_eq!(outcome["ok"], json!(false));
    assert!(
        outcome["says"]
            .as_str()
            .unwrap_or_default()
            .contains("does not take --org"),
        "{outcome:#}"
    );

    // And `--all` is no longer advertised where it would mean nothing: it
    // belongs to the verbs that read it.
    let help = world
        .ephor_raw()
        .args(["rebase", "--help"])
        .output()
        .expect("ran");
    let help = String::from_utf8(help.stdout).expect("utf-8");
    assert!(!help.contains("--all"), "{help}");
    let update = world
        .ephor_raw()
        .args(["update", "--help"])
        .output()
        .expect("ran");
    let update = String::from_utf8(update.stdout).expect("utf-8");
    assert!(update.contains("--all"), "{update}");
}

/// A scope nobody watches is said rather than printed as a quiet site: the
/// failure this rule exists to end is the empty table nobody can tell from a
/// site with nothing to report.
#[test]
fn a_scope_that_selects_nothing_is_said() {
    let world = two_organizations();
    world
        .ephor()
        .args(["status", "--cached", "--org", "elsewhre"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--org elsewhre matches no project in",
        ));
}
