//! `ephor rebase` end to end (§FS-004-quick-actions.6): the deterministic move
//! the reader's key and a program state both run, and the exit code that
//! decides who works next (§FS-005-dispatch.12).

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use common::*;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} in {}", dir.display());
}

fn commit(dir: &Path, file: &str, contents: &str, message: &str) {
    std::fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", message]);
}

/// An origin on `master`, cloned, with a `feature` branch holding one commit.
fn workspace(root: &Path) -> PathBuf {
    let origin = root.join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--initial-branch=master", "-q"]);
    git(&origin, &["config", "user.email", "t@example.com"]);
    git(&origin, &["config", "user.name", "t"]);
    commit(&origin, "shared.txt", "one\n", "one");

    let checkout = root.join("checkout");
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&checkout)
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    git(&checkout, &["config", "user.email", "t@example.com"]);
    git(&checkout, &["config", "user.name", "t"]);
    git(&checkout, &["checkout", "-q", "-b", "feature"]);
    checkout
}

fn advance_master(root: &Path, file: &str, contents: &str) {
    commit(&root.join("origin.git"), file, contents, "master moves");
}

#[test]
fn a_trailing_branch_is_replayed_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = workspace(tmp.path());
    commit(&checkout, "mine.txt", "mine\n", "mine");
    advance_master(tmp.path(), "theirs.txt", "theirs\n");

    ephor_cmd()
        .args(["rebase", "--onto", "master", "--checkout"])
        .arg(&checkout)
        .assert()
        .success()
        .stdout(predicates::str::contains("Replayed onto `origin/master`"));
    assert!(checkout.join("theirs.txt").exists());
    assert!(checkout.join("mine.txt").exists());
}

/// A conflict exits 3 — the code the machine reads as "this one is worth a
/// model" — and the report it writes is what that model is handed.
#[test]
fn a_conflict_exits_three_and_writes_the_report_a_state_hands_on() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = workspace(tmp.path());
    commit(&checkout, "shared.txt", "ours\n", "ours");
    advance_master(tmp.path(), "shared.txt", "theirs\n");
    let report = tmp.path().join("runtime/rebase.md");

    ephor_cmd()
        .args(["rebase", "--onto", "master", "--checkout"])
        .arg(&checkout)
        .arg("--report")
        .arg(&report)
        .assert()
        .code(3);

    let written = std::fs::read_to_string(&report).unwrap();
    assert!(written.contains("stopped in a conflict"));
    assert!(written.contains("shared.txt"));
    // Left mid-rebase, which is the state resolving it needs.
    assert!(
        checkout.join(".git/rebase-merge").exists() || checkout.join(".git/rebase-apply").exists()
    );
}

/// Everything a flag says, a program state's `env:` says too — that is how the
/// machine passes `{meta.*}` to a program (the manual's §8.5).
#[test]
fn the_environment_a_program_state_sets_is_read_as_arguments() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = workspace(tmp.path());
    advance_master(tmp.path(), "theirs.txt", "theirs\n");
    let report = tmp.path().join("runtime/rebase.md");

    ephor_cmd()
        .args(["rebase"])
        .env("CHECKOUT", &checkout)
        .env("ONTO", "master")
        .env("REPORT", &report)
        .assert()
        .success();
    assert!(std::fs::read_to_string(&report)
        .unwrap()
        .contains("Replayed"));
}

#[test]
fn uncommitted_work_stops_it_rather_than_being_stashed() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = workspace(tmp.path());
    commit(&checkout, "mine.txt", "mine\n", "mine");
    advance_master(tmp.path(), "theirs.txt", "theirs\n");
    std::fs::write(checkout.join("mine.txt"), "half-written\n").unwrap();

    ephor_cmd()
        .args(["rebase", "--onto", "master", "--checkout"])
        .arg(&checkout)
        .assert()
        .code(1)
        .stdout(predicates::str::contains("Uncommitted work"));
    assert_eq!(
        std::fs::read_to_string(checkout.join("mine.txt")).unwrap(),
        "half-written\n"
    );
}

#[test]
fn a_directory_that_is_not_a_checkout_is_refused_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    ephor_cmd()
        .args(["rebase", "--onto", "master", "--checkout"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("No git repository"));
}
