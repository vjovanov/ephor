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
        "type": "string", "required": true, "default": null, "validate": null,
        "format": null, "positional": 1, "items": null, "properties": null },
      { "name": "intake_target", "description": "Who reads the call.",
        "type": "string", "required": false, "default": "someone-elses-model",
        "validate": null, "format": "execution-target",
        "positional": null, "items": null, "properties": null },
      { "name": "harvest_targets", "description": "Who harvests the roster.",
        "type": "array", "required": false, "default": [], "validate": null,
        "format": null, "positional": null, "properties": null,
        "items": { "type": "string", "format": "execution-target",
                   "required": true, "default": null, "validate": null,
                   "items": null, "properties": null } },
      { "name": "harvest_pc", "description": "Whether to harvest at all.",
        "type": "string", "required": false, "default": "none",
        "validate": "^(none|listed|all)$", "format": null,
        "positional": null, "items": null, "properties": null }
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
                                "agents": { "claude-code": {} } },
                      "sol": { "provider": "openai", "model": "gpt",
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

/// A workflow the runtime keeps inside itself — no directory beside it, and so
/// no manifest of its own to read — still has who does the work answered by
/// ephor, because the runtime's own listing says which inputs those are
/// (§DA-006-hands-fill-a-workflows-targets). An input wanting several of them
/// is answered with several, said on one line.
#[test]
fn the_listing_alone_says_which_inputs_name_who_does_the_work() {
    let world = watching();

    // The listing says it, so the reading says it — with nothing on disk to
    // read it from.
    world
        .ephor()
        .args(["work", "workflows", "venue-intake"])
        .assert()
        .success()
        .stdout(predicate::str::contains("intake_target"))
        .stdout(predicate::str::contains("names who does the work"));

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
            "--set",
            "intake_target=luna:high",
            "--set",
            r#"harvest_targets=["luna:high","sol:high"]"#,
        ])
        .assert()
        .success();

    let laid = work_root(&world).join("acmeforge-app-101-venue-intake");
    let given = read_json(&laid.join("values-as-given.json"));
    assert_eq!(given["conference"], json!("ECOOP 2027"));
    // Hands, rendered into the binding's own selector — the workflow's own
    // default for the input never stands where a hand was named.
    assert_eq!(
        given["intake_target"],
        json!("claude-code[high]:anthropic:opus")
    );
    assert_eq!(
        given["harvest_targets"],
        json!([
            "claude-code[high]:anthropic:opus",
            "claude-code[high]:openai:gpt"
        ])
    );
}

/// A hand a project does not permit is refused wherever it was named — inside
/// a list of them as much as beside one (§DA-006-hands-fill-a-workflows-targets).
#[test]
fn a_hand_named_inside_a_list_is_refused_like_any_other() {
    let world = watching();
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
            "--set",
            r#"harvest_targets=["luna:high","nobody"]"#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nobody"));
    assert!(
        !work_root(&world)
            .join("acmeforge-app-101-venue-intake")
            .exists(),
        "nothing is written behind a refusal"
    );
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

/// A workflow entry that asked to run itself, and the loop that closes around
/// it (§FS-005-dispatch.28).
///
/// Four issues nobody here opened, with no conversation and no branch — so no
/// shipped recipe covers any of them, and the only thing that applies is the
/// entry beside the runtime's own workflow. That is the matter the ticket is
/// about: an issue a machine filed, which a workflow can fix end to end and
/// which never entered that workflow without a person.
mod autorun {
    use super::*;

    const ISSUE_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"issues":true}'
    ;;
  issues)
    printf '%s' '[
      { "key": "acme/widget#10", "title": "Alpha", "url": "https://acme.example/issue/10",
        "updated_at": "2026-07-27T12:00:00Z", "status": "open",
        "role": "reviewer" },
      { "key": "acme/widget#11", "title": "Beta", "url": "https://acme.example/issue/11",
        "updated_at": "2026-07-28T12:00:00Z", "status": "open",
        "role": "reviewer" },
      { "key": "acme/widget#12", "title": "Gamma", "url": "https://acme.example/issue/12",
        "updated_at": "2026-07-29T12:00:00Z", "status": "open",
        "role": "reviewer" },
      { "key": "acme/widget#13", "title": "Delta", "url": "https://acme.example/issue/13",
        "updated_at": "2026-07-30T12:00:00Z", "status": "open",
        "role": "reviewer" }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

    /// The half of the runtime this scenario needs to lay: its own listing,
    /// and an `instantiate` that renders the workspace shape — an index, a
    /// state machine of its own, and the tasks in files beside them.
    const RENDERS: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="$1"; shift

if [ "$verb" = templates ]; then
  cat <<JSON
[
  { "name": "supervised-fix", "version": "1.0.0", "source": "project",
    "path": "$WORKFLOWS/supervised-fix",
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
  mkdir -p "$output/tasks"
  printf '# Rhei: fix it\n**States:** supervised-fix\n' > "$output/index.rhei.md"
  printf 'name: supervised-fix\nstates:\n  implementing:\n    agent: x\n  done:\n    final: true\n' \
    > "$output/states.yaml"
  printf '### Task fix: fix the ticket\n**State:** implementing\n\nwork\n' \
    > "$output/tasks/01-fix.md"
  cp "$values" "$output/values-as-given.json"
  echo "Instantiated $(basename "$ref") into $output"
  exit 0
fi
"#;

    /// And the other half: a runner that can start a run beneath the screen,
    /// which is what makes a laid plan startable by the sweep after.
    const DETACHES: &str = r#"
if [ "$verb" = run ]; then
  case "$*" in
    *--help*)
      printf 'Options:\n      --headless  detach it\n'
      exit 0 ;;
    *--headless*)
      printf '%s\n' "$*" >> "$RUNS"
      printf '{"id":"3f9a2c","pid":9,"status":"running","exit_code":null}\n'
      exit 0 ;;
  esac
fi
"#;

    const REFUSES: &str = r#"
echo "unknown verb $verb" >&2
exit 1
"#;

    /// A world whose only offer about an issue is the entry beside the
    /// workflow. `runner` names the stub deliberately: whether a runtime can
    /// detach is asked once per runner name and remembered, so a case about
    /// laying and a case about starting must not share one.
    fn world(runner: &str, script: &str, autorun: bool) -> World {
        let world = World::new();
        world.stub("ephor-forge-acmeforge", ISSUE_FORGE);
        let workflows = world.path().join("workflows");
        std::fs::create_dir_all(workflows.join("supervised-fix")).expect("a workflow directory");
        std::fs::write(
            workflows.join("supervised-fix").join("template.yaml"),
            "name: supervised-fix\ninputs:\n  - name: ticket\n    type: string\n",
        )
        .expect("the workflow's own manifest");
        std::fs::write(
            workflows.join("supervised-fix").join(".ephor.json"),
            format!(
                r#"{{
                  "id": "fix-issue",
                  "icon": "⛬",
                  "description": "fix this issue end to end",
                  "when": {{ "kinds": ["issue"] }},
                  "autorun": {autorun},
                  "inputs": {{ "ticket": "{{repo}}#{{number}}" }}
                }}"#
            ),
        )
        .expect("the entry beside the workflow");
        world.stub(
            runner,
            &script
                .replace("$WORKFLOWS", &workflows.to_string_lossy())
                .replace("$RUNS", &world.path().join("runs.log").to_string_lossy()),
        );
        world.configure(json!({
            "projects": { PROJECT: {
                "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["widget"] } ]
            } },
            "work": { "runner": runner }
        }));
        world.ephor().args(["refresh", PROJECT]).assert().success();
        world
    }

    fn laying_world(autorun: bool) -> World {
        world("acme-lay", &format!("{RENDERS}{REFUSES}"), autorun)
    }

    fn dispatch(world: &World, extra: &[&str]) -> serde_json::Value {
        let mut args = vec!["work", "dispatch", "--json"];
        args.extend_from_slice(extra);
        let output = world.ephor().args(&args).output().expect("dispatch runs");
        assert!(
            output.status.success(),
            "dispatch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        json_of(&output)
    }

    fn plan_of(world: &World, n: u32) -> std::path::PathBuf {
        work_root(world).join(format!("acmeforge-acme-widget-{n}-fix-issue"))
    }

    /// The sweep lays it about every matter it applies to, and the record of
    /// the laying is what makes the next sweep leave those matters alone
    /// (§FS-005-dispatch.28).
    #[test]
    fn the_sweep_lays_an_entry_that_asked_and_does_not_lay_it_twice() {
        let world = laying_world(true);
        let swept = dispatch(&world, &[]);
        assert_eq!(swept["laid"], 4, "{swept}");
        assert_eq!(swept["opened"], 0, "no recipe covers any of them");
        let outcomes: Vec<&str> = swept["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["outcome"].as_str().unwrap())
            .collect();
        assert_eq!(outcomes, ["laid"; 4]);
        assert_eq!(swept["items"][0]["entry"], json!("fix-issue"));
        assert_eq!(swept["items"][0]["workflow"], json!("supervised-fix"));

        // What the entry answered reached the workflow, exactly as a laying
        // by hand would have (§FS-005-dispatch.19).
        for n in [10, 11, 12, 13] {
            let given = read_json(&plan_of(&world, n).join("values-as-given.json"));
            assert_eq!(given["ticket"], json!(format!("acme/widget#{n}")));
        }

        // And a second sweep lays nothing: the matters have work now.
        let again = dispatch(&world, &[]);
        assert_eq!(again["laid"], 0, "{again}");
        assert!(again["items"].as_array().unwrap().is_empty(), "{again}");
    }

    /// Silence means the key (§FS-005-dispatch.28): an entry that said
    /// nothing is a menu row, laid by the reader and started by the reader.
    #[test]
    fn an_entry_that_did_not_ask_is_never_laid_by_the_sweep() {
        let world = laying_world(false);
        let swept = dispatch(&world, &[]);
        assert_eq!(swept["laid"], 0, "{swept}");
        assert_eq!(swept["refused"], 0, "{swept}");
        assert!(!plan_of(&world, 13).exists());

        // The reader's own key is untouched.
        world
            .ephor()
            .args([
                "work",
                "lay",
                "fix-issue",
                "--item",
                "acmeforge:acme/widget#13",
            ])
            .assert()
            .success();
        assert!(plan_of(&world, 13).join("index.rhei.md").is_file());
    }

    /// Recipes keep their priority (§FS-005-dispatch.28): a matter a recipe
    /// covers gets the ticket it always got, and the entry gets its turn only
    /// where nothing else applies. A reader who wants the workflow instead
    /// narrows the recipe, which is the mechanism recipes already have.
    #[test]
    fn a_matter_a_recipe_covers_still_gets_the_ticket() {
        let world = laying_world(true);
        world.configure(json!({
            "projects": { PROJECT: {
                "providers": [ { "provider": "acmeforge", "user": "you", "repos": ["widget"] } ]
            } },
            "work": {
                "runner": "acme-lay",
                "recipes": [{
                    "id": "triage",
                    "icon": "◆",
                    "description": "look at the issue",
                    "state": "fix",
                    "when": { "kinds": ["issue"] },
                    "brief": "Look at {title}."
                }]
            }
        }));
        let swept = dispatch(&world, &["--limit", "1"]);
        assert_eq!(swept["laid"], 0, "{swept}");
        assert_eq!(swept["opened"], 1, "{swept}");
        assert_eq!(swept["items"][0]["recipe"], json!("triage"));
        assert!(!plan_of(&world, 13).exists());
    }

    /// A sweep asked what it would do makes nothing at all: not the plan, not
    /// the record of it, and not the work root the laying would have needed
    /// (§FS-005-dispatch.28).
    #[test]
    fn a_dry_run_says_what_it_would_lay_and_writes_nothing() {
        let world = laying_world(true);
        let swept = dispatch(&world, &["--dry-run"]);
        assert_eq!(swept["laid"], 4, "{swept}");
        assert_eq!(swept["items"][0]["outcome"], json!("would-lay"));
        assert!(!work_root(&world).exists(), "a dry run made a work root");
        assert!(!world.path().join("state/ephor/work.json").exists());
    }

    /// The ordering the reader already made orders what the sweep lays, and
    /// `--limit` bounds it from the top of that order
    /// (§FS-005-dispatch.28, §FS-005-dispatch.26).
    #[test]
    fn the_ranking_orders_what_is_laid_and_a_limit_bounds_it() {
        let world = laying_world(true);
        let ranking = world.path().join("ranking.txt");
        std::fs::write(
            &ranking,
            "acmeforge:acme/widget#10\nacmeforge:acme/widget#12\n",
        )
        .unwrap();
        let swept = dispatch(
            &world,
            &["--ranking", ranking.to_str().unwrap(), "--limit", "2"],
        );
        assert_eq!(swept["laid"], 2, "{swept}");
        let items: Vec<&str> = swept["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["item"].as_str().unwrap())
            .collect();
        assert_eq!(
            items,
            ["acmeforge:acme/widget#10", "acmeforge:acme/widget#12"]
        );
        assert!(plan_of(&world, 10).exists());
        assert!(!plan_of(&world, 13).exists());
    }

    /// The whole of it: what the sweep laid, the sweep starts. The plan's own
    /// task lives in a file beside its index and runs under the machine
    /// beside it, and neither of those is the work root's
    /// (§FS-005-dispatch.28).
    #[test]
    fn what_the_sweep_laid_is_due_and_the_sweep_after_starts_it() {
        let world = world("acme-loop", &format!("{RENDERS}{DETACHES}{REFUSES}"), true);
        world
            .ephor()
            .args(["work", "dispatch", "--limit", "1"])
            .assert()
            .success();
        assert!(plan_of(&world, 13).join("tasks/01-fix.md").is_file());

        let output = world
            .ephor()
            .args(["work", "run", "--due", "--json"])
            .output()
            .expect("the due sweep runs");
        assert!(output.status.success());
        let reading = json_of(&output);
        assert_eq!(reading["failed"], 0, "{reading}");
        assert_eq!(reading["runs"][0]["outcome"], "started", "{reading}");
        assert_eq!(
            reading["runs"][0]["tickets"],
            json!(["acmeforge-acme-widget-13-fix-issue.fix"]),
            "{reading}"
        );

        // The task is over, and the root goes quiet — the plan's own machine
        // is what says so.
        std::fs::write(
            plan_of(&world, 13).join("tasks/01-fix.md"),
            "### Task fix: fix the ticket\n**State:** done\n\nwork\n",
        )
        .unwrap();
        world
            .ephor()
            .args(["work", "run", "--due"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Nothing is due"));
    }
}
