//! E2E-005-dispatch: a watched item becomes tickets, and a runtime turns them into work.
//!
//! The scenario is the loop ephor exists to close (§FS-005-dispatch): a pull
//! request with a red gate and an unanswered question is handed to an agent
//! runtime, and what the runtime made of it comes back to the person who
//! dispatched it. The two halves are deliberately separate. Writing tickets is
//! ephor's: it happens with no runtime installed at all, and the plans sit on
//! disk waiting. Running them is the runtime's: it is a binding, and with
//! nothing bound on PATH the run is refused in the ladder's own words rather
//! than half-attempted (§FS-006-project-interface.10, §DA-001-runtime-bound-default).
//!
//! What comes back is read out of the work root every time it is asked for: the
//! ticket's state out of the plan the runtime owns (§FS-005-dispatch.4), the
//! verdict out of the file the state machine asks for, and a drafted reply out
//! of the file ephor named — which stays a file until a person posts it
//! (§FS-005-dispatch.13).

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::json;

use support::*;

/// The plan id ephor derives from the item's key, which is also what it hands
/// the runtime on the command line.
const PLAN: &str = "acmeforge-app-101";

/// A forge with one pull request of the user's: the gate is red and the last
/// word in the conversation is somebody else's, so it is both a thing to fix
/// and a thing to answer.
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":true,"gate":true,"replies":true}'
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
        "gate": { "repos": [ { "repo": "app", "passed": 5, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime in bash: it reads the plan it was handed, writes the verdict the
/// shipped state machine asks each ticket for, drafts the reply where ephor
/// said to, and moves the tickets it finished into the final state. Work state
/// is the runtime's, which is why it is the runtime that edits the plan
/// (§FS-005-dispatch.4).
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
root="$2"
plan=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --rhei) plan="$2"; shift 2 ;;
    *) shift ;;
  esac
done
artifacts="$root/runtime/ephor"
mkdir -p "$artifacts"
printf '%s\n' "$PWD" > "$artifacts/ran-from"

file="$root/$plan.rhei.md"
for ticket in $(grep -oE '^### Task [A-Za-z0-9_-]+:' "$file" | sed 's/^### Task //; s/:$//'); do
  printf '# %s.%s — verdict\n\nVERDICT: answered — the window resets per attempt\n' \
    "$plan" "$ticket" > "$artifacts/$plan.$ticket.verdict.md"
done
printf 'Yes — the window resets on every attempt, which is what the test asserts.\n' \
  > "$artifacts/$plan.reply.md"

awk '{ if ($0 ~ /^\*\*State:\*\*/) print "**State:** done"; else print }' "$file" > "$file.tmp"
mv "$file.tmp" "$file"
"#;

/// A world watching the forge, with the runtime bound to a name that is not on
/// PATH yet — which is every machine before the runtime is installed.
fn watching() -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
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

fn plan_path(world: &World) -> std::path::PathBuf {
    world.forest().join("panta").join(format!("{PLAN}.rhei.md"))
}

/// Tickets are written whether or not anything can run them, and the run says
/// so in the one sentence the whole of ephor refuses with
/// (§AR-005-capabilities.2).
#[test]
fn the_tickets_are_written_and_the_run_is_refused_while_no_runtime_is_installed() {
    let world = watching();

    // A dry run promises where the work goes and writes nothing.
    world
        .ephor()
        .args(["work", "dispatch", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fix-gate"))
        .stdout(predicate::str::contains("1 ticket(s) would be opened"));
    assert!(!world.forest().join("panta").exists());

    world
        .ephor()
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    // The plan carries what the watch knew — the dossier, not a link to it
    // (§FS-005-dispatch.2) — and one ticket in the state the recipe names.
    let plan = std::fs::read_to_string(plan_path(&world)).expect("the plan is on disk");
    assert!(plan.contains("## The item"), "{plan}");
    assert!(plan.contains("https://acme.example/pr/101"), "{plan}");
    assert!(
        plan.contains("does the window reset per attempt?"),
        "{plan}"
    );
    assert!(plan.contains("### Task fix-gate-1:"), "{plan}");
    assert!(plan.contains("**State:** fix"), "{plan}");

    // Nothing on PATH answers to the bound name, so running is refused with
    // the ladder's sentence — and the tickets stay exactly where they are.
    world
        .ephor()
        .args(["work", "run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "acme-runtime is not on PATH; ephor writes the tickets but the runtime runs them.",
        ));
    assert!(plan_path(&world).is_file());

    let listed = world
        .ephor()
        .args(["work", "list", "--json"])
        .output()
        .expect("the ledger lists");
    let rows = json_of(&listed);
    assert_eq!(rows[0]["item"], "acmeforge:app/101");
    assert_eq!(rows[0]["tickets"][0]["state"], "fix");
    assert_eq!(rows[0]["tickets"][0]["finished"], false);
}

/// With a runtime on PATH the loop closes: it runs from the checkout the work
/// is about, and what it left behind is read back out of the work root
/// (§FS-005-dispatch.3, §FS-005-dispatch.12, §FS-005-dispatch.13).
#[test]
fn a_bound_runtime_runs_the_plan_and_its_verdict_and_drafted_reply_come_back() {
    let world = watching();
    // The question is what this ticket is for, so the reply file is part of
    // what the runtime is asked for.
    world
        .ephor()
        .args([
            "work",
            "dispatch",
            "--item",
            "acmeforge:app/101",
            "--recipe",
            "answer",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    let work_root = world.forest().join("panta");
    let reply = work_root
        .join("runtime/ephor")
        .join(format!("{PLAN}.reply.md"));
    let plan = std::fs::read_to_string(plan_path(&world)).expect("the plan is on disk");
    // The brief names that file absolutely: the runtime runs from the
    // checkout, not from the work root, so a brief that asks for a file has to
    // say which one.
    assert!(
        plan.contains(&reply.to_string_lossy().to_string()),
        "{plan}"
    );

    world.stub("acme-runtime", ACME_RUNTIME);
    world
        .ephor()
        .args(["work", "run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("acme-runtime run"));

    // It ran from the checkout the work is about, which is the one thing the
    // ledger records rather than guesses.
    assert_eq!(
        std::fs::read_to_string(work_root.join("runtime/ephor/ran-from"))
            .expect("the runtime recorded where it ran")
            .trim(),
        world.forest().to_string_lossy()
    );

    // The state is read back out of the plan the runtime owns, never out of
    // the ledger, and the verdict out of the file the state machine asked for.
    let listed = world
        .ephor()
        .args(["work", "list", "--json"])
        .output()
        .expect("the ledger lists");
    let rows = json_of(&listed);
    let ticket = &rows[0]["tickets"][0];
    assert_eq!(ticket["id"], "answer-1");
    assert_eq!(ticket["state"], "done");
    assert_eq!(ticket["finished"], true);
    assert_eq!(
        ticket["verdict"],
        "answered — the window resets per attempt"
    );

    // The reply is read back whole — a reply summarized is a different reply —
    // and it is still a file. Posting is the person's (§FS-005-dispatch.13).
    let proposal =
        ephor::work::runtime::results::proposal(&work_root, PLAN).expect("the run drafted a reply");
    assert_eq!(
        proposal.text,
        "Yes — the window resets on every attempt, which is what the test asserts."
    );
    assert_eq!(proposal.path, reply);
    assert!(reply.is_file());
    assert!(!work_root
        .join("runtime/ephor")
        .join(format!("{PLAN}.reply.posted.md"))
        .exists());
}
