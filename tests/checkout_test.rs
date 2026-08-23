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
            "work": { "runner": RUNNER },
            "projects": { "demo": { "providers": [] } }
        }))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(tmp.join("fakebin")).unwrap();
    project_root
}

/// The runtime these tests bind. A name of the fixture's own, on a PATH the
/// fixture owns, so whether the runner answers is something a test decides by
/// writing [`stub_runner`] — never something the machine running the suite
/// decides by having the real one installed.
const RUNNER: &str = "checkout-test-runtime";

/// A runner that records what it was asked and makes the manifest the real one
/// would, so the directory it was pointed at comes back a project
/// (§FS-006-project-interface.7). Its `.gitignore` is deliberately not ephor's:
/// what a runner's project says about version control is the runner's, and the
/// self-ignore ephor adds on top is what this proves.
const RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="$1"; shift
if [ "$verb" = init ]; then
  here=""; note=1; title=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --here) here=1; shift ;;
      --no-agents) note=""; shift ;;
      --title) title="$2"; shift 2 ;;
      *) dir="$1"; shift ;;
    esac
  done
  [ -n "$here" ] || { echo "expected --here" >&2; exit 1; }
  mkdir -p "$dir"
  printf '%s\n' "$dir" > "$dir/asked"
  printf '%s\n' "$title" > "$dir/titled"
  printf '# Panta: %s\n' "$title" > "$dir/index.panta.md"
  printf 'runtime/\n' > "$dir/.gitignore"
  # The note the real one leaves in the host directory, where it was not
  # told to skip it — the checkout, which ephor promised not to change.
  [ -z "$note" ] || printf 'rhei lives here\n' >> "$(dirname "$dir")/AGENTS.md"
  exit 0
fi
exit 1
"#;

fn stub_runner(tmp: &Path) {
    make_executable(&tmp.join("fakebin").join(RUNNER), RUNTIME);
}

fn ephor(tmp: &Path) -> assert_cmd::Command {
    let mut cmd = ephor_cmd();
    cmd.env("XDG_STATE_HOME", tmp.join("state"));
    cmd.env("EPHOR_STATUS_CONFIG", tmp.join("status.json"));
    cmd.env("EPHOR_REGISTRY", tmp.join("workspaces.json"));
    // The fixture's own bin first, so a runner answers exactly when a test put
    // one there; git and the rest of the world stay reachable behind it.
    let path = std::env::join_paths(std::iter::once(tmp.join("fakebin")).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    cmd.env("PATH", path);
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

    // A workspace ephor makes gets a task store, so the first dispatch into
    // this branch has somewhere to land and what is under way is visible from
    // the moment the tree exists (§FS-006-project-interface.7). It sits at the
    // root of the multi-repo workspace, beside the repositories rather than
    // inside one of them.
    let store = target.join("panta");
    assert!(store.is_dir(), "no task store at {}", store.display());
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

/// The runtime makes its own project and ephor says where
/// (§FS-006-project-interface.7): the runner is asked for the work root ephor
/// resolved, ephor's own state machine goes in beside what it wrote, and the
/// self-ignore is ephor's whatever the runner's project says about version
/// control.
#[test]
fn the_runtime_makes_its_own_project_where_ephor_says() {
    let tmp = tempdir();
    let root = fixture(tmp.path());
    stub_runner(tmp.path());
    let _ce = repo(tmp.path(), "ce");
    let _ee = repo(tmp.path(), "ee");

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success();

    let store = root.join("feature/panta");
    // What the runner was asked, in its own words: the work root, named
    // outright, and told to be the project rather than to make one under a
    // name of its own.
    let asked = fs::read_to_string(store.join("asked")).expect("the runner was asked");
    assert_eq!(asked.trim(), store.to_string_lossy(), "{asked}");
    // Named for the workspace it stands in, not for the directory it is.
    let titled = fs::read_to_string(store.join("titled")).unwrap();
    assert_eq!(titled.trim(), "feature", "{titled}");
    // What it wrote is still there, ephor's machine is beside it, and the
    // directory ignores itself all the same.
    assert!(store.join("index.panta.md").is_file());
    assert!(store.join("states.yaml").is_file());
    let ignore = fs::read_to_string(store.join(".gitignore")).unwrap();
    assert!(ignore.contains("runtime/"), "{ignore}");
    assert!(ignore.lines().any(|line| line.trim() == "*"), "{ignore}");
    // And nothing of the runner's landed in the checkout: ephor promised the
    // branch would be byte-for-byte what it was (§REQ-001-boundary.3), and the
    // runner's own discovery note would have been a change to it.
    assert!(
        !root.join("feature/AGENTS.md").exists(),
        "the runner left a note in the checkout"
    );
}

/// A runner that is not on the machine does not fail the checkout: the
/// workspace is made either way, ephor writes the store it can, and the note
/// says what it could not do (§FS-004-quick-actions.7).
#[test]
fn a_runner_that_is_not_there_leaves_the_checkout_whole() {
    let tmp = tempdir();
    let root = fixture(tmp.path());
    let _ce = repo(tmp.path(), "ce");
    let _ee = repo(tmp.path(), "ee");

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success()
        .stderr(predicates::str::contains(RUNNER));

    let store = root.join("feature/panta");
    assert!(store.join("index.panta.md").is_file());
    assert!(store.join("states.yaml").is_file());
}

/// *Already checked out* answers the question about repositories, not the one
/// about work (§FS-004-quick-actions.7.1). A workspace that holds every
/// repository and no store is repaired by asking for the checkout again —
/// which is the shape of a workspace made before ephor made stores at all, or
/// made by a project's own checkout command.
#[test]
fn a_workspace_that_is_there_is_still_given_its_store() {
    let tmp = tempdir();
    let root = fixture(tmp.path());
    let _ce = repo(tmp.path(), "ce");
    let _ee = repo(tmp.path(), "ee");

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success();
    // The workspace as somebody else's checkout command would have left it:
    // every repository, and nowhere for a plan to land.
    let store = root.join("feature/panta");
    fs::remove_dir_all(&store).unwrap();

    ephor(tmp.path())
        .args(["checkout", "--project", "demo", "--branch", "feature"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already checked out"))
        .stdout(predicates::str::contains("task store at"));
    assert!(store.join("states.yaml").is_file());

    // And the answer a runtime reads says the same thing (§REQ-002-parity.3).
    let output = ephor(tmp.path())
        .args([
            "checkout",
            "--project",
            "demo",
            "--branch",
            "feature",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let view: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(view["store"]["made"], json!(false));
    assert_eq!(view["store"]["dir"], json!(store.to_string_lossy()));
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
