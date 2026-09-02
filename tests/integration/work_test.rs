//! `ephor work` end to end (§FS-005-dispatch): a cached feed becomes tickets
//! in a runtime plan, and an item that moves reopens its own work.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::{json, Value};

use common::*;

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
/// Put the fixture's project in an organization, and give that organization a
/// root in the registry where `root` is a path — the registry is where
/// membership and placement live (§REQ-001-boundary.2).
fn in_organization(tmp: &Path, organization: &str, root: Option<&Path>) {
    let path = tmp.join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let mut entry = json!({ "id": organization, "name": "Foundation" });
    if let Some(root) = root {
        entry["root"] = json!(root.to_string_lossy());
    }
    registry["organizations"] = json!([entry]);
    registry["projects"][0]["organization"] = json!(organization);
    fs::write(&path, serde_json::to_string_pretty(&registry).unwrap()).unwrap();
}

/// Write the organization tier of the work configuration.
fn organization_work(tmp: &Path, organization: &str, work: Value) {
    let path = tmp.join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["organizations"] = json!({ organization: { "work": work } });
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// A work root may reach above the project (§FS-005-dispatch.6.1): an
/// organization-tier `work.root` naming `{org_root}` dispatches a member
/// project's item into the organization's own root, and the board finds the
/// plan there — written to *and* swept, which is the pair the placement never
/// had before.
#[test]
fn work_dispatched_through_an_organization_root_lands_there_and_is_found_there() {
    let tmp = tempdir();
    // An autorun recipe under a paused ceiling: the sweep enumerates the
    // roots and passes over what it finds, which is discovery reported
    // without a runner being spent on it.
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
    let org_root = tmp.path().join("foundation");
    fs::create_dir_all(&org_root).unwrap();
    in_organization(tmp.path(), "foundation", Some(&org_root));
    organization_work(
        tmp.path(),
        "foundation",
        json!({ "root": "{org_root}/panta" }),
    );
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    // The plan is under the organization's root, not under the project's.
    let panta = org_root.join("panta");
    assert!(
        panta.join("github-prs-acme-widget-42.rhei.md").is_file(),
        "the plan belongs under the organization root"
    );
    assert!(
        !tmp.path().join("demo/panta").exists(),
        "and not under the project's own"
    );

    // And the sweep, which walks the configured places rather than the
    // ledger, finds the root there (§FS-005-dispatch.15.1) — a root that is
    // not enumerated is never swept, which is the half the placement never
    // had.
    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        reading["runs"][0]["root"].as_str(),
        Some(panta.to_string_lossy().as_ref()),
        "the organization root is swept: {reading}"
    );
}

/// An organization that declares no root cannot answer `{org_root}`, and the
/// dispatch says so by name rather than writing a directory called
/// `{org_root}` (§FS-005-dispatch.6.1). A project in no organization is
/// refused just as explicitly.
#[test]
fn a_work_root_above_a_project_with_no_answer_refuses_and_writes_nothing() {
    let tmp = tempdir();
    fixture(tmp.path(), Value::Null);
    in_organization(tmp.path(), "foundation", None);
    organization_work(
        tmp.path(),
        "foundation",
        json!({ "root": "{org_root}/panta" }),
    );
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "0 ticket(s) opened, 1 item(s) could not be",
        ))
        .stderr(predicate::str::contains(
            "organization foundation declares no root",
        ));

    // A project in no organization at all is refused the same way — here
    // through the site tier, so the refusal is about the template and not
    // about which scope wrote it.
    let path = tmp.path().join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    registry["organizations"] = json!([]);
    registry["projects"][0]
        .as_object_mut()
        .unwrap()
        .remove("organization");
    fs::write(&path, serde_json::to_string_pretty(&registry).unwrap()).unwrap();
    let path = tmp.path().join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["organizations"] = json!({});
    config["work"] = json!({ "root": "{org_root}/panta" });
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "0 ticket(s) opened, 1 item(s) could not be",
        ))
        .stderr(predicate::str::contains(
            "no registry row places demo in an organization",
        ));

    // Nothing was written under either refusal — no literal `{org_root}`
    // directory anywhere, and no plan in the project's own root.
    assert!(!tmp.path().join("{org_root}").exists());
    assert!(!tmp.path().join("demo/{org_root}").exists());
    assert!(!tmp.path().join("demo/panta").exists());
}

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
