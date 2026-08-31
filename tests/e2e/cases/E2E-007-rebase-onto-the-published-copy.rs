//! E2E-007-rebase-onto-the-published-copy: a branch git itself will not replay is replayed onto its own copy.
//!
//! The scenario is the checkout a poly-repo workspace actually leaves behind
//! (§FS-004-quick-actions.8). The branch was grown with `git worktree add -b`
//! and pushed, so it has a copy on the remote and no tracking configuration
//! naming it — and then somebody else pushed to that copy: a teammate, a second
//! machine, or the forge writing onto the branch. The person's own checkout is
//! now behind the branch everybody else can see.
//!
//! Bare `git rebase` cannot help here. It leans on the tracking configuration
//! this branch has none of and refuses before it starts, which is the whole
//! reason ephor resolves the published copy itself rather than deferring to
//! git's shorthand (§FS-006-project-interface.3 is the same argument for
//! verbs: ephor does the part that is the same everywhere).
//!
//! Three things the scenario holds ephor to. The replay lands on the branch's
//! own copy and says which ref that was, not on the project's main branch —
//! two entries, two operations, and a report that names the one it did. A
//! branch that was never pushed is reported as *nothing published* and the
//! command still succeeds, because nothing to do is an answer and not a
//! failure. And `--upstream` and `--onto` are refused together: a per-repository
//! ref has no branch name to give, so the two cannot be combined — in their
//! environment spellings (`UPSTREAM`, `ONTO`) as much as in flags, because a
//! state machine speaks only the first (§FS-004-quick-actions.8).

#[path = "../support.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use predicates::prelude::*;

use support::*;

/// A git command that has to work for the world to be the world.
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

fn commit(dir: &Path, file: &str, contents: &str, message: &str) {
    std::fs::write(dir.join(file), contents).expect("write the file");
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// The branch workspace as it is on this machine: a clone whose branch was cut
/// locally — so git records no upstream for it — pushed once, and then moved on
/// by somebody else.
fn workspace_pushed_and_left_behind(world: &World) -> PathBuf {
    let origin = world.path().join("origin");
    std::fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
    commit(&origin, "README.md", "the project\n", "the project");

    let workspace = world.path().join("work/ABC-42-retry-window");
    std::fs::create_dir_all(workspace.parent().expect("a parent")).expect("the work directory");
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&workspace)
        .status()
        .expect("git clones");
    assert!(status.success());
    git(&workspace, &["config", "user.email", "t@example.com"]);
    git(&workspace, &["config", "user.name", "t"]);

    // Cut off the local branch, which records no tracking configuration, and
    // published once — the shape `git worktree add -b` leaves behind.
    git(&workspace, &["checkout", "-q", "-b", "you/ABC-42-retry"]);
    commit(&workspace, "mine.txt", "mine\n", "what I am working on");
    git(&workspace, &["push", "-q", "origin", "you/ABC-42-retry"]);

    // And then somebody else pushed to it.
    git(&origin, &["checkout", "-q", "you/ABC-42-retry"]);
    commit(&origin, "theirs.txt", "theirs\n", "somebody else pushed");
    git(&origin, &["checkout", "-q", "main"]);

    // Main moved too, by a different distance: the two rebases are two
    // operations and this scenario is about the second one.
    commit(&origin, "main-moved.txt", "main\n", "main moves");

    workspace
}

#[test]
fn a_branch_with_no_tracking_config_is_replayed_onto_the_copy_it_does_have() {
    let world = World::new();
    let workspace = workspace_pushed_and_left_behind(&world);

    // What git says when asked to do this itself: the branch records no
    // upstream, so the replay never starts.
    let refused = Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .arg("rebase")
        .output()
        .expect("git runs");
    assert!(!refused.status.success());
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        said.contains("no tracking information"),
        "git refused for another reason: {said}"
    );

    // ephor resolves the copy the branch actually has and replays onto it,
    // saying which ref that was — the branch's own, not the project's main.
    world
        .ephor()
        .args([
            "rebase",
            "--upstream",
            "--project",
            PROJECT,
            "--checkout",
            &workspace.to_string_lossy(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("origin/you/ABC-42-retry"))
        .stdout(predicate::str::contains("Replayed onto"));

    // Their commit is underneath, the reader's is on top of it, and main's is
    // nowhere near this: that is the other rebase.
    assert!(workspace.join("theirs.txt").exists());
    assert!(workspace.join("mine.txt").exists());
    assert!(!workspace.join("main-moved.txt").exists());

    // Asked again, there is nothing left to replay — an answer, not a no-op.
    world
        .ephor()
        .args([
            "rebase",
            "--upstream",
            "--checkout",
            &workspace.to_string_lossy(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Already on top of `origin/you/ABC-42-retry`",
        ));
}

#[test]
fn a_branch_that_was_never_pushed_is_reported_rather_than_refused() {
    let world = World::new();
    let workspace = workspace_pushed_and_left_behind(&world);
    git(&workspace, &["checkout", "-q", "-b", "debug-of-the-day"]);

    // No copy, so nothing to replay onto — and the command succeeds, because
    // the reader is being told what was found and not what went wrong.
    world
        .ephor()
        .args([
            "rebase",
            "--upstream",
            "--checkout",
            &workspace.to_string_lossy(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing published"))
        .stdout(predicate::str::contains("debug-of-the-day"));
}

#[test]
fn the_two_bases_cannot_be_asked_for_at_once() {
    let world = World::new();
    let workspace = workspace_pushed_and_left_behind(&world);

    world
        .ephor()
        .args([
            "rebase",
            "--upstream",
            "--onto",
            "main",
            "--checkout",
            &workspace.to_string_lossy(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

/// A state machine cannot press `--upstream`, so `UPSTREAM` set to any
/// non-empty value asks for the same replay — the environment spelling every
/// other argument of this command already has (§FS-004-quick-actions.8).
#[test]
fn the_environment_spelling_asks_for_the_same_replay() {
    let world = World::new();
    let workspace = workspace_pushed_and_left_behind(&world);

    world
        .ephor()
        .env("UPSTREAM", "1")
        .args(["rebase", "--checkout", &workspace.to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("origin/you/ABC-42-retry"))
        .stdout(predicate::str::contains("Replayed onto"));
}

/// The refusal of the two bases together holds in the environment spellings
/// too, where clap's flag conflict cannot see them: silently preferring one
/// would run a different rebase than the state asked for
/// (§FS-004-quick-actions.8).
#[test]
fn the_two_bases_refuse_together_in_their_environment_spellings() {
    let world = World::new();
    let workspace = workspace_pushed_and_left_behind(&world);

    world
        .ephor()
        .env("UPSTREAM", "1")
        .args([
            "rebase",
            "--onto",
            "main",
            "--checkout",
            &workspace.to_string_lossy(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("UPSTREAM"));
}
