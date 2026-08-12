//! `ephor work` end to end (§FS-005-dispatch): a cached feed becomes tickets
//! in a runtime plan, and an item that moves reopens its own work.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::{json, Value};

use common::*;

/// A fake `gh` serving one pull request of the user's own with a failing
/// check, and one review comment awaiting an answer.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
case "$args" in
  *"search prs"*"--author"*)
    printf '[{"number": 42, "title": "Retry window", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z", "isDraft": false}]'
    ;;
  *"pr view"*reviewDecision*)
    printf '{"reviewDecision": "CHANGES_REQUESTED", "headRefName": "you/ABC-42-work"}'
    ;;
  *"pr checks"*)
    printf '[{"name": "gate", "state": "FAILURE", "link": "https://ci/1"}, {"name": "style", "state": "SUCCESS", "link": "https://ci/2"}]'
    ;;
  *"pr list"*)
    printf '[{"number": 42, "title": "Retry window", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z"}]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

fn env(tmp: &Path) -> Vec<(String, String)> {
    vec![
        (
            "XDG_STATE_HOME".to_string(),
            tmp.join("state").to_string_lossy().into_owned(),
        ),
        (
            "EPHOR_STATUS_CONFIG".to_string(),
            tmp.join("status.json").to_string_lossy().into_owned(),
        ),
        (
            "EPHOR_REGISTRY".to_string(),
            tmp.join("workspaces.json").to_string_lossy().into_owned(),
        ),
    ]
}

fn ephor(tmp: &Path) -> assert_cmd::Command {
    let mut cmd = ephor_cmd();
    let fake_bin = tmp.join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("gh"), FAKE_GH);
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            fake_bin.to_string_lossy(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    for (key, value) in env(tmp) {
        cmd.env(key, value);
    }
    cmd
}

fn fixture(tmp: &Path, work: Value) {
    let template = write_template(tmp);
    let project_root = tmp.join("demo");
    fs::create_dir_all(&project_root).unwrap();

    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [{
                "id": "demo",
                "type": "monorepo",
                "display_name": "Demo",
                "root": project_root.to_string_lossy(),
                "main_branch": "main",
                "branches": [
                    { "id": "demo-ticket", "branch": "you/ABC-42-work", "active": true, "ticket": "ABC-42" }
                ]
            }]
        }),
    );
    let mut config = json!({
        "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
        "projects": {
            "demo": {
                "providers": [{ "provider": "github-prs", "repos": ["acme/widget"] }]
            }
        }
    });
    if !work.is_null() {
        config["work"] = work;
    }
    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

fn feed_path(tmp: &Path) -> std::path::PathBuf {
    tmp.join("state/ephor/feed/demo.json")
}

fn read_feed(tmp: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(feed_path(tmp)).unwrap()).unwrap()
}

/// The one pull request in the fixture feed, for mutating it the way a later
/// refresh would.
fn with_pr(tmp: &Path, edit: impl Fn(&mut Value)) {
    let mut feed = read_feed(tmp);
    let items = feed["providers"]["github-prs"]["items"]
        .as_array_mut()
        .unwrap();
    for item in items {
        if item["kind"] == "pr" {
            edit(item);
        }
    }
    fs::write(feed_path(tmp), serde_json::to_string_pretty(&feed).unwrap()).unwrap();
}

#[test]
fn a_red_gate_becomes_a_ticket_that_carries_what_ephor_knew() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    // A dry run promises where the ticket goes and writes nothing.
    ephor(tmp.path())
        .args(["work", "dispatch", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fix-gate"))
        .stdout(predicate::str::contains("1 ticket(s) would be opened"));
    assert!(!tmp.path().join("demo/panta").exists());

    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    // The runtime project is set up, and ignores itself so the checkout it
    // lives in stays clean (§FS-005-dispatch.3).
    let panta = tmp.path().join("demo/panta");
    assert!(panta.join("index.panta.md").is_file());
    assert!(panta.join("states.yaml").is_file());
    assert!(fs::read_to_string(panta.join(".gitignore"))
        .unwrap()
        .contains('*'));

    let plan = fs::read_to_string(panta.join("github-prs-acme-widget-42.rhei.md")).unwrap();
    // The dossier: what the watch knew, not a link to it (§FS-005-dispatch.2).
    assert!(plan.contains("**States:** ephor-work"), "{plan}");
    assert!(plan.contains("## The item"), "{plan}");
    assert!(
        plan.contains("https://github.com/acme/widget/pull/42"),
        "{plan}"
    );
    assert!(plan.contains("you/ABC-42-work"), "{plan}");
    assert!(plan.contains("ABC-42"), "{plan}");
    assert!(plan.contains("## The gate"), "{plan}");
    assert!(plan.contains("✗1"), "{plan}");
    // And one ticket, in the state the recipe names.
    assert!(plan.contains("### Task fix-gate-1:"), "{plan}");
    assert!(plan.contains("**State:** fix"), "{plan}");
    assert!(plan.contains("The gate on"), "{plan}");

    // The ledger reads the state back out of the plan, never out of itself.
    let listed = ephor(tmp.path())
        .args(["work", "list", "--json"])
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(rows[0]["item"], "github-prs:acme/widget#42");
    assert_eq!(rows[0]["tickets"][0]["state"], "fix");
    assert_eq!(rows[0]["tickets"][0]["recipe"], "fix-gate");
    assert_eq!(rows[0]["stale"], false);

    // Dispatching again leaves the work alone: nothing is handed over twice.
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 ticket(s) opened"));

    // Asking for a different recipe on the same item is a different request,
    // and lands as a second ticket in the same plan (§FS-005-dispatch.3).
    ephor(tmp.path())
        .args([
            "work",
            "dispatch",
            "--item",
            "github-prs:acme/widget#42",
            "--recipe",
            "answer",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));
    let plan = fs::read_to_string(panta.join("github-prs-acme-widget-42.rhei.md")).unwrap();
    assert!(plan.contains("### Task answer-1:"), "{plan}");
    assert!(plan.contains("**Prior:** Task fix-gate-1"), "{plan}");
}

#[test]
fn an_item_that_moved_reopens_its_own_work_in_the_same_plan() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    // Unchanged: sync leaves it alone.
    ephor(tmp.path())
        .args(["work", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 ticket(s) reopened"));

    // The gate got worse and someone commented — what a later refresh sees.
    with_pr(tmp.path(), |item| {
        item["updated_at"] = json!("2026-08-02T10:00:00Z");
        item["raw"]["gate"]["repos"][0]["failed"] = json!(3);
        item["raw"]["threads"] = json!([{ "messages": [
            { "author": "Ada", "when": "2026-08-02T09:00:00Z", "text": "this still breaks on windows" }
        ]}]);
    });

    ephor(tmp.path())
        .args(["work", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) reopened"))
        .stdout(predicate::str::contains("1 new message"));

    let plan = fs::read_to_string(
        tmp.path()
            .join("demo/panta/github-prs-acme-widget-42.rhei.md"),
    )
    .unwrap();
    // One plan, two tickets, the second following the first
    // (§FS-005-dispatch.3, §FS-005-dispatch.5).
    assert!(plan.contains("### Task fix-gate-1:"), "{plan}");
    assert!(plan.contains("### Task fix-gate-2:"), "{plan}");
    assert!(plan.contains("**Prior:** Task fix-gate-1"), "{plan}");
    assert!(plan.contains("Since the previous ticket:"), "{plan}");
    assert!(plan.contains("still red — ✗3 where it was ✗1"), "{plan}");
    // The dossier was rewritten to what the item is now.
    assert!(plan.contains("this still breaks on windows"), "{plan}");
    assert_eq!(plan.matches("## The item").count(), 1, "{plan}");
}

#[test]
fn finished_work_is_never_dispatched_and_a_configured_recipe_wins() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(
        tmp.path(),
        json!({
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "our own gate fix",
                "brief": "fix {title} on {branch}",
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    // Merged: it is news, not a task (§FS-005-dispatch.6).
    with_pr(tmp.path(), |item| item["state"] = json!("merged"));
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 ticket(s) opened"));

    with_pr(tmp.path(), |item| item["state"] = json!("open"));
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    let plan = fs::read_to_string(
        tmp.path()
            .join("demo/panta/github-prs-acme-widget-42.rhei.md"),
    )
    .unwrap();
    assert!(
        plan.contains("fix Retry window on you/ABC-42-work"),
        "{plan}"
    );
}

#[test]
fn a_recipe_naming_a_state_the_machine_does_not_have_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(
        tmp.path(),
        json!({
            "recipes": [{
                "id": "fix-gate",
                "description": "fix it",
                "state": "nowhere",
                "brief": "b",
                "when": { "kinds": ["pr"] }
            }]
        }),
    );
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch", "--item", "github-prs:acme/widget#42"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("does not declare"))
        .stderr(predicate::str::contains("fix, review, done"));
}

/// What no recipe covers can still be asked for, in the reader's own words
/// (§FS-005-dispatch.8) — and asking is refused for nothing but being
/// unrunnable.
#[test]
fn an_item_can_be_asked_for_anything_including_what_no_recipe_matches() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    // Merged just now, so it is under Recent: no recipe would volunteer on
    // finished work, and the reader asks anyway.
    with_pr(tmp.path(), |item| {
        item["state"] = json!("merged");
        item["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
    });
    ephor(tmp.path())
        .args([
            "work",
            "ask",
            "--item",
            "github-prs:acme/widget#42",
            "open a follow-up issue for the retry-window edge case",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("asked"));

    let plan = fs::read_to_string(
        tmp.path()
            .join("demo/panta/github-prs-acme-widget-42.rhei.md"),
    )
    .unwrap();
    assert!(plan.contains("### Task ask-1:"), "{plan}");
    assert!(plan.contains("open a follow-up issue"), "{plan}");
    assert!(plan.contains("**State:** fix"), "{plan}");
    // The dossier is the same one a recipe would have carried.
    assert!(plan.contains("## The item"), "{plan}");

    // Piped in, which is how a longer ask composed in an editor arrives; and
    // a second ask follows the first.
    ephor(tmp.path())
        .args(["work", "ask", "--item", "github-prs:acme/widget#42"])
        .write_stdin("rename the flag\n\nKeep the old spelling for one release.\n")
        .assert()
        .success();
    let plan = fs::read_to_string(
        tmp.path()
            .join("demo/panta/github-prs-acme-widget-42.rhei.md"),
    )
    .unwrap();
    assert!(plan.contains("### Task ask-2: rename the flag"), "{plan}");
    assert!(plan.contains("**Prior:** Task ask-1"), "{plan}");
    assert!(
        plan.contains("Keep the old spelling for one release."),
        "{plan}"
    );

    // Nothing asked for is not a ticket.
    ephor(tmp.path())
        .args(["work", "ask", "--item", "github-prs:acme/widget#42"])
        .write_stdin("   \n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Nothing was asked for"));
}

#[test]
fn forgetting_an_entry_keeps_the_plan_it_points_at() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    let plan = tmp
        .path()
        .join("demo/panta/github-prs-acme-widget-42.rhei.md");
    ephor(tmp.path())
        .args(["work", "forget", "--item", "github-prs:acme/widget#42"])
        .assert()
        .success()
        .stdout(predicate::str::contains("its plan stays at"));
    assert!(plan.is_file());
    ephor(tmp.path())
        .args(["work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No work dispatched"));
}
