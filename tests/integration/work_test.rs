//! `ephor work` end to end (§FS-005-dispatch): a cached feed becomes tickets
//! in a runtime plan, and an item that moves reopens its own work.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::{json, Value};

use common::*;

/// A fake `gh` serving one pull request of the user's own with a failing
/// check, and one review comment awaiting an answer. The role searches arrive
/// as one aliased GraphQL request (§FS-001-forge-interface.8), and the head
/// branch and review decision ride in with it.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
conn() { printf '{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}' "$1"; }
none=$(conn '')
case "$args" in
  *"search(query:"*"is:pr"*)
    pr42='{"number": 42, "title": "Retry window", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z", "state": "OPEN", "headRefName": "you/ABC-42-work", "reviewDecision": "CHANGES_REQUESTED", "repository": {"nameWithOwner": "acme/widget"}}'
    printf '{"data":{"r0":%s,"r1":%s,"r2":%s,"r3":%s,"r4":%s}}' "$(conn "$pr42")" "$none" "$none" "$none" "$none"
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
    let project_root = tmp.join("demo");
    fs::create_dir_all(&project_root).unwrap();
    fixture_with(tmp, &project_root, "main", work);
}

/// The same fixture rooted at a checkout that already exists, on the main
/// branch that checkout was grown from — for the recipes whose opening move is
/// measured in git rather than reported by a forge.
fn fixture_on(tmp: &Path, project_root: &Path, main_branch: &str) {
    fixture_with(tmp, project_root, main_branch, Value::Null);
}

fn fixture_with(tmp: &Path, project_root: &Path, main_branch: &str, work: Value) {
    let template = write_template(tmp);
    let mut types = base_project_types(&template);
    // The shared fixture's monorepo type leaves `default_branch` as the
    // `{branch}` template, which no test expands. A project whose staleness is
    // actually measured needs the branch its repository is replayed against.
    types[0]["repos"][0]["default_branch"] = json!(main_branch);

    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": types,
            "hook_sets": [],
            "projects": [{
                "id": "demo",
                "type": "monorepo",
                "display_name": "Demo",
                "root": project_root.to_string_lossy(),
                "main_branch": main_branch,
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
    let items = feed["providers"]["github-prs"]["matters"]
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
    let tmp = tempdir();
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
    let tmp = tempdir();
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
    let tmp = tempdir();
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
    let tmp = tempdir();
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

/// A recipe carrying both a hand and the runtime's own execution identity is
/// refused rather than having one of them silently lose
/// (§FS-006-project-interface.9): the hand is the checkable name for exactly
/// what `target` spells raw, and nothing is written under the ambiguity.
#[test]
fn a_recipe_naming_both_a_hand_and_a_target_is_refused() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "recipes": [{
                "id": "fix-gate",
                "description": "fix it",
                "brief": "b",
                "when": { "kinds": ["pr"] },
                "hand": "sonnet",
                "target": "claude-code[yolo]:anthropic:claude-sonnet-4-6"
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
        .stderr(predicate::str::contains(
            "names both a hand and the runtime's own execution identity",
        ));
}

/// What no recipe covers can still be asked for, in the reader's own words
/// (§FS-005-dispatch.8) — and asking is refused for nothing but being
/// unrunnable.
#[test]
fn an_item_can_be_asked_for_anything_including_what_no_recipe_matches() {
    let tmp = tempdir();
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

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} in {}", dir.display());
}

fn commit(dir: &Path, file: &str, contents: &str, message: &str) {
    fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", message]);
}

/// A real checkout on the fixture's branch, trailing `master` by one commit.
/// The project root *is* the checkout, so the item resolves to it without a
/// workspace template.
fn trailing_checkout(tmp: &Path, conflicting: bool) -> std::path::PathBuf {
    let origin = tmp.join("origin.git");
    fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--initial-branch=master", "-q"]);
    git(&origin, &["config", "user.email", "t@example.com"]);
    git(&origin, &["config", "user.name", "t"]);
    commit(&origin, "shared.txt", "one\n", "one");

    let checkout = tmp.join("demo");
    let status = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&checkout)
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    git(&checkout, &["config", "user.email", "t@example.com"]);
    git(&checkout, &["config", "user.name", "t"]);
    git(&checkout, &["checkout", "-q", "-b", "you/ABC-42-work"]);

    // What the branch did, and what master did after it.
    if conflicting {
        commit(&checkout, "shared.txt", "ours\n", "ours");
        commit(&origin, "shared.txt", "theirs\n", "master moves");
    } else {
        commit(&checkout, "mine.txt", "mine\n", "mine");
        commit(&origin, "theirs.txt", "theirs\n", "master moves");
    }
    // How far a branch trails is measured against the remote ref the checkout
    // holds, so the fixture has to have seen master move — the same thing a
    // reader's `git fetch` or a previous refresh would have done.
    git(&checkout, &["fetch", "-q", "origin"]);
    checkout
}

/// §FS-005-dispatch.12: the deterministic move runs first, and where it
/// finished nothing is dispatched at all. Handing `ephor rebase` to a model
/// pays a pass to have two commands typed, and a clean replay is a done thing
/// rather than a ticket.
#[test]
fn a_clean_rebase_is_a_done_thing_and_not_a_ticket() {
    let tmp = tempdir();
    let checkout = trailing_checkout(tmp.path(), false);
    fixture_on(tmp.path(), &checkout, "master");
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    ephor(tmp.path())
        .args(["work", "dispatch", "--recipe", "rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 ticket(s) opened"))
        .stdout(predicate::str::contains("1 item(s) finished without one"))
        .stdout(predicate::str::contains("rebase finished"));

    // The replay actually happened, and no plan was written for it.
    assert!(
        checkout.join("theirs.txt").exists(),
        "the branch was replayed"
    );
    assert!(
        !checkout.join("panta").exists(),
        "a clean rebase is no ticket"
    );
}

/// And where it stopped, that is the ticket: what is handed over is the
/// situation, with the repository left standing in the conflict
/// (§FS-005-dispatch.12).
#[test]
fn a_rebase_that_stopped_is_the_ticket_and_carries_where_it_got_to() {
    let tmp = tempdir();
    let checkout = trailing_checkout(tmp.path(), true);
    fixture_on(tmp.path(), &checkout, "master");
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    ephor(tmp.path())
        .args(["work", "dispatch", "--recipe", "rebase"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    let plan =
        fs::read_to_string(checkout.join("panta/github-prs-acme-widget-42.rhei.md")).unwrap();
    assert!(plan.contains("### Task rebase-1:"), "{plan}");
    // The situation, not the request to reproduce it.
    assert!(plan.contains("stopped in a conflict"), "{plan}");
    assert!(plan.contains("shared.txt"), "{plan}");
    // And the brief no longer asks a model to run the replay itself.
    assert!(!plan.contains("Run `ephor rebase"), "{plan}");
    // Left mid-rebase, which is the state resolving it needs.
    assert!(
        checkout.join(".git/rebase-merge").exists() || checkout.join(".git/rebase-apply").exists()
    );
}

/// §FS-005-dispatch.13: work about a conversation needs no checkout — the
/// plan is written at the branch workspace where one resolves and at the
/// forest root where none does, so the checkout-able rung
/// (§FS-006-project-interface.10) is not required for it — and the reply it
/// asks for is a file ephor names and reads back.
#[test]
fn an_answer_is_dispatched_without_a_checkout_and_its_reply_comes_back() {
    let tmp = tempdir();
    fixture(tmp.path(), Value::Null);
    // The project resolves branches to workspaces of their own, and this
    // branch has none on disk: nothing that edits the change can run here.
    let registry_path = tmp.path().join("workspaces.json");
    let mut registry: Value =
        serde_json::from_str(&fs::read_to_string(&registry_path).unwrap()).unwrap();
    registry["projects"][0]["branch_root_template"] = json!(format!(
        "{}/branches/{{branch}}",
        tmp.path().to_string_lossy()
    ));
    write_registry(&registry_path, &registry);

    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    // Someone asked something, which is what an answer is dispatched for.
    with_pr(tmp.path(), |item| {
        item["needs_response"] = json!(true);
        item["raw"]["threads"] = json!([{ "messages": [
            { "author": "Ada", "when": "2026-08-02T09:00:00Z", "text": "does the retry window reset?" }
        ]}]);
    });

    // Work that edits the change is refused while the branch is not here…
    ephor(tmp.path())
        .args([
            "work",
            "dispatch",
            "--item",
            "github-prs:acme/widget#42",
            "--recipe",
            "fix-gate",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("is not checked out"));

    // …and the answer is dispatched anyway, at the forest root.
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

    let root = tmp.path().join("demo/panta");
    let plan_path = root.join("github-prs-acme-widget-42.rhei.md");
    let plan = fs::read_to_string(&plan_path).unwrap();
    assert!(plan.contains("### Task answer-1:"), "{plan}");
    // The brief names the file the reply goes into, absolutely: the runtime
    // runs from the checkout, not from the work root.
    let reply = root.join("runtime/ephor/github-prs-acme-widget-42.reply.md");
    assert!(
        plan.contains(&reply.to_string_lossy().to_string()),
        "{plan}"
    );

    // Nothing to read back until a run writes one — an unanswered ticket is
    // not a failure.
    let listed = ephor(tmp.path())
        .args(["work", "list", "--json"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&listed.stdout).contains("answer-1"));

    // The run writes the reply where it was asked to, and ephor reads it back
    // whole — never posting it.
    fs::create_dir_all(reply.parent().unwrap()).unwrap();
    fs::write(&reply, "Yes — the window resets per attempt.\n").unwrap();
    let proposal = ephor::work::runtime::results::proposal(&root, "github-prs-acme-widget-42")
        .expect("the run drafted a reply");
    assert_eq!(proposal.text, "Yes — the window resets per attempt.");
    assert_eq!(proposal.path, reply);
}

/// A multi-repo workspace has no one repository to be found by looking, so
/// where the runtime runs is recorded rather than guessed
/// (§FS-005-dispatch.3).
#[test]
fn the_ledger_records_the_checkout_the_runtime_runs_from() {
    let tmp = tempdir();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    let ledger: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/work.json")).unwrap(),
    )
    .unwrap();
    let entry = &ledger["entries"]["github-prs:acme/widget#42"];
    let checkout = tmp.path().join("demo");
    assert_eq!(entry["checkout"], json!(checkout.to_string_lossy()));
    // The work root is inside it, not the other way round.
    assert_eq!(
        entry["root"],
        json!(checkout.join("panta").to_string_lossy())
    );
}

#[test]
fn forgetting_an_entry_keeps_the_plan_it_points_at() {
    let tmp = tempdir();
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

/// Running the runtime is a summons like everything else ephor asks of the
/// world (§AR-002-summons, §FS-005-dispatch.12): one construction of the
/// invocation, run from the checkout the work is about (§FS-005-dispatch.3),
/// with its exit code read the one way.
#[test]
fn work_run_summons_the_runner_from_the_checkout() {
    let tmp = tempdir();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    // A stub runner that records where it ran and what it was asked for.
    let log = tmp.path().join("runner.log");
    make_executable(
        &tmp.path().join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\nprintf '%s\\n%s\\n' \"$PWD\" \"$*\" > {}\nexit \"${{FAKE_RHEI_EXIT:-0}}\"\n",
            log.to_string_lossy()
        ),
    );

    ephor(tmp.path())
        .args(["work", "run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rhei run"));

    let recorded = fs::read_to_string(&log).unwrap();
    let mut lines = recorded.lines();
    let cwd = lines.next().unwrap();
    let args = lines.next().unwrap();
    assert_eq!(cwd, tmp.path().join("demo").to_string_lossy());
    assert!(
        args.starts_with(&format!(
            "run {} --rhei ",
            tmp.path().join("demo/panta").to_string_lossy()
        )),
        "{args}"
    );
    assert!(args.ends_with("--rhei github-prs-acme-widget-42"), "{args}");

    // A runner that fails fails the command; a runner that parks does not.
    ephor(tmp.path())
        .env("FAKE_RHEI_EXIT", "2")
        .args(["work", "run"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("rhei run: failed (2)"));
    ephor(tmp.path())
        .env("FAKE_RHEI_EXIT", "75")
        .args(["work", "run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("parked"));
}

/// A run starts beneath the screen and is watched by attaching
/// (§FS-005-dispatch.20). `ephor work run` asks the binding whether it has a
/// detached shape, starts the run through it, and prints the id the launcher
/// gave it — the terminal stays the reader's. `--watch` keeps the run here, as
/// this command always did (§FS-011-command-line.8).
#[test]
fn work_run_starts_the_run_beneath_the_screen_and_prints_its_id() {
    let tmp = tempdir();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    // A runner with a detached shape: its own help names the flag, and the
    // launcher prints the descriptor of the run it started.
    let log = tmp.path().join("runner.log");
    make_executable(
        &tmp.path().join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*) printf 'Options:\\n      --headless  detach it\\n'; exit 0 ;;\n\
             *--headless*) printf '%s\\n' \"$*\" >> {}; printf '{{\"id\":\"3f9a2c\",\"pid\":9,\"status\":\"running\",\"exit_code\":null}}\\n'; exit 0 ;;\n\
             *) printf 'watched %s\\n' \"$*\" >> {}; exit 0 ;;\n\
             esac\n",
            log.to_string_lossy(),
            log.to_string_lossy(),
        ),
    );

    let output = ephor(tmp.path())
        .args(["work", "run", "--json"])
        .output()
        .expect("ephor work run");
    let reading: Value = serde_json::from_slice(&output.stdout).expect("a reading");
    let run = &reading["runs"][0];
    assert_eq!(run["outcome"], "started", "{reading}");
    assert_eq!(run["id"], "3f9a2c", "{reading}");
    let recorded = fs::read_to_string(&log).unwrap();
    assert!(
        recorded.contains("--headless --json"),
        "the launcher was asked to detach: {recorded}"
    );

    // `--watch` is the run watched here, and it never asks to detach.
    fs::write(&log, "").unwrap();
    ephor(tmp.path())
        .args(["work", "run", "--watch"])
        .assert()
        .success();
    let recorded = fs::read_to_string(&log).unwrap();
    assert!(recorded.starts_with("watched run "), "{recorded}");
    assert!(!recorded.contains("--headless"), "{recorded}");
}

/// Where the binding cannot detach, the run is watched as it was — the terminal
/// handed over — **and the line says so rather than pretending**
/// (§FS-005-dispatch.20, §AR-007-runtime.3).
///
/// The note arrives *before* the run takes the terminal, not after it gives it
/// back: a reader told why they lost their terminal once they have it again has
/// been told nothing they could act on. And nothing asks such a runner to
/// detach, which is the whole point of asking its help first.
#[test]
fn a_runner_with_no_detached_shape_runs_here_and_says_so_first() {
    let tmp = tempdir();
    fixture(tmp.path(), Value::Null);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    // A runner whose own help names no detached shape: it runs attached, as it
    // always did.
    let log = tmp.path().join("runner.log");
    make_executable(
        &tmp.path().join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*) printf 'Options:\\n      --tui  force the interface\\n'; exit 0 ;;\n\
             *) printf 'watched %s\\n' \"$*\" >> {}; exit 0 ;;\n\
             esac\n",
            log.to_string_lossy(),
        ),
    );

    let output = ephor(tmp.path())
        .args(["work", "run"])
        .output()
        .expect("ephor work run");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains("cannot start a run detached here"),
        "the note says why the terminal went: {said}"
    );
    // Before the run, not after it: the note stands above the line that says
    // which root is running.
    let note = said.find("cannot start a run detached here");
    let run = said.find("▶ ");
    assert!(note < run, "the note comes first: {said}");

    // And it was never asked to detach.
    let recorded = fs::read_to_string(&log).unwrap();
    assert!(recorded.starts_with("watched run "), "{recorded}");
    assert!(!recorded.contains("--headless"), "{recorded}");
}

/// Who does the work is the project's to default
/// (§FS-006-project-interface.9): its table names a hand for this action, the
/// roster turns that name into what the runtime will execute, and the ticket
/// carries it. Then the same project narrows the roster, and the hand it no
/// longer permits is refused with that reason rather than quietly replaced.
#[test]
fn a_project_defaults_the_hand_per_action_and_narrows_who_may_be_asked() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({ "runner": "runner-of-ours", "hands": { "default": "sonnet" } }),
    );
    project_hands(tmp.path(), json!({ "fix-gate": "impl-fast:high" }), &[]);

    // The runtime's own registry, and a PATH holding exactly what it names:
    // which hands exist here is this fixture's fact, not the machine's.
    let home = tmp.path().join("home");
    let settings = home.join(".config/rhei/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(
        &settings,
        r#"{
            "agents": { "our-agent": { "command": ["sh"], "modes": { "high": [] } } },
            "models": {
                "impl-fast": { "provider": "acme", "model": "m-fast", "default_agent": "our-agent" },
                "sonnet": { "provider": "acme", "model": "m-slow", "default_agent": "our-agent" }
            }
        }"#,
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join("fakebin")).unwrap();
    make_executable(
        &tmp.path().join("fakebin/runner-of-ours"),
        "#!/bin/sh\nexit 0\n",
    );
    let run = |args: &[&str]| {
        let mut command = ephor(tmp.path());
        command.env("HOME", &home).args(args);
        command
    };

    run(&["refresh", "demo"]).assert().success();
    run(&["work", "dispatch"]).assert().success();
    let plan_path = tmp
        .path()
        .join("demo/panta/github-prs-acme-widget-42.rhei.md");
    let plan = fs::read_to_string(&plan_path).unwrap();
    // The project's entry for this action, not the site's default for
    // everything — and rendered into the runtime's own selector.
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:m-fast"),
        "{plan}"
    );
    assert!(!plan.contains("m-slow"), "{plan}");

    // Narrowed to hands that do not include it: refused with the reason, and
    // nothing is written under a hand the project does not permit.
    project_hands(
        tmp.path(),
        json!({ "fix-gate": "impl-fast:high" }),
        &["sonnet"],
    );
    run(&[
        "work",
        "dispatch",
        "--item",
        "github-prs:acme/widget#42",
        "--again",
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains("impl-fast"))
    .stderr(predicate::str::contains("does not permit"))
    .stderr(predicate::str::contains("sonnet"));
    assert_eq!(
        fs::read_to_string(&plan_path)
            .unwrap()
            .matches("### Task")
            .count(),
        1
    );

    // With nothing on `PATH` under the bound runner there is nobody to ask:
    // the configured hand resolves to nothing, says so in the workable rung's
    // own words, and the ticket is written all the same
    // (§FS-006-project-interface.9).
    project_hands(tmp.path(), json!({ "fix-gate": "impl-fast:high" }), &[]);
    fs::remove_file(tmp.path().join("fakebin/runner-of-ours")).unwrap();
    run(&[
        "work",
        "dispatch",
        "--item",
        "github-prs:acme/widget#42",
        "--again",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("runner-of-ours is not on PATH"));
    let plan = fs::read_to_string(&plan_path).unwrap();
    assert_eq!(plan.matches("### Task").count(), 2);
    assert_eq!(plan.matches("**Target:**").count(), 1);
}

/// A hand the plan language cannot spell binds anyway (§FS-005-dispatch.14).
/// On a machine whose runtime settings declare agents and no model profiles —
/// so every hand is agent-only — the choice pins nothing on the ticket and
/// rides `work run` as the runtime's own agent flags, resolved when the run
/// is invoked; an effort-less choice of a hand declaring exactly one effort
/// is completed to it, so the flags never travel bare — a bare `--agent`
/// would let the state machine's own mode fall in, refused where the agent
/// does not declare it. And the flags ride only where they can re-aim
/// nothing: a ticket with the full execution line is resolved from that line
/// alone and rides beside them, while one pinning a model would take its
/// carrier from them, so its presence runs the plan unflagged and the reader
/// is told the hand went unbound — until somebody claims that ticket, which
/// takes it out of the run entirely (§FS-005-dispatch.15).
#[test]
fn an_agent_only_hand_binds_as_flags_on_the_run() {
    let tmp = tempdir();
    fixture(tmp.path(), json!({ "hands": { "default": "our-agent" } }));

    // The runtime's registry the way this machine's actually is — agents and
    // no model profiles — plus one profile used only to pin a later ticket.
    let home = tmp.path().join("home");
    let settings = home.join(".config/rhei/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(
        &settings,
        r#"{
            "agents": { "our-agent": { "command": ["sh"], "modes": { "high": [] } } },
            "models": {
                "carried": { "provider": "acme", "model": "m-carried", "default_agent": "our-agent" }
            }
        }"#,
    )
    .unwrap();
    // A stub runner that records what it was asked for.
    let log = tmp.path().join("runner.log");
    fs::create_dir_all(tmp.path().join("fakebin")).unwrap();
    make_executable(
        &tmp.path().join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" > {}\nexit 0\n",
            log.to_string_lossy()
        ),
    );
    let run = |args: &[&str]| {
        let mut command = ephor(tmp.path());
        command.env("HOME", &home).args(args);
        command
    };

    run(&["refresh", "demo"]).assert().success();
    run(&["work", "dispatch"])
        .assert()
        .success()
        // The choice is made, and the note says how it will bind.
        .stdout(predicate::str::contains("names no model of its own"))
        .stdout(predicate::str::contains("agent flags"));
    // Nothing is pinned on the ticket: the plan language has no line for it.
    let plan_path = tmp
        .path()
        .join("demo/panta/github-prs-acme-widget-42.rhei.md");
    let plan = fs::read_to_string(&plan_path).unwrap();
    assert!(!plan.contains("**Target:**"), "{plan}");
    assert!(!plan.contains("**Model:**"), "{plan}");

    // The run carries the choice as the runtime's own flags instead —
    // resolved at the moment the run is invoked, exactly once, with the
    // reader's own passthrough still last.
    run(&["work", "run", "--", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("agent our-agent at high"));
    let args = fs::read_to_string(&log).unwrap();
    assert!(
        args.trim_end()
            .ends_with("--agent our-agent --agent-mode high --dry-run"),
        "{args}"
    );
    assert!(!args.contains("--model"), "{args}");

    // One item's plan alone — the shape the key in the interface runs
    // (§FS-005-dispatch.14): one ledger entry, one plan named, so that plan's
    // own tickets settle the flags and there is no second plan to group
    // against. The same resolution, and the same command line.
    let ledger: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/work.json")).unwrap(),
    )
    .unwrap();
    let one = ledger["entries"]
        .as_object()
        .expect("the ledger's entries")
        .keys()
        .next()
        .expect("the dispatched item")
        .clone();
    fs::remove_file(&log).unwrap();
    run(&["work", "run", "--item", &one])
        .assert()
        .success()
        .stdout(predicate::str::contains("agent our-agent at high"));
    let args = fs::read_to_string(&log).unwrap();
    assert!(args.contains("github-prs-acme-widget-42"), "{args}");
    assert!(
        args.trim_end()
            .ends_with("--agent our-agent --agent-mode high"),
        "{args}"
    );

    // A second ticket dispatched under a model hand carries its own line —
    // with the carrier's one effort completed into it, because an effort-less
    // selector would run without any of the hand's efforts.
    project_hands(tmp.path(), json!({ "fix-gate": "carried" }), &[]);
    run(&["work", "dispatch", "--again"]).assert().success();
    let plan = fs::read_to_string(&plan_path).unwrap();
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:m-carried"),
        "{plan}"
    );

    // Back on the agent-only default, the flags still ride: the pinned
    // ticket is resolved from its own full line, and the run's flags are
    // invisible to it — nothing to contradict.
    project_hands(tmp.path(), json!({}), &[]);
    run(&["work", "run"]).assert().success();
    let args = fs::read_to_string(&log).unwrap();
    assert!(
        args.trim_end()
            .ends_with("--agent our-agent --agent-mode high"),
        "{args}"
    );

    // A ticket pinning a model alone is different: it would take its carrier
    // from the run's flags, so its presence runs the plan unflagged and the
    // reader is told the hand went unbound for that run.
    let mut plan = fs::read_to_string(&plan_path).unwrap();
    plan.push_str(
        "\n### Task manual-1: pinned to a model by hand\n\
         **State:** fix\n**Model:** carried\n\n\
         Work under whatever carries 'carried'.\n",
    );
    fs::write(&plan_path, &plan).unwrap();
    run(&["work", "run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("do not agree on one hand"));
    let args = fs::read_to_string(&log).unwrap();
    assert!(!args.contains("--agent"), "{args}");

    // A claim takes that ticket out of the run entirely — the runtime will
    // not schedule it (§FS-005-dispatch.15) — and the flags ride again.
    let plan = fs::read_to_string(&plan_path).unwrap();
    fs::write(
        &plan_path,
        plan.replacen(
            "**State:** fix\n**Model:** carried\n",
            "**State:** fix\n**Assignee:** somebody\n**Model:** carried\n",
            1,
        ),
    )
    .unwrap();
    run(&["work", "run"]).assert().success();
    let args = fs::read_to_string(&log).unwrap();
    assert!(
        args.trim_end()
            .ends_with("--agent our-agent --agent-mode high"),
        "{args}"
    );
}

/// The reader's pick is made at the moment of dispatch and spent by it
/// (§FS-005-dispatch.14): `--hand` displaces every table for exactly one
/// dispatch — the same first step the interface's picker feeds — nothing
/// records it, and the next dispatch resolves from the tables again. A pick
/// that is not a hand is refused before anything is written.
#[test]
fn the_readers_pick_is_made_at_dispatch_and_spent_by_it() {
    let tmp = tempdir();
    fixture(tmp.path(), json!({ "runner": "runner-of-ours" }));
    project_hands(tmp.path(), json!({ "fix-gate": "impl-fast:high" }), &[]);

    let home = tmp.path().join("home");
    let settings = home.join(".config/rhei/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(
        &settings,
        r#"{
            "agents": { "our-agent": { "command": ["sh"], "modes": { "high": [] } } },
            "models": {
                "impl-fast": { "provider": "acme", "model": "m-fast", "default_agent": "our-agent" },
                "picked": { "provider": "acme", "model": "m-picked", "default_agent": "our-agent" }
            }
        }"#,
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join("fakebin")).unwrap();
    make_executable(
        &tmp.path().join("fakebin/runner-of-ours"),
        "#!/bin/sh\nexit 0\n",
    );
    let run = |args: &[&str]| {
        let mut command = ephor(tmp.path());
        command.env("HOME", &home).args(args);
        command
    };

    run(&["refresh", "demo"]).assert().success();
    // The pick displaces the project's own entry for this action.
    run(&[
        "work",
        "dispatch",
        "--item",
        "github-prs:acme/widget#42",
        "--hand",
        "picked:high",
    ])
    .assert()
    .success();
    let plan_path = tmp
        .path()
        .join("demo/panta/github-prs-acme-widget-42.rhei.md");
    let plan = fs::read_to_string(&plan_path).unwrap();
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:m-picked"),
        "{plan}"
    );
    assert!(!plan.contains("m-fast"), "{plan}");

    // And it is spent: the next dispatch of the same action resolves from
    // the second step down — the project's table answers again.
    run(&[
        "work",
        "dispatch",
        "--item",
        "github-prs:acme/widget#42",
        "--recipe",
        "fix-gate",
        "--again",
    ])
    .assert()
    .success();
    let plan = fs::read_to_string(&plan_path).unwrap();
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:m-fast"),
        "{plan}"
    );
    assert_eq!(plan.matches("m-picked").count(), 1, "{plan}");

    // A pick nothing resolves is refused with what the roster does have,
    // and nothing is written under it.
    run(&[
        "work",
        "dispatch",
        "--item",
        "github-prs:acme/widget#42",
        "--again",
        "--hand",
        "lnua",
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains("names 'lnua'"));
    assert_eq!(
        fs::read_to_string(&plan_path)
            .unwrap()
            .matches("### Task")
            .count(),
        2
    );

    // A pick that is not even a hand is refused before the sweep starts.
    run(&["work", "dispatch", "--hand", "a:b:c"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("spell them out"));
}

/// `--hand` on `ephor rebase --dispatch` is the same pick
/// (§FS-005-dispatch.14): the key and the command line are one operation
/// (§FS-005-dispatch.12), so the conflict's ticket carries the picked hand —
/// completed to its one declared effort, with the note said where the reader
/// still is.
#[test]
fn a_rebase_conflict_is_handed_to_the_picked_hand() {
    let tmp = tempdir();
    let checkout = trailing_checkout(tmp.path(), true);
    fixture_with(
        tmp.path(),
        &checkout,
        "master",
        json!({ "runner": "runner-of-ours" }),
    );

    let home = tmp.path().join("home");
    let settings = home.join(".config/rhei/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(
        &settings,
        r#"{
            "agents": { "our-agent": { "command": ["sh"], "modes": { "high": [] } } },
            "models": {
                "picked": { "provider": "acme", "model": "m-picked", "default_agent": "our-agent" }
            }
        }"#,
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join("fakebin")).unwrap();
    make_executable(
        &tmp.path().join("fakebin/runner-of-ours"),
        "#!/bin/sh\nexit 0\n",
    );
    let run = |args: &[&str]| {
        let mut command = ephor(tmp.path());
        command.env("HOME", &home).args(args);
        command
    };

    run(&["refresh", "demo"]).assert().success();
    run(&[
        "rebase",
        "--project",
        "demo",
        "--checkout",
        checkout.to_str().unwrap(),
        "--item",
        "github-prs:acme/widget#42",
        "--dispatch",
        "--hand",
        "picked",
    ])
    .assert()
    .code(3)
    .stdout(predicate::str::contains("handed over"))
    // The effort-less pick of a hand declaring exactly one is completed to
    // it, and the completion is said (§FS-005-dispatch.14).
    .stdout(predicate::str::contains("the one it declares"));

    let plan =
        fs::read_to_string(checkout.join("panta/github-prs-acme-widget-42.rhei.md")).unwrap();
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:m-picked"),
        "{plan}"
    );
}

/// Rewrite `projects.demo.work` with a hands table and a narrowing, the way a
/// person editing status.json would.
fn project_hands(tmp: &Path, hands: Value, permitted: &[&str]) {
    let path = tmp.join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["projects"]["demo"]["work"] = json!({
        "hands": hands,
        "permitted_hands": permitted,
    });
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// A fake runner that detaches: its own help names the flag, and the launcher
/// prints the descriptor of the run it started. Every invocation is appended
/// to `log`, so a test can say what was asked of it and how often.
fn detaching_runner(tmp: &Path, log: &Path) {
    fs::create_dir_all(tmp.join("fakebin")).unwrap();
    make_executable(
        &tmp.join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*) printf 'Options:\\n      --headless  detach it\\n'; exit 0 ;;\n\
             *--headless*) printf '%s\\n' \"$*\" >> {log}; printf '{{\"id\":\"3f9a2c\",\"pid\":9,\"status\":\"running\",\"exit_code\":null}}\\n'; exit 0 ;;\n\
             *) printf 'other %s\\n' \"$*\" >> {log}; exit 0 ;;\n\
             esac\n",
            log = log.to_string_lossy(),
        ),
    );
}

/// How many runs the fake launcher was asked to start.
fn starts(log: &Path) -> usize {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("--headless"))
        .count()
}

/// Copy the dispatched fixture's plan and ledger entry onto another checkout,
/// producing another independently lockable due root without inventing a
/// second dispatch implementation inside the test.
fn duplicate_work_root(tmp: &Path, item: &str, checkout_name: &str) {
    let ledger_path = tmp.join("state/ephor/work.json");
    let mut ledger: Value =
        serde_json::from_str(&fs::read_to_string(&ledger_path).unwrap()).unwrap();
    let original = ledger["entries"]["github-prs:acme/widget#42"].clone();
    let source_root = tmp.join("demo/panta");
    let checkout = tmp.join(checkout_name);
    let root = checkout.join("panta");
    fs::create_dir_all(&root).unwrap();
    fs::copy(source_root.join("states.yaml"), root.join("states.yaml")).unwrap();
    let plan_id = format!("github-prs-acme-widget-{checkout_name}");
    let plan = root.join(format!("{plan_id}.rhei.md"));
    fs::copy(source_root.join("github-prs-acme-widget-42.rhei.md"), &plan).unwrap();

    let mut copied = original;
    copied["root"] = json!(root);
    copied["checkout"] = json!(checkout);
    copied["branch"] = Value::Null;
    copied["plan_id"] = json!(plan_id);
    copied["plan"] = json!(plan);
    ledger["entries"][item] = copied;
    fs::write(&ledger_path, serde_json::to_string_pretty(&ledger).unwrap()).unwrap();
}

/// A detached runner whose `second` root finishes inside the handshake and
/// whose other roots hold their runtime lock long enough for the sweep to
/// account for them. Its child redirects the inherited pipes so the launcher
/// itself can return immediately, as a real detached launcher does.
fn capacity_runner(tmp: &Path, log: &Path) {
    fs::create_dir_all(tmp.join("fakebin")).unwrap();
    make_executable(
        &tmp.join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*) printf 'Options:\\n      --headless  detach it\\n'; exit 0 ;;\n\
             *--headless*)\n\
               printf '%s\\n' \"$*\" >> {log}\n\
               root=\"$4\"\n\
               if [[ \"$root\" == *second* ]]; then\n\
                 printf '{{\"id\":\"finished\",\"status\":\"finished\",\"exit_code\":0}}\\n'\n\
                 exit 0\n\
               fi\n\
               mkdir -p \"$root/.rhei\"\n\
               ready=\"$root/.rhei/run-lock-ready\"\n\
               python -c 'import fcntl,pathlib,sys,time; lock=open(sys.argv[1],\"w\"); fcntl.flock(lock,fcntl.LOCK_EX); pathlib.Path(sys.argv[2]).touch(); time.sleep(20)' \"$root/.rhei/run.lock\" \"$ready\" >/dev/null 2>&1 &\n\
               for _ in {{1..100}}; do\n\
                 [[ -e \"$ready\" ]] && break\n\
                 sleep 0.01\n\
               done\n\
               [[ -e \"$ready\" ]] || exit 1\n\
               printf '{{\"id\":\"live\",\"status\":\"running\",\"exit_code\":null}}\\n'\n\
               exit 0 ;;\n\
             *) exit 0 ;;\n\
             esac\n",
            log = log.to_string_lossy(),
        ),
    );
}

/// A detached runner whose help probes rendezvous before either command can
/// take the autorun reservation. Once released, a launch holds its root long
/// enough for the other command's authoritative snapshot to see it.
fn overlapping_capacity_runner(tmp: &Path, log: &Path) {
    let rendezvous = tmp.join("autorun-rendezvous");
    fs::create_dir_all(&rendezvous).unwrap();
    fs::create_dir_all(tmp.join("fakebin")).unwrap();
    make_executable(
        &tmp.join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*)\n\
               touch {rendezvous}/$$\n\
               for _ in {{1..500}}; do\n\
                 [[ $(find {rendezvous} -type f | wc -l) -ge 2 ]] && break\n\
                 sleep 0.01\n\
               done\n\
               printf 'Options:\\n      --headless  detach it\\n'\n\
               exit 0 ;;\n\
             *--headless*)\n\
               printf '%s\\n' \"$*\" >> {log}\n\
               root=\"$4\"\n\
               mkdir -p \"$root/.rhei\"\n\
               ready=\"$root/.rhei/run-lock-ready\"\n\
               python -c 'import fcntl,pathlib,sys,time; lock=open(sys.argv[1],\"w\"); fcntl.flock(lock,fcntl.LOCK_EX); pathlib.Path(sys.argv[2]).touch(); time.sleep(20)' \"$root/.rhei/run.lock\" \"$ready\" >/dev/null 2>&1 &\n\
               for _ in {{1..100}}; do\n\
                 [[ -e \"$ready\" ]] && break\n\
                 sleep 0.01\n\
               done\n\
               [[ -e \"$ready\" ]] || exit 1\n\
               printf '{{\"id\":\"live\",\"status\":\"running\",\"exit_code\":null}}\\n'\n\
               exit 0 ;;\n\
             *) exit 0 ;;\n\
             esac\n",
            rendezvous = rendezvous.to_string_lossy(),
            log = log.to_string_lossy(),
        ),
    );
}

/// §FS-005-dispatch.24: filtered sweeps in separate processes reserve the
/// shared aggregate slot before either starts a root.
#[test]
fn review_repro_concurrent_sweeps_share_the_global_ceiling() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 0,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    let config_path = tmp.path().join("status.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["work"]["max_concurrent"] = json!(1);
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    let other_item = "github-prs:acme/widget#other";
    duplicate_work_root(tmp.path(), other_item, "other");
    let ledger_path = tmp.path().join("state/ephor/work.json");
    let mut ledger: Value =
        serde_json::from_str(&fs::read_to_string(&ledger_path).unwrap()).unwrap();
    ledger["entries"][other_item]["project"] = json!("other");
    fs::write(&ledger_path, serde_json::to_string_pretty(&ledger).unwrap()).unwrap();
    overlapping_capacity_runner(tmp.path(), &log);

    let mut demo = ephor(tmp.path());
    demo.args(["work", "run", "--due", "--project", "demo", "--json"]);
    let demo = std::thread::spawn(move || demo.output().unwrap());
    let mut other = ephor(tmp.path());
    other.args(["work", "run", "--due", "--project", "other", "--json"]);
    let other = std::thread::spawn(move || other.output().unwrap());
    let demo = demo.join().unwrap();
    let other = other.join().unwrap();
    assert!(
        demo.status.success(),
        "{}",
        String::from_utf8_lossy(&demo.stderr)
    );
    assert!(
        other.status.success(),
        "{}",
        String::from_utf8_lossy(&other.stderr)
    );
    let readings: [Value; 2] = [
        serde_json::from_slice(&demo.stdout).unwrap(),
        serde_json::from_slice(&other.stdout).unwrap(),
    ];
    let outcomes: Vec<&str> = readings
        .iter()
        .flat_map(|reading| reading["runs"].as_array().unwrap())
        .filter_map(|run| run["outcome"].as_str())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == "started")
            .count(),
        1,
        "only one filtered sweep may consume the shared slot: {readings:?}"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == "passed-over")
            .count(),
        1,
        "the other eligible root is reported without failing: {readings:?}"
    );
    assert_eq!(starts(&log), 1, "only one launcher was invoked");

    let roots = [
        tmp.path().join("demo/panta"),
        tmp.path().join("other/panta"),
    ];
    let live = roots
        .iter()
        .filter(|root| {
            fs::File::open(root.join(".rhei/run.lock")).is_ok_and(|lock| {
                matches!(lock.try_lock_shared(), Err(fs::TryLockError::WouldBlock))
            })
        })
        .count();
    assert_eq!(live, 1, "global cap=1 must leave at most one live root");
}

#[test]
fn review_repro_due_rows_hold_to_the_published_schema() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 0,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert_eq!(reading["failed"], 0, "{reading}");
    let problems = ephor::api::schema::holds("work-run", &reading);
    assert!(problems.is_empty(), "{problems:?}\n{reading}");
}

/// §FS-005-dispatch.24: capacity is a ceiling on live roots, selected in
/// configured rank order. Existing live work consumes it, a completed start
/// returns it, and roots omitted only for capacity are non-failing results.
#[test]
fn the_autorun_sweep_caps_live_roots_and_reports_every_capacity_pass_over() {
    let tmp = tempdir();
    let ranking = tmp.path().join("ranking.txt");
    fs::write(
        &ranking,
        "github-prs:acme/widget#second\ngithub-prs:acme/widget#42\n",
    )
    .unwrap();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 0,
            "ranking": ranking,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    capacity_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    // The configured zero lets dispatch write the ticket but starts nothing.
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"));
    assert_eq!(starts(&log), 0);
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#second", "demo-second");
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#third", "demo-third");

    // An existing live root fills the CLI's one aggregate slot even though
    // that root is itself excluded by the one-live-run-per-root guarantee.
    let held_root = tmp.path().join("demo-third/panta");
    fs::create_dir_all(held_root.join(".rhei")).unwrap();
    fs::write(held_root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(held_root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();
    let full = ephor(tmp.path())
        .args(["work", "run", "--due", "--max-concurrent", "1", "--json"])
        .output()
        .unwrap();
    let full: Value = serde_json::from_slice(&full.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &full).is_empty(),
        "capacity rows must hold to the published work-run shape: {full}"
    );
    assert_eq!(full["failed"], 0, "{full}");
    assert_eq!(full["runs"].as_array().unwrap().len(), 2, "{full}");
    assert!(full["runs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|run| run["outcome"] == "passed-over"));
    assert_eq!(starts(&log), 0);
    drop(holder);

    // The flag overrides configured zero. Ranking tries `second` first; it
    // finishes immediately and returns the slot, the original root stays
    // live, and the remaining root is explicitly passed over.
    let swept = ephor(tmp.path())
        .args(["work", "run", "--due", "--max-concurrent", "1", "--json"])
        .output()
        .unwrap();
    let swept: Value = serde_json::from_slice(&swept.stdout).unwrap();
    assert_eq!(swept["failed"], 0, "{swept}");
    let runs = swept["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 3, "{swept}");
    assert_eq!(runs[0]["item"], "github-prs:acme/widget#second");
    assert_eq!(runs[0]["outcome"], "done");
    assert_eq!(runs[1]["item"], "github-prs:acme/widget#42");
    assert_eq!(runs[1]["outcome"], "started");
    assert_eq!(runs[2]["item"], "github-prs:acme/widget#third");
    assert_eq!(runs[2]["outcome"], "passed-over");
    assert!(runs[2]["reason"]
        .as_str()
        .unwrap()
        .contains("global work.max_concurrent 1"));
    assert_eq!(starts(&log), 2);

    // Prose carries the same non-failing outcome, and configured zero is
    // still in force on the next invocation because the flag was one-sweep.
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"));
}

/// Park a duplicated work root on a person: teach its machine a poll that
/// declares whose answer it waits for, move its only ticket into that state,
/// and hold the root's run lock. What is left is a live root spending
/// nothing — the shape the ticket measured for six and a half hours.
fn park_on_a_person(root: &Path) -> fs::File {
    let states = root.join("states.yaml");
    let machine = fs::read_to_string(&states).unwrap();
    assert!(machine.contains("\n  done:\n"), "{machine}");
    fs::write(
        &states,
        machine.replace(
            "\n  done:\n",
            "\n  plan-approval:\n    program: [\"bash\", \"await.sh\"]\n    \
             poll:\n      interval: 15m\n      max_attempts: 96\n      \
             waiting_on: author\n\n  done:\n",
        ),
    )
    .unwrap();
    for plan in fs::read_dir(root).unwrap().flatten() {
        let path = plan.path();
        if path.extension().is_some_and(|ext| ext == "md") {
            let text = fs::read_to_string(&path).unwrap();
            fs::write(
                &path,
                text.replace("**State:** fix", "**State:** plan-approval"),
            )
            .unwrap();
        }
    }
    hold_the_run_lock(root)
}

fn hold_the_run_lock(root: &Path) -> fs::File {
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::write(root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();
    holder
}

/// Rewrite the site's two autorun ceilings between sweeps, so one fixture
/// answers for every combination of them.
fn set_ceilings(tmp: &Path, concurrent: Value, active: Value) {
    let path = tmp.join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["work"]["max_concurrent"] = concurrent;
    config["work"]["max_active"] = active;
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// §FS-005-dispatch.24, §AR-007-runtime.1: two ceilings count two different
/// things. A root parked on a person's answer is live — it holds a
/// `max_concurrent` slot — and is not working, so it holds no `max_active`
/// one; a refusal names the key it refused on; and a site that never names
/// the second key is bounded exactly as it was.
#[test]
fn a_root_parked_on_a_person_costs_a_flight_slot_and_no_agent_slot() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            // Zero while the tickets are written, so dispatch starts nothing
            // and every start below is this test's own.
            "max_concurrent": 0,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    assert_eq!(starts(&log), 0);
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#second", "demo-second");
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#third", "demo-third");

    let sweep = |tmp: &Path| -> Value {
        let output = ephor(tmp)
            .args(["work", "run", "--due", "--json"])
            .output()
            .unwrap();
        let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
        let problems = ephor::api::schema::holds("work-run", &reading);
        assert!(problems.is_empty(), "{problems:?}\n{reading}");
        reading
    };

    // Nothing live yet, and zero admits nothing under the new key either.
    set_ceilings(tmp.path(), json!(9), json!(0));
    let none = sweep(tmp.path());
    assert_eq!(none["failed"], 0, "{none}");
    assert!(
        none["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|run| run["outcome"] == "passed-over"),
        "{none}"
    );
    assert!(
        none["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("global work.max_active 0"),
        "{none}"
    );
    assert_eq!(none["capacity"]["live"], 0, "{none}");
    assert_eq!(starts(&log), 0);

    // One root working, one parked on its author. Both are live; only one of
    // them is spending anything.
    let working = hold_the_run_lock(&tmp.path().join("demo-second/panta"));
    let parked = park_on_a_person(&tmp.path().join("demo-third/panta"));

    set_ceilings(tmp.path(), json!(9), json!(1));
    let full = sweep(tmp.path());
    assert_eq!(full["capacity"]["live"], 2, "{full}");
    assert_eq!(full["capacity"]["active"], 1, "{full}");
    assert_eq!(full["capacity"]["parked"], 1, "{full}");
    assert_eq!(full["runs"][0]["outcome"], "passed-over", "{full}");
    assert_eq!(full["failed"], 0, "{full}");
    assert_eq!(
        full["runs"][0]["reason"], "global work.max_active 1 is full (1 active run(s), 1 parked)",
        "the refusal names its key and says what the other slot is doing"
    );
    assert_eq!(starts(&log), 0);

    // The parked root still holds a roots-in-flight slot: it exists, and that
    // is what the older key counts. Its wording is untouched.
    set_ceilings(tmp.path(), json!(2), json!(9));
    let flight = sweep(tmp.path());
    assert_eq!(
        flight["runs"][0]["reason"], "global work.max_concurrent 2 is full (2 live run(s))",
        "{flight}"
    );
    assert_eq!(starts(&log), 0);

    // And with room for one more agent, the parked root's freed slot is what
    // the remaining root starts in — which is the whole point of the second
    // number.
    set_ceilings(tmp.path(), json!(9), json!(2));
    let started = sweep(tmp.path());
    assert_eq!(started["runs"][0]["outcome"], "started", "{started}");
    assert_eq!(started["failed"], 0, "{started}");
    assert_eq!(starts(&log), 1);
    drop((working, parked));
}

/// §FS-005-dispatch.24: a site that never names the second key behaves as it
/// did before there was one — same starts, same wording — and its reading
/// still says how much of what is live is a person rather than an agent.
#[test]
fn a_site_that_names_only_the_flight_ceiling_is_bounded_exactly_as_before() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 1,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#second", "demo-second");
    let parked = park_on_a_person(&tmp.path().join("demo-second/panta"));

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    // The parked root fills the one aggregate slot exactly as any live root
    // did before, in the words it always used.
    assert_eq!(reading["failed"], 0, "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert_eq!(
        reading["runs"][0]["reason"], "global work.max_concurrent 1 is full (1 live run(s))",
        "{reading}"
    );
    assert_eq!(reading["capacity"]["max_active"], Value::Null, "{reading}");
    // And the reading says what the old wording could not: the slot holding
    // the sweep back is a person.
    assert_eq!(reading["capacity"]["live"], 1, "{reading}");
    assert_eq!(reading["capacity"]["parked"], 1, "{reading}");

    // The prose carries the same split beside the pass-over it explains.
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"))
        .stdout(predicate::str::contains(
            "capacity: 1 live root(s) — 0 active, 1 parked",
        ));
    drop(parked);
}

#[test]
fn a_project_autorun_ceiling_remains_inside_the_cli_aggregate_override() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 1,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let config_path = tmp.path().join("status.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["projects"]["demo"]["work"] = json!({ "max_concurrent": 0 });
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--max-concurrent", "3", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert!(reading["runs"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("projects.demo.work.max_concurrent 0"));
    assert_eq!(starts(&log), 0, "the project ceiling remains in force");
}

/// Put the fixture's project and one more inside a single organization, and
/// give the second one a work root of its own. The grouping is the registry's
/// to declare and is declared nowhere else (§FS-005-dispatch.24).
fn one_organization(tmp: &Path, organization: &str, sibling: &str, item: &str) {
    let path = tmp.join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    registry["organizations"] = json!([{ "id": organization, "name": "The Guild" }]);
    registry["projects"][0]["organization"] = json!(organization);
    let mut second = registry["projects"][0].clone();
    second["id"] = json!(sibling);
    second["display_name"] = json!(sibling);
    second["root"] = json!(tmp.join(sibling).to_string_lossy());
    second["branches"] = json!([]);
    registry["projects"].as_array_mut().unwrap().push(second);
    write_registry(&path, &registry);
    duplicate_work_root(tmp, item, sibling);
    assign_project(tmp, item, sibling);
}

/// Say which project a duplicated root's plan belongs to. `duplicate_work_root`
/// copies the fixture's entry, and the copy would otherwise keep the fixture's
/// project — and with it that project's organization.
fn assign_project(tmp: &Path, item: &str, project: &str) {
    let ledger_path = tmp.join("state/ephor/work.json");
    let mut ledger: Value =
        serde_json::from_str(&fs::read_to_string(&ledger_path).unwrap()).unwrap();
    ledger["entries"][item]["project"] = json!(project);
    fs::write(&ledger_path, serde_json::to_string_pretty(&ledger).unwrap()).unwrap();
}

/// Take and hold a work root's runtime lock, so the sweep reads it as a root
/// somebody else's run already has.
fn hold(root: &Path) -> fs::File {
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::write(root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();
    holder
}

/// A dispatched fixture with one autorun recipe, whose site ceiling of zero
/// lets the ticket be written and starts nothing — the state every ceiling
/// test below builds its own configuration on top of.
fn dispatched_but_unstarted(tmp: &Path, log: &Path) {
    fixture(
        tmp,
        json!({
            "max_concurrent": 0,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    detaching_runner(tmp, log);
    ephor(tmp).args(["refresh", "demo"]).assert().success();
    ephor(tmp).args(["work", "dispatch"]).assert().success();
}

/// Read the site configuration, let the test rewrite it, and write it back.
fn reconfigure(tmp: &Path, edit: impl Fn(&mut Value)) {
    let path = tmp.join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    edit(&mut config);
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// §FS-005-dispatch.24: the organization ceiling bounds the sum of its
/// projects' live roots, and reaches exactly the projects the registry puts
/// inside it — a project it names no organization for is bound by none.
#[test]
fn an_organization_ceiling_bounds_the_sum_of_its_projects_live_roots() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    one_organization(
        tmp.path(),
        "guild",
        "gadget",
        "github-prs:acme/widget#gadget",
    );
    // A root outside the organization entirely: the registry knows no project
    // by this name, so nothing puts it under the guild's ceiling.
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#outsider", "outsider");
    assign_project(tmp.path(), "github-prs:acme/widget#outsider", "outsider");
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({ "guild": { "work": { "max_concurrent": 1 } } });
    });

    // One of the guild's two projects already has a live run, which spends the
    // guild's only slot.
    let holder = hold(&tmp.path().join("gadget/panta"));
    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &reading).is_empty(),
        "the organization tier must hold to the published work-run shape: {reading}"
    );
    assert_eq!(reading["failed"], 0, "{reading}");
    let runs = reading["runs"].as_array().unwrap();
    let demo = runs
        .iter()
        .find(|run| run["root"].as_str().unwrap().contains("/demo/"))
        .unwrap_or_else(|| panic!("{reading}"));
    assert_eq!(demo["outcome"], "passed-over", "{reading}");
    assert!(
        demo["reason"]
            .as_str()
            .unwrap()
            .contains("organizations.guild.work.max_concurrent 1 is full (1 live run(s))"),
        "{reading}"
    );
    // The project the registry left out of every organization is untouched by
    // the guild's ceiling and gets its run.
    let outsider = runs
        .iter()
        .find(|run| run["root"].as_str().unwrap().contains("/outsider/"))
        .unwrap_or_else(|| panic!("{reading}"));
    assert_eq!(outsider["outcome"], "started", "{reading}");
    assert_eq!(starts(&log), 1);
    drop(holder);
}

/// §FS-005-dispatch.24: an absent `organizations` map is an omitted ceiling
/// for every organization, so a configuration written before the tier existed
/// starts exactly what it started before.
#[test]
fn an_absent_organizations_map_leaves_every_organization_unbounded() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    one_organization(
        tmp.path(),
        "guild",
        "gadget",
        "github-prs:acme/widget#gadget",
    );
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    let runs = reading["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{reading}");
    assert!(
        runs.iter().all(|run| run["outcome"] == "started"),
        "{reading}"
    );
    assert!(
        reading["notes"].as_array().unwrap().is_empty(),
        "nothing is wrong with a configuration that omits the tier: {reading}"
    );
    assert_eq!(starts(&log), 2);
}

/// §FS-005-dispatch.24: a ceiling keyed on an organization no registry row
/// places a project inside bounds nobody, so the sweep that would have read it
/// says so by name — a key may not quietly remove the bound its author meant
/// to set — and starts exactly what it would have started without the key. An
/// organization the registry declares that no project has joined is as empty
/// as one it never declared, and is named the same way.
#[test]
fn a_ceiling_over_an_organization_holding_no_project_is_noted() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    let path = tmp.path().join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    registry["organizations"] = json!([{ "id": "emptyguild", "name": "The Empty Guild" }]);
    write_registry(&path, &registry);
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({
            "emptyguild": { "work": { "max_concurrent": 0 } },
            "nosuchorg": { "work": { "max_concurrent": 0 } }
        });
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    // The declared-but-empty id and the undeclared one are one condition, and
    // the sweep says the same sentence about each.
    assert_eq!(
        reading["notes"],
        json!([
            "organizations.emptyguild: no registry row places a project in it, so the ceiling written there bounds nothing",
            "organizations.nosuchorg: no registry row places a project in it, so the ceiling written there bounds nothing"
        ]),
        "{reading}"
    );
    assert_eq!(reading["runs"][0]["outcome"], "started", "{reading}");
    assert_eq!(starts(&log), 1, "a ceiling over nobody refuses nobody");
}

/// §FS-005-dispatch.24: membership is the project row's `organization` field
/// and nothing else, so a registry that declares no `organizations` array at
/// all — which the validator permits, and which this fixture writes — still
/// puts its project inside the organization its row names — the fixture this
/// case starts from writes no such array, so the shape is the repository's own
/// default. The ceiling binds there, and nothing announces it as bounding
/// nobody: one reading answers both, so no document can refuse by a key it
/// calls empty in the same breath.
#[test]
fn a_ceiling_binds_where_a_row_joins_it_though_the_registry_declares_no_array() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    let path = tmp.path().join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    registry["projects"][0]["organization"] = json!("acme");
    write_registry(&path, &registry);
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({ "acme": { "work": { "max_concurrent": 0 } } });
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    // A ceiling that is refusing starts bounds somebody.
    assert_eq!(reading["notes"], json!([]), "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    let why = reading["runs"][0]["reason"].as_str().unwrap_or_default();
    let full = "organizations.acme.work.max_concurrent 0 is full";
    assert!(why.contains(full), "{reading}");
    assert_eq!(starts(&log), 0, "{reading}");
}

/// §FS-005-dispatch.24: a project ceiling above its organization's is named
/// at the sweep, in prose and `--json`, and changes nothing — the project's
/// number stands and the organization's total still refuses the next start.
#[test]
fn an_inverted_project_ceiling_is_named_and_the_organization_total_still_binds() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    one_organization(
        tmp.path(),
        "guild",
        "gadget",
        "github-prs:acme/widget#gadget",
    );
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({ "guild": { "work": { "max_concurrent": 1 } } });
        config["projects"]["demo"]["work"] = json!({ "max_concurrent": 4 });
    });

    let holder = hold(&tmp.path().join("gadget/panta"));
    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    let notes = reading["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1, "{reading}");
    let note = notes[0].as_str().unwrap();
    assert!(
        note.contains("projects.demo.work.max_concurrent 4"),
        "{note}"
    );
    assert!(
        note.contains("organizations.guild.work.max_concurrent 1"),
        "{note}"
    );
    // Warned about, not corrected: the guild's total still refuses the start
    // the project's own number would have allowed four times over.
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert!(
        reading["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("organizations.guild.work.max_concurrent 1 is full"),
        "{reading}"
    );
    assert_eq!(starts(&log), 0);

    // The same sentence in prose, where the reader who did not ask for JSON is
    // (§AR-009-surfaces.1).
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "projects.demo.work.max_concurrent 4 is above \
             organizations.guild.work.max_concurrent 1",
        ));
    drop(holder);
}

/// §FS-005-dispatch.24: the site half of the same rule — a project ceiling
/// above the site's aggregate is named the same way, and is still not
/// rewritten.
#[test]
fn an_inverted_project_ceiling_is_named_against_the_site_ceiling_too() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = json!(2);
        config["projects"]["demo"]["work"] = json!({ "max_concurrent": 5 });
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    let notes = reading["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1, "{reading}");
    assert!(
        notes[0]
            .as_str()
            .unwrap()
            .contains("projects.demo.work.max_concurrent 5 is above global work.max_concurrent 2"),
        "{reading}"
    );
    // Named, not refused: the root the project's number allows still starts.
    assert_eq!(reading["runs"][0]["outcome"], "started", "{reading}");
    assert_eq!(starts(&log), 1);
}

/// Work nobody has to start starts itself (§FS-005-dispatch.24).
///
/// A recipe that says `autorun` gets its run in the same breath as its ticket
/// — nobody presses anything — and the sweep behind that is idempotent: asked
/// again while the run holds the root it starts nothing, because a second run
/// there would only wait for the first. A recipe that says nothing is still
/// the reader's to start.
#[test]
fn a_recipe_that_asks_to_run_itself_gets_its_run_without_anyone_pressing_a_key() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    // Dispatch alone starts it: the ticket and its run in one breath.
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("run 3f9a2c started"));
    assert_eq!(starts(&log), 1, "{}", fs::read_to_string(&log).unwrap());

    // And the sweep is idempotent. Nothing here holds the lock — the fake
    // launcher exits — so this would start a second run if the root were not
    // read as it is: the ticket is still open, so it starts one, and that is
    // the honest answer for a root with no run on it.
    ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .assert()
        .success();
    assert_eq!(starts(&log), 2);
}

/// Silence means the key (§FS-005-dispatch.24): a recipe that never asked to
/// run itself is dispatched and left, and the sweep says there is nothing due.
#[test]
fn a_recipe_that_did_not_ask_is_never_started_by_the_sweep() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("run ").not());
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing is due"));
    assert_eq!(starts(&log), 0);

    // The reader's own key is unchanged and starts it.
    ephor(tmp.path()).args(["work", "run"]).assert().success();
    assert_eq!(starts(&log), 1);
}

/// A root a run already holds gets nothing from the sweep
/// (§FS-005-dispatch.24): the runtime schedules one run per root, and the live
/// run reaches a ticket written beneath it.
#[test]
fn the_sweep_starts_nothing_on_a_root_a_run_already_holds() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    let before = starts(&log);

    // A run takes the root's lock, exactly as the runtime does.
    let root = tmp.path().join("demo/panta");
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::write(root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();

    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing is due"));
    assert_eq!(starts(&log), before, "a second run would only wait");
    drop(holder);
}

/// A start that fails is remembered, and that root rests before it is tried
/// again (§FS-005-dispatch.24) — otherwise every sweep for as long as the
/// ticket stays open is another spawn. The failure is said, never swallowed.
#[test]
fn a_start_that_fails_is_said_and_rests_before_it_is_tried_again() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    let log = tmp.path().join("runner.log");
    // A runner that names the flag and then refuses to launch.
    fs::create_dir_all(tmp.path().join("fakebin")).unwrap();
    make_executable(
        &tmp.path().join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*) printf 'Options:\\n      --headless  detach it\\n'; exit 0 ;;\n\
             *--headless*) printf '%s\\n' \"$*\" >> {log}; printf 'no\\n' >&2; exit 3 ;;\n\
             *) exit 0 ;;\n\
             esac\n",
            log = log.to_string_lossy(),
        ),
    );
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    // The dispatch still lands — the ticket is written whatever the run does
    // — and the reader is told the run did not begin.
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stderr(predicate::str::contains("no run started"));
    let tried = starts(&log);
    assert_eq!(tried, 1);

    // Asked again at once, the root is resting and nothing is spawned.
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing is due"));
    assert_eq!(starts(&log), tried, "the root rests after a failed start");
}

/// A workflow entry a person wrote in their own configuration — the third of
/// the three homes (§FS-005-dispatch.19) — asks to run itself and the sweep
/// lays it (§FS-005-dispatch.28).
///
/// The matter is a project's own task: it carries no role at all, so no
/// shipped recipe covers it (§FS-005-dispatch.27), and the entry is the only
/// thing that applies. What lands is a plan of its own beside the matter's,
/// and the record of the laying is what makes the next sweep leave it alone.
#[test]
fn a_workflow_entry_in_the_persons_own_configuration_is_laid_by_the_sweep() {
    let tmp = tempdir();
    fixture(tmp.path(), Value::Null);
    // A runner that offers one workflow and renders it as a workspace of its
    // own — an index, a machine beside it, and the tasks in files.
    fs::create_dir_all(tmp.path().join("fakebin")).unwrap();
    make_executable(
        &tmp.path().join("fakebin/rhei"),
        r#"#!/usr/bin/env bash
set -euo pipefail
verb="${1:-}"; shift || true
if [ "$verb" = templates ]; then
  printf '%s' '[{"name":"supervised-fix","version":"1.0.0","source":"user","path":"supervised-fix",
                 "description":"Fix a ticket end to end.",
                 "inputs":[{"name":"ticket","description":"The ticket.","type":"string",
                            "required":true,"default":null,"validate":null}]}]'
  exit 0
fi
if [ "$verb" = instantiate ]; then
  output=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --output) output="$2"; shift 2 ;;
      *) shift ;;
    esac
  done
  mkdir -p "$output/tasks"
  printf '# Rhei: fix it\n**States:** supervised-fix\n' > "$output/index.rhei.md"
  printf 'name: supervised-fix\nstates:\n  implementing:\n  done:\n    final: true\n' \
    > "$output/states.yaml"
  printf '### Task fix: fix the ticket\n**State:** implementing\n\nwork\n' \
    > "$output/tasks/01-fix.md"
  echo "instantiated into $output"
  exit 0
fi
exit 1
"#,
    );
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("status.json")).unwrap()).unwrap();
    config["actions"] = json!([{
        "id": "fix-task",
        "icon": "⛬",
        "description": "fix this task end to end",
        "workflow": "supervised-fix",
        "autorun": true,
        "when": { "kinds": ["task"] },
        "inputs": { "ticket": "{title}" }
    }]);
    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    // A project's own task, in the cached feed the sweep reads: no role, and
    // so nothing ephor ships has anything to say about it.
    with_pr(tmp.path(), |item| {
        item["kind"] = json!("task");
        item.as_object_mut().unwrap().remove("role");
    });

    let output = ephor(tmp.path())
        .args(["work", "dispatch", "--json"])
        .output()
        .expect("the sweep runs");
    assert!(output.status.success());
    let swept: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(swept["laid"], 1, "{swept}");
    assert_eq!(swept["opened"], 0, "{swept}");
    assert_eq!(swept["items"][0]["outcome"], json!("laid"), "{swept}");
    assert_eq!(swept["items"][0]["entry"], json!("fix-task"), "{swept}");

    let plan = tmp
        .path()
        .join("demo/panta/github-prs-acme-widget-42-fix-task");
    assert!(plan.join("tasks/01-fix.md").is_file());
    // The record says which entry laid it, which is what the due sweep reads
    // (§FS-005-dispatch.28) and what makes a second sweep lay nothing.
    let ledger: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/work.json")).unwrap(),
    )
    .unwrap();
    let dispatch = &ledger["entries"]["github-prs:acme/widget#42"]["dispatches"][0];
    assert_eq!(dispatch["recipe"], json!("fix-task"));
    assert_eq!(
        dispatch["plan"],
        json!("github-prs-acme-widget-42-fix-task")
    );

    let again = ephor(tmp.path())
        .args(["work", "dispatch", "--json"])
        .output()
        .expect("the sweep runs again");
    let swept: Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(swept["laid"], 0, "{swept}");
}
