//! E2E-012-command-line-parity: every ability the screen holds is a command,
//! and every answer has a machine form.
//!
//! The scenario is §REQ-002-parity from the outside, with no terminal
//! anywhere. A watcher who never opens the interface asks the same questions
//! the interface answers — what may be done here, where the branches stand,
//! what is running, what was said — and gets the same facts, as prose and as
//! JSON (§FS-011-command-line).
//!
//! What it holds ephor to. The menu a matter carries is one list, assembled
//! below both surfaces, so a command sees the entry a key would have run and
//! the gate a greyed row would have shown (§AR-009-surfaces.1). An entry runs
//! from the command line exactly as it runs from a keystroke: the same working
//! directory, the same `EPHOR_*` context, the same refusals. The conversation
//! is numbered once, so the index a reading printed is the index a move takes
//! — a command nobody can aim is not parity. And `--json` puts the reading
//! alone on standard output, because a note interleaved with it is a program
//! reading ephor's prose by accident (§FS-011-command-line.7).

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::{json, Value};

use support::*;

/// A forge with one pull request, a conversation on it, and a red gate — the
/// three things a watcher acts on. It records how to react and how to reply,
/// which is what makes those moves reachable at all (§FS-007-matters.4).
const FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
request="$(cat)"
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":true,"gate":true,"react":true,"reply":true,"resolve_task":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/7", "repo": "app", "number": "7",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/7",
        "branch": "you/retry",
        "updated_at": "2026-08-01T12:00:00Z",
        "role": "author", "state": "open", "cited": false,
        "threads": [ { "reply": { "kind": "pr", "id": "7" }, "messages": [
          { "author": "Ada", "text": "does the window reset per attempt?",
            "when": "2026-08-01T11:00:00Z", "mine": false,
            "react": { "subject": "c1" } },
          { "author": "Bo", "text": "add a test for the reset",
            "when": "2026-08-01T11:30:00Z", "mine": false,
            "task": { "id": "t1", "state": "open" } } ] } ],
        "gate": { "blocked": true, "blockers": ["1 review required"],
                  "repos": [ { "repo": "app", "passed": 4, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  failures)
    printf '%s' '[ { "job": "gate / integration",
                     "trace": "retry_window_test: expected 3 attempts, saw 1" } ]'
    ;;
  react|reply|resolve-task)
    printf '%s\n' "$request" >> "${PARITY_WROTE:?}"
    printf '{}'
    ;;
  *) printf '[]' ;;
esac
"#;

const ITEM: &str = "acme:app/7";

/// A world watching one forge, with a menu entry of the person's own on it.
fn watching(world: &World) {
    world.stub("ephor-forge-acme", FORGE);
    world.configure(json!({
        "actions": [{
            "id": "note",
            "icon": "✎",
            "description": "leave a note about it",
            "command": "printf '%s\\n' \"$EPHOR_ITEM_ID\" > note.txt",
            "when": { "kinds": ["pr"] }
        }],
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
}

/// The menu a matter carries, asked for without a screen — and the entry run
/// from the same list, in the same place a keystroke would have run it.
#[test]
fn what_may_be_done_here_is_a_command_and_running_it_is_another() {
    let world = World::new();
    watching(&world);

    let listing = world
        .ephor()
        .args(["actions", "--item", ITEM, "--json"])
        .output()
        .expect("the menu");
    let menu = shaped("actions", &listing);
    assert_eq!(menu["subject"], "item");
    assert_eq!(menu["id"], ITEM);
    // Where an entry runs is on the reading, because it is the fact that
    // decides whether running the entry is safe (§AR-004-forest.3).
    assert_eq!(menu["workspace"], json!(world.forest()));
    assert_eq!(menu["workspace_state"], "ready");

    let ids: Vec<&str> = menu["offers"]
        .as_array()
        .expect("a menu")
        .iter()
        .map(|offer| offer["id"].as_str().expect("an id"))
        .collect();
    assert!(ids.contains(&"note"), "the person's own entry: {ids:?}");
    // The freehand row is always last and always there (§FS-005-dispatch.10):
    // a menu whose first key is "type something" would be a menu that gave up,
    // and a command line with no name for that row could not reach it at all.
    assert_eq!(ids.last(), Some(&"@command"));

    let note = menu["offers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|offer| offer["id"] == "note")
        .expect("the entry");
    assert_eq!(note["kind"], "command");
    assert_eq!(note["gate"], "ready");
    assert_eq!(note["background"], false);

    // Running it is the other half of the ability. The entry runs in the
    // checkout with the matter's context exported, exactly as the key does
    // (§FS-004-quick-actions.1).
    world
        .ephor()
        .args(["actions", "run", "note", "--item", ITEM])
        .assert()
        .success();
    assert_eq!(world.read(&format!("{PROJECT}/note.txt")).trim(), ITEM);

    // And the freehand row is reachable by name: whatever the reader wants to
    // run once, in the resolved place, with the dossier already there.
    world
        .ephor()
        .args([
            "actions",
            "run",
            "--item",
            ITEM,
            "--command",
            "printf '%s\\n' \"$EPHOR_PROJECT\" > freehand.txt",
        ])
        .assert()
        .success();
    assert_eq!(
        world.read(&format!("{PROJECT}/freehand.txt")).trim(),
        PROJECT
    );
}

/// An entry naming something the project cannot do is refused in the ladder's
/// own sentence — the same words a greyed row shows, never a second opinion
/// (§FS-006-project-interface.10, §AR-005-capabilities.2).
#[test]
fn an_entry_that_cannot_run_says_why_rather_than_vanishing() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.configure(json!({
        "actions": [{
            "id": "rebuild",
            "icon": "🔁",
            "description": "rebuild it",
            "command": "true",
            "requires": ["ticketed"],
            "when": { "kinds": ["pr"] }
        }],
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();

    let menu = shaped(
        "actions",
        &world
            .ephor()
            .args(["actions", "--item", ITEM, "--json"])
            .output()
            .expect("the menu"),
    );
    let blocked = menu["offers"]
        .as_array()
        .expect("a menu")
        .iter()
        .find(|offer| offer["id"] == "rebuild")
        .expect("the entry is listed, not removed");
    assert_eq!(blocked["gate"], "blocked");
    assert!(
        blocked["refusal"]
            .as_str()
            .is_some_and(|why| !why.is_empty()),
        "a blocked entry carries the reason: {blocked}"
    );

    // Asked to run it anyway, the command answers with that same sentence and
    // fails — rather than spawning something that cannot work.
    let refused = world
        .ephor()
        .args(["actions", "run", "rebuild", "--item", ITEM])
        .output()
        .expect("the refusal");
    assert!(!refused.status.success(), "a blocked entry does not run");
}

/// The conversation is numbered once, below both surfaces, and the moves take
/// that number (§AR-009-surfaces.1). A reading whose indices a move did not
/// share would be a command nobody can aim.
#[test]
fn the_conversation_is_numbered_once_and_the_moves_take_that_number() {
    let world = World::new();
    watching(&world);
    let wrote = world.path().join("wrote.jsonl");

    let thread = shaped(
        "thread",
        &world
            .ephor()
            .args(["thread", ITEM, "--json"])
            .output()
            .expect("the conversation"),
    );
    let messages = thread["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["author"], "Ada");
    // What can be done to each message is on the message, so a reader knows
    // before pressing rather than after (§FS-004-quick-actions.2).
    assert_eq!(messages[0]["can_react"], true);
    assert_eq!(messages[1]["task"]["resolved"], false);

    // Message 0 is the one the reading called 0.
    world
        .ephor()
        .env("PARITY_WROTE", &wrote)
        .args(["react", ITEM, "THUMBS_UP", "--message", "0"])
        .assert()
        .success();
    // And ticking aims at the message that carries a task.
    world
        .ephor()
        .env("PARITY_WROTE", &wrote)
        .args(["tick", ITEM, "--message", "1"])
        .assert()
        .success();
    let asked = std::fs::read_to_string(&wrote).expect("the forge was asked");
    assert!(
        asked.contains("\"c1\""),
        "the reaction named the subject the reading did: {asked}"
    );
    assert!(
        asked.contains("\"t1\""),
        "the tick named the task the reading did: {asked}"
    );

    // A message carrying no task is refused by name rather than by an error
    // from the far side.
    world
        .ephor()
        .env("PARITY_WROTE", &wrote)
        .args(["tick", ITEM, "--message", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a task"));

    // A reply goes out through the channel that declared it can carry one
    // (§FS-007-matters.4).
    world
        .ephor()
        .env("PARITY_WROTE", &wrote)
        .args(["reply", ITEM, "it", "resets", "per", "attempt"])
        .assert()
        .success();
    let asked = std::fs::read_to_string(&wrote).expect("the forge was asked");
    assert!(
        asked.contains("resets per attempt"),
        "the reply carried the words: {asked}"
    );
}

/// Where the branches stand, and what is running, answered without a screen
/// (§FS-011-command-line.2, §FS-011-command-line.3).
#[test]
fn the_branches_and_the_board_are_readings_of_their_own() {
    let world = World::new();
    // A branch the registry names, so there is a row to stand where the
    // interface would put one (§FS-004-quick-actions.6).
    world.register(
        json!({ "branches": [{ "id": "retry", "branch": "you/retry", "active": true }] }),
    );
    watching(&world);

    let branches = shaped(
        "branches",
        &world
            .ephor()
            .args(["branches", PROJECT, "--json"])
            .output()
            .expect("the branches"),
    );
    let rows = branches.as_array().expect("an array");
    let retry = rows
        .iter()
        .find(|row| row["branch"] == "you/retry")
        .unwrap_or_else(|| panic!("the declared branch is a row: {branches}"));
    // The registry named it, so it is declared — a branch ephor merely found
    // on disk is placed and worked like any other but may not widen the
    // project's identity (§FS-008-attribution.1).
    assert_eq!(retry["declared"], true);
    assert_eq!(retry["main_branch"], "main");
    for row in rows {
        assert_eq!(row["project"], PROJECT);
        // Every distance carries the day it was measured as of, or says
        // nothing: a number with no day on it is a claim about now that
        // nothing measured (§FS-004-quick-actions.6).
        if let Some(behind) = row["behind"].as_object() {
            assert!(behind.contains_key("behind"), "a distance says how far");
        }
    }

    // The board answers even with no runtime bound: the refusal is the rung's
    // own sentence, and ephor's own jobs would still stand on it
    // (§FS-005-dispatch.15).
    let board = shaped(
        "operations",
        &world
            .ephor()
            .args(["operations", "--json"])
            .output()
            .expect("the board"),
    );
    assert!(
        board["operations"].is_array(),
        "the board is a list: {board}"
    );
}

/// Under `--json`, standard output carries the reading alone — a provider's
/// note, a progress line, a hint all go to the error stream, so what a program
/// parses is never interleaved with what a person reads
/// (§FS-011-command-line.7).
#[test]
fn the_machine_form_is_alone_on_standard_output() {
    let world = World::new();
    watching(&world);

    for args in [
        vec!["actions", "--item", ITEM, "--json"],
        vec!["branches", "--json"],
        vec!["operations", "--json"],
        vec!["thread", ITEM, "--json"],
        vec!["work", "offers", "--item", ITEM, "--json"],
        vec!["failures", "--item", ITEM, "--json"],
        vec!["feed", "--json"],
        vec!["refresh", "--json"],
    ] {
        let out = world.ephor().args(&args).output().expect("a reading");
        let stdout = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str::<Value>(stdout.trim()).unwrap_or_else(|err| {
            panic!(
                "`ephor {}` did not print JSON alone: {err}\n{stdout}",
                args.join(" ")
            )
        });
    }

    // A move is held to the same rule, and it is the harder half: an entry
    // that runs prints its own output, and that output is not the reading.
    // Under `--json` it goes beside the answer, so a program parsing the
    // outcome never picks up the entry's chatter.
    let ran = world
        .ephor()
        .args([
            "actions",
            "run",
            "--item",
            ITEM,
            "--json",
            "--command",
            "printf 'the entry said this\n'",
        ])
        .output()
        .expect("the entry runs");
    let stdout = String::from_utf8_lossy(&ran.stdout);
    let outcome: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("the outcome was not alone on stdout: {err}\n{stdout}"));
    assert_eq!(outcome["ok"], true);
    assert_eq!(outcome["steps"][0]["ok"], true);
    assert!(
        String::from_utf8_lossy(&ran.stderr).contains("the entry said this"),
        "the entry's own output still reaches whoever is watching"
    );

    // The gate command reaches the same pull request by its feed id as by the
    // four coordinates a quick action passes it (§FS-011-command-line.6).
    let by_id = shaped(
        "failures",
        &world
            .ephor()
            .args(["failures", "--item", ITEM, "--json"])
            .output()
            .expect("the gate"),
    );
    let by_parts = shaped(
        "failures",
        &world
            .ephor()
            .args([
                "failures",
                "--project",
                PROJECT,
                "--source",
                "acme",
                "--repo",
                "app",
                "--number",
                "7",
                "--json",
            ])
            .output()
            .expect("the gate"),
    );
    assert_eq!(by_id, by_parts, "one pull request, one answer");
    assert_eq!(by_id["gate"]["blocked"], true);
    assert_eq!(by_id["gate"]["blockers"][0], "1 review required");
    assert_eq!(by_id["failures"][0]["job"], "gate / integration");
}

/// Every view the API answers with publishes a schema, and `ephor schema
/// views` prints it: what a release may change is answerable by diffing it
/// (§REQ-002-parity.4, §AR-009-surfaces.3).
#[test]
fn every_reading_publishes_the_shape_it_prints() {
    let world = World::new();
    let printed = world
        .ephor()
        .args(["schema", "views"])
        .output()
        .expect("the schema");
    let schema: Value =
        serde_json::from_slice(&printed.stdout).expect("the published schema is JSON");
    let properties = schema["properties"].as_object().expect("views");
    for view in ephor::api::schema::NAMES {
        assert!(
            properties.contains_key(view),
            "the reading '{view}' publishes no schema"
        );
    }
}

/// A forge that answers everything a sweep of the machine forms needs: a
/// matter with a conversation, a red gate, the failures behind it, and a
/// restart it will accept.
const EVERYTHING: &str = r#"#!/usr/bin/env bash
set -euo pipefail
request="$(cat)"
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":true,"gate":true,"failures":true,"react":true,"reply":true,"resolve_task":true,"restart":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/7", "repo": "app", "number": "7",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/7",
        "updated_at": "2026-08-01T12:00:00Z",
        "role": "author", "state": "open", "cited": false,
        "threads": [ { "reply": { "kind": "pr", "id": "7" }, "messages": [
          { "author": "Ada", "text": "does the window reset per attempt?",
            "when": "2026-08-01T11:00:00Z", "mine": false,
            "react": { "subject": "c1" } },
          { "author": "Bo", "text": "add a test for the reset",
            "when": "2026-08-01T11:30:00Z", "mine": false,
            "task": { "id": "t1", "state": "open" } } ] } ],
        "gate": { "blocked": true, "blockers": ["1 review required"],
                  "repos": [ { "repo": "app", "passed": 4, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  failures)
    printf '%s' '[ { "job": "gate / integration",
                     "trace": "retry_window_test: expected 3 attempts, saw 1" } ]'
    ;;
  restart) printf '{"asked":2}' ;;
  react|reply|resolve-task) printf '{}' ;;
  *) printf '[]' ;;
esac
"#;

/// Every machine form this world can reach, and the shape each one publishes.
///
/// The command is spelled as a reader types it; the shape is the name
/// `src/api/schema.rs` files it under. Nothing here is a re-description of the
/// command tree — the assertion below holds this table to `SHAPES`, so a
/// `--json` added anywhere fails until it is swept too.
const SWEEP: &[(&str, &[&str])] = &[
    ("actions", &["actions", "--item", ITEM, "--json"]),
    ("actions", &["actions", "list", "--item", ITEM, "--json"]),
    (
        "outcome",
        &["actions", "run", "note", "--item", ITEM, "--json"],
    ),
    ("branches", &["branches", "--json"]),
    ("operations", &["operations", "--json"]),
    ("burn", &["burn", "--json"]),
    (
        "burn",
        &["burn", "--window", "7d", "--by", "model", "--json"],
    ),
    (
        "burn",
        &["burn", "--window", "24h", "--by", "session", "--json"],
    ),
    ("burn", &["burn", "--by", "plan", "--json"]),
    ("burn", &["burn", "--by", "matter", "--json"]),
    ("thread", &["thread", ITEM, "--json"]),
    (
        "outcome",
        &["react", ITEM, "THUMBS_UP", "--message", "0", "--json"],
    ),
    ("outcome", &["tick", ITEM, "--message", "1", "--json"]),
    ("outcome", &["reply", ITEM, "it", "resets", "--json"]),
    ("failures", &["failures", "--item", ITEM, "--json"]),
    ("restart", &["restart", "--item", ITEM, "--json"]),
    ("feed", &["feed", "--json"]),
    ("status", &["status", "--json"]),
    ("refresh", &["refresh", PROJECT, "--json"]),
    ("mark-read", &["mark-read", "--id", ITEM, "--json"]),
    ("list", &["list", "--json"]),
    ("validate", &["validate", "--json"]),
    ("validate", &["validate", "--manifest", ".", "--json"]),
    ("capabilities", &["capabilities", "--json"]),
    ("doctor", &["doctor", "--json"]),
    ("job", &["job", "list", "--json"]),
    ("check", &["check", "--json"]),
    ("features", &["check", "--list-features", "--json"]),
    ("update", &["update", "--all", "--json"]),
    ("ensure-agents", &["ensure-agents", "--all", "--json"]),
    ("work-states", &["work", "states", "--json"]),
    ("work-workflows", &["work", "workflows", "--json"]),
    ("work", &["work", "offers", "--item", ITEM, "--json"]),
    (
        "work-dispatch",
        &["work", "dispatch", "--item", ITEM, "--json"],
    ),
    ("work-list", &["work", "list", "--json"]),
    ("work-sync", &["work", "sync", "--json"]),
    (
        "work-ask",
        &[
            "work", "ask", "--item", ITEM, "look", "at", "this", "--json",
        ],
    ),
    (
        "work-cancel",
        &["work", "cancel", "--item", ITEM, "fix-gate-1", "--json"],
    ),
    ("work-run", &["work", "run", "--item", ITEM, "--json"]),
    ("work-forget", &["work", "forget", "--item", ITEM, "--json"]),
];

/// Shapes the sweep cannot reach from one world, and the case that does reach
/// them. Written down rather than left out, so that "not swept" is a decision
/// somebody made rather than an oversight nobody noticed.
const ELSEWHERE: &[(&str, &str)] = &[
    (
        "rebase",
        "a_conflict_handed_over_is_part_of_the_reading — it needs a git forest mid-conflict",
    ),
    (
        "checkout",
        "a_checkout_answers_in_the_shape_it_publishes — it needs a git forest",
    ),
    (
        "job-log",
        "following_a_job_under_json_waits_for_it_and_then_answers",
    ),
    (
        "work-lay",
        "E2E-011-workflows's own lay — it needs a runtime that offers workflows",
    ),
];

/// Every `--json` prints the shape it publishes — validated against the
/// schema, not merely named in it (§REQ-002-parity.4).
///
/// This is the check the lists could never be. `every_shape_publishes_a_schema`
/// asks whether a name appears in the document, and `every_reading_publishes_
/// the_shape_it_prints` above asks the same of the printed document — both of
/// which a schema describing something else entirely passes. They did: the
/// `branches` entry declared an object while the command has always printed an
/// array, half a dozen fields were declared `string` where the code emits
/// `null`, and one entry listed outcome words ephor never prints. A declared
/// shape nobody validates against is a declaration, not a contract, and
/// §REQ-002-parity.4 promises a contract.
///
/// So: run the real commands, and put what they actually print through the
/// published schema. The table is held to `SHAPES` in both directions, which
/// is what stops this from decaying into a list somebody remembers to update.
#[test]
fn every_machine_form_prints_the_shape_it_publishes() {
    let world = World::new();
    world.stub("ephor-forge-acme", EVERYTHING);
    world.stub("acme-runtime", RUNTIME);
    // A check verb of the project's own, so `ephor check` has something to run
    // in the forest (§FS-006-project-interface.5).
    world.script("check.sh", "#!/usr/bin/env bash\nexit 0\n");
    world.configure(json!({
        "work": { "runner": "acme-runtime" },
        "actions": [{
            "id": "note",
            "icon": "✎",
            "description": "leave a note about it",
            "command": "printf '%s\\n' \"$EPHOR_ITEM_ID\" > note.txt",
            "when": { "kinds": ["pr"] }
        }],
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    // A manifest to validate, and a ticket to take back: both are what one of
    // the rows below is about.
    world.manifest(json!({ "version": 1 }));

    for (shape, args) in SWEEP {
        let out = world
            .ephor()
            .current_dir(world.forest())
            .args(*args)
            .output()
            .unwrap_or_else(|err| panic!("`ephor {}`: {err}", args.join(" ")));
        // A refused command prints an `outcome` instead, which is a different
        // (and also published) shape: the sweep is about what a command prints
        // when it works, so a refusal here is the row's own setup being wrong.
        assert!(
            out.status.success(),
            "`ephor {}` did not succeed:\n{}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        shaped(shape, &out);
    }

    // And the table is the command tree's, not anybody's memory of it: every
    // published shape is swept here or named above with the case that sweeps
    // it, and nothing is named that ephor does not publish.
    let swept: Vec<&str> = SWEEP
        .iter()
        .map(|(shape, _)| *shape)
        .chain(ELSEWHERE.iter().map(|(shape, _)| *shape))
        .collect();
    for shape in ephor::api::schema::NAMES {
        assert!(
            swept.contains(&shape),
            "nothing validates what `{shape}` actually prints — add it to SWEEP, or to \
             ELSEWHERE with the case that does"
        );
    }
    for shape in &swept {
        assert!(
            ephor::api::schema::NAMES.contains(shape),
            "'{shape}' is swept and published by nothing"
        );
    }
}

/// A world whose project keeps a branch workspace per branch, and whose one
/// matter is on a branch nobody has checked out. This is where the chain lives:
/// an entry that needs the workspace runs the checkout first, and then runs
/// *in the workspace that checkout just made* (§FS-004-quick-actions.7).
fn uncheckedout() -> World {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.register(json!({
        "branch_root_template": "{project_root}-{branch}",
        "branches": [{ "id": "retry", "branch": "you/retry", "active": true }]
    }));
    world.configure(json!({
        "actions": [{
            "id": "build",
            "icon": "🔨",
            "description": "build it",
            "command": "pwd > built.txt",
            "requires_checkout": true,
            "when": { "kinds": ["pr"] }
        }],
        "projects": { PROJECT: {
            "providers": [{ "provider": "acme", "user": "you", "repos": ["app"] }],
            // The project's own way of making a workspace, so the chain has a
            // real first step rather than ephor's `ephor checkout`.
            "checkout": {
                "icon": "⇣",
                "description": "make the workspace",
                // It runs in the root, so the tally lands there — one line per run.
                "command": "mkdir -p \"$EPHOR_WORKSPACE\" && printf 'x\\n' >> made.txt"
            }
        } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world
}

/// The checkout row *is* the checkout, and running it runs its command once.
/// The row's own action is the very command the chain already places first, so
/// a second step would run `mkdir`/`git worktree add` again — the second time
/// inside the workspace it had just created (§FS-004-quick-actions.7). One
/// chain answers here, whether the entry runs in the terminal or beneath it
/// (§AR-009-surfaces.1).
#[test]
fn a_checkout_entry_runs_its_command_once_whichever_way_it_is_started() {
    let world = uncheckedout();

    // Started beneath the screen (§FS-005-dispatch.17): the job carries the
    // chain, and the chain is one step.
    let started = shaped(
        "outcome",
        &world
            .ephor()
            .args([
                "actions",
                "run",
                "checkout",
                "--item",
                ITEM,
                "--background",
                "--json",
            ])
            .output()
            .expect("the job starts"),
    );
    assert_eq!(started["ok"], true, "{started}");
    let id = started["job"].as_str().expect("a job id");
    let record: Value =
        serde_json::from_str(&world.read(&format!("state/ephor/jobs/{id}/job.json")))
            .expect("the job record");
    let steps = record["steps"].as_array().expect("the steps");
    assert_eq!(
        steps.len(),
        1,
        "the checkout is one step, not the same command twice: {steps:?}"
    );
    assert_eq!(steps[0]["becomes_workspace"], true);
    assert!(steps[0]["creates"].is_string(), "{steps:?}");

    // And in the terminal, where the count is what the command actually did.
    let world = uncheckedout();
    world
        .ephor()
        .args(["actions", "run", "checkout", "--item", ITEM])
        .assert()
        .success();
    assert_eq!(
        world.read(&format!("{PROJECT}/made.txt")),
        "x\n",
        "the workspace was made once, not twice"
    );
}

/// A dry run reports the chain the real run would walk — every step, and the
/// directory each one lands in. An entry that needs the branch workspace runs
/// *after* the checkout and *inside* what it is about to create, so a dry run
/// naming one step and the current directory would be describing a different
/// run than the one it is standing in for (§FS-011-command-line.1).
#[test]
fn a_dry_run_reports_the_whole_chain_and_where_each_step_lands() {
    let world = uncheckedout();
    let planned = shaped(
        "outcome",
        &world
            .ephor()
            .args([
                "actions",
                "run",
                "build",
                "--item",
                ITEM,
                "--dry-run",
                "--json",
            ])
            .output()
            .expect("the dry run"),
    );
    let steps = planned["steps"].as_array().expect("the steps");
    assert_eq!(steps.len(), 2, "the checkout, then the entry: {steps:?}");
    assert!(
        steps[0]["command"].as_str().unwrap_or("").contains("mkdir"),
        "the project's own checkout goes first: {steps:?}"
    );
    // The checkout runs in the root, because the workspace is not there yet.
    assert_eq!(steps[0]["cwd"], json!(world.forest()));
    assert_eq!(steps[1]["command"], "pwd > built.txt");
    let workspace = format!("{}-you/retry", world.forest().display());
    assert_eq!(
        steps[1]["cwd"],
        json!(workspace),
        "the entry runs in the workspace the checkout is about to make: {steps:?}"
    );
    // Nothing was asked of the world.
    assert!(
        !std::path::Path::new(&workspace).exists(),
        "a dry run makes nothing"
    );

    // And the real run lands exactly there.
    world
        .ephor()
        .args(["actions", "run", "build", "--item", ITEM])
        .assert()
        .success();
    assert_eq!(
        world.read(&format!("{PROJECT}-you/retry/built.txt")).trim(),
        workspace
    );
}

/// The three moves inside a conversation are the ones the command line exists
/// for — a runtime that can read a feed but cannot post the reply its own run
/// drafted holds half a tool (§REQ-002-parity). Each takes `--json` and prints
/// the outcome shape, refusals included (§FS-011-command-line.7).
#[test]
fn the_moves_inside_a_conversation_answer_a_program() {
    let world = World::new();
    watching(&world);
    let wrote = world.path().join("wrote.jsonl");

    for args in [
        vec!["react", ITEM, "THUMBS_UP", "--message", "0", "--json"],
        vec!["tick", ITEM, "--message", "1", "--json"],
        vec!["reply", ITEM, "it", "resets", "--json"],
    ] {
        let done = world
            .ephor()
            .env("PARITY_WROTE", &wrote)
            .args(&args)
            .output()
            .expect("the move");
        let outcome = shaped("outcome", &done);
        assert_eq!(
            outcome["ok"],
            true,
            "`ephor {}` did not report an outcome: {outcome}",
            args.join(" ")
        );
        assert!(
            outcome["says"]
                .as_str()
                .is_some_and(|says| !says.is_empty()),
            "the machine form carries the sentence the prose one prints: {outcome}"
        );
    }

    // A refusal is the same shape with `ok` false, so a script reads the
    // reason rather than only the exit code (§AR-005-capabilities.2).
    let refused = world
        .ephor()
        .env("PARITY_WROTE", &wrote)
        .args(["tick", ITEM, "--message", "0", "--json"])
        .output()
        .expect("the refusal");
    assert!(!refused.status.success());
    let outcome = shaped("outcome", &refused);
    assert_eq!(outcome["ok"], false);
    assert!(
        outcome["says"]
            .as_str()
            .unwrap_or("")
            .contains("not a task"),
        "{outcome}"
    );
}

/// What could be handed over about a matter has one answer. The work screen
/// and `ephor work offers` go through one derivation, so a row the reading
/// says is not on the table is not offered on the screen either — and the
/// gating is where the dispatch's own refusals are: work that edits the change
/// is not offered where the change is not on this machine
/// (§FS-004-quick-actions.7, §REQ-002-parity.2).
#[test]
fn work_is_offered_only_where_the_dispatch_would_take_it() {
    let world = uncheckedout();
    let read = |world: &World| -> Value {
        shaped(
            "work",
            &world
                .ephor()
                .args(["work", "offers", "--item", ITEM, "--json"])
                .output()
                .expect("the work reading"),
        )
    };

    let away = read(&world);
    for offer in away["offers"].as_array().expect("the offers") {
        // Every row is one the dispatch would accept: nothing is offered that
        // the keystroke would refuse (§FS-004-quick-actions.2).
        assert_eq!(
            offer["gate"], "ready",
            "an offer that cannot run is worse than no offer: {offer}"
        );
        assert_ne!(
            offer["id"], "fix-gate",
            "work that edits the change is not offered with the change not here: {away}"
        );
        // And each one carries the words its ticket would ask for, so the
        // reading knows what the screen shows (§REQ-002-parity.3).
        if offer["kind"] == "agent" {
            assert!(
                offer["brief"]
                    .as_str()
                    .is_some_and(|brief| !brief.is_empty()),
                "an agent offer says what it would ask for: {offer}"
            );
        }
    }

    // With the workspace on disk, the same reading offers the work that edits
    // the change — one list, answered against a different disk.
    std::fs::create_dir_all(world.path().join(format!("{PROJECT}-you/retry")))
        .expect("the branch workspace");
    let here = read(&world);
    let ids: Vec<&str> = here["offers"]
        .as_array()
        .expect("the offers")
        .iter()
        .filter_map(|offer| offer["id"].as_str())
        .collect();
    assert!(
        ids.contains(&"fix-gate"),
        "the change is here now, so the work that edits it is on the table: {ids:?}"
    );
}

/// ephor's own rows live in a namespace configuration may not enter. An entry
/// configured as `@command` used to stand beside the freehand row under the
/// same name, and `ephor actions run @command` — which takes the first entry
/// answering to the id — ran the impostor (§FS-005-dispatch.10).
#[test]
fn configuration_cannot_claim_one_of_ephors_own_rows() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.configure(json!({
        "actions": [{
            "id": "@command",
            "icon": "☠",
            "description": "not the freehand row",
            "command": "printf 'the impostor ran\\n' > impostor.txt",
            "when": { "kinds": ["pr"] }
        }],
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    // Refused where it is written, so nothing downstream has to guard against
    // it — and the refusal names the id and says what to do.
    world
        .ephor()
        .args(["refresh", PROJECT])
        .assert()
        .failure()
        .stderr(predicate::str::contains("@command"))
        .stderr(predicate::str::contains("name of your own"));
}

/// A second matter, this one somebody else's, so a recipe applies to it and
/// work can actually be handed over — and a runtime stub that says something
/// on both streams, because what a run says is exactly what must not land on
/// the reading (§FS-011-command-line.7).
const THEIRS: &str = "acme:app/9";

const REVIEWABLE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
case "${1:?subcommand}" in
  capabilities) printf '{"pull_requests":true}' ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/9", "repo": "app", "number": "9", "title": "Theirs",
        "url": "https://acme.example/pr/9", "branch": "you/retry",
        "updated_at": "2026-08-01T12:00:00Z",
        "role": "reviewer", "state": "open", "cited": false }
    ]'
    ;;
  *) printf '[]' ;;
esac
"#;

/// A runtime that narrates on both streams and finishes.
const RUNTIME: &str = r#"#!/usr/bin/env bash
printf 'the runtime said this on standard output\n'
printf 'and this on the error stream\n' >&2
exit 0
"#;

fn dispatchable() -> World {
    let world = World::new();
    world.stub("ephor-forge-acme", REVIEWABLE);
    world.stub("acme-runtime", RUNTIME);
    world.configure(json!({
        "work": { "runner": "acme-runtime" },
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world
}

/// A sweep that leaves a matter alone says so in both forms. The prose line
/// and the reading's row are one answer rendered twice: a script that asked
/// about one matter and got an empty list could not tell "nothing matched"
/// from "it already has work", and the prose surface could
/// (§REQ-002-parity.3).
#[test]
fn a_sweep_that_changes_nothing_says_why_in_both_forms() {
    let world = dispatchable();
    world
        .ephor()
        .args(["work", "dispatch", "--item", THEIRS])
        .assert()
        .success()
        .stdout(predicate::str::contains("opened"));

    // Asked again in prose: the line is on standard output, where a person
    // reads it. Nothing was opened, so the exit code says so — the same code
    // the machine form exits with, because the two are one answer.
    world
        .ephor()
        .args(["work", "dispatch", "--item", THEIRS])
        .assert()
        .failure()
        .stdout(predicate::str::contains("already has work"));

    // Asked again under `--json`: standard output carries the reading alone,
    // and the reading carries the same fact.
    let out = world
        .ephor()
        .args(["work", "dispatch", "--item", THEIRS, "--json"])
        .output()
        .expect("the sweep");
    assert!(!out.status.success(), "and it exits the same way");
    let sweep = shaped("work-dispatch", &out);
    let landed = sweep["items"].as_array().expect("a row per matter reached");
    assert_eq!(landed.len(), 1, "{sweep}");
    assert_eq!(landed[0]["item"], THEIRS);
    assert_eq!(landed[0]["outcome"], "has-work");
    assert!(
        landed[0]["says"]
            .as_str()
            .is_some_and(|says| says.contains("already has work")),
        "{sweep}"
    );
}

/// A run under `--json` hands the reading its standard output and puts the
/// runtime's own beside it (§FS-011-command-line.7). The runtime still has a
/// terminal — it asks questions on it — but nothing a program is parsing is
/// interleaved with what it says.
#[test]
fn a_run_narrates_beside_the_reading_rather_than_into_it() {
    let world = dispatchable();
    world
        .ephor()
        .args(["work", "dispatch", "--item", THEIRS])
        .assert()
        .success();

    let out = world
        .ephor()
        .args(["work", "run", "--item", THEIRS, "--json"])
        .output()
        .expect("the run");
    let account = shaped("work-run", &out);
    assert_eq!(account["failed"], 0, "{account}");
    assert_eq!(account["runs"][0]["outcome"], "done", "{account}");
    let aside = String::from_utf8_lossy(&out.stderr);
    assert!(
        aside.contains("the runtime said this on standard output"),
        "the run's own output still reaches whoever is watching: {aside}"
    );
    assert!(
        aside.contains("and this on the error stream"),
        "both streams, both beside the reading: {aside}"
    );
}

/// `--follow` is how a script waits on a job, and `--json` is how it reads
/// one. Together they wait and then answer: the whole log and the state the
/// job ended in, rather than the half-written one at the moment of asking
/// (§FS-005-dispatch.17, §REQ-002-parity.3).
#[test]
fn following_a_job_under_json_waits_for_it_and_then_answers() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.configure(json!({
        "actions": [{
            "id": "slow",
            "icon": "⏳",
            "description": "a slow one",
            // A multi-byte character on every line: a log is whatever the job
            // wrote, and a character split across two writes must not stall
            // the follow.
            "command": "for i in 1 2 3; do printf 'line %s ▶\\n' \"$i\"; sleep 0.2; done",
            "when": { "kinds": ["pr"] }
        }],
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();

    let started = shaped(
        "outcome",
        &world
            .ephor()
            .args([
                "actions",
                "run",
                "slow",
                "--item",
                ITEM,
                "--background",
                "--json",
            ])
            .output()
            .expect("the job starts"),
    );
    let id = started["job"].as_str().expect("a job id").to_string();

    let followed = shaped(
        "job-log",
        &world
            .ephor()
            .args(["job", "log", &id, "--follow", "--json"])
            .output()
            .expect("the follow"),
    );
    assert_eq!(followed["id"], json!(id));
    assert_eq!(
        followed["live"], false,
        "the follow waited for the job to end: {followed}"
    );
    assert_eq!(followed["outcome"], "done", "{followed}");
    let log = followed["log"].as_str().expect("the whole log");
    for line in ["line 1 ▶", "line 2 ▶", "line 3 ▶"] {
        assert!(log.contains(line), "the log is whole: {log:?}");
    }
}

/// A flag that names who does a piece of work, given to an entry that hands
/// none over, is refused by name. It used to be parsed, put on the entry, and
/// never read again — and a reader who named a hand and got the default had no
/// way to tell (§FS-004-quick-actions.2, §REQ-002-parity.2).
#[test]
fn a_flag_that_cannot_apply_is_refused_rather_than_dropped() {
    let world = World::new();
    watching(&world);

    world
        .ephor()
        .args(["actions", "run", "note", "--item", ITEM, "--hand", "luna"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hands none over"));
    // And the entry did not run behind the refusal.
    assert!(
        !world.path().join(PROJECT).join("note.txt").exists(),
        "a refused invocation runs nothing"
    );

    world
        .ephor()
        .args(["actions", "run", "note", "--item", ITEM, "--set", "a=b"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lays no workflow down"));
}

/// A git command that has to work for the world to be the world.
fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn commit(dir: &std::path::Path, file: &str, contents: &str, message: &str) {
    std::fs::write(dir.join(file), contents).expect("write the file");
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// A replay that stops in a conflict hands it over, and what it opened is a
/// field of the reading rather than three lines of prose after it. A script
/// that read only the JSON used to learn nothing about the ticket that now
/// exists — and the prose that followed it made the machine form unparseable
/// besides (§REQ-002-parity.3, §FS-011-command-line.7).
#[test]
fn a_conflict_handed_over_is_part_of_the_reading() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.register(json!({
        "branches": [{ "id": "retry", "branch": "you/retry", "active": true }]
    }));
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));

    // A checkout whose branch and whose base both moved the same line.
    let origin = world.path().join("origin");
    std::fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
    commit(&origin, "f.txt", "base\n", "base");
    let checkout = world.forest();
    let cloned = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&checkout)
        .status()
        .expect("git clones");
    assert!(cloned.success());
    git(&checkout, &["config", "user.email", "t@example.com"]);
    git(&checkout, &["config", "user.name", "t"]);
    git(&checkout, &["checkout", "-q", "-b", "you/retry"]);
    commit(&checkout, "f.txt", "mine\n", "mine");
    commit(&origin, "f.txt", "theirs\n", "theirs");

    world.ephor().args(["refresh", PROJECT]).assert().success();

    let out = world
        .ephor()
        .args([
            "rebase",
            "--checkout",
            &checkout.to_string_lossy(),
            "--project",
            PROJECT,
            "--item",
            ITEM,
            "--dispatch",
            "--json",
        ])
        .output()
        .expect("the replay");
    // A conflict is not a failure: it is where the work starts
    // (§FS-005-dispatch.12), and the exit code says which of the three it was.
    assert_eq!(out.status.code(), Some(3), "a conflict exits 3");
    let replay = shaped("rebase", &out);
    assert_eq!(replay["conflicted"], 1, "{replay}");
    let handed = &replay["dispatched"];
    assert_eq!(handed["item"], ITEM, "{replay}");
    assert!(
        handed["ticket"].as_str().is_some_and(|id| !id.is_empty()),
        "the ticket it opened is on the reading: {replay}"
    );
    assert!(
        handed["says"].as_str().is_some_and(|says| !says.is_empty()),
        "{replay}"
    );

    // Without `--dispatch` the field is there and empty, so a program reads
    // one shape either way.
    let alone = shaped(
        "rebase",
        &world
            .ephor()
            .args([
                "rebase",
                "--checkout",
                &checkout.to_string_lossy(),
                "--project",
                PROJECT,
                "--json",
            ])
            .output()
            .expect("the replay"),
    );
    assert!(alone["dispatched"].is_null(), "{alone}");
}

/// Where an entry says it runs is where it runs — on the terminal, beneath it,
/// and in the `--dry-run` that describes it (§AR-002-summons.1).
///
/// The reading publishes the directory; the summons used to be built without
/// it and defaulted to the workspace, so an entry written `cwd: repo:app` ran
/// one directory too high while every reading said otherwise. The background
/// run, which goes through the supervisor, ran in the right place all along —
/// so one entry had three answers to where it runs, and two of them were
/// printed. This asserts the one thing that makes any of them worth printing:
/// that the published directory is the directory the command was actually in.
#[test]
fn an_entry_runs_in_the_place_its_reading_publishes() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    // A repository of the forest to be placed in, one level under the
    // checkout (§AR-004-forest.1).
    world.file("app/.keep", "");
    world.configure(json!({
        "actions": [{
            "id": "where",
            "icon": "▸",
            "description": "say where it ran",
            "command": "pwd > where.txt",
            "cwd": "repo:app",
            "when": { "kinds": ["pr"] }
        }],
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    let inside = world.forest().join("app");

    // Described.
    let planned = shaped(
        "outcome",
        &world
            .ephor()
            .args([
                "actions",
                "run",
                "where",
                "--item",
                ITEM,
                "--dry-run",
                "--json",
            ])
            .output()
            .expect("the dry run"),
    );
    assert_eq!(planned["steps"][0]["cwd"], json!(inside), "{planned}");

    // Run here.
    let ran = shaped(
        "outcome",
        &world
            .ephor()
            .args(["actions", "run", "where", "--item", ITEM, "--json"])
            .output()
            .expect("the run"),
    );
    assert_eq!(ran["steps"][0]["cwd"], json!(inside), "{ran}");
    assert_eq!(
        world.read(&format!("{PROJECT}/app/where.txt")).trim(),
        inside.to_string_lossy(),
        "the entry ran where the reading said it would"
    );

    // And beneath the screen, which is the one that was right before: all
    // three now answer alike.
    std::fs::remove_file(inside.join("where.txt")).expect("start again");
    let started = shaped(
        "outcome",
        &world
            .ephor()
            .args([
                "actions",
                "run",
                "where",
                "--item",
                ITEM,
                "--background",
                "--json",
            ])
            .output()
            .expect("the job starts"),
    );
    let id = started["job"].as_str().expect("a job id").to_string();
    world
        .ephor()
        .args(["job", "log", &id, "--follow", "--json"])
        .assert()
        .success();
    assert_eq!(
        world.read(&format!("{PROJECT}/app/where.txt")).trim(),
        inside.to_string_lossy(),
        "and the job ran there too"
    );
}

/// `--report` lands whether or not the dispatch beside it could be made
/// (§FS-005-dispatch.12, §REQ-001-boundary.1).
///
/// A state machine reads the report to learn what the replay stopped at, and
/// the moment it needs it most is the moment the hand-over failed — no recipe
/// configured, a ledger it cannot write. Writing the report only after a
/// successful dispatch made those exactly the runs with nothing to read.
#[test]
fn the_report_lands_even_when_the_dispatch_beside_it_does_not() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.register(json!({
        "branches": [{ "id": "retry", "branch": "you/retry", "active": true }]
    }));
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));

    let origin = world.path().join("origin");
    std::fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
    commit(&origin, "f.txt", "base\n", "base");
    let checkout = world.forest();
    let cloned = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&checkout)
        .status()
        .expect("git clones");
    assert!(cloned.success());
    git(&checkout, &["config", "user.email", "t@example.com"]);
    git(&checkout, &["config", "user.name", "t"]);
    git(&checkout, &["checkout", "-q", "-b", "you/retry"]);
    commit(&checkout, "f.txt", "mine\n", "mine");
    commit(&origin, "f.txt", "theirs\n", "theirs");
    world.ephor().args(["refresh", PROJECT]).assert().success();

    // The work root cannot be made: a file is sitting where the directory
    // would go. This is the ordinary shape of a dispatch that cannot be
    // written, and it is exactly when the replay's own account matters.
    std::fs::write(checkout.join("panta"), "not a directory\n").expect("in the way");

    let report = world.path().join("replay.md");
    let out = world
        .ephor()
        .args([
            "rebase",
            "--checkout",
            &checkout.to_string_lossy(),
            "--project",
            PROJECT,
            "--item",
            ITEM,
            "--dispatch",
            "--report",
            &report.to_string_lossy(),
        ])
        .output()
        .expect("the replay");
    assert!(
        !out.status.success(),
        "the dispatch could not be made, and that is this command's exit"
    );
    let written = std::fs::read_to_string(&report).unwrap_or_else(|err| {
        panic!(
            "no report at {}: {err} — the one thing a state machine reads",
            report.display()
        )
    });
    assert!(
        written.contains("f.txt"),
        "and it is the replay's own account: {written}"
    );
}

/// A matter whose project cannot be placed says so, on both surfaces, rather
/// than answering with an empty list (§REQ-001-boundary.1, §REQ-002-parity.2).
///
/// Assembling the menu needs the project placed, and a registry root that is
/// not on disk fails that for reasons that have nothing to do with whether
/// work can be handed over — `ephor work dispatch` goes through regardless.
/// Swallowing the refusal made both the work screen and this reading say
/// "nothing matches this matter" about a matter plenty matches.
#[test]
fn work_that_cannot_be_offered_says_why_rather_than_answering_empty() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    // The root moves out from under the row, which is what an unmounted disk
    // or a checkout somebody deleted looks like.
    world.register(json!({ "root": world.path().join("gone").to_string_lossy() }));

    let reading = shaped(
        "work",
        &world
            .ephor()
            .args(["work", "offers", "--item", ITEM, "--json"])
            .output()
            .expect("the work reading"),
    );
    assert!(
        reading["offers"].as_array().is_some_and(Vec::is_empty),
        "{reading}"
    );
    assert!(
        reading["unavailable"]
            .as_str()
            .is_some_and(|why| !why.is_empty()),
        "an empty list with no reason reads as an oversight: {reading}"
    );

    // And the prose form says the same thing, rather than the sentence it says
    // when nothing merely matched.
    world
        .ephor()
        .args(["work", "offers", "--item", ITEM])
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing could be offered"))
        .stdout(predicate::str::contains("nothing matches this matter").not());
}

/// `--dry-run` on a reply is the move with the post left out, never a second
/// description of it (§REQ-002-parity.3).
///
/// `src/cli.rs` calls this flag "the reading a program checks before letting
/// the move happen", so reporting `ok` where the real move refuses is the one
/// thing it may not do. It did: the dry run assembled the words and declared
/// success without ever asking whether the channel could carry a reply.
#[test]
fn a_dry_run_of_a_reply_refuses_exactly_where_the_reply_would() {
    let world = World::new();
    world.stub("ephor-forge-acme", UNANSWERABLE);
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();

    let dry = world
        .ephor()
        .args(["reply", ITEM, "it", "resets", "--dry-run", "--json"])
        .output()
        .expect("the dry run");
    let real = world
        .ephor()
        .args(["reply", ITEM, "it", "resets", "--json"])
        .output()
        .expect("the move");

    let planned = shaped("outcome", &dry);
    let happened = shaped("outcome", &real);
    assert_eq!(
        planned["ok"], happened["ok"],
        "the dry run and the move disagree about whether it would go out:\n{planned}\n{happened}"
    );
    assert_eq!(planned["ok"], false, "{planned}");
    assert_eq!(
        planned["says"], happened["says"],
        "and they refuse in one sentence"
    );
    assert_eq!(
        dry.status.code(),
        real.status.code(),
        "and exit the same way"
    );
}

/// A forge whose conversation declares no way to send a reply — the ordinary
/// read-only channel, not a broken one (§FS-007-matters.4).
const UNANSWERABLE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities) printf '{"pull_requests":true,"conversation":true}' ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/7", "repo": "app", "number": "7",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/7",
        "updated_at": "2026-08-01T12:00:00Z",
        "role": "author", "state": "open", "cited": false,
        "threads": [ { "messages": [
          { "author": "Ada", "text": "does the window reset per attempt?",
            "when": "2026-08-01T11:00:00Z", "mine": false } ] } ] }
    ]'
    ;;
  *) printf '[]' ;;
esac
"#;

/// `ephor checkout --json` prints the shape it publishes (§REQ-002-parity.4).
///
/// Kept apart from the sweep because it is the one reading that needs a git
/// forest with something to grow a working tree from — but validated all the
/// same, because a shape nothing validates is a shape whose entry can say
/// anything.
#[test]
fn a_checkout_answers_in_the_shape_it_publishes() {
    let world = World::new();
    world.stub("ephor-forge-acme", FORGE);
    world.register(json!({
        "branch_root_template": "{project_root}-{branch}",
        "branches": [{ "id": "retry", "branch": "you/retry", "active": true }]
    }));
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } }
    }));

    let origin = world.path().join("origin");
    std::fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
    commit(&origin, "f.txt", "base\n", "base");
    let checkout = world.forest();
    let cloned = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&checkout)
        .status()
        .expect("git clones");
    assert!(cloned.success());
    git(&checkout, &["config", "user.email", "t@example.com"]);
    git(&checkout, &["config", "user.name", "t"]);

    let made = shaped(
        "checkout",
        &world
            .ephor()
            .args([
                "checkout",
                "--project",
                PROJECT,
                "--branch",
                "you/retry",
                "--json",
            ])
            .output()
            .expect("the checkout"),
    );
    assert_eq!(made["ready"], true, "{made}");
    // A branch the repository does not have is grown, and the row says what
    // from — the field that used to be printed as `null` and declared a
    // string, which is a reading that does not hold to its own schema.
    assert_eq!(made["repos"][0]["created"], "branched", "{made}");
    assert!(made["repos"][0]["from"].as_str().is_some(), "{made}");

    // The rows a working tree that was merely found, or refused, would carry
    // are exercised where every variant can be built at once — beside the view
    // itself, in `src/git.rs`.
}
