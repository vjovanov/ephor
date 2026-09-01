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
      { "name": "dossier_path", "description": "The dossier file.",
        "type": "path", "required": true, "default": null, "validate": null },
      { "name": "item_path", "description": "The item file.",
        "type": "path", "required": true, "default": null, "validate": null },
      { "name": "smart_target", "description": "Who adjudicates.",
        "type": "string", "required": false,
        "default": "someone-elses-model", "validate": null },
      { "name": "review_focus", "description": "Focus areas.",
        "type": "array", "required": false, "default": [], "validate": null },
      { "name": "ci_commands", "description": "Project CI commands.",
        "type": "array", "required": false, "default": [], "validate": null },
      { "name": "conventions", "description": "Project conventions.",
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
  if grep -q '"reject"' "$values"; then
    echo "× runtime rejected workflow values" >&2
    exit 1
  fi
  if [ "$(basename "$ref")" = changeset-review ]; then
    dossier_path="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["dossier_path"])' "$values")"
    item_path="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["item_path"])' "$values")"
    test -f "$dossier_path" || { echo "× dossier path does not exist: $dossier_path" >&2; exit 1; }
    test -f "$item_path" || { echo "× item path does not exist: $item_path" >&2; exit 1; }
    if [ -n "$dry" ]; then
      printf '%s\n' "$values" >> "$STAGING_LOG"
      if [ "${REFUSE_STAGED:-}" = yes ]; then
        echo "× carried files refused: values=$values dossier=$dossier_path item=$item_path" >&2
        exit 1
      fi
    fi
  fi
  if [ -n "$dry" ]; then
    if [ "$(basename "$ref")" = changeset-review ]; then
      echo "would render $(basename "$ref") into $output values=$values dossier=$dossier_path item=$item_path"
    else
      echo "would render $(basename "$ref") into $output"
    fi
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
         positional: 1\n  - name: dossier_path\n    type: path\n  - name: item_path\n    type: path\n  - name: smart_target\n    type: string\n    \
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
          "inputs": { "change_ref": "{repo}#{number}",
                      "dossier_path": "{dossier}", "item_path": "{item}" }
        }"#,
    )
    .expect("the entry beside the workflow");
    world.stub(
        "acme-runtime",
        &ACME_RUNTIME
            .replace("$WORKFLOWS", &workflows.to_string_lossy())
            .replace(
                "$STAGING_LOG",
                &world.path().join("staging.log").to_string_lossy(),
            ),
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

fn assert_dry_run_left_destination_untouched(world: &World) {
    let root = work_root(world);
    for path in [
        root.clone(),
        root.join("index.panta.md"),
        root.join(".gitignore"),
        root.join("states.yaml"),
        root.join(".ephor/acmeforge-app-101-review-change/dossier.md"),
        root.join(".ephor/acmeforge-app-101-review-change/item.json"),
        root.join(".ephor/acmeforge-app-101-review-change/values.json"),
        root.join("acmeforge-app-101-review-change"),
    ] {
        assert!(!path.exists(), "dry run created {}", path.display());
    }
}

fn assert_destination_carried_paths(world: &World, text: &str) {
    let carried = work_root(world).join(".ephor/acmeforge-app-101-review-change");
    for (label, file) in [
        ("values", "values.json"),
        ("dossier", "dossier.md"),
        ("item", "item.json"),
    ] {
        let expected = carried.join(file).to_string_lossy().replace('\\', "/");
        assert!(
            text.contains(&format!("{label}={expected}")),
            "public runtime text does not name destination {label}: {text}"
        );
    }
    assert!(
        !text.contains("ephor-workflow-values-"),
        "ephemeral paths escaped into public runtime text: {text}"
    );
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

/// Explicit values files are merged in invocation order, while `--set` wins;
/// the answer account and the runtime's values file receive the same
/// structured project inputs (§FS-005-dispatch.19).
#[test]
fn values_files_are_merged_and_carried_to_the_runtime() {
    let world = watching();
    std::fs::write(
        world.path().join("base-values.yaml"),
        "ci_commands:\n  - cargo test --workspace\nconventions:\n  - rebase-only\nreview_focus:\n  - base\n",
    )
    .expect("the base values file");
    std::fs::write(
        world.path().join("project-values.json"),
        r#"{
          "ci_commands": ["cargo test --workspace", "python -m unittest"],
          "conventions": ["changelog-pr"],
          "review_focus": ["overlay"]
        }"#,
    )
    .expect("the project values file");

    let args = [
        "work",
        "lay",
        "review-change",
        "--item",
        "acmeforge:app/101",
        "--values",
        "base-values.yaml",
        "--values",
        "project-values.json",
        "--set",
        r#"review_focus=["explicit"]"#,
    ];
    let dry = world
        .ephor()
        .current_dir(world.path())
        .args([args.as_slice(), &["--dry-run", "--json"]].concat())
        .output()
        .expect("the values-file dry run");
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let view = shaped("work-lay", &dry);
    let answer = |input: &str| {
        view["answers"]
            .as_array()
            .expect("answers")
            .iter()
            .find(|answer| answer["input"] == input)
            .unwrap_or_else(|| panic!("no answer for {input}: {view}"))
    };
    assert_eq!(answer("ci_commands")["from"], json!("a values file"));
    assert_eq!(
        answer("ci_commands")["shown"],
        json!("[\"cargo test --workspace\",\"python -m unittest\"]")
    );
    assert_eq!(answer("conventions")["shown"], json!("[\"changelog-pr\"]"));
    assert_eq!(answer("conventions")["from"], json!("a values file"));
    assert_eq!(answer("review_focus")["shown"], json!("[\"explicit\"]"));
    assert_eq!(answer("review_focus")["from"], json!("you"));
    assert_dry_run_left_destination_untouched(&world);

    let laid = world
        .ephor()
        .current_dir(world.path())
        .args(args)
        .assert()
        .success();
    assert!(
        String::from_utf8_lossy(&laid.get_output().stdout).contains("laying"),
        "{}",
        String::from_utf8_lossy(&laid.get_output().stdout)
    );
    let given = read_json(
        &work_root(&world)
            .join("acmeforge-app-101-review-change")
            .join("values-as-given.json"),
    );
    assert_eq!(
        given["ci_commands"],
        json!(["cargo test --workspace", "python -m unittest"])
    );
    assert_eq!(given["conventions"], json!(["changelog-pr"]));
    assert_eq!(given["review_focus"], json!(["explicit"]));
}

/// Values input is parsed and runtime-validated before a real destination is
/// created, including the runtime's own refusal path (§FS-005-dispatch.19).
#[test]
fn invalid_values_files_leave_no_workflow_workspace() {
    let world = watching();
    std::fs::write(world.path().join("malformed.yaml"), "[\n").expect("malformed values");
    std::fs::write(world.path().join("not-a-map.yaml"), "- one\n- two\n")
        .expect("non-mapping values");
    std::fs::write(
        world.path().join("rejected.yaml"),
        "ci_commands:\n  - reject\n",
    )
    .expect("runtime-rejected values");

    for file in ["missing.yaml", "malformed.yaml", "not-a-map.yaml"] {
        world
            .ephor()
            .current_dir(world.path())
            .args([
                "work",
                "lay",
                "review-change",
                "--item",
                "acmeforge:app/101",
                "--values",
                file,
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("values file"));
        assert!(!work_root(&world).exists(), "{file} left a work root");
    }

    world
        .ephor()
        .current_dir(world.path())
        .args([
            "work",
            "lay",
            "review-change",
            "--item",
            "acmeforge:app/101",
            "--values",
            "rejected.yaml",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("runtime rejected workflow values"));
    assert!(
        !work_root(&world).exists(),
        "runtime refusal left a work root"
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
    let json_dry_run = world
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
        .expect("the dry run");
    let view = shaped("work-lay", &json_dry_run);
    assert_eq!(view["dry_run"], json!(true));
    assert!(
        view["report"]
            .as_str()
            .expect("the runtime report")
            .contains("would render changeset-review"),
        "{}",
        view["report"]
    );
    assert_destination_carried_paths(&world, view["report"].as_str().expect("the runtime report"));
    for (input, file) in [("dossier_path", "dossier.md"), ("item_path", "item.json")] {
        let shown = view["answers"]
            .as_array()
            .expect("answers")
            .iter()
            .find(|answer| answer["input"] == input)
            .unwrap_or_else(|| panic!("no answer for {input}"));
        let expected = work_root(&world)
            .join(".ephor/acmeforge-app-101-review-change")
            .join(file)
            .to_string_lossy()
            .replace('\\', "/");
        assert_eq!(shown["shown"], json!(expected));
    }
    assert!(
        !String::from_utf8_lossy(&json_dry_run.stdout).contains("ephor-workflow-values-"),
        "ephemeral paths are not public answers"
    );
    assert_dry_run_left_destination_untouched(&world);
    let prose_dry_run = world
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
        .stdout(predicate::str::contains("(the hand)"))
        .stdout(predicate::str::contains("would render changeset-review"));
    assert_destination_carried_paths(
        &world,
        &String::from_utf8_lossy(&prose_dry_run.get_output().stdout),
    );
    assert_dry_run_left_destination_untouched(&world);

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

fn staged_values(world: &World) -> Vec<std::path::PathBuf> {
    world
        .read("staging.log")
        .lines()
        .map(std::path::PathBuf::from)
        .collect()
}

fn assert_staging_removed(path: &std::path::Path) {
    let dir = path.parent().expect("the staging directory");
    for file in ["dossier.md", "item.json", "values.json"] {
        assert!(!dir.join(file).exists(), "staging left {file}");
    }
    assert!(!dir.exists(), "staging left {}", dir.display());
}

/// A dry run gives the runtime the complete carried-file set, but leaves no
/// destination behind. Both the successful validation and a runtime refusal
/// clean up the set, and each invocation gets its own collision-resistant
/// staging directory.
#[test]
fn a_workflow_dry_run_stages_and_cleans_carried_files() {
    let world = watching();
    let dry_run = [
        "work",
        "lay",
        "review-change",
        "--item",
        "acmeforge:app/101",
        "--dry-run",
    ];

    world.ephor().args(dry_run).assert().success();
    let first = staged_values(&world);
    assert_eq!(first.len(), 1, "runtime saw one staging set: {first:?}");
    assert_staging_removed(&first[0]);
    assert_dry_run_left_destination_untouched(&world);

    let refused = world
        .ephor()
        .env("REFUSE_STAGED", "yes")
        .args(dry_run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("carried files refused"));
    assert_destination_carried_paths(
        &world,
        &String::from_utf8_lossy(&refused.get_output().stderr),
    );
    let all = staged_values(&world);
    assert_eq!(all.len(), 2, "runtime saw both staging sets: {all:?}");
    assert_ne!(first[0], all[1], "dry runs do not reuse a staging path");
    assert_staging_removed(&all[1]);
    assert_dry_run_left_destination_untouched(&world);
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

    /// One issue that is over, and still in the feed: closed with the answer
    /// it was owed never given, which is the loose end that keeps finished
    /// work under Recent (§FS-003-feed-categories.2). Its timestamp is made
    /// at the moment it is asked for, because the recency window is measured
    /// against now and a fixture date would age out of it.
    const CLOSED_ISSUE_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"issues":true}'
    ;;
  issues)
    printf '[
      { "key": "acme/widget#20", "title": "Closed with the last word theirs",
        "url": "https://acme.example/issue/20",
        "updated_at": "%s", "status": "closed", "role": "reviewer",
        "messages": [ { "author": "nadia", "text": "and?", "mine": false } ] }
    ]' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
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
    fn world_of(forge: &str, runner: &str, script: &str, autorun: bool) -> World {
        let world = World::new();
        world.stub("ephor-forge-acmeforge", forge);
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

    fn world(runner: &str, script: &str, autorun: bool) -> World {
        world_of(ISSUE_FORGE, runner, script, autorun)
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

    /// Finished work is never dispatched (§FS-005-dispatch.6), and a workflow
    /// entry that asked to run itself is dispatch: an issue that is closed
    /// but still under Recent — the answer it was owed was never given, which
    /// is the loose end keeping it in the feed the sweep reads — is news, and
    /// laying a plan about it would ask an agent to invent something to do.
    /// The menu withholds the entry on such a matter already; the sweep, the
    /// loop's first unattended act, must withhold it too.
    #[test]
    fn a_finished_matter_is_never_laid_by_the_sweep() {
        let world = world_of(CLOSED_ISSUE_FORGE, "acme-closed", RENDERS, true);

        // The matter really is in front of the sweep: closed, and in the feed
        // for its loose end. A test where it had aged out would prove nothing.
        let listed = world
            .ephor()
            .args(["feed", "--json"])
            .output()
            .expect("feed runs");
        let listed = json_of(&listed);
        let shown: Vec<&str> = listed
            .as_array()
            .expect("the feed is a list of items")
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect();
        assert!(
            shown.contains(&"acmeforge:acme/widget#20"),
            "the closed issue is still under Recent: {listed}"
        );

        let swept = dispatch(&world, &[]);
        assert_eq!(swept["laid"], 0, "{swept}");
        assert_eq!(swept["refused"], 0, "{swept}");
        assert!(swept["items"].as_array().unwrap().is_empty(), "{swept}");
        assert!(!work_root(&world)
            .join("acmeforge-acme-widget-20-fix-issue")
            .exists());
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

    /// A laid plan is a store of its own, and its tasks mean what the machine
    /// **beside that plan** says they mean — on every surface that reads the
    /// floor, not only the due sweep (§FS-005-dispatch.28,
    /// §FS-006-project-interface.7). The root's machine has no word for a
    /// state the workflow's own machine parks work in, and reaching for it
    /// anyway calls a parked task *queued* — the one word
    /// §FS-005-dispatch.15 forbids, because it promises a turn that never
    /// comes. So the board and the menu row must say the same thing the due
    /// sweep says (§AR-009-surfaces.1).
    #[test]
    fn a_laid_plans_task_is_judged_by_the_machine_beside_that_plan() {
        let world = laying_world(true);
        let swept = dispatch(&world, &["--item", "acmeforge:acme/widget#13"]);
        assert_eq!(swept["laid"], 1, "{swept}");
        let plan = plan_of(&world, 13);

        // The workflow parks its task on a question, in a state of its own
        // machine's — the root's machine has no `waiting` at all.
        std::fs::write(
            plan.join("states.yaml"),
            concat!(
                "name: supervised-fix\n",
                "states:\n",
                "  implementing:\n    agent: x\n",
                "  waiting:\n    gating: true\n",
                "  done:\n    final: true\n",
            ),
        )
        .expect("the workflow's own machine");
        std::fs::write(
            plan.join("tasks/01-fix.md"),
            "### Task fix: fix the ticket\n**State:** waiting\n\nwork\n",
        )
        .expect("the parked task");

        // A run is live on the root, which is what makes the wrong machine's
        // answer *queued* rather than nothing at all.
        let lock = work_root(&world).join(".rhei/run.lock");
        std::fs::create_dir_all(lock.parent().expect("a parent")).expect("the lock directory");
        std::fs::write(&lock, "").expect("the lock file");
        let holder = std::fs::File::open(&lock).expect("the lock file");
        holder.lock().expect("the run lock");

        let board = world
            .ephor()
            .args(["operations", "--json"])
            .output()
            .expect("operations runs");
        let board = json_of(&board);
        let row = board["operations"]
            .as_array()
            .expect("an operation per root")
            .iter()
            .flat_map(|op| op["tickets"].as_array().cloned().unwrap_or_default())
            .find(|ticket| ticket["id"] == "acmeforge-acme-widget-13-fix-issue.fix")
            .unwrap_or_else(|| panic!("the laid plan's task has a row: {board}"));
        assert_eq!(row["state"], "waiting", "{row}");
        assert_eq!(row["doing"], "waiting", "{row}");
        assert_eq!(row["says"], "waiting on you", "{row}");

        // And the menu row about the same matter, which is the board's
        // reading narrowed to one row rather than a second reading.
        let view = world
            .ephor()
            .args(["actions", "--item", "acmeforge:acme/widget#13", "--json"])
            .output()
            .expect("actions runs");
        let view = json_of(&view);
        let offer = view["offers"]
            .as_array()
            .expect("a menu")
            .iter()
            .find(|offer| offer["id"] == "fix-issue")
            .unwrap_or_else(|| panic!("the entry is on the menu: {view}"));
        assert_eq!(offer["running"]["kind"], "waiting", "{offer}");
        drop(holder);
    }

    /// And the same question where the laid plan declares **no** machine of
    /// its own: `**States:**` names one the runtime resolves from the project
    /// the plan sits in, so the root's is the machine in force
    /// (§FS-005-dispatch.28, §FS-006-project-interface.7). The root's says
    /// `done` is over; the runtime's built-in default has no word for it at
    /// all, and reaching for that one calls a finished matter *queued* on the
    /// menu — work waiting for a turn that is not coming
    /// (§FS-005-dispatch.15).
    #[test]
    fn a_laid_plan_with_no_machine_beside_it_is_judged_by_the_roots() {
        let world = laying_world(true);
        let swept = dispatch(&world, &["--item", "acmeforge:acme/widget#13"]);
        assert_eq!(swept["laid"], 1, "{swept}");
        let plan = plan_of(&world, 13);
        std::fs::remove_file(plan.join("states.yaml")).expect("no machine beside the plan");
        std::fs::write(
            plan.join("tasks/01-fix.md"),
            "### Task fix: fix the ticket\n**State:** done\n\nwork\n",
        )
        .expect("the finished task");

        // A run is live on the root, which is what makes the wrong machine's
        // answer *queued* rather than nothing at all.
        let lock = work_root(&world).join(".rhei/run.lock");
        std::fs::create_dir_all(lock.parent().expect("a parent")).expect("the lock directory");
        std::fs::write(&lock, "").expect("the lock file");
        let holder = std::fs::File::open(&lock).expect("the lock file");
        holder.lock().expect("the run lock");

        let view = world
            .ephor()
            .args(["actions", "--item", "acmeforge:acme/widget#13", "--json"])
            .output()
            .expect("actions runs");
        let view = json_of(&view);
        let offer = view["offers"]
            .as_array()
            .expect("a menu")
            .iter()
            .find(|offer| offer["id"] == "fix-issue")
            .unwrap_or_else(|| panic!("the entry is on the menu: {view}"));
        assert!(offer["running"].is_null(), "the work is over: {offer}");
        drop(holder);
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
