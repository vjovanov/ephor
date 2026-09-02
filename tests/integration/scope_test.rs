//! The scope selectors, across the two files they have to bridge.
//!
//! `--workspace`, `--tag` and `--org` name rows of the **registry**, and the
//! verbs that read a set of projects pick theirs from the **site's watch
//! list**. One session is built over the projects that survive both
//! (§AR-009-surfaces.2), so the screen, `ephor branches` and every table
//! answer about the same projects; the rest of the verbs refuse a selector
//! they would have ignored. The world here is two organizations, because a
//! scope that is not applied looks exactly like one that is until there is a
//! second organization to leave out.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::{json, Value};

use common::*;

/// Two organizations, three projects, one watched feed each. Every project
/// reports one `custom-status` item naming itself, so a table, a feed and a
/// dispatch all say out loud which projects they read.
fn two_orgs(tmp: &Path) {
    let template = write_template(tmp);
    let mut projects = Vec::new();
    let mut watched = serde_json::Map::new();
    for (id, org) in [
        ("ephor", "foundation"),
        ("rhei", "foundation"),
        ("graal", "graal"),
    ] {
        let root = tmp.join(id);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("status.txt"), format!("{id} is well\n")).unwrap();
        projects.push(json!({
            "id": id,
            "type": "monorepo",
            "organization": org,
            "display_name": id,
            "root": root.to_string_lossy(),
            "main_branch": "main",
            "tags": [org],
            "branches": [
                { "id": format!("{id}-work"), "branch": "work", "active": true }
            ]
        }));
        watched.insert(
            id.to_string(),
            json!({
                "providers": [{ "provider": "custom-status", "command": "cat status.txt" }]
            }),
        );
    }
    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "organizations": [
                { "id": "foundation", "name": "Foundation" },
                { "id": "graal", "name": "GraalVM" }
            ],
            "projects": projects
        }),
    );
    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
            "work": {
                "recipes": [{
                    "id": "look",
                    "description": "look at it",
                    "brief": "look at {title}",
                    "when": { "kinds": ["status"] }
                }]
            },
            "projects": Value::Object(watched)
        }))
        .unwrap(),
    )
    .unwrap();
}

/// The world with every feed already cached, which is what `--cached` reads.
fn refreshed(tmp: &Path) {
    two_orgs(tmp);
    ephor(tmp).args(["refresh"]).assert().success();
}

fn json_of(output: &[u8]) -> Value {
    serde_json::from_slice(output).expect("a JSON reading on standard output")
}

/// §AR-009-surfaces.2: the session both surfaces read is opened over the
/// projects the selectors left, so the summary table is that organization's
/// and nothing else. This is the ticket's first reproducer — the two
/// organizations printed the same seven-row table.
#[test]
fn the_status_table_is_the_organizations_and_not_the_sites() {
    let tmp = tempdir();
    refreshed(tmp.path());

    ephor(tmp.path())
        .args(["status", "--cached", "--org", "foundation"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ephor"))
        .stdout(predicate::str::contains("rhei"))
        .stdout(predicate::str::contains("graal").not());

    ephor(tmp.path())
        .args(["status", "--cached", "--org", "graal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("graal"))
        .stdout(predicate::str::contains("rhei").not());

    // And with nothing given, the whole site as before.
    ephor(tmp.path())
        .args(["status", "--cached"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ephor"))
        .stdout(predicate::str::contains("graal"));
}

/// A tag and a workspace id select the same way, and a `--workspace` naming a
/// *branch* workspace selects the project it belongs to: a reader who named
/// one meant that project's feed.
#[test]
fn a_tag_and_a_branch_workspace_select_the_same_way() {
    let tmp = tempdir();
    refreshed(tmp.path());

    ephor(tmp.path())
        .args(["feed", "--tag", "graal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("graal is well"))
        .stdout(predicate::str::contains("ephor is well").not());

    ephor(tmp.path())
        .args(["feed", "--workspace", "rhei-work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rhei is well"))
        .stdout(predicate::str::contains("graal is well").not());
}

/// §AR-008-pipeline.1: a dispatch reads the cached feeds of the projects in
/// scope, so a sweep scoped to one organization proposes that organization's
/// work and no other's. The ticket's third reproducer.
#[test]
fn a_dispatch_proposes_only_the_scoped_organizations_work() {
    let tmp = tempdir();
    refreshed(tmp.path());

    let out = ephor(tmp.path())
        .args(["work", "dispatch", "--org", "graal", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let reading = json_of(&out);
    let proposed: Vec<&str> = reading["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|row| row["item"].as_str().unwrap_or_default())
        .collect();
    assert!(!proposed.is_empty(), "{reading:#}");
    assert!(
        proposed.iter().all(|item| item.contains("graal")),
        "a foundation item was proposed under --org graal: {proposed:?}"
    );

    // `work list` reads the ledger through the same restriction.
    ephor(tmp.path())
        .args(["work", "dispatch", "--org", "graal"])
        .assert()
        .success();
    let listed = ephor(tmp.path())
        .args(["work", "list", "--org", "foundation", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        json_of(&listed).as_array().map(Vec::len),
        Some(0),
        "graal's ticket was listed under --org foundation"
    );
}

/// §FS-011-command-line.9 through the branch reading: `ephor branches` opens
/// the same scoped session the screen does.
#[test]
fn the_branch_rows_are_the_scoped_projects_rows() {
    let tmp = tempdir();
    refreshed(tmp.path());
    let out = ephor(tmp.path())
        .args(["branches", "--org", "foundation", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let rows = json_of(&out);
    let owners: Vec<&str> = rows
        .as_array()
        .expect("branch rows")
        .iter()
        .map(|row| row["project"].as_str().unwrap_or_default())
        .collect();
    assert!(!owners.is_empty(), "{rows:#}");
    assert!(!owners.contains(&"graal"), "{owners:?}");
}

/// A verb that names one target refuses each selector by name, exits non-zero,
/// and under `--json` says so on standard output as an outcome
/// (§REQ-002-parity.3) — the same shape every other refusal takes.
#[test]
fn a_verb_that_names_one_target_refuses_the_selector() {
    let tmp = tempdir();
    two_orgs(tmp.path());

    for (named, argv) in [
        ("rebase", vec!["rebase"]),
        ("checkout", vec!["checkout"]),
        ("restart", vec!["restart"]),
        ("failures", vec!["failures"]),
        ("doctor", vec!["doctor"]),
        ("capabilities", vec!["capabilities"]),
        ("operations", vec!["operations"]),
        ("actions", vec!["actions"]),
        ("thread", vec!["thread", "x"]),
        ("reply", vec!["reply", "x"]),
        ("tick", vec!["tick", "x", "--message", "1"]),
        ("react", vec!["react", "x", "EYES", "--message", "1"]),
        ("job", vec!["job", "list"]),
        ("check", vec!["check"]),
        ("work offers", vec!["work", "offers", "--item", "x"]),
        ("work ask", vec!["work", "ask", "--item", "x", "words"]),
        ("work cancel", vec!["work", "cancel", "--item", "x", "t"]),
        ("work lay", vec!["work", "lay", "--item", "x", "entry"]),
        ("work forget", vec!["work", "forget", "--done"]),
        ("work workflows", vec!["work", "workflows"]),
        ("work states", vec!["work", "states"]),
        (
            "ensure-agents --type",
            vec!["ensure-agents", "--type", "monorepo"],
        ),
        ("validate --schema-only", vec!["validate", "--schema-only"]),
        ("feed --unattributed", vec!["feed", "--unattributed"]),
    ] {
        let mut args = argv.clone();
        args.extend(["--org", "foundation"]);
        ephor(tmp.path())
            .args(&args)
            .assert()
            .code(1)
            .stderr(predicate::str::contains(format!(
                "{named} does not take --org"
            )));
    }

    // Every selector it was given, named, and the outcome on standard output.
    let out = ephor(tmp.path())
        .args([
            "rebase",
            "--org",
            "foundation",
            "--tag",
            "rust",
            "--workspace",
            "ephor",
            "--json",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let outcome = json_of(&out);
    assert_eq!(outcome["ok"], json!(false));
    let says = outcome["says"].as_str().unwrap_or_default();
    assert!(
        says.contains("--workspace, --tag and --org"),
        "the refusal named only some of them: {says}"
    );
}

/// A refusal must not need the registry: the verbs that answer without one
/// still answer without one when they are refusing (§FS-011-command-line.9).
#[test]
fn refusing_needs_no_registry() {
    let tmp = tempdir();
    two_orgs(tmp.path());
    fs::remove_file(tmp.path().join("workspaces.json")).unwrap();

    ephor(tmp.path())
        .args(["schema", "views", "--org", "foundation"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("schema does not take --org"));
    ephor(tmp.path())
        .args([
            "validate",
            "--manifest",
            tmp.path().join("nothing.json").to_str().unwrap(),
            "--tag",
            "rust",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "validate --manifest does not take --tag",
        ));
}

/// An empty selection is said, never printed as a quiet site
/// (§FS-011-command-line.9). The two ends are told apart: a name no registry
/// row carries, and a name whose projects nobody watches here.
#[test]
fn an_empty_selection_is_refused_rather_than_printed() {
    let tmp = tempdir();
    refreshed(tmp.path());

    ephor(tmp.path())
        .args(["status", "--cached", "--org", "graall"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--org graall matches no project in",
        ));

    // The registry knows the organization; the site watches none of it.
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("status.json")).unwrap()).unwrap();
    config["projects"].as_object_mut().unwrap().remove("graal");
    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    ephor(tmp.path())
        .args(["status", "--cached", "--org", "graal"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--org graal matches no watched project (watched: ephor, rhei)",
        ));
}

/// A project named beside a selector that excludes it is refused by name:
/// the reader asked two things and only one of them can be answered.
#[test]
fn a_named_project_outside_the_scope_is_refused() {
    let tmp = tempdir();
    refreshed(tmp.path());
    ephor(tmp.path())
        .args(["status", "--cached", "graal", "--org", "foundation"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Project 'graal' is outside --org foundation.",
        ));
}

/// §FS-011-command-line.9: `mark-read`'s `--all` is its own, means every
/// *watched project*, and is narrowed by the project selectors. The global
/// `--all` it used to read is gone, so the two meanings cannot be confused —
/// and a narrowed sweep leaves the rest of the site unread.
#[test]
fn mark_read_sweeps_the_scope_and_leaves_the_rest_unread() {
    let tmp = tempdir();
    refreshed(tmp.path());

    // Before the verb it is no longer a flag at all.
    ephor(tmp.path())
        .args(["--all", "mark-read"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));

    ephor(tmp.path())
        .args(["mark-read", "--all", "--org", "foundation", "--json"])
        .assert()
        .success();

    ephor(tmp.path())
        .args(["feed", "--unread"])
        .assert()
        .success()
        .stdout(predicate::str::contains("graal is well"))
        .stdout(predicate::str::contains("ephor is well").not());

    // The narrowed sweep read some of the feeds, so it prunes none of what it
    // did not read: `graal`'s item is still remembered as unread above, and
    // marking the rest finishes the site.
    ephor(tmp.path())
        .args(["mark-read", "--all"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["feed", "--unread"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No feed items"));
}

/// `--all` belongs to the verbs that read it, and means there what their help
/// says: every branch entry rather than only the active ones.
#[test]
fn all_is_still_the_managed_workspace_verbs_flag() {
    let tmp = tempdir();
    two_orgs(tmp.path());
    ephor(tmp.path())
        .args(["validate", "--all", "--org", "foundation", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"valid\": true"));
    ephor(tmp.path())
        .args(["status", "--all"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}
