//! E2E-019-where-the-toolchain-keeps-its-own: a work root's settings overlay is
//! found under its home, and the deprecated name still works and says so.
//!
//! The scenario is a person moving one file (§FS-006-project-interface.12).
//! `.agents/` is where an agent's instructions live and a sandboxed runtime
//! mounts it read-only, so what the toolchain maintains moved out from under it
//! — and the move has to cost the person nothing. A work root carrying the
//! overlay at its home produces the roster it always produced; a work root
//! still carrying it at `.agents/` produces that same roster and is told, in one
//! sentence, where the file belongs now; a work root carrying both is answered
//! by the home and told the other was passed over.
//!
//! The deprecation is news and not a fault, which is what separates it from the
//! settings file that does not parse: that one empties the roster in a sentence
//! of its own (§FS-005-dispatch.14). This one takes nothing away — every hand
//! survives, the ticket is written, and nothing exits differently for it.

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::json;

use support::*;

/// The plan id ephor derives from the item's key.
const PLAN: &str = "acmeforge-app-101";

/// A forge with one pull request of the user's, red gate and all, so that the
/// shipped `fix-gate` recipe has something to lay a ticket about.
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"gate":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/101", "repo": "app", "number": "101",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/101",
        "branch": "you/ABC-42-retry",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open", "cited": false, "threads": [],
        "gate": { "repos": [ { "repo": "app", "passed": 5, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime that does nothing but exist on `PATH`: with nobody to run work
/// there is nobody to ask, and the roster this case is about would be empty
/// before the overlay was ever looked for (§FS-005-dispatch.14).
const RUNTIME: &str = "#!/bin/sh\nexit 0\n";

/// The hand every case names. One id in the project's table, and the model
/// behind it is what says which file answered.
const HAND: &str = "scribe";

/// The overlay, declaring `scribe` over a model named after the file it was
/// written into — so the ticket's execution line names the file that was read.
fn overlay(model: &str) -> String {
    json!({
        "agents": { "our-agent": { "command": ["sh"], "modes": { "high": [] } } },
        "models": {
            HAND: { "provider": "acme", "model": model, "default_agent": "our-agent" }
        }
    })
    .to_string()
}

/// Where the overlay belongs, relative to the work root.
const HOME: &str = "panta/.agent-grounds/rhei/settings.json";

/// Where it used to live, and still may.
const DEPRECATED: &str = "panta/.agents/rhei/settings.json";

/// A world watching the forge, with a runtime bound and on `PATH`, and the
/// project naming `scribe` for the action it dispatches.
fn watching() -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    world.stub("acme-runtime", RUNTIME);
    world.configure(json!({
        "projects": { PROJECT: {
            "providers": [{ "provider": "acmeforge", "user": "you", "repos": ["app"] }],
            "work": { "hands": { "default": format!("{HAND}:high") } }
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

/// The dispatch every case runs, and the plan it leaves behind.
fn dispatch(world: &World) -> assert_cmd::assert::Assert {
    world
        .ephor()
        .args(["work", "dispatch", "--item", "acmeforge:app/101"])
        .assert()
}

fn plan(world: &World) -> String {
    std::fs::read_to_string(world.forest().join("panta").join(format!("{PLAN}.rhei.md")))
        .expect("the dispatch wrote a plan")
}

/// A work root carrying the overlay at its home contributes its hands: the
/// project names `scribe`, the roster has it because the file was read, and the
/// ticket carries what the runtime will execute
/// (§FS-006-project-interface.12, §FS-005-dispatch.14).
#[test]
fn the_overlay_is_found_under_the_home_it_moved_to() {
    let world = watching();
    world.file(HOME, &overlay("from-the-home"));

    dispatch(&world).success();
    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:from-the-home"),
        "the overlay under `.agent-grounds/` was not read:\n{plan}"
    );
}

/// The deprecated name goes on working, and says where the file belongs now
/// (§FS-006-project-interface.12). The roster is the one the home produces —
/// same file, same hands — the note travels beside the work rather than as a
/// refusal, and nothing exits differently for it.
#[test]
fn the_deprecated_name_still_answers_and_says_where_it_moved() {
    let world = watching();
    world.file(DEPRECATED, &overlay("from-the-deprecated-name"));

    dispatch(&world)
        .success()
        .stdout(predicate::str::contains(".agents/rhei/settings.json"))
        .stdout(predicate::str::contains(
            ".agent-grounds/rhei/settings.json",
        ));
    // Nothing was taken away: the hand the file declares still answered, and
    // the ticket carries it.
    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:from-the-deprecated-name"),
        "the deprecated name stopped contributing its hands:\n{plan}"
    );
}

/// The same sentence reaches a program (§REQ-002-parity.3): what the reader is
/// told about the deprecated name is in `--json` too, and the run is still a
/// success.
#[test]
fn a_program_reads_the_same_sentence() {
    let world = watching();
    world.file(DEPRECATED, &overlay("from-the-deprecated-name"));

    let output = world
        .ephor()
        .args(["work", "dispatch", "--item", "acmeforge:app/101", "--json"])
        .output()
        .expect("the dispatch ran");
    assert!(output.status.success(), "a deprecated path failed the run");
    let notes = json_of(&output)["notes"].to_string();
    assert!(
        notes.contains(".agents/rhei/settings.json")
            && notes.contains(".agent-grounds/rhei/settings.json"),
        "the machine form said nothing about the deprecated name: {notes}"
    );
}

/// Two homes for one file is a tie, and the home wins
/// (§FS-006-project-interface.12). The overlay under `.agents/` is passed over
/// rather than merged, and the reader is told it was — reading both would make
/// the answer depend on an order nobody wrote down.
#[test]
fn a_work_root_carrying_both_reads_the_home_and_says_the_other_was_ignored() {
    let world = watching();
    world.file(HOME, &overlay("from-the-home"));
    world.file(DEPRECATED, &overlay("from-the-deprecated-name"));

    let output = dispatch(&world).success();
    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** our-agent[high]:acme:from-the-home"),
        "the deprecated name answered over the home it moved to:\n{plan}"
    );
    assert!(
        !plan.contains("from-the-deprecated-name"),
        "the two files were merged rather than one passed over:\n{plan}"
    );
    output.stdout(predicate::str::contains(".agents/rhei/settings.json"));
}
