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
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
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
    world.organize("foundation", "Foundation");
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
    world.configure(json!({
        "projects": { PROJECT: {
            "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["widget"] } ]
        } }
    }));

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
