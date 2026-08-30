//! E2E-014-mint-the-branch: work about a matter with no branch mints the
//! branch it needs.
//!
//! The scenario is §FS-005-dispatch.25 end to end. An issue has no branch, so
//! a project whose checkouts are one per branch has no workspace for the work
//! about it — and work that edits the change used to be written at the project
//! root, which on such a project is the directory the workspaces are in rather
//! than a checkout of anything. An agent started there would be standing in
//! the wrong tree.
//!
//! So the entry that hands the work over says which branch it belongs on, as a
//! template rendered from the matter — `fix/issue-{number}` — and the dispatch
//! makes that workspace through the one checkout operation
//! (§FS-004-quick-actions.7) before it writes anything. What this case holds
//! ephor to: the dry run makes nothing at all and still names the plan inside
//! the workspace it would make; the real run makes the worktree, the store and
//! the plan, and the ledger says the branch; a second run lands in the same
//! place; `ephor checkout` afterwards agrees the workspace is already there;
//! and without a template the same work is refused by name rather than written
//! at the root.

#[path = "../support.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use predicates::prelude::*;
use serde_json::json;

use support::*;

/// A forge with one issue of the reader's — no branch, because an issue has
/// none until somebody cuts one.
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"issues":true}'
    ;;
  issues)
    printf '%s' '[
      { "key": "acme/widget#95", "title": "Durations read as seconds",
        "url": "https://acme.example/issue/95",
        "updated_at": "2026-07-30T12:00:00Z", "status": "open" }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime that carries one workflow and renders it where it is pointed —
/// the same shape as E2E-011's, cut down to what this scenario asks of it.
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="$1"; shift

if [ "$verb" = templates ]; then
  cat <<JSON
[
  { "name": "supervised-ticket-fix", "version": "1.0.0", "source": "project",
    "path": "$WORKFLOWS/supervised-ticket-fix",
    "description": "Fix a ticket end to end.",
    "inputs": [
      { "name": "ticket", "description": "The ticket to fix.",
        "type": "string", "required": true, "default": null, "validate": null } ] }
]
JSON
  exit 0
fi

if [ "$verb" = instantiate ]; then
  ref="$1"; shift
  values=""; output=""; dry=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --values) values="$2"; shift 2 ;;
      --output) output="$2"; shift 2 ;;
      --dry-run) dry=yes; shift ;;
      *) shift ;;
    esac
  done
  if [ -n "$dry" ]; then
    echo "would render $(basename "$ref") into $output"
    exit 0
  fi
  mkdir -p "$output"
  printf '# Rhei: fix\n\n**States:** supervised-ticket-fix\n' > "$output/index.rhei.md"
  printf 'name: fix\nstates:\n  fix:\n  done:\n    final: true\n' > "$output/states.yaml"
  cp "$values" "$output/values-as-given.json"
  echo "Instantiated $(basename "$ref") into $output"
  exit 0
fi

echo "unknown verb $verb" >&2
exit 1
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
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

/// The project as it stands before anything is dispatched: an origin, and the
/// main branch checked out at `<root>/main`. Every other branch of it is a
/// workspace beside that one, and none of them exists yet.
fn project_with_main_checked_out(world: &World) {
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
}

/// A world watching the forge, with the runtime bound, the project's branches
/// one per directory, and an entry beside the workflow that says which branch
/// the work it lays down belongs on.
fn watching(branch_template: Option<&str>, branch_root: bool) -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    project_with_main_checked_out(&world);

    let workflows = world.path().join("workflows");
    std::fs::create_dir_all(workflows.join("supervised-ticket-fix")).expect("a workflow directory");
    let mut entry = json!({
        "id": "fix-issue",
        "icon": "⛬",
        "description": "fix the issue",
        "when": { "kinds": ["issue"] },
        "inputs": { "ticket": "{repo}#{number}" }
    });
    match branch_template {
        // Saying which branch the work belongs on says it needs the checkout
        // (§FS-005-dispatch.25).
        Some(template) => entry["branch"] = json!(template),
        // The same work, saying only that it edits the change: this is the
        // entry the command line used to write at the project root.
        None => entry["requires_checkout"] = json!(true),
    }
    std::fs::write(
        workflows.join("supervised-ticket-fix").join(".ephor.json"),
        serde_json::to_string_pretty(&entry).expect("an entry"),
    )
    .expect("the entry beside the workflow");
    world.stub(
        "acme-runtime",
        &ACME_RUNTIME.replace("$WORKFLOWS", &workflows.to_string_lossy()),
    );

    world.configure(json!({
        "projects": { PROJECT: {
            "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["widget"] } ]
        } },
        "work": { "runner": "acme-runtime" }
    }));
    let mut row = json!({ "branches": [] });
    if branch_root {
        row["branch_root_template"] = json!("{project_root}/{branch}");
    }
    world.register(row);
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world
}

const ITEM: &str = "acmeforge:acme/widget#95";

/// Where the workspace the template names belongs, and the plan inside it.
fn workspace(world: &World) -> PathBuf {
    world.forest().join("fix/issue-95")
}

fn plan(world: &World) -> PathBuf {
    workspace(world)
        .join("panta")
        .join("acmeforge-acme-widget-95-fix-issue")
}

/// The dry run promises the plan inside the workspace the template names, and
/// makes nothing at all — not the workspace, not the work root inside it, and
/// not the files the runtime would be shown (§FS-005-dispatch.25).
#[test]
fn a_dry_run_names_the_workspace_it_would_make_and_makes_nothing() {
    let world = watching(Some("fix/issue-{number}"), true);

    let said = world
        .ephor()
        .args([
            "work",
            "lay",
            "fix-issue",
            "--item",
            ITEM,
            "--dry-run",
            "--json",
        ])
        .assert()
        .success();
    let view = json_of(said.get_output());

    assert_eq!(view["plan"], json!(plan(&world).to_string_lossy()));
    // And it says what it would have made, since it made none of it.
    let report = view["report"].as_str().expect("a report");
    assert!(report.contains("fix/issue-95"), "{report}");
    assert!(
        report.contains(&workspace(&world).to_string_lossy().to_string()),
        "{report}"
    );

    assert!(
        !workspace(&world).exists(),
        "a dry run made {}",
        workspace(&world).display()
    );
    // Nor anywhere else: the fallback this replaced would have written the
    // work root beside the checkouts, in the project root itself.
    assert!(!world.forest().join("panta").exists());
}

/// The whole of the fix on one matter: the workspace is made through the one
/// checkout operation, the plan lands inside it, the ledger says the branch,
/// and asking again lands in the same place (§FS-005-dispatch.25).
#[test]
fn the_dispatch_makes_the_workspace_and_lays_the_plan_inside_it() {
    let world = watching(Some("fix/issue-{number}"), true);

    world
        .ephor()
        .args(["work", "lay", "fix-issue", "--item", ITEM])
        .assert()
        .success();

    // A working tree on the branch the template named, grown from the
    // project's main branch.
    let workspace = workspace(&world);
    assert!(workspace.join(".git").exists(), "no working tree");
    let head = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&workspace)
        .output()
        .expect("git runs");
    assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), "fix/issue-95");
    assert!(workspace.join("README.md").exists(), "nothing was grown");

    // With the task store every workspace ephor makes gets
    // (§FS-006-project-interface.7), and the plan inside it.
    assert!(workspace.join("panta/states.yaml").is_file());
    assert!(plan(&world).join("index.rhei.md").is_file());

    // What the work is told about where it is says the minted branch: the
    // values the workflow was answered with carry it through `{repo}#{number}`
    // resolved against the same matter, and the ledger records the branch and
    // the workspace the runtime runs from.
    let ledger = read_json(&world.path().join("state/ephor/work.json"));
    let entry = &ledger["entries"][ITEM];
    assert_eq!(entry["branch"], json!("fix/issue-95"));
    assert_eq!(entry["checkout"], json!(workspace.to_string_lossy()));
    assert_eq!(
        entry["root"],
        json!(workspace.join("panta").to_string_lossy())
    );

    // The same operation `ephor checkout` is: asked for the same branch now,
    // it finds the workspace already there rather than making a second one.
    world
        .ephor()
        .args(["checkout", "--project", PROJECT, "--branch", "fix/issue-95"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already checked out"));

    // And a second lay resolves the same template to the same workspace: the
    // rendering is the resolution, so nothing had to be written down for the
    // two to agree.
    world
        .ephor()
        .args(["work", "lay", "fix-issue", "--item", ITEM])
        .assert()
        .success();
    let laid: Vec<_> = std::fs::read_dir(workspace.join("panta"))
        .expect("the work root")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("acmeforge-acme-widget-95-fix-issue"))
        .collect();
    assert_eq!(laid.len(), 2, "two runs are two records: {laid:?}");
}

/// Without a template there is no branch for work that edits the change to be
/// done on, and the command line refuses by name rather than writing the work
/// at the project root — which is what the menu has always done
/// (§FS-005-dispatch.25).
#[test]
fn work_that_edits_the_change_with_no_branch_and_no_template_is_refused() {
    let world = watching(None, true);

    world
        .ephor()
        .args(["work", "lay", "fix-issue", "--item", ITEM])
        .assert()
        .failure()
        .stderr(predicate::str::contains("'branch' template"));
    assert!(
        !world.forest().join("panta").exists(),
        "the refusal wrote the work root at the project root"
    );
}

/// Nothing is minted into a root that is itself the checkout: a project with
/// no `branch_root_template` is refused by name (§FS-005-dispatch.25).
#[test]
fn a_project_with_no_workspace_per_branch_is_refused_by_name() {
    let world = watching(Some("fix/issue-{number}"), false);

    world
        .ephor()
        .args(["work", "lay", "fix-issue", "--item", ITEM])
        .assert()
        .failure()
        .stderr(predicate::str::contains("branch_root_template"));
    assert!(!world.forest().join("fix").exists());
}

/// A refusal after the template resolved leaves no workspace behind: the
/// workspace is made after every refusal point, so an unanswered input costs
/// nothing on disk (§FS-005-dispatch.25).
#[test]
fn a_refusal_after_the_template_resolved_leaves_no_workspace() {
    let world = watching(Some("fix/issue-{number}"), true);

    // The entry resolves its template first and asks who does the work after,
    // and this hand is nobody the runtime knows.
    world
        .ephor()
        .args([
            "work",
            "lay",
            "fix-issue",
            "--item",
            ITEM,
            "--hand",
            "nobody",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nobody"));
    assert!(
        !workspace(&world).exists(),
        "a refusal left {} behind",
        workspace(&world).display()
    );
}

/// A recipe says it the same way, and the ticket it opens lands in the
/// workspace the template named (§FS-005-dispatch.25). The key is read
/// wherever an entry hands work over, not only beside a workflow.
#[test]
fn a_recipe_may_say_the_branch_its_work_belongs_on() {
    let world = watching(Some("fix/issue-{number}"), true);
    world.configure(json!({
        "projects": { PROJECT: {
            "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["widget"] } ],
            "work": { "recipes": [ {
                "id": "do-issue",
                "description": "do the issue",
                "when": { "kinds": ["issue"] },
                "branch": "fix/issue-{number}",
                "brief": "Do {title} on {branch}, in {workspace}."
            } ] }
        } },
        "work": { "runner": "acme-runtime" }
    }));

    // Asked what it would do, it makes nothing and names the plan inside the
    // workspace it would make.
    world
        .ephor()
        .args([
            "work",
            "dispatch",
            "--item",
            ITEM,
            "--recipe",
            "do-issue",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("fix/issue-95"));
    assert!(!workspace(&world).exists());

    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM, "--recipe", "do-issue"])
        .assert()
        .success();

    // The ticket is in the minted workspace, and what it says about where it
    // is says the minted branch.
    let plan = workspace(&world)
        .join("panta")
        .join("acmeforge-acme-widget-95.rhei.md");
    let written = std::fs::read_to_string(&plan).expect("the plan");
    assert!(written.contains("fix/issue-95"), "{written}");
    assert!(
        written.contains(&workspace(&world).to_string_lossy().to_string()),
        "{written}"
    );
}

/// The menu and every reading of it offer an entry that says which branch its
/// work belongs on, in the *will check out first* shape — and go on blocking
/// one that says nothing (§FS-005-dispatch.25, §REQ-002-parity.3).
#[test]
fn the_offer_follows_the_template_on_both_surfaces() {
    let world = watching(Some("fix/issue-{number}"), true);

    let said = world
        .ephor()
        .args(["work", "offers", "--item", ITEM, "--json"])
        .assert()
        .success();
    let view = json_of(said.get_output());
    let offer = view["offers"]
        .as_array()
        .expect("offers")
        .iter()
        .find(|offer| offer["id"] == "fix-issue")
        .expect("the entry is offered on an issue with no branch");
    assert_eq!(offer["gate"], json!("needs-checkout"));
    assert_eq!(offer["branch"], json!("fix/issue-95"));
    assert_eq!(
        offer["workspace"],
        json!(workspace(&world).to_string_lossy())
    );

    // The same work saying only that it edits the change is not on the table:
    // there is no workspace for it and the dispatch refuses it.
    let world = watching(None, true);
    let said = world
        .ephor()
        .args(["work", "offers", "--item", ITEM, "--json"])
        .assert()
        .success();
    let view = json_of(said.get_output());
    assert!(
        !view["offers"]
            .as_array()
            .expect("offers")
            .iter()
            .any(|offer| offer["id"] == "fix-issue"),
        "{view}"
    );
}
