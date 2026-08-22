//! E2E-011-workflows: a workflow the runtime offers is an action, and its
//! inputs are answered here.
//!
//! The scenario is §FS-005-dispatch.19 end to end. The runtime carries whole
//! workflows — parameterized plans that lay down tasks of their own — and a
//! reader who has one should not have to leave the watch, remember another
//! vocabulary, and come back. So ephor asks the binding what it offers, an
//! entry written beside a workflow turns it into a menu entry, the matter's
//! own fields answer its inputs, and who does the work is ephor's answer
//! rather than whatever model the workflow's author happened to be running
//! (§DA-006-hands-fill-a-workflows-targets).
//!
//! What is laid down is a plan of its own beside the matter's — never a ticket
//! inside it (§FS-005-dispatch.3) — written into the matter's own work root, so
//! the operations board finds it by looking like every other plan there
//! (§FS-005-dispatch.15). Laying one down writes files and nothing else
//! (§FS-005-dispatch.7): running it is the move after.

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::json;

use support::*;

const PROJECT: &str = "demo";

/// A forge with one pull request of the reader's, on a branch that is checked
/// out — which is what a workflow about a change needs on the machine.
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":true,"gate":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/101", "repo": "app", "number": "101",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/101",
        "branch": "you/ABC-42-retry",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open", "cited": false,
        "threads": [ { "messages": [
          { "author": "Ada", "text": "does the window reset per attempt?",
            "when": "2026-07-30T11:00:00Z", "mine": false } ] } ],
        "gate": { "repos": [ { "repo": "app", "passed": 6, "failed": 0, "running": 0 } ] } }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime in bash that carries workflows. `templates --json` is its own
/// listing of what it offers — ephor asks rather than keeping a copy
/// (§DA-004-roster-is-asked-not-configured) — and `instantiate` renders one
/// into the directory it was pointed at, reading the values file ephor wrote.
///
/// The rendered workspace is the binding's own shape: a directory with an
/// index plan inside it, which is exactly what the operations board
/// recognizes without being told anything new.
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="$1"; shift

if [ "$verb" = templates ]; then
  cat <<JSON
[
  { "name": "changeset-review", "version": "1.1.0", "source": "project",
    "path": "$WORKFLOWS/changeset-review",
    "description": "Review a code change with two agents.",
    "inputs": [
      { "name": "change_ref", "description": "The change to review.",
        "type": "string", "required": true, "default": null, "validate": null },
      { "name": "smart_target", "description": "Who adjudicates.",
        "type": "string", "required": false,
        "default": "someone-elses-model", "validate": null },
      { "name": "review_focus", "description": "Focus areas.",
        "type": "array", "required": false, "default": [], "validate": null }
    ] },
  { "name": "venue-intake", "version": "1.0.0", "source": "user",
    "path": "venue-intake", "description": "Resolve a venue.",
    "inputs": [
      { "name": "conference", "description": "The venue.",
        "type": "string", "required": true, "default": null, "validate": null }
    ] }
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
  if [ "$(basename "$ref")" = changeset-review ] && ! grep -q '"change_ref"' "$values"; then
    echo "× no change_ref in $values" >&2
    exit 1
  fi
  if [ -n "$dry" ]; then
    echo "would render $(basename "$ref") into $output"
    exit 0
  fi
  mkdir -p "$output/tasks"
  printf '# Rhei: review\n\n**States:** changeset-review\n' > "$output/index.rhei.md"
  printf 'name: changeset-review\nstates:\n  split:\n  done:\n    final: true\n' \
    > "$output/states.yaml"
  cp "$values" "$output/values-as-given.json"
  echo "Instantiated $(basename "$ref") into $output"
  exit 0
fi

echo "unknown verb $verb" >&2
exit 1
"#;

/// A world watching the forge with the runtime bound and on PATH, one hand in
/// the binding's registry, and a project workflow with an entry written beside
/// it — the third home an entry may live in (§FS-005-dispatch.19).
fn watching() -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    // The workflow directory the runtime's listing points at, with the entry
    // that makes it an action beside it.
    let workflows = world.path().join("workflows");
    std::fs::create_dir_all(workflows.join("changeset-review")).expect("a workflow directory");
    std::fs::write(
        workflows.join("changeset-review").join("template.yaml"),
        "name: changeset-review\ninputs:\n  - name: change_ref\n    type: string\n    \
         positional: 1\n  - name: smart_target\n    type: string\n    \
         format: execution-target\n",
    )
    .expect("the workflow's own manifest");
    std::fs::write(
        workflows.join("changeset-review").join(".ephor.json"),
        r#"{
          "id": "review-change",
          "icon": "⌥",
          "description": "review this change",
          "when": { "kinds": ["pr"] },
          "inputs": { "change_ref": "{repo}#{number}" }
        }"#,
    )
    .expect("the entry beside the workflow");
    world.stub(
        "acme-runtime",
        &ACME_RUNTIME.replace("$WORKFLOWS", &workflows.to_string_lossy()),
    );
    // One hand the binding's own registry declares, so who does the work is
    // something ephor can answer (§FS-005-dispatch.14).
    std::fs::create_dir_all(world.path().join(".config").join("rhei")).expect("a settings home");
    std::fs::write(
        world
            .path()
            .join(".config")
            .join("rhei")
            .join("settings.json"),
        r#"{
          "agents": { "claude-code": { "command": ["sh"], "modes": { "high": [] } } },
          "models": { "luna": { "provider": "anthropic", "model": "opus",
                                "default_agent": "claude-code",
                                "agents": { "claude-code": {} } } }
        }"#,
    )
    .expect("the binding's registry of who can be asked");
    world.configure(json!({
        "projects": { PROJECT: {
            "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["app"] } ],
            "work": { "hands": { "review-change": "luna:high" } }
        } },
        "work": { "runner": "acme-runtime" }
    }));
    world.register(json!({
        "branches": [
            { "id": "demo-retry", "branch": "you/ABC-42-retry", "active": true, "ticket": "ABC-42" }
        ]
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world
}

fn work_root(world: &World) -> std::path::PathBuf {
    world.forest().join("panta")
}

/// What the runtime offers is asked of the runtime, and what it says is what
/// the listing shows — including which workflows already have an entry beside
/// them (§FS-005-dispatch.19).
#[test]
fn the_runtime_is_asked_what_workflows_it_offers() {
    let world = watching();
    world
        .ephor()
        .args(["work", "workflows"])
        .assert()
        .success()
        .stdout(predicate::str::contains("changeset-review"))
        .stdout(predicate::str::contains("'review-change' beside it"))
        .stdout(predicate::str::contains("venue-intake"));

    // One workflow named shows its inputs, and says which of them names who
    // does the work — read from the workflow's own manifest, which the
    // listing does not carry.
    world
        .ephor()
        .args(["work", "workflows", "changeset-review"])
        .assert()
        .success()
        .stdout(predicate::str::contains("change_ref"))
        .stdout(predicate::str::contains("required"))
        .stdout(predicate::str::contains("names who does the work"));
}

/// The whole of §FS-005-dispatch.19 on one item: the entry beside the workflow
/// answers its principal input from the matter's own fields, ephor's hand
/// displaces the workflow's own default for the input that names who does the
/// work, and what lands is a plan of its own in the matter's work root.
#[test]
fn a_workflow_is_laid_down_beside_the_matters_own_plan() {
    let world = watching();

    // A dry run says what would answer every input, and writes no plan. Under
    // `--json` it says the same in the shape it publishes — validated against
    // the published schema rather than merely named by it, because a declared
    // shape nobody checks is a declaration and §REQ-002-parity.4 promises a
    // contract.
    shaped(
        "work-lay",
        &world
            .ephor()
            .args([
                "work",
                "lay",
                "review-change",
                "--item",
                "acmeforge:app/101",
                "--dry-run",
                "--json",
            ])
            .output()
            .expect("the dry run"),
    );
    world
        .ephor()
        .args([
            "work",
            "lay",
            "review-change",
            "--item",
            "acmeforge:app/101",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("change_ref"))
        .stdout(predicate::str::contains("app#101"))
        .stdout(predicate::str::contains("(the entry)"))
        // The hand answers the target input, not the workflow's own default.
        .stdout(predicate::str::contains("claude-code[high]:anthropic:opus"))
        .stdout(predicate::str::contains("(the hand)"));
    assert!(!work_root(&world)
        .join("acmeforge-app-101-review-change")
        .exists());

    world
        .ephor()
        .args([
            "work",
            "lay",
            "review-change",
            "--item",
            "acmeforge:app/101",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("review-change (changeset-review)"));

    // A plan of its own, beside the matter's — never a ticket inside it
    // (§FS-005-dispatch.3).
    let laid = work_root(&world).join("acmeforge-app-101-review-change");
    assert!(laid.join("index.rhei.md").is_file(), "the plan is on disk");
    assert!(
        !work_root(&world).join("acmeforge-app-101.rhei.md").exists(),
        "nothing was written into the matter's own plan"
    );

    // The values the runtime read are the ones ephor resolved: the matter's
    // fields where the entry named them, ephor's hand where the workflow
    // declared an execution target, and nothing at all for an input standing
    // at its own default.
    let given = read_json(&laid.join("values-as-given.json"));
    assert_eq!(given["change_ref"], json!("app#101"));
    assert_eq!(
        given["smart_target"],
        json!("claude-code[high]:anthropic:opus")
    );
    assert!(given.get("review_focus").is_none(), "{given}");

    // What ephor knows reached the workflow as files, in the work root's own
    // hidden corner so that enumerating the root steps over it.
    let carried = work_root(&world)
        .join(".ephor")
        .join("acmeforge-app-101-review-change");
    let dossier = std::fs::read_to_string(carried.join("dossier.md")).expect("the dossier");
    assert!(dossier.contains("Widen the retry window"), "{dossier}");
    assert!(
        dossier.contains("does the window reset per attempt?"),
        "{dossier}"
    );
    let identifiers = read_json(&carried.join("item.json"));
    assert_eq!(identifiers["number"], json!("101"));
    assert_eq!(identifiers["branch"], json!("you/ABC-42-retry"));

    // The board finds it by looking, like every other plan in the root — and
    // it is an operation with no matter behind it in the ledger's sense: what
    // the ledger says is which dispatch made it.
    let listed = world
        .ephor()
        .args(["work", "list", "--json"])
        .output()
        .expect("work list runs");
    let rows = json_of(&listed);
    let laid_down = rows[0]["workflows"]
        .as_array()
        .expect("a workflow dispatch");
    assert_eq!(laid_down.len(), 1);
    assert_eq!(
        laid_down[0]["plan"],
        json!("acmeforge-app-101-review-change")
    );
    assert_eq!(laid_down[0]["entry"], json!("review-change"));
    // Nothing was appended to the matter's own plan, so it has no tickets.
    assert!(rows[0]["tickets"].as_array().expect("tickets").is_empty());

    // And the listing says what there is rather than mourning a plan nobody
    // promised: a matter whose only work is a workflow has no plan of its own
    // and is not reported as having lost one (§FS-005-dispatch.4).
    world
        .ephor()
        .args(["work", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("⛬ 1 workflow"))
        .stdout(predicate::str::contains("plan missing").not())
        .stdout(predicate::str::contains("acmeforge-app-101.rhei.md").not());

    // A second laying is a second record, not a correction of the first
    // (§FS-005-dispatch.19).
    world
        .ephor()
        .args([
            "work",
            "lay",
            "review-change",
            "--item",
            "acmeforge:app/101",
        ])
        .assert()
        .success();
    assert!(work_root(&world)
        .join("acmeforge-app-101-review-change-2")
        .join("index.rhei.md")
        .is_file());
}

/// An input nobody answered is refused by name rather than written with a hole
/// in it, and what the reader says for this laying alone displaces the entry
/// (§FS-005-dispatch.19).
#[test]
fn an_unanswered_input_is_refused_by_name_and_the_reader_displaces_the_entry() {
    let world = watching();

    // `venue-intake` has no entry beside it and a required input nothing
    // answers. Asked for by name it is still laid down — what is asked for is
    // asked for (§FS-005-dispatch.10) — but not with a hole in it.
    world
        .ephor()
        .args(["work", "lay", "venue-intake", "--item", "acmeforge:app/101"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing answers conference"));

    // Answered on the command line, the same laying goes through.
    world
        .ephor()
        .args([
            "work",
            "lay",
            "venue-intake",
            "--item",
            "acmeforge:app/101",
            "--set",
            "conference=ECOOP 2027",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ECOOP 2027"))
        .stdout(predicate::str::contains("(you)"));

    // And the reader's own answer displaces what the entry says.
    world
        .ephor()
        .args([
            "work",
            "lay",
            "review-change",
            "--item",
            "acmeforge:app/101",
            "--set",
            "change_ref=HEAD~3..HEAD",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("HEAD~3..HEAD"));
}

/// A project that narrows which hands may be used on it is narrowed for a
/// workflow too: a hand outside the list is refused with that reason wherever
/// it was named, the workflow's own default included
/// (§DA-006-hands-fill-a-workflows-targets).
#[test]
fn a_narrowing_binds_the_hand_a_workflow_would_have_used() {
    let world = watching();
    world.configure(json!({
        "projects": { PROJECT: {
            "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["app"] } ],
            "work": { "hands": { "review-change": "sol" }, "permitted_hands": ["luna"] }
        } },
        "work": { "runner": "acme-runtime" }
    }));
    world
        .ephor()
        .args([
            "work",
            "lay",
            "review-change",
            "--item",
            "acmeforge:app/101",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sol"));
    assert!(!work_root(&world)
        .join("acmeforge-app-101-review-change")
        .exists());
}
