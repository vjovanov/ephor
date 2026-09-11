//! E2E-024-every-idle-checkout-onto-main: the replay swept over every checkout
//! nobody is holding (§FS-004-quick-actions.6.1).
//!
//! The scenario is a machine with several branch checkouts on it and nobody
//! looking at most of them. Each one's base moves without it, and the drift is
//! discovered at the end — when the change will not land — rather than at the
//! start, when correcting it was a fetch and a rebase. The move that corrects
//! it already exists and is already offered per checkout; what does not exist
//! is anything that runs it over all of them without a person remembering to.
//!
//! Six directories, and the point is which of them are touched. The main
//! checkout belongs to `ephor update` and is passed over. A branch behind main
//! is replayed, and so is one whose pull request is still a draft, because a
//! draft is not yet anybody's to review. A branch that measured level is
//! reported level and moved nowhere. A branch whose tree a live run holds is
//! passed over, because rebasing under an agent mid-edit loses work nothing
//! recovers (§FS-005-dispatch.24). A branch under open, non-draft review is
//! passed over, because it is somebody's to move.
//!
//! And nothing above happens without the word. Sweeping writes into trees, so
//! at every width it sweeps at the verb reports and acts only under `--act`
//! (§FS-011-command-line.10); the flags that name one checkout or one matter
//! are refused beside a selector, and a bare `ephor rebase` inside a checkout
//! is what it always was (§FS-011-command-line.9).

#[path = "../support.rs"]
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use predicates::prelude::*;
use serde_json::json;

use support::*;

/// The six directories, and what each one is for. The names are the scenario:
/// a reader of the report should be able to tell what was supposed to happen
/// to each from the row it is on.
const BEHIND: &str = "behind/base";
const LEVEL: &str = "level/base";
const BUSY: &str = "busy/tree";
const REVIEW: &str = "under/review";
const DRAFT: &str = "still/draft";
const CLASH: &str = "clash/here";

/// A forge with two pull requests: one out of draft on `under/review`, one
/// still a draft on `still/draft`. The draft flag rides free with the row the
/// search already returns (§FS-001-forge-interface.8.3) and is the only thing
/// that tells the two branches apart.
///
/// It also writes down every call anybody makes to it, because one of the
/// claims below is about calls not made: a sweep held at the gate fetches
/// nothing.
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
printf '%s\n' "${1:-}" >> "$HOME/forge-calls.log"
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "widget/101", "repo": "widget", "number": "101",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/101",
        "branch": "under/review", "draft": false,
        "updated_at": "2026-09-10T12:00:00Z",
        "role": "author", "state": "open", "cited": false },
      { "id": "widget/102", "repo": "widget", "number": "102",
        "title": "Not finished yet",
        "url": "https://acme.example/pr/102",
        "branch": "still/draft", "draft": true,
        "updated_at": "2026-09-10T12:00:00Z",
        "role": "author", "state": "open", "cited": false }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// The site configuration this scenario runs on: the project's own forge, and
/// whatever the case adds. Written whole every time — `configure` writes a
/// file rather than merging into one, so the forge comes along with every
/// change to it.
fn configured(world: &World, work: serde_json::Value, ttl: u64) {
    world.configure(json!({
        "defaults": { "ttl_seconds": ttl },
        "projects": { PROJECT: {
            "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["widget"] } ],
            "work": work
        } }
    }));
}

/// The recipe the manual tells a reader to configure so that a conflict the
/// sweep stopped on becomes work rather than only a paragraph
/// (§FS-004-quick-actions.6.1). `state` is which state of the shipped machine
/// its tickets start in — a case passes one the machine does not declare to
/// see what a write-up that cannot be made says.
fn a_sweep_recipe(state: &str) -> serde_json::Value {
    json!({ "recipes": [ {
        "id": "rebase-sweep",
        "description": "resolve the sweep conflict",
        "state": state,
        "needs_checkout": false,
        "brief": "A rebase onto main stopped in this checkout."
    } ] })
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn commit(dir: &Path, file: &str, message: &str) {
    fs::write(dir.join(file), format!("{message}\n")).expect("write the file");
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// Where a branch's working tree is, and where its HEAD stands — the whole of
/// what "was it replayed" means from outside.
fn workspace(world: &World, branch: &str) -> PathBuf {
    world.forest().join(branch)
}

fn head(dir: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git runs");
    assert!(out.status.success(), "rev-parse in {}", dir.display());
    String::from_utf8(out.stdout)
        .expect("utf-8")
        .trim()
        .to_string()
}

/// Every branch's HEAD at once, so a claim about what a sweep moved is made
/// against all six rather than against the one the assertion remembered.
fn heads(world: &World) -> Vec<(String, String)> {
    [BEHIND, LEVEL, BUSY, REVIEW, DRAFT, CLASH, "main"]
        .iter()
        .map(|branch| (branch.to_string(), head(&workspace(world, branch))))
        .collect()
}

/// The site as it is at three in the morning: five branch checkouts and a main
/// one, the base two commits further on than four of them, the watch's reading
/// of the forge already fetched, and nobody present.
fn a_machine_left_alone() -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);

    let origin = world.path().join("origin");
    fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
    commit(&origin, "README.md", "the project");

    let main = world.forest().join("main");
    fs::create_dir_all(main.parent().expect("a parent")).expect("the project root");
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&main)
        .status()
        .expect("git clones");
    assert!(status.success());
    git(&main, &["config", "user.email", "t@example.com"]);
    git(&main, &["config", "user.name", "t"]);

    let declared: Vec<serde_json::Value> = [BEHIND, LEVEL, BUSY, REVIEW, DRAFT, CLASH]
        .iter()
        .map(|branch| json!({ "id": branch, "branch": branch, "active": true }))
        .collect();
    world.register(json!({
        "branch_root_template": "{project_root}/{branch}",
        "branches": declared
    }));
    // After the row, not before it: `register` writes a whole registry rather
    // than merging into one, so an organization declared first is not there
    // afterwards — and `--org` selects on the project row's own
    // `organization` field.
    world.organize("foundation", "Foundation");
    configured(&world, json!({}), 600);

    // Four checkouts cut from the base as it stands, then the base moves on
    // without them — which is the whole condition this sweep exists for.
    for branch in [BEHIND, BUSY, REVIEW, DRAFT, CLASH] {
        world
            .ephor()
            .args(["checkout", "--project", PROJECT, "--branch", branch])
            .assert()
            .success();
    }
    // One of them has a commit of its own, over the same lines the base is
    // about to move: the replay there is the one that stops.
    commit(
        &workspace(&world, CLASH),
        "README.md",
        "my version of the project",
    );
    commit(&origin, "moved-1.txt", "main moves");
    commit(&origin, "README.md", "somebody else edited the same lines");
    // And one cut afterwards, which is therefore level: a sweep that moved it
    // would be moving a checkout that had nothing to catch up with.
    world
        .ephor()
        .args(["checkout", "--project", PROJECT, "--branch", LEVEL])
        .assert()
        .success();
    for branch in [BEHIND, LEVEL, BUSY, REVIEW, DRAFT, CLASH, "main"] {
        git(&workspace(&world, branch), &["fetch", "-q", "origin"]);
    }

    world.ephor().args(["refresh", PROJECT]).assert().success();
    world
}

/// A run holding a checkout's tree, the way the runtime holds one: the work
/// root's lock, taken and kept for as long as the returned handle lives
/// (§FS-005-dispatch.24). The guard is over the tree, so which root took it
/// does not matter — only which tree it is in.
fn a_live_run_in(world: &World, branch: &str) -> fs::File {
    let root = workspace(world, branch).join("panta");
    fs::create_dir_all(root.join(".rhei")).expect("the work root");
    fs::write(root.join("index.rhei.md"), "# Rhei: held\n").expect("a plan");
    fs::write(root.join(".rhei/run.lock"), "").expect("the lock file");
    let holder = fs::File::open(root.join(".rhei/run.lock")).expect("open the lock");
    holder.lock().expect("hold it");
    holder
}

/// The whole of it: nothing moves without the word, and under the word exactly
/// the unprotected checkouts move.
#[test]
fn the_sweep_reports_first_and_replays_only_what_nobody_is_holding() {
    let world = a_machine_left_alone();
    let _run = a_live_run_in(&world, BUSY);
    let before = heads(&world);

    // Reading at any width is free; writing is not. So the wide form says what
    // it would do and names the word that does it.
    let held = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation"])
        .output()
        .expect("ran");
    let reported = String::from_utf8_lossy(&held.stdout).to_string();
    assert!(
        held.status.success(),
        "a sweep that only reported failed:\n{reported}{}",
        String::from_utf8_lossy(&held.stderr)
    );
    assert!(
        reported.contains("--act"),
        "a gated report must name the word that acts:\n{reported}"
    );
    assert_eq!(
        heads(&world),
        before,
        "a sweep that was only reporting moved a branch"
    );

    // And with the word said: the two nobody is holding, and nothing else.
    let acted = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act"])
        .output()
        .expect("ran");
    let said = String::from_utf8_lossy(&acted.stdout).to_string();
    let after: std::collections::HashMap<String, String> = heads(&world).into_iter().collect();
    let before: std::collections::HashMap<String, String> = before.into_iter().collect();

    for branch in [BEHIND, DRAFT] {
        assert_ne!(
            after[branch], before[branch],
            "{branch} was not replayed:\n{said}"
        );
    }
    for branch in [LEVEL, BUSY, REVIEW, CLASH, "main"] {
        assert_eq!(
            after[branch], before[branch],
            "{branch} was replayed and should not have been:\n{said}"
        );
    }

    // Passing a checkout over is an outcome with a reason, never a silence:
    // a sweep that says nothing about what it decided not to touch cannot be
    // told from one that never saw it.
    assert!(
        said.contains(BUSY) && said.to_lowercase().contains("live run"),
        "the checkout a live run holds was passed over without saying so:\n{said}"
    );
    assert!(
        said.contains(REVIEW) && said.to_lowercase().contains("pull request"),
        "the branch under review was passed over without saying so:\n{said}"
    );
    assert!(
        said.contains(LEVEL) && said.to_lowercase().contains("level"),
        "a branch that measured level is told so, not silently skipped:\n{said}"
    );

    // The conflict, and the whole difference from a replay somebody is waiting
    // on: the tree is put back where it was, and the report is what is left
    // behind instead (§FS-005-dispatch.12).
    let clash = workspace(&world, CLASH);
    assert!(
        said.contains(CLASH) && said.to_lowercase().contains("conflict"),
        "the checkout that stopped is not in the report:\n{said}"
    );
    assert!(
        !clash.join(".git/rebase-merge").exists() && !clash.join(".git/rebase-apply").exists(),
        "a timer left a tree standing mid-rebase"
    );
    let porcelain = Command::new("git")
        .arg("-C")
        .arg(&clash)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .expect("git runs");
    assert!(
        String::from_utf8_lossy(&porcelain.stdout).trim().is_empty(),
        "the restored tree is not clean: {}",
        String::from_utf8_lossy(&porcelain.stdout)
    );
    assert_eq!(
        acted.status.code(),
        Some(3),
        "a sweep one of whose checkouts conflicted did not exit 3:\n{said}"
    );
}

/// The same answer for a program, held to the shape the command publishes:
/// forty checkouts become one reading, and every fact the prose gave is a
/// field in it (§REQ-002-parity.3, §REQ-002-parity.4).
#[test]
fn the_sweeps_reading_is_the_published_shape() {
    let world = a_machine_left_alone();
    let _run = a_live_run_in(&world, BUSY);

    let acted = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act", "--json"])
        .output()
        .expect("ran");
    assert_eq!(
        acted.status.code(),
        Some(3),
        "one checkout conflicted, so the sweep exits 3: {}",
        String::from_utf8_lossy(&acted.stderr)
    );
    let reading = shaped("rebase", &acted);
    let checkouts = reading["checkouts"]
        .as_array()
        .unwrap_or_else(|| panic!("the sweep's reading names no checkouts: {reading:#}"));
    assert_eq!(
        checkouts.len(),
        6,
        "six branch checkouts were swept, main is not one of them: {reading:#}"
    );
    let outcome = |branch: &str| -> String {
        checkouts
            .iter()
            .find(|row| row["branch"] == json!(branch))
            .unwrap_or_else(|| panic!("no row for {branch}: {reading:#}"))["outcome"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(outcome(BEHIND), "replayed", "{reading:#}");
    assert_eq!(outcome(DRAFT), "replayed", "{reading:#}");
    assert_eq!(outcome(LEVEL), "level", "{reading:#}");
    assert_eq!(outcome(BUSY), "passed-over", "{reading:#}");
    assert_eq!(outcome(REVIEW), "passed-over", "{reading:#}");
    assert_eq!(outcome(CLASH), "conflicted", "{reading:#}");
    assert_eq!(
        reading["restored"],
        json!(true),
        "a reader sent to a conflict must be told the tree was put back: {reading:#}"
    );
}

/// *Could not tell* is not *no*. A project whose pull-request reading cannot
/// be had answers "no open pull request" for every branch of it, which would
/// replay exactly the branches under review that the question protects. So
/// none of its checkouts is replayed, it is reported as not reached, and the
/// sweep exits non-zero — the timer's only way to say it is not doing its job.
#[test]
fn a_project_whose_review_state_cannot_be_read_has_nothing_replayed() {
    let world = a_machine_left_alone();
    let before: std::collections::HashMap<String, String> = heads(&world).into_iter().collect();

    // The cache a refresh left behind, gone — and the forge with it, so the
    // freshening the sweep tries before giving up cannot succeed either.
    fs::remove_file(
        world
            .path()
            .join(format!("state/ephor/feed/{PROJECT}.json")),
    )
    .expect("drop the cached reading");
    fs::remove_file(world.path().join("fakebin/ephor-forge-acmeforge")).expect("drop the forge");

    let swept = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act"])
        .output()
        .expect("ran");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&swept.stdout),
        String::from_utf8_lossy(&swept.stderr)
    );
    assert_ne!(
        swept.status.code(),
        Some(0),
        "a sweep that reached no project exited 0:\n{said}"
    );
    assert_eq!(
        heads(&world)
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>(),
        before,
        "a checkout was replayed on a reading that could not be had:\n{said}"
    );
    assert!(
        said.contains(PROJECT),
        "the project that was not reached is not named:\n{said}"
    );
}

/// What names one checkout or one matter has no meaning across a sweep, so it
/// is refused by name rather than quietly applied to one of forty. Every one of
/// these exits 2 today for the selector alone, so no command line that works
/// now gains a refusal (§FS-011-command-line.9).
#[test]
fn what_names_one_checkout_is_refused_beside_a_selector() {
    let world = a_machine_left_alone();

    for flag in ["--project", "--checkout", "--item"] {
        let value = if flag == "--project" {
            PROJECT.to_string()
        } else {
            workspace(&world, BEHIND).to_string_lossy().to_string()
        };
        world
            .ephor()
            .args(["rebase", "--org", "foundation", flag, &value])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(flag));
    }
}

/// The other disposition, unchanged and pinned beside the sweep's: a replay
/// somebody is waiting on leaves the conflict standing in the tree, because
/// that is the state resolving it needs and the ticket it writes is handed that
/// tree (§FS-005-dispatch.12). The two are one implementation taking an
/// argument, so the way to show that is to run both against the same conflict.
#[test]
fn the_replay_a_reader_is_waiting_on_still_leaves_the_conflict_standing() {
    let world = a_machine_left_alone();
    let clash = workspace(&world, CLASH);

    world
        .ephor()
        .args([
            "rebase",
            "--project",
            PROJECT,
            "--checkout",
            &clash.to_string_lossy(),
        ])
        .assert()
        .code(3);
    let porcelain = Command::new("git")
        .arg("-C")
        .arg(&clash)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .expect("git runs");
    assert!(
        String::from_utf8_lossy(&porcelain.stdout).contains("README.md"),
        "the conflict was not left where the replay stopped: {}",
        String::from_utf8_lossy(&porcelain.stdout)
    );
}

/// And the half that does not change: a bare `ephor rebase` inside a checkout
/// resolves to that one checkout and is byte for byte what it was, which is the
/// safety property the whole contract change rests on.
#[test]
fn a_rebase_that_sweeps_nothing_is_the_verb_it_always_was() {
    let world = a_machine_left_alone();
    let behind = workspace(&world, BEHIND);
    let was = head(&behind);

    world
        .ephor()
        .args([
            "rebase",
            "--project",
            PROJECT,
            "--checkout",
            &behind.to_string_lossy(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Replayed onto"));
    assert_ne!(head(&behind), was, "the one-checkout replay did nothing");

    // And the checkout cut after the base moved is told it is current, in the
    // register a current repository is always told it in — the same reading the
    // sweep reports as a good end.
    world
        .ephor()
        .args([
            "rebase",
            "--project",
            PROJECT,
            "--checkout",
            &workspace(&world, LEVEL).to_string_lossy(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Already on top of"));

    // `--act` cannot fire a gate on one checkout, so it is refused by name
    // there rather than parsing and changing nothing
    // (§FS-011-command-line.10).
    world
        .ephor()
        .args(["rebase", "--act"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--act"));
}

/// One checkout's row in the sweep's reading, by branch.
fn row(reading: &serde_json::Value, branch: &str) -> serde_json::Value {
    reading["checkouts"]
        .as_array()
        .unwrap_or_else(|| panic!("the sweep's reading names no checkouts: {reading:#}"))
        .iter()
        .find(|row| row["branch"] == json!(branch))
        .unwrap_or_else(|| panic!("no row for {branch}: {reading:#}"))
        .clone()
}

/// One ticket's own text out of the plan it shares with the others: from its
/// heading to the next one. A plan holds one ticket per conflicted checkout,
/// so a claim about what a ticket says has to be made about that ticket.
fn ticket_body(plan: &str, id: &str) -> String {
    plan.split("### Task ")
        .find(|section| section.starts_with(&format!("{id}:")))
        .unwrap_or_else(|| panic!("no ticket {id} in the plan:\n{plan}"))
        .to_string()
}

/// Every heading in a plan the plan language did not put there. A heading
/// inside a ticket's body is a *node* to the runtime — it reads one as a task
/// and refuses the whole file — so an embedded report that kept its own
/// headings is a plan nobody can ever run (§FS-005-dispatch.3). Fenced lines
/// are not headings, which is the one exemption the runtime's parser makes
/// too.
fn stray_headings(plan: &str) -> Vec<String> {
    let mut fenced = false;
    let mut stray = Vec::new();
    for line in plan.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced || !line.starts_with('#') {
            continue;
        }
        let written_by_the_plan = line.starts_with("# Rhei: ")
            || line.trim_end() == "## Tasks"
            || line.trim_start_matches('#').starts_with(" Task ");
        if !written_by_the_plan {
            stray.push(line.to_string());
        }
    }
    stray
}

/// A conflict in an idle checkout is work, and work is a ticket
/// (§FS-004-quick-actions.6.1, §FS-005-dispatch.3) — a ticket the runtime can
/// actually load, in a plan named after the sweep, that the next sweep an hour
/// later finds and passes the checkout over on rather than stopping on the
/// same conflict forever.
#[test]
fn a_conflict_becomes_a_ticket_the_next_sweep_passes_the_checkout_over_on() {
    let world = a_machine_left_alone();
    configured(&world, a_sweep_recipe("fix"), 600);

    let acted = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act", "--json"])
        .output()
        .expect("ran");
    assert_eq!(
        acted.status.code(),
        Some(3),
        "one checkout conflicted: {}",
        String::from_utf8_lossy(&acted.stderr)
    );
    let reading = shaped("rebase", &acted);
    let stopped = row(&reading, CLASH);
    assert_eq!(stopped["outcome"], json!("conflicted"), "{reading:#}");
    let written = stopped["ticket"]
        .as_str()
        .unwrap_or_else(|| panic!("the conflict was not written up: {reading:#}"))
        .to_string();
    let (path, id) = written
        .rsplit_once('#')
        .unwrap_or_else(|| panic!("a ticket names its plan and its id: {written}"));
    let plan = fs::read_to_string(path).expect("the plan the sweep wrote");
    assert!(
        plan.contains(&format!("### Task {id}:")),
        "the ticket the row names is not in the plan:\n{plan}"
    );
    assert!(
        plan.contains(CLASH),
        "the ticket does not say which checkout it is about:\n{plan}"
    );
    assert_eq!(
        stray_headings(&plan),
        Vec::<String>::new(),
        "the plan the sweep wrote cannot be loaded by the runtime:\n{plan}"
    );

    // An hour later. The conflict is still there and still unresolved, so the
    // checkout is passed over on its own ticket, by id — and nothing is
    // written twice.
    let again = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act", "--json"])
        .output()
        .expect("ran");
    let reading = shaped("rebase", &again);
    let passed = row(&reading, CLASH);
    assert_eq!(passed["outcome"], json!("passed-over"), "{reading:#}");
    let says = passed["says"].as_str().unwrap_or_default();
    assert!(
        says.contains(id),
        "the pass-over does not name the ticket it is about: {says}"
    );
    assert_eq!(
        fs::read_to_string(path)
            .expect("the plan")
            .matches("### Task ")
            .count(),
        1,
        "the second sweep wrote the same conflict up twice"
    );

    // And the recipe is the sweep's alone: no matter is its subject, so it is
    // not an entry in any item's menu (§FS-004-quick-actions.6.1).
    world
        .ephor()
        .args(["actions", "--item", "acmeforge:widget/101"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rebase-sweep").not());
}

/// A checkout the sweep finds *already* stopped in a rebase is touched under
/// neither disposition (§FS-005-dispatch.12) — so the conflict is still
/// standing in it, and both the row and the ticket say that rather than
/// claiming a restoration nobody performed. A ticket that sends a reader to a
/// clean tree is bad; one that tells them a conflicted tree was restored is
/// worse, because they believe it.
#[test]
fn a_tree_already_stopped_in_a_rebase_is_reported_as_found() {
    let world = a_machine_left_alone();
    configured(&world, a_sweep_recipe("fix"), 600);
    let stopped_here = workspace(&world, BEHIND);
    commit(
        &stopped_here,
        "README.md",
        "my own edit, over the same lines",
    );
    let halted = Command::new("git")
        .arg("-C")
        .arg(&stopped_here)
        .args(["rebase", "origin/main"])
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git runs");
    assert!(
        !halted.status.success(),
        "the rebase this case needs stopped nowhere"
    );

    let acted = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act", "--json"])
        .output()
        .expect("ran");
    let reading = shaped("rebase", &acted);
    let found = row(&reading, BEHIND);
    assert_eq!(found["outcome"], json!("conflicted"), "{reading:#}");
    let says = found["says"].as_str().unwrap_or_default().to_string();
    assert!(
        !says.contains("put back"),
        "a tree the sweep never touched is reported as put back: {says}"
    );
    assert!(
        says.contains("as found"),
        "the row does not say the tree was left as it was found: {says}"
    );
    assert_eq!(
        reading["restored"],
        json!(false),
        "a conflict still standing in a tree is read as restored: {reading:#}"
    );

    let written = found["ticket"]
        .as_str()
        .unwrap_or_else(|| panic!("the conflict was not written up: {reading:#}"));
    let (path, id) = written.rsplit_once('#').expect("a plan and an id");
    let plan = fs::read_to_string(path).expect("the plan the sweep wrote");
    // This checkout's own ticket, and not the one beside it: the checkout that
    // conflicted under the sweep was restored, and says so truthfully.
    let ticket = ticket_body(&plan, id);
    assert!(
        !ticket.contains("The tree was restored"),
        "the ticket claims a restoration the sweep never performed:\n{ticket}"
    );
    assert!(
        ticket.contains("git rebase --continue"),
        "the ticket does not send the reader to finish the rebase:\n{ticket}"
    );

    // And the fact behind all of it: the conflict is where it was.
    let porcelain = Command::new("git")
        .arg("-C")
        .arg(&stopped_here)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .expect("git runs");
    assert!(
        String::from_utf8_lossy(&porcelain.stdout).contains("UU README.md"),
        "the sweep touched a rebase somebody else had begun: {}",
        String::from_utf8_lossy(&porcelain.stdout)
    );
}

/// Uncommitted work is reported and left alone (§FS-004-quick-actions.6), and
/// for a run nobody is watching that is a **good end**: a tree somebody is
/// working in has uncommitted work most of the time, so an hourly unit that
/// went red for it would read failed on every machine anybody uses and its
/// exit code would stop meaning anything (§FS-004-quick-actions.6.1).
#[test]
fn a_checkout_with_uncommitted_work_is_a_row_and_a_good_end() {
    let world = a_machine_left_alone();
    // Nothing conflicts in this scenario: the checkout that would have stopped
    // is put level first, so the exit code is about the refusal and nothing
    // else.
    git(
        &workspace(&world, CLASH),
        &["reset", "--hard", "origin/main"],
    );
    let mine = workspace(&world, BEHIND);
    fs::write(mine.join("notes.txt"), "half a thought\n").expect("write");
    git(&mine, &["add", "notes.txt"]);

    let acted = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act", "--json"])
        .output()
        .expect("ran");
    let reading = shaped("rebase", &acted);
    assert_eq!(
        acted.status.code(),
        Some(0),
        "a checkout somebody is working in failed the sweep: {reading:#}"
    );
    let left = row(&reading, BEHIND);
    assert_eq!(left["outcome"], json!("refused"), "{reading:#}");
    assert!(
        left["says"].as_str().unwrap_or_default().contains("alone"),
        "the refusal is a row without its reason: {reading:#}"
    );
    assert_eq!(
        String::from_utf8_lossy(
            &Command::new("git")
                .arg("-C")
                .arg(&mine)
                .args(["status", "--porcelain", "--untracked-files=no"])
                .output()
                .expect("git runs")
                .stdout
        )
        .trim(),
        "A  notes.txt",
        "the sweep did something to a tree with uncommitted work in it"
    );
}

/// *Could not tell* is not *no* — but only a source that could have told.
/// A status line, a message, a project's own tasks: none of them could ever
/// have answered whether a branch is under review, so none of them stops a
/// rebase however it failed (§FS-004-quick-actions.6.1). Otherwise one expired
/// credential on a source that says nothing about pull requests stops every
/// rebase in that project, hourly, forever.
#[test]
fn a_failed_source_that_could_carry_no_pull_request_stops_nothing() {
    let world = a_machine_left_alone();
    world.configure(json!({
        "defaults": { "ttl_seconds": 600 },
        "projects": { PROJECT: {
            "providers": [
                { "provider": "acmeforge", "user": "you", "repos": ["widget"] },
                { "provider": "custom-status", "command": "exit 7" }
            ]
        } }
    }));
    let refreshed = world
        .ephor_raw()
        .args(["refresh", PROJECT])
        .output()
        .expect("ran");
    assert!(
        String::from_utf8_lossy(&refreshed.stderr).contains("custom-status"),
        "the source this case is about did not fail: {}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let before: std::collections::HashMap<String, String> = heads(&world).into_iter().collect();

    let acted = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act", "--json"])
        .output()
        .expect("ran");
    let reading = shaped("rebase", &acted);
    assert_eq!(
        reading["projects"][0]["reached"],
        json!(true),
        "a status source that failed stopped the project's rebases: {reading:#}"
    );
    let after: std::collections::HashMap<String, String> = heads(&world).into_iter().collect();
    assert_ne!(
        after[BEHIND], before[BEHIND],
        "the checkout nobody is holding was not replayed: {reading:#}"
    );
    // The question the rule protects is still asked of the source that can
    // answer it: the branch under review is passed over as it always was.
    assert_eq!(row(&reading, REVIEW)["outcome"], json!("passed-over"));
}

/// A ticket that could not be opened does not swallow the report — and does
/// not swallow itself either: what stopped the write-up is on that checkout's
/// own row and in the reading, the way the one-checkout hand-over already
/// carries it, so a program learns the ticket is not there
/// (§REQ-002-parity.3, §FS-004-quick-actions.6.1).
#[test]
fn a_write_up_that_could_not_be_opened_is_carried_on_the_row() {
    let world = a_machine_left_alone();
    configured(&world, a_sweep_recipe("no-such-state"), 600);

    let acted = world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act", "--json"])
        .output()
        .expect("ran");
    assert_eq!(
        acted.status.code(),
        Some(3),
        "the conflict is still the exit, whatever became of its ticket"
    );
    let reading = shaped("rebase", &acted);
    let stopped = row(&reading, CLASH);
    assert_eq!(stopped["outcome"], json!("conflicted"), "{reading:#}");
    assert_eq!(stopped["ticket"], json!(null), "{reading:#}");
    let note = stopped["note"]
        .as_str()
        .unwrap_or_else(|| panic!("nothing on the row says the write-up failed: {reading:#}"));
    assert!(
        note.contains("no-such-state"),
        "the note does not say what stopped it: {note}"
    );
    assert!(
        reading["report"]
            .as_str()
            .unwrap_or_default()
            .contains("no-such-state"),
        "the prose report is silent about the ticket that was not opened: {reading:#}"
    );
    // The conflict itself is reported whatever happened to the ticket.
    assert!(
        reading["report"]
            .as_str()
            .unwrap_or_default()
            .contains("conflict"),
        "{reading:#}"
    );
}

/// A run held at the gate writes nothing at all — including into the feed
/// cache. Fetching is a write: it calls the forge and rewrites what the last
/// refresh left, so a reporting sweep over forty projects that freshened each
/// one would make eighty calls to say what it would do
/// (§FS-011-command-line.10, §FS-004-quick-actions.6.1).
#[test]
fn a_sweep_held_at_the_gate_fetches_nothing() {
    let world = a_machine_left_alone();
    // The cache is stale the moment it is written, so nothing but the gate can
    // be what holds the fetch back.
    configured(&world, json!({}), 0);
    let calls = world.path().join("forge-calls.log");
    let before = fs::read_to_string(&calls).unwrap_or_default();
    let cached = world.feed()["fetched_at"].clone();

    world
        .ephor()
        .args(["rebase", "--org", "foundation"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(&calls).unwrap_or_default(),
        before,
        "a sweep that was only reporting called the forge"
    );
    assert_eq!(
        world.feed()["fetched_at"],
        cached,
        "a sweep that was only reporting rewrote the feed cache"
    );

    // And the acting one does freshen, which is what the reading is for.
    world
        .ephor_raw()
        .args(["rebase", "--org", "foundation", "--act"])
        .output()
        .expect("ran");
    assert_ne!(
        fs::read_to_string(&calls).unwrap_or_default(),
        before,
        "the acting sweep asked the forge nothing"
    );
}
