//! `ephor checkout` end to end (§FS-004-quick-actions.7): the branch workspace
//! a project describes but has not got, made from the registry alone — no
//! command configured anywhere.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;

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
    fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", message]);
}

/// One repository's origin on `master`, cloned into `<root>/main/<name>` —
/// the main-branch workspace a new one is made from.
fn repo(root: &Path, name: &str) -> PathBuf {
    let origin = root.join(format!("{name}.git"));
    fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--initial-branch=master", "-q"]);
    git(&origin, &["config", "user.email", "t@example.com"]);
    git(&origin, &["config", "user.name", "t"]);
    commit(&origin, "shared.txt", "one\n", "one");

    let clone = root.join("proj").join("main").join(name);
    fs::create_dir_all(clone.parent().unwrap()).unwrap();
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&clone)
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    git(&clone, &["config", "user.email", "t@example.com"]);
    git(&clone, &["config", "user.name", "t"]);
    clone
}

/// Two repositories under one checkout, sharing a branch name — the shape
/// §FS-004-quick-actions.7 is mostly about.
fn poly_project_type(template: &Path) -> serde_json::Value {
    let repo = |id: &str| {
        json!({
            "id": id,
            "path": id,
            "role": "Repository",
            "required": true,
            "update_mode": "branch",
            "default_branch": "{branch}",
            "agents_description": format!("`{id}/` is a repository of this workspace.")
        })
    };
    json!([{
        "id": "poly",
        "layout": "polyrepo",
        "repos": [repo("ce"), repo("ee")],
        "agents": {
            "template": template.to_string_lossy(),
            "structure_intro": "This workspace contains separate repositories:",
            "summary_template": "This workspace is for branch `{branch}`."
        }
    }])
}

/// A poly-repo project whose checkouts are one per branch, with `main` on disk.
fn fixture(tmp: &Path) -> PathBuf {
    let template = write_template(tmp);
    let project_root = tmp.join("proj");
    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": poly_project_type(&template),
            "hook_sets": [],
            "projects": [{
                "id": "demo",
                "type": "poly",
                "display_name": "Demo",
                "root": project_root.to_string_lossy(),
                "main_branch": "master",
                "branch_root_template": "{project_root}/{branch}",
                "release_branches": [
                    { "id": "demo-main", "branch": "main", "active": true }
                ],
                "branches": [
                    { "id": "demo-ticket", "branch": "feature", "active": true }
                ]
            }]
        }),
    );
    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10 },
            "projects": { "demo": { "providers": [] } }
        }))
        .unwrap(),
    )
    .unwrap();
    project_root
}

fn ephor(tmp: &Path) -> assert_cmd::Command {
    let mut cmd = ephor_cmd();
    cmd.env("XDG_STATE_HOME", tmp.join("state"));
    cmd.env("EPHOR_STATUS_CONFIG", tmp.join("status.json"));
    cmd.env("EPHOR_REGISTRY", tmp.join("workspaces.json"));
    cmd
}

/// The repository that has the branch is checked out on it; the one that does
/// not gets a branch of the same name off the base — and neither needed a
/// `checkout` command in anybody's configuration.
#[test]
fn a_missing_workspace_is_made_from_the_registry_alone() {
    let tmp = tempdir();
    let root = fixture(tmp.path());
    let ce = repo(tmp.path(), "ce");
    let _ee = repo(tmp.path(), "ee");
    // The change lives in `ce` only, pushed to its origin.
    git(&ce, &["checkout", "-q", "-b", "feature"]);
    commit(&ce, "mine.txt", "mine\n", "mine");
    git(&ce, &["push", "-q", "origin", "feature"]);
    git(&ce, &["checkout", "-q", "master"]);
    git(&ce, &["branch", "-q", "-D", "feature"]);

    let target = root.join("feature");
    assert!(!target.exists());

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success()
        .stdout(predicates::str::contains("tracking the branch"))
        .stdout(predicates::str::contains("started from `origin/master`"));

    for name in ["ce", "ee"] {
        let path = target.join(name);
        assert!(path.join(".git").exists(), "{name} has no working tree");
    }
    // The repository with the change carries it; the other is at the base.
    assert!(target.join("ce/mine.txt").exists());
    assert!(!target.join("ee/mine.txt").exists());

    // A workspace ephor makes gets a ticket store, so the first dispatch into
    // this branch has somewhere to land and what is under way is visible from
    // the moment the tree exists (§FS-006-project-interface.7). It sits at the
    // root of the multi-repo workspace, beside the repositories rather than
    // inside one of them.
    let store = target.join("panta");
    assert!(store.is_dir(), "no ticket store at {}", store.display());
    assert!(store.join("states.yaml").is_file());
    // And it ignores itself, so it is ephor's planning state living in a
    // checkout rather than content the project carries
    // (§REQ-001-boundary.3): a `git status` in here is unchanged by it.
    let ignore = fs::read_to_string(store.join(".gitignore")).unwrap();
    assert!(ignore.contains('*'), "{ignore}");
}

/// Asked again it is not an error, and nothing is remade.
#[test]
fn a_workspace_that_is_already_there_says_so_and_succeeds() {
    let tmp = tempdir();
    let root = fixture(tmp.path());
    let _ce = repo(tmp.path(), "ce");
    let _ee = repo(tmp.path(), "ee");

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success();
    let stamp = fs::metadata(root.join("feature/ce"))
        .unwrap()
        .modified()
        .unwrap();

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already checked out"));
    assert_eq!(
        fs::metadata(root.join("feature/ce"))
            .unwrap()
            .modified()
            .unwrap(),
        stamp
    );
}

/// A directory is not a workspace. This is the one command whose exit code
/// answers whether a workspace is whole (§AR-004-forest.1) — every other fold
/// names a declared repository that is not on disk and carries on — so a
/// workspace holding some of the project's repositories is completed rather
/// than reported as already there. The shape is ordinary: a workspace somebody
/// made by hand, or a repository the project gained after the workspace was
/// made.
#[test]
fn a_workspace_missing_a_declared_repository_is_completed() {
    let tmp = tempdir();
    let root = fixture(tmp.path());
    let ce = repo(tmp.path(), "ce");
    let _ee = repo(tmp.path(), "ee");

    // Half a workspace, made by hand: `ce` has a working tree on the branch
    // and `ee` was never added.
    let target = root.join("feature");
    git(
        &ce,
        &[
            "worktree",
            "add",
            "-b",
            "feature",
            &target.join("ce").to_string_lossy(),
            "master",
        ],
    );
    assert!(target.join("ce/.git").exists());
    assert!(!target.join("ee").exists());

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success()
        .stdout(predicates::str::contains("is missing ee"))
        .stdout(predicates::str::contains("A working tree was already here"));

    assert!(target.join("ee/.git").exists(), "ee was not made");
    assert!(target.join("ce/.git").exists(), "ce was disturbed");
}

/// Everything a flag says, a program state's `env:` says too — the same
/// handover `ephor rebase` takes (§FS-005-dispatch.12).
#[test]
fn the_environment_a_program_state_sets_is_read_as_arguments() {
    let tmp = tempdir();
    let root = fixture(tmp.path());
    let _ce = repo(tmp.path(), "ce");
    let _ee = repo(tmp.path(), "ee");
    let report = tmp.path().join("runtime/checkout.md");

    ephor(tmp.path())
        .arg("checkout")
        .env("PROJECT", "demo")
        .env("BRANCH", "feature")
        .env("REPORT", &report)
        .assert()
        .success();

    assert!(root.join("feature/ce/.git").exists());
    assert!(fs::read_to_string(&report).unwrap().contains("check out"));
}

/// A project whose root is its checkout has no workspace to make, and is told
/// that rather than being handed an empty directory.
#[test]
fn a_project_without_branch_workspaces_is_refused_by_name() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let project_root = tmp.path().join("solo");
    fs::create_dir_all(&project_root).unwrap();
    write_registry(
        &tmp.path().join("workspaces.json"),
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [{
                "id": "solo",
                "type": "monorepo",
                "display_name": "Solo",
                "root": project_root.to_string_lossy(),
                "main_branch": "master",
                "branches": [{ "id": "solo-b", "branch": "feature", "active": true }]
            }]
        }),
    );
    fs::write(tmp.path().join("status.json"), "{}").unwrap();

    ephor(tmp.path())
        .args(["checkout", "--project", "solo", "--branch", "feature"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "does not use a checkout per branch",
        ));
}

/// Nothing on disk to add a working tree from is a refusal that says so,
/// rather than a half-made directory.
#[test]
fn a_project_with_no_checkout_yet_is_refused_by_name() {
    let tmp = tempdir();
    fixture(tmp.path());

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no checkout on disk"));
}
