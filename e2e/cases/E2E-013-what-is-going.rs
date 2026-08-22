//! E2E-013-what-is-going: a run starts beneath the screen, and what is already
//! going is shown where it could be started again.
//!
//! The scenario is §FS-005-dispatch.20, §FS-005-dispatch.21 and
//! §FS-005-dispatch.22 from the outside, with no terminal anywhere. A run of
//! the runtime outlives the screen that started it and is watched by attaching;
//! the menu that could start a thing says when that thing is already going, and
//! prints the way in; and a program that wants a terminal of its own gets a
//! window rather than ephor's.
//!
//! What it holds ephor to. **Nothing here is remembered from a keypress.** A
//! run is a held lock and the descriptor the binding writes beside it, so a run
//! somebody started in another terminal marks the menu exactly as one ephor
//! started itself; a job is a held lock and a record saying which entry it came
//! from, so a job that died is not running whatever started it. The way in is
//! printed, because the way in is the ability and spawning the reader's own
//! program on it is not (§REQ-002-parity.1): a run's id and the runner's own
//! attach command, a job's log, a window's handle.
//!
//! And the window is a seam like every other (§REQ-001-boundary.1): the two
//! commands here are shell scripts that write down what they were called with,
//! which is the whole contract — one opens a window running a command and
//! prints a handle, one takes a handle back.

#[path = "../support.rs"]
mod support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use support::*;

/// The matter every case here is about.
const ITEM: &str = "acmeforge:app/101";

/// A forge with one pull request of the user's, with a red gate: something to
/// hand over, and so something that can be going.
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
        "gate": { "repos": [ { "repo": "app", "passed": 5, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime that can be asked whether it detaches, and that starts a detached
/// run by publishing the descriptor a real one publishes beside its lock
/// (§FS-005-dispatch.20). It is not a runtime: it is the two halves of the
/// coupling ephor actually depends on — the flag in the help, and the id in the
/// launcher's JSON.
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  *--help*)
    printf 'Options:\n      --headless  Detach the run into its own session\n'
    exit 0
    ;;
esac
verb="$1"; shift
case "$verb" in
  run)
    headless=""
    root=""
    for word in "$@"; do
      case "$word" in
        --headless) headless=yes ;;
        --json|--rhei) ;;
        -*) ;;
        *) [ -n "$root" ] || root="$word" ;;
      esac
    done
    if [ -n "$headless" ]; then
      mkdir -p "$root/runtime"
      printf '{"id":"3f9a2c","pid":48213,"status":"running","workspace":"%s","control_url":"http://127.0.0.1:54321","started_at":"2026-08-22T14:03:22Z","headless":true,"exit_code":null}\n' "$root" > "$root/runtime/run.json"
      cat "$root/runtime/run.json"
    fi
    ;;
  attach|stop|list)
    printf '[]'
    ;;
esac
"#;

/// A world watching the forge with the runtime bound and on PATH.
fn watching() -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    world.stub("acme-runtime", ACME_RUNTIME);
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

fn work_root(world: &World) -> PathBuf {
    world.forest().join("panta")
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
    fs::write(path, content).expect("write");
}

/// Hold a lock the way a run and a job hold theirs: flock conflicts across
/// processes, so the probe ephor makes in a child sees it held
/// (§FS-005-dispatch.15, §FS-005-dispatch.17).
fn hold(path: &Path) -> fs::File {
    write(path, "");
    let file = fs::File::open(path).expect("the lock file");
    file.lock().expect("the lock");
    file
}

/// What a live run leaves beside its lock: the descriptor that names it, and
/// the journal and log that say which ticket it is holding.
fn a_run_is_live_on(root: &Path, ticket: &str, state: &str) -> fs::File {
    let holder = hold(&root.join(".rhei/run.lock"));
    write(
        &root.join("runtime/run.json"),
        concat!(
            "{\"id\":\"3f9a2c\",\"pid\":48213,\"status\":\"running\",",
            "\"control_url\":\"http://127.0.0.1:54321\",",
            "\"started_at\":\"2026-08-22T14:03:22Z\",\"headless\":true,\"exit_code\":null}"
        ),
    );
    write(
        &root.join("runtime/transitions.log"),
        &format!("2026-08-22T14:05:00Z  {ticket}  start@{state}  runtime/logs/task-{ticket}.log\n"),
    );
    write(
        &root.join(format!("runtime/logs/task-{ticket}.log")),
        "at it\n",
    );
    holder
}

/// `ephor actions --json` for the matter, as a program reads it.
fn actions(world: &World) -> Value {
    let out = world
        .ephor()
        .args(["actions", "--item", ITEM, "--json"])
        .output()
        .expect("ephor actions");
    shaped("actions", &out)
}

fn offer<'a>(view: &'a Value, id: &str) -> &'a Value {
    view["offers"]
        .as_array()
        .expect("a menu")
        .iter()
        .find(|offer| offer["id"] == id)
        .unwrap_or_else(|| panic!("no entry called '{id}' in {view}"))
}

/// Where jobs are written in this world.
fn jobs_dir(world: &World) -> PathBuf {
    world.path().join("state/ephor/jobs")
}

/// A job written down as the interface writes one, with the entry it came from
/// on the record — which is what matches it back to the row
/// (§FS-005-dispatch.21).
fn write_job(world: &World, id: &str, record: Value) -> PathBuf {
    let dir = jobs_dir(world).join(id);
    fs::create_dir_all(&dir).expect("the job directory");
    let mut whole = json!({
        "version": 2,
        "project": PROJECT,
        "item": ITEM,
        "icon": "⤴",
        "description": "leave a note about it",
        "root": world.forest(),
        "workspace": world.forest(),
        "steps": [],
        "dossier": [],
        "started": "2026-08-22T09:00:00Z",
    });
    for (key, value) in record.as_object().expect("an object") {
        whole[key] = value.clone();
    }
    write_json(&dir.join("job.json"), &whole);
    fs::write(dir.join("log"), "replaying\n").expect("the log");
    dir
}

/// A run of the runtime starts beneath the screen and is watched by attaching
/// (§FS-005-dispatch.20).
///
/// `ephor work run` asks the binding whether it has a detached shape, starts
/// the run through it, and prints the id the launcher gave it — the terminal
/// stays the reader's. The root then turns live on the board from the lock, as
/// every run does, and the row carries the id, the control address, and the two
/// commands that reach it: the way in, and the way out shown but never run.
#[test]
fn a_run_starts_beneath_the_screen_and_the_board_says_how_to_reach_it() {
    let world = watching();
    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM])
        .assert()
        .success();

    let out = world
        .ephor()
        .args(["work", "run", "--item", ITEM, "--json"])
        .output()
        .expect("ephor work run");
    let reading = shaped("work-run", &out);
    let run = &reading["runs"][0];
    assert_eq!(run["outcome"], "started", "{reading}");
    assert_eq!(run["id"], "3f9a2c", "{reading}");

    // The descriptor the launcher's child published is beside the lock, and it
    // is where every surface reads the run's identity from — never from
    // anything ephor remembers having started.
    let root = work_root(&world);
    assert!(root.join("runtime/run.json").is_file());

    // Live from the lock, with the id, the control address, and both commands
    // on the row.
    let _holder = hold(&root.join(".rhei/run.lock"));
    let out = world
        .ephor()
        .args(["operations", "--json"])
        .output()
        .expect("ephor operations");
    let board = shaped("operations", &out);
    let row = board["operations"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["kind"] == "root")
        .unwrap_or_else(|| panic!("no execution root on the board: {board}"));
    assert_eq!(row["run"], "3f9a2c", "{board}");
    assert_eq!(row["control_url"], "http://127.0.0.1:54321", "{board}");
    assert_eq!(row["attach"], "acme-runtime attach '3f9a2c'", "{board}");
    assert_eq!(row["stop"], "acme-runtime stop '3f9a2c'", "{board}");
}

/// What is already going is shown where it could be started again
/// (§FS-005-dispatch.21), and the list a program reads carries the same mark
/// with the same facts (§FS-011-command-line.8).
///
/// The entry that hands this work over is running because the ticket it opened
/// is open on a root a run holds — the very facts the badge on the row is made
/// of — and the way in is the run's id and the runner's own attach command.
#[test]
fn a_live_run_marks_the_entry_that_hands_that_work_over() {
    let world = watching();
    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM])
        .assert()
        .success();
    let root = work_root(&world);

    // Nothing is holding the root yet: the ticket is open, and open work is
    // not an operation (§FS-005-dispatch.15).
    assert!(offer(&actions(&world), "fix-gate")["running"].is_null());

    let _holder = a_run_is_live_on(&root, "fix-gate-1", "fix");
    let view = actions(&world);
    let running = &offer(&view, "fix-gate")["running"];
    assert_eq!(running["kind"], "run", "{running}");
    assert_eq!(running["run"], "3f9a2c", "{running}");
    assert_eq!(
        running["attach"], "acme-runtime attach '3f9a2c'",
        "{running}"
    );
    assert_eq!(
        running["control_url"], "http://127.0.0.1:54321",
        "{running}"
    );
    assert!(
        running["says"]
            .as_str()
            .expect("what it is at")
            .contains("fix-gate-1"),
        "{running}"
    );
}

/// A job is matched back to the row that started it by the entry it came from
/// and the subject it was about (§FS-005-dispatch.21) — which is why a job
/// records both. A job about another subject is another row's, and a job whose
/// lock nobody holds is not running whatever its record says.
#[test]
fn a_live_job_marks_the_entry_it_came_from_and_no_other() {
    let world = watching();
    world.configure(json!({
        "actions": [
            { "id": "note", "icon": "✎", "description": "leave a note about it",
              "command": "true" },
            { "id": "other", "icon": "✎", "description": "something else",
              "command": "true" }
        ],
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": { "runner": "acme-runtime" }
    }));

    // A job about this matter, from the `note` entry, with its lock held.
    let mine = write_job(
        &world,
        "20260822T090000.000Z-note",
        json!({ "action": "note" }),
    );
    let _holder = hold(&mine.join("lock"));
    // And one from the same entry on a branch row, which is a different
    // subject: a replay is offered per branch (§FS-004-quick-actions.6).
    let elsewhere = write_job(
        &world,
        "20260822T090100.000Z-note",
        json!({ "action": "note", "item": Value::Null, "branch": "you/other" }),
    );
    let _other = hold(&elsewhere.join("lock"));
    // And a job from the `other` entry that nobody is running: a record that a
    // job started is a different claim from a job that is running.
    write_job(
        &world,
        "20260822T090200.000Z-other",
        json!({ "action": "other" }),
    );

    let view = actions(&world);
    let running = &offer(&view, "note")["running"];
    assert_eq!(running["kind"], "job", "{running}");
    assert_eq!(running["job"], "20260822T090000.000Z-note", "{running}");
    assert!(
        running["log"]
            .as_str()
            .expect("the way in is the log")
            .ends_with("20260822T090000.000Z-note/log"),
        "{running}"
    );
    assert_eq!(running["says"], "replaying", "{running}");
    assert!(offer(&view, "other")["running"].is_null(), "{view}");
}

/// A machine with a state the runtime will not leave on its own: where work
/// waits for a person (§FS-005-dispatch.9). Written over the one a dispatch
/// installs, because the shipped machine declares no such state and the row's
/// *waiting on you* is exactly what a gating state means.
const PARKING_MACHINE: &str = r#"name: ephor-work
version: 1.0

states:
  fix:
    description: Do what the ticket asks.
  needs-human:
    description: A person has to answer before this goes on.
    gating: true
  done:
    description: Over.
    final: true

transitions:
  - from: fix
    to: needs-human
  - from: needs-human
    to: done
"#;

/// The plan a dispatch wrote in this root, found by what a plan *is* rather
/// than by a suffix — the suffix is the runtime's own word, not ephor's
/// (§REQ-001-boundary.5).
fn the_plan(root: &Path) -> PathBuf {
    fs::read_dir(root)
        .expect("the work root")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            fs::read_to_string(path)
                .map(|text| text.contains("**State:**"))
                .unwrap_or(false)
        })
        .expect("a dispatch wrote a plan")
}

/// Move the dispatched ticket into the state the machine parks work in, the
/// way the runtime would have: the plan is the artifact, and every surface
/// reads it back from there.
fn park(root: &Path) -> PathBuf {
    write(&root.join("states.yaml"), PARKING_MACHINE);
    let plan = the_plan(root);
    let text = fs::read_to_string(&plan).expect("the plan");
    let parked = text.replace("**State:** fix", "**State:** needs-human");
    assert_ne!(parked, text, "the plan holds a ticket in `fix`");
    fs::write(&plan, parked).expect("the plan is rewritten");
    plan
}

/// A ticket the run parked is *waiting on you*, and the row that opened it says
/// so rather than offering to lay a second one beside it
/// (§FS-005-dispatch.20, §FS-005-dispatch.21).
///
/// It counts whether or not a run still holds the root — a run with nobody at
/// its terminal waits at the gate rather than exiting, and one that exited
/// leaves the question standing all the same. What changes is the way in: the
/// run where one is still standing there, and the plan the question is written
/// in where none is, because that is where the answer belongs
/// (§FS-005-dispatch.9).
#[test]
fn a_parked_ticket_is_waiting_on_you_with_a_run_and_without_one() {
    let world = watching();
    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM])
        .assert()
        .success();
    let root = work_root(&world);
    let plan = park(&root);

    // Nothing is holding the root. The question stands, and the way in is the
    // plan — never *queued*, which would promise a turn that never comes.
    let view = actions(&world);
    let running = &offer(&view, "fix-gate")["running"];
    assert_eq!(running["kind"], "waiting", "{running}");
    assert!(
        running["says"]
            .as_str()
            .expect("what it is at")
            .contains("waiting on you"),
        "{running}"
    );
    assert_eq!(
        running["plan"],
        plan.to_string_lossy().as_ref(),
        "{running}"
    );
    assert!(running["run"].is_null(), "{running}");

    // And with a run still standing at the gate, the way in is that run: §20's
    // own answer to a question a detached run asks is *attach*.
    let _holder = a_run_is_live_on(&root, "fix-gate-1", "needs-human");
    let view = actions(&world);
    let running = &offer(&view, "fix-gate")["running"];
    assert_eq!(running["kind"], "waiting", "{running}");
    assert_eq!(running["run"], "3f9a2c", "{running}");
    assert_eq!(
        running["attach"], "acme-runtime attach '3f9a2c'",
        "{running}"
    );
}

/// A run live on the root that is not holding this entry's ticket will reach
/// it: the runtime schedules one run per execution root, so the row says
/// *queued* and the way in is that run (§FS-005-dispatch.15,
/// §FS-005-dispatch.21).
///
/// And *queued* is all it says. The run's age is the run's, not this entry's
/// wait: a row reading `queued · 3h` about a ticket dispatched a minute ago
/// would have a reader believing it had been waiting three hours.
#[test]
fn an_open_ticket_a_live_run_has_not_taken_up_is_queued() {
    let world = watching();
    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM])
        .assert()
        .success();
    let root = work_root(&world);
    // Live, and holding some *other* ticket: the journal names one this entry
    // never opened.
    let _holder = a_run_is_live_on(&root, "something-else-1", "fix");

    let view = actions(&world);
    let running = &offer(&view, "fix-gate")["running"];
    assert_eq!(running["kind"], "queued", "{running}");
    assert_eq!(running["says"], "queued", "{running}");
    assert_eq!(running["run"], "3f9a2c", "{running}");
    assert_eq!(
        running["attach"], "acme-runtime attach '3f9a2c'",
        "{running}"
    );
    assert!(running["since_seconds"].is_null(), "{running}");
}

/// The checkout row is running where the job that is making the workspace is
/// (§FS-005-dispatch.21).
///
/// That job is never the checkout row's own — the checkout always runs in the
/// foreground — it is another entry's job carrying the checkout as its first
/// step. Pressing the row while that job is still writing the workspace used to
/// start a second checkout of the same directory.
///
/// And an entry with no id of its own is matched back by its description, which
/// is the only thing that distinguishes it on a screen either
/// (§FS-006-project-interface.9).
#[test]
fn the_checkout_row_is_running_while_the_job_making_the_workspace_is() {
    let world = watching();
    // A project whose branches live in workspaces of their own, and a branch
    // whose workspace is not on this machine yet: the menu then carries the row
    // that would make it (§FS-004-quick-actions.7).
    world.register(json!({
        "branch_root_template": "{project_root}/ws/{branch}",
        "branches": [
            { "id": "demo-retry", "branch": "you/ABC-42-retry", "active": true,
              "ticket": "ABC-42" }
        ]
    }));
    world.configure(json!({
        "actions": [
            { "icon": "⤴", "description": "rebase onto master", "command": "true" }
        ],
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": { "runner": "acme-runtime" }
    }));

    // A job from the anonymous entry, carrying the checkout as its first step —
    // exactly what an entry pressed on a branch whose workspace is missing
    // writes down.
    let dir = write_job(
        &world,
        "20260822T090000.000Z-rebase",
        json!({
            "action": "rebase onto master",
            "description": "rebase onto master",
            "workspace": Value::Null,
            "steps": [
                { "icon": "⇣", "description": "check out branch workspace",
                  "command": "true", "creates": world.forest().join("ws"),
                  "becomes_workspace": true },
                { "icon": "⤴", "description": "rebase onto master", "command": "true" }
            ]
        }),
    );
    let _holder = hold(&dir.join("lock"));

    let view = actions(&world);
    assert_eq!(view["workspace_state"], "missing", "{view}");
    // The anonymous entry answers to its description, and its own job marks it.
    let running = &offer(&view, "rebase onto master")["running"];
    assert_eq!(running["kind"], "job", "{running}");
    assert_eq!(running["job"], "20260822T090000.000Z-rebase", "{running}");
    // And the row that would make the workspace is running too, on the same
    // job: the checkout is that job's first step.
    let checkout = &offer(&view, "checkout")["running"];
    assert_eq!(checkout["kind"], "job", "{checkout}");
    assert_eq!(checkout["job"], "20260822T090000.000Z-rebase", "{checkout}");
}

/// A window of the reader's own, where one is bound (§FS-005-dispatch.22).
///
/// The seam is two commands in materials: one opens a window running a command
/// and prints a handle, one brings a handle forward. An entry that says
/// `window` starts as a job whose supervisor the opener runs, the handle goes
/// into the record, and *opening* that row from then on is bringing the window
/// forward — never a second copy of the program (§FS-005-dispatch.21).
#[test]
fn an_entry_that_asks_for_a_window_gets_one_and_opening_it_brings_it_forward() {
    let world = watching();
    let opened = world.path().join("opened");
    let focused = world.path().join("focused");
    let open = world.stub(
        "stand-in-open",
        &format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" > {}\nprintf '@7\\n'\n",
            opened.to_string_lossy()
        ),
    );
    let focus = world.stub(
        "stand-in-focus",
        &format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" > {}\n",
            focused.to_string_lossy()
        ),
    );
    world.configure(json!({
        "defaults": {
            "window": {
                "open": format!("{} {{title}} {{command}}", open.to_string_lossy()),
                "focus": format!("{} {{handle}}", focus.to_string_lossy()),
            }
        },
        "actions": [
            { "id": "edit", "icon": "✎", "description": "open it in the editor",
              "command": "true", "window": true }
        ],
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": { "runner": "acme-runtime" }
    }));

    // Running it hands the supervisor to the opener rather than taking a
    // terminal, and what the opener printed is what the record keeps.
    let out = world
        .ephor()
        .args(["actions", "run", "edit", "--item", ITEM, "--json"])
        .output()
        .expect("ephor actions run");
    let outcome = shaped("outcome", &out);
    assert_eq!(outcome["ok"], true, "{outcome}");
    assert!(
        outcome["says"]
            .as_str()
            .expect("a sentence")
            .contains("in window @7"),
        "{outcome}"
    );
    let called = fs::read_to_string(&opened).expect("the opener was called");
    assert!(called.contains("job run"), "{called}");

    // The record carries the handle, and the row that could start it again
    // says it is going in that window (§FS-005-dispatch.21).
    let id = outcome["job"].as_str().expect("a job id");
    let dir = jobs_dir(&world).join(id);
    let record = read_json(&dir.join("job.json"));
    assert_eq!(record["window"], "@7", "{record}");
    assert_eq!(record["action"], "edit", "{record}");

    let _holder = hold(&dir.join("lock"));
    let view = actions(&world);
    let running = &offer(&view, "edit")["running"];
    assert_eq!(running["kind"], "window", "{running}");
    assert_eq!(running["window"], "@7", "{running}");

    // And opening it brings that window forward, by the same binding: `focus`
    // is handed the handle `open` printed, and nothing else happens.
    let out = world
        .ephor()
        .args(["actions", "open", "edit", "--item", ITEM, "--json"])
        .output()
        .expect("ephor actions open");
    let outcome = shaped("outcome", &out);
    assert_eq!(outcome["ok"], true, "{outcome}");
    assert!(
        outcome["says"].as_str().expect("a sentence").contains("@7"),
        "{outcome}"
    );
    assert_eq!(
        fs::read_to_string(&focused)
            .expect("focus was called")
            .trim(),
        "@7"
    );
}

/// A window pair written down in configuration, with the files each half writes
/// what it was called with into (§REQ-001-boundary.1): the whole contract is
/// two commands in materials.
fn a_window_is_bound(world: &World) -> (PathBuf, PathBuf) {
    let opened = world.path().join("opened");
    let focused = world.path().join("focused");
    let open = world.stub(
        "stand-in-open",
        &format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" > {}\nprintf '@7\\n'\n",
            opened.to_string_lossy()
        ),
    );
    let focus = world.stub(
        "stand-in-focus",
        &format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" > {}\n",
            focused.to_string_lossy()
        ),
    );
    world.configure(json!({
        "defaults": {
            "window": {
                "open": format!("{} {{title}} {{command}}", open.to_string_lossy()),
                "focus": format!("{} {{handle}}", focus.to_string_lossy()),
            }
        },
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": { "runner": "acme-runtime" }
    }));
    (opened, focused)
}

/// Attaching goes to a window where one is bound — **by the same binding the
/// key uses** (§FS-005-dispatch.22, §FS-011-command-line.8).
///
/// Both commands that put a surface on a run go through the opener: the key on
/// a running row as a command (`ephor actions open`) and the board's own key as
/// a command (`ephor operations attach`). What the opener is handed is the
/// runner's own attach command, and what comes back is the handle it printed —
/// so a reader whose `a` opens a window does not get a terminal taken from them
/// by the same ability typed out.
#[test]
fn attaching_from_the_command_line_goes_to_the_bound_window() {
    let world = watching();
    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM])
        .assert()
        .success();
    let (opened, _focused) = a_window_is_bound(&world);
    let root = work_root(&world);
    let _holder = a_run_is_live_on(&root, "fix-gate-1", "fix");

    // The key on the running row, as a command.
    let out = world
        .ephor()
        .args(["actions", "open", "fix-gate", "--item", ITEM, "--json"])
        .output()
        .expect("ephor actions open");
    let outcome = shaped("outcome", &out);
    assert_eq!(outcome["ok"], true, "{outcome}");
    let says = outcome["says"].as_str().expect("a sentence");
    assert!(says.contains("run 3f9a2c"), "{says}");
    assert!(says.contains("window @7"), "{says}");
    let called = fs::read_to_string(&opened).expect("the opener was called");
    assert!(called.contains("acme-runtime attach"), "{called}");
    assert!(called.contains("3f9a2c"), "{called}");

    // And the board's own key, as a command: one ability, one binding.
    fs::remove_file(&opened).expect("the record of the last call");
    let out = world
        .ephor()
        .args(["operations", "attach", "3f9a2c", "--json"])
        .output()
        .expect("ephor operations attach");
    let outcome = shaped("outcome", &out);
    assert_eq!(outcome["ok"], true, "{outcome}");
    assert!(
        outcome["says"]
            .as_str()
            .expect("a sentence")
            .contains("window @7"),
        "{outcome}"
    );
    let called = fs::read_to_string(&opened).expect("the opener was called again");
    assert!(called.contains("acme-runtime attach"), "{called}");
}

/// A window that opened and named nothing is still a window
/// (§FS-005-dispatch.22, §AR-002-summons.6).
///
/// The opener's contract is *exit when the window exists*, so a binding that
/// ran the command and printed nothing has a window open with `ephor job run`
/// already inside it. Treating that as "the window never opened" deleted the
/// directory of a program that was at that moment running, and left it with no
/// record, no row and a lock nothing could probe — the program ephor handed a
/// terminal to and forgot. The record stands instead, with no handle on it, and
/// the row says where the program went.
///
/// And a windowed job leaves **no log**: what it wrote went to a screen the
/// reader was watching and is not duplicated into a file.
#[test]
fn a_window_the_opener_did_not_name_keeps_the_job_it_started() {
    let world = watching();
    // An opener that runs the command in the background and says nothing: the
    // window is open, and nothing named it.
    let open = world.stub(
        "stand-in-open",
        "#!/usr/bin/env bash\nsh -c \"$2\" >/dev/null 2>&1 &\n",
    );
    world.configure(json!({
        "defaults": {
            "window": {
                "open": format!("{} {{title}} {{command}}", open.to_string_lossy()),
                "focus": "true {handle}",
            }
        },
        "actions": [
            { "id": "edit", "icon": "✎", "description": "open it in the editor",
              "command": "true", "window": true }
        ],
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": { "runner": "acme-runtime" }
    }));

    let out = world
        .ephor()
        .args(["actions", "run", "edit", "--item", ITEM, "--json"])
        .output()
        .expect("ephor actions run");
    let outcome = shaped("outcome", &out);
    assert_eq!(outcome["ok"], true, "{outcome}");
    assert!(
        outcome["says"]
            .as_str()
            .expect("a sentence")
            .contains("did not name"),
        "{outcome}"
    );

    // The record stands, and says the program went to a window without saying
    // which one — the one thing the opener did not hand back.
    let id = outcome["job"].as_str().expect("a job id");
    let dir = jobs_dir(&world).join(id);
    let record = read_json(&dir.join("job.json"));
    assert_eq!(record["windowed"], true, "{record}");
    assert!(record["window"].is_null(), "{record}");
    assert_eq!(record["action"], "edit", "{record}");
    assert!(!dir.join("log").exists(), "a windowed job writes no log");
}

/// `ephor actions open` starts nothing, and refuses by name where the entry has
/// nothing going (§FS-011-command-line.8).
#[test]
fn opening_an_entry_with_nothing_going_is_refused_by_name() {
    let world = watching();
    world.configure(json!({
        "actions": [
            { "id": "note", "icon": "✎", "description": "leave a note about it",
              "command": "touch ran.txt" }
        ],
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": { "runner": "acme-runtime" }
    }));
    let out = world
        .ephor()
        .current_dir(world.forest())
        .args(["actions", "open", "note", "--item", ITEM, "--json"])
        .output()
        .expect("ephor actions open");
    let outcome = shaped("outcome", &out);
    assert_eq!(outcome["ok"], false, "{outcome}");
    assert!(
        outcome["says"]
            .as_str()
            .expect("a sentence")
            .contains("note"),
        "{outcome}"
    );
    // It never ran it: the command line starts an entry with `actions run`, and
    // the refusal there is the lock's own sentence.
    assert!(!world.forest().join("ran.txt").exists());
}
