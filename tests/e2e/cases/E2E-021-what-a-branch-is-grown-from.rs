//! E2E-021-what-a-branch-is-grown-from: `--from` takes a branch name on the
//! project's remote, and says so where the reader reads it
//! (§FS-004-quick-actions.7.4).
//!
//! The scenario is a person who knows git and has never read the manual. They
//! run `ephor checkout --help`, find `--from <FROM>` described as "Grow a
//! branch the repository does not have from this instead of the project's main
//! branch" — a sentence that names no kind of value — and reach for the
//! spelling git taught them, `origin/main`. That is itself a legal branch
//! name, so nothing refuses it: ephor supplies the remote, looks for
//! `origin/origin/main`, and the checkout is refused for a branch nobody asked
//! about. The reader pays a command to learn what the help could have said.
//!
//! What this case holds ephor to: the entry a reader reads about `--from` says
//! that what it takes is a branch name and that the remote is ephor's to
//! supply. And the half that must not change with it — the accepted value is
//! the bare branch name it always was, so `--from main` still grows the branch
//! from `origin/main` and `--from origin/main` is still carried through as the
//! branch name it looks like.

#[path = "../support.rs"]
mod support;

use std::path::Path;
use std::process::{Command, Stdio};

use predicates::prelude::*;
use serde_json::json;

use support::*;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} in {}", dir.display());
}

/// The project as a person has it: one repository published on `main`, cloned
/// into the `main` workspace, with the branch checkouts one directory per
/// branch beside it.
fn project() -> World {
    let world = World::new();

    let origin = world.path().join("origin");
    std::fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
    git(&origin, &["config", "user.email", "t@example.com"]);
    git(&origin, &["config", "user.name", "t"]);
    std::fs::write(origin.join("README.md"), "the project\n").expect("a file");
    git(&origin, &["add", "README.md"]);
    git(&origin, &["commit", "-q", "-m", "the project"]);

    let main = world.forest().join("main");
    std::fs::create_dir_all(main.parent().expect("a parent")).expect("the project root");
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&main)
        .status()
        .expect("git clones");
    assert!(status.success());
    git(&main, &["config", "user.email", "t@example.com"]);
    git(&main, &["config", "user.name", "t"]);

    world.register(json!({
        "branch_root_template": "{project_root}/{branch}",
        "branches": []
    }));
    world
}

/// The one entry a reader reads about a flag: the line that opens with it and
/// whatever the renderer wrapped underneath, as one string. Read this way
/// rather than as a fixed line, because how wide the help wraps belongs to the
/// terminal and not to this scenario.
fn entry(help: &str, flag: &str) -> String {
    let mut lines: Option<Vec<String>> = None;
    for line in help.lines() {
        let trimmed = line.trim();
        let opens = trimmed.starts_with('-');
        if opens && trimmed.starts_with(flag) {
            lines = Some(vec![trimmed.to_string()]);
            continue;
        }
        match (&mut lines, opens, trimmed.is_empty()) {
            (Some(_), true, _) | (Some(_), _, true) => break,
            (Some(found), _, _) => found.push(trimmed.to_string()),
            (None, _, _) => {}
        }
    }
    lines
        .unwrap_or_else(|| panic!("no entry for {flag} in this help:\n{help}"))
        .join(" ")
}

/// The reproducer, from the surface the reader is actually on. `--help` is
/// what a person has instead of the manual, and the manual has said
/// `--from BRANCH` all along: the entry has to name the kind of value, and say
/// that the remote is supplied rather than typed.
#[test]
fn the_help_says_what_from_takes() {
    let world = project();

    let help = world
        .ephor_raw()
        .args(["checkout", "--help"])
        .output()
        .expect("ran");
    assert!(help.status.success(), "checkout --help failed");
    let help = String::from_utf8(help.stdout).expect("utf-8");
    let from = entry(&help, "--from");

    assert!(
        from.contains("branch name"),
        "the --from entry does not say that FROM is a branch name: {from}"
    );
    assert!(
        from.contains("remote"),
        "the --from entry does not say the branch is on the project's remote, \
         so `origin/main` still reads like the right spelling: {from}"
    );
}

/// And the half that must not change with the wording. What `--from` accepts
/// is the bare branch name it always accepted: the documented spelling grows
/// the branch from the project's remote, and a remote-qualified one is taken
/// as the branch name it looks like — refused for `origin/origin/main`, which
/// is the cost the help is being made to spare the reader rather than a
/// behaviour this change touches.
#[test]
fn the_accepted_value_is_still_the_bare_branch_name() {
    let world = project();

    world
        .ephor()
        .args([
            "checkout",
            "--project",
            PROJECT,
            "--branch",
            "feat/one",
            "--from",
            "main",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("started from `origin/main`"));

    let refused = world
        .ephor_raw()
        .args([
            "checkout",
            "--project",
            PROJECT,
            "--branch",
            "feat/two",
            "--from",
            "origin/main",
        ])
        .output()
        .expect("ran");
    assert_eq!(refused.status.code(), Some(1));
    let said = String::from_utf8(refused.stdout).expect("utf-8");
    assert!(
        said.contains("origin/origin/main"),
        "the remote-qualified value was not carried through as a branch name: {said}"
    );
}
