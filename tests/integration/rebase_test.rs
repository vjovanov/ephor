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
    let tmp = tempdir();
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
    let tmp = tempdir();
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
    let tmp = tempdir();
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
    let tmp = tempdir();
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

/// A repository the project declares and the disk has not got is named in the
/// answer and changes no exit code (§AR-004-forest.1): the replay did what it
/// was asked of every repository that is there, and the missing tree is a
/// question for `ephor checkout` rather than an outcome of this run
/// (§FS-004-quick-actions.7). A machine reading the code alone sends this to
/// `land`, and the report it was handed says which repository is not here.
#[test]
fn a_declared_repository_that_is_not_on_disk_does_not_fail_the_rebase() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let root = tmp.path().join("widget");
    write_registry(
        &tmp.path().join("workspaces.json"),
        &serde_json::json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [required_project(&root)],
        }),
    );

    // The project type declares `app`, `plugins` and `docs-site`; this
    // workspace holds the first and nothing else — the ordinary shape of a
    // poly-repo checkout somebody made by hand.
    let ws = root.join("feature");
    std::fs::create_dir_all(&ws).unwrap();
    let origin = tmp.path().join("app.git");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--initial-branch=master", "-q"]);
    git(&origin, &["config", "user.email", "t@example.com"]);
    git(&origin, &["config", "user.name", "t"]);
    commit(&origin, "shared.txt", "one\n", "one");
    let app = ws.join("app");
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&app)
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    git(&app, &["config", "user.email", "t@example.com"]);
    git(&app, &["config", "user.name", "t"]);
    git(&app, &["checkout", "-q", "-b", "feature"]);
    commit(&app, "mine.txt", "mine\n", "mine");
    commit(&origin, "theirs.txt", "theirs\n", "master moves");

    ephor_cmd()
        .env("EPHOR_REGISTRY", tmp.path().join("workspaces.json"))
        .args([
            "rebase",
            "--project",
            "widget",
            "--onto",
            "master",
            "--checkout",
        ])
        .arg(&ws)
        .assert()
        .success()
        .stdout(predicates::str::contains("Replayed onto `origin/master`"))
        .stdout(predicates::str::contains("## plugins"))
        .stdout(predicates::str::contains("## docs-site"))
        .stdout(predicates::str::contains("No working tree here"));
}

#[test]
fn a_directory_that_is_not_a_checkout_is_refused_by_name() {
    let tmp = tempdir();
    ephor_cmd()
        .args(["rebase", "--onto", "master", "--checkout"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("No git repository"));
}

/// An unresolved `{workspace}` in the environment a program state set used to
/// be filtered away, and a rebase that was told exactly which checkout to
/// replay silently replayed the directory it happened to be started in. A
/// value that is there is refused naming the variable it came in on, and exits
/// 2 (§FS-011-command-line.9).
#[test]
fn an_unresolved_checkout_is_refused_rather_than_read_as_the_current_directory() {
    let tmp = tempdir();
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();

    ephor_cmd()
        .current_dir(&elsewhere)
        .args(["rebase", "--onto", "master"])
        .env("CHECKOUT", "{workspace}")
        .assert()
        .code(2)
        .stderr(predicates::str::contains("CHECKOUT"))
        .stderr(predicates::str::contains("{workspace}"));
}

/// The compatibility half: an environment variable empty once trimmed is
/// nothing given, so the rebase still falls through to the directory it was
/// started in (§FS-011-command-line.9).
#[test]
fn an_empty_checkout_still_means_the_current_directory() {
    let tmp = tempdir();
    let checkout = workspace(tmp.path());
    advance_master(tmp.path(), "theirs.txt", "theirs\n");

    ephor_cmd()
        .current_dir(&checkout)
        .args(["rebase", "--onto", "master"])
        .env("CHECKOUT", "  ")
        .assert()
        .success()
        .stdout(predicates::str::contains("Replayed onto `origin/master`"));
}
