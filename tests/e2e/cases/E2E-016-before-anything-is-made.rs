//! E2E-016-before-anything-is-made: the checkout decides where the workspace
//! goes before it makes anything (§FS-004-quick-actions.7.3).
//!
//! The scenario is a person at a prompt typing a branch name the project
//! cannot take, three ways, and a program state handing the same command a
//! placeholder its runtime never filled. Before this rule each of the three
//! ended somewhere other than a refusal about what was typed: a name holding
//! an unexpanded `{branch}` was dropped and answered with "nothing says which
//! branch to check out", so the person who passed `--branch` was told they had
//! passed none; a name that climbed out of the project rendered outside it and
//! was refused by git from *inside* the making, with the directories on the way
//! to it already on disk; and `panta`, on a project whose work root is written
//! beside its checkouts, put a working tree on top of the work root and exited
//! zero.
//!
//! What this case holds ephor to: each of the three is refused naming the value
//! that was given, exits 2, answers a program the same way it answers a person
//! (§FS-011-command-line.7), and leaves the disk as it found it. And the half
//! that must not change with them: the shipped work root sits inside each
//! workspace, so a project on the default configuration still checks out a
//! branch called `panta`, and a value that is absent or empty is still nothing
//! given (§FS-011-command-line.9).

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
/// branch beside it. `work_root` is where this project puts its work — the
/// shipped `{workspace}/panta`, or a template that names a place of its own.
fn project(work_root: &str) -> World {
    let world = World::new();

    let origin = world.path().join("origin");
    std::fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
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
    world.configure(json!({ "work": { "root": work_root } }));
    world
}

/// Every directory directly under the project root, which is where a checkout
/// this project makes lands — the reading a scenario about *nothing was made*
/// has to take, since an exit code says nothing about the disk.
fn workspaces(world: &World) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(world.forest())
        .expect("the project root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// The first reproducer: a value that was given, dropped, and answered as
/// though nothing had been given. It is now refused naming the flag it came in
/// on and quoting what it held, and a program reading only `--json` learns the
/// same thing (§FS-011-command-line.9).
#[test]
fn a_value_that_is_there_is_never_answered_as_no_value() {
    let world = project("{workspace}/panta");

    world
        .ephor()
        .args(["checkout", "--project", PROJECT, "--branch", "{branch}"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--branch"))
        .stderr(predicate::str::contains("{branch}"))
        .stderr(predicate::str::contains("Nothing says which branch").not());

    let refused = world
        .ephor_raw()
        .args([
            "checkout",
            "--project",
            PROJECT,
            "--branch",
            "{branch}",
            "--json",
        ])
        .output()
        .expect("ran");
    assert_eq!(refused.status.code(), Some(2));
    let outcome = json_of(&refused);
    assert_eq!(outcome["ok"], json!(false));
    assert!(
        outcome["says"]
            .as_str()
            .unwrap_or_default()
            .contains("--branch"),
        "{outcome:#}"
    );

    // The same value the other way it arrives: a state machine handing the
    // program `BRANCH: "{meta.branch}"` about a matter that has no branch, and
    // a runtime that never filled it.
    world
        .ephor()
        .arg("checkout")
        .env("PROJECT", PROJECT)
        .env("BRANCH", "{meta.branch}")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("BRANCH"));

    assert_eq!(workspaces(&world), vec!["main".to_string()]);
}

/// The second reproducer: git refused the name, but from inside the making —
/// by then the directory above the project root was on disk. The refusal now
/// happens instead of the making rather than during it, so the reading is of
/// the filesystem and not only of the exit code.
#[test]
fn a_name_that_climbs_out_of_the_project_leaves_no_directory_behind() {
    let world = project("{workspace}/panta");

    world
        .ephor()
        .args(["checkout", "--project", PROJECT, "--branch", "../escaped"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("../escaped"));

    assert!(
        !world.path().join("escaped").exists(),
        "a directory was made above the project root"
    );
    assert_eq!(workspaces(&world), vec!["main".to_string()]);
}

/// The third reproducer: on a project whose work root is written beside its
/// branch checkouts, `panta` used to land a working tree on the work root and
/// exit zero — the checkout and the place plans go became one directory. The
/// refusal reads the way the reader would say it.
#[test]
fn the_work_root_is_not_a_branch() {
    let world = project("{root}/panta");

    world
        .ephor()
        .args(["checkout", "--project", PROJECT, "--branch", "panta"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("work root"))
        .stderr(predicate::str::contains("panta"));

    assert_eq!(workspaces(&world), vec!["main".to_string()]);
}

/// And what must not change with them. The shipped work root is a directory
/// *inside* each workspace, so it is not a place a branch name can land on:
/// every project on the default configuration keeps `panta` as an ordinary
/// branch. An absent or empty value is still nothing given, and still falls
/// through to the answer it always did (§FS-011-command-line.9).
#[test]
fn the_shipped_work_root_and_an_empty_value_are_left_alone() {
    let world = project("{workspace}/panta");

    world
        .ephor()
        .args(["checkout", "--project", PROJECT, "--branch", "panta"])
        .assert()
        .success();
    assert!(world.forest().join("panta/.git").exists());
    assert!(world.forest().join("panta/panta").is_dir());

    world
        .ephor()
        .arg("checkout")
        .env("PROJECT", PROJECT)
        .env("BRANCH", "   ")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Nothing says which branch"));
}
