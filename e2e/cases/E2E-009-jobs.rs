//! E2E-009-jobs: a move that needs nobody runs beneath the screen, and is
//! answerable afterwards without it.
//!
//! The scenario is the whole of §FS-005-dispatch.17 from the outside. The
//! reader presses a key for a replay; the interface does not go anywhere; the
//! move runs as its own process and is watched from a row. What is proven here
//! is the contract that row stands on — artifacts on disk, and nothing else.
//!
//! Liveness is the lock and never the record: a job that says it started is a
//! different claim from a job that is running, and a supervisor that died
//! leaves the first without the second. Everything the reader would have
//! watched is kept, whole and in order, so "what is it doing right now" has an
//! answer while it runs and an hour after it ended. The chain travels with the
//! job: an entry that needed the branch workspace carries its checkout as the
//! first step, and that step's contract — the directory — is verified rather
//! than trusted (§FS-006-project-interface.8), which is what stops a replay
//! from running somewhere surprising when the checkout quietly did nothing.
//!
//! And a job outlives the interface: it is read here entirely from the command
//! line, by a process that never saw the one that started it.

#[path = "../support.rs"]
mod support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use support::*;

/// Where jobs are written: one directory, listed rather than remembered
/// (§FS-005-dispatch.17), inside the world's own state.
fn jobs_dir(world: &World) -> PathBuf {
    world.path().join("state/ephor/jobs")
}

/// A job written down but not yet started — which is exactly what the
/// interface leaves behind before it hands the work to a supervisor.
fn write_job(world: &World, id: &str, steps: Value) -> PathBuf {
    let dir = jobs_dir(world).join(id);
    fs::create_dir_all(&dir).expect("the job directory");
    let record = serde_json::json!({
        "version": 1,
        "project": PROJECT,
        "item": format!("forge:{PROJECT}/42"),
        "icon": "⤴",
        "description": "rebase onto master (3 behind as of Jul 28)",
        "root": world.forest(),
        "workspace": world.forest(),
        "steps": steps,
        "dossier": [["EPHOR_ITEM_ID", format!("forge:{PROJECT}/42")]],
        "started": "2026-08-18T09:00:00Z",
    });
    fs::write(
        dir.join("job.json"),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .expect("the job record");
    dir
}

fn one_step(command: &str) -> Value {
    serde_json::json!([{
        "icon": "⤴",
        "description": "replay",
        "command": command,
    }])
}

/// Run a job's supervisor the way the interface does: its streams *are* the
/// log, which is the whole of what detached means here (§AR-002-summons.5) —
/// so a case that pointed them anywhere else would be testing a different
/// program.
fn supervise(world: &World, dir: &Path) -> bool {
    let log = fs::File::create(dir.join("log")).expect("the log");
    let errors = log.try_clone().expect("the log");
    world
        .ephor_raw()
        .args(["job", "run", &dir.to_string_lossy()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(errors))
        .status()
        .expect("the supervisor")
        .success()
}

/// `ephor job list --json`, which is the same reading the board does.
fn listed(world: &World) -> Vec<Value> {
    let output = world
        .ephor()
        .args(["job", "list", "--json"])
        .output()
        .expect("ephor job list");
    serde_json::from_slice(&output.stdout).expect("a listing")
}

fn one(world: &World) -> Value {
    let mut jobs = listed(world);
    assert_eq!(jobs.len(), 1, "one job was written: {jobs:?}");
    jobs.remove(0)
}

/// Hold a job's lock the way its supervisor does, from this process: flock
/// conflicts across descriptors, so a reader's try-lock sees it held.
fn hold_lock(dir: &Path) -> fs::File {
    fs::write(dir.join("lock"), "").expect("the lock file");
    let file = fs::File::open(dir.join("lock")).expect("the lock file");
    file.lock().expect("the lock");
    file
}

/// Everything the reader would have watched is kept, and the outcome is a
/// sentence rather than a code to look up. The step ran where the job said and
/// with the matter's own `EPHOR_*` vocabulary (§FS-005-dispatch.8), because a
/// job is a summons like every other (§AR-002-summons.5).
#[test]
fn a_job_keeps_what_the_reader_would_have_watched() {
    let world = World::new();
    let dir = write_job(
        &world,
        "20260818T090000.000Z-replay",
        one_step("echo replaying $EPHOR_ITEM_ID; echo in $(pwd)"),
    );

    assert!(supervise(&world, &dir), "the supervisor ran");

    let log = fs::read_to_string(dir.join("log")).expect("the log");
    assert!(
        log.contains(&format!("replaying forge:{PROJECT}/42")),
        "the step's own output is the log: {log}"
    );
    assert!(
        log.contains(&format!("in {}", world.forest().display())),
        "it ran where the job said: {log}"
    );

    let job = one(&world);
    assert_eq!(job["outcome"], "done");
    assert_eq!(job["live"], false, "nothing holds the lock now");
    assert_eq!(job["died"], false);
    assert!(
        job["says"].as_str().unwrap().ends_with(": ok"),
        "the outcome is a sentence: {job:?}"
    );
}

/// Liveness is the lock, never the record. A job whose lock is held is running
/// whoever is holding it — that is what lets a job started by one ephor be a
/// row in another — and while it runs the row says what the log last said,
/// because "still going" and "stuck" are otherwise the same word.
#[test]
fn a_held_lock_is_what_running_means() {
    let world = World::new();
    let dir = write_job(&world, "20260818T090100.000Z-replay", one_step("true"));
    fs::write(dir.join("log"), "fetching origin\nreplaying repo 2 of 3\n").expect("a log");

    let held = hold_lock(&dir);
    let job = one(&world);
    assert_eq!(job["live"], true, "the lock is held: {job:?}");
    assert_eq!(
        job["says"], "replaying repo 2 of 3",
        "a live job says what it is doing now: {job:?}"
    );

    drop(held);
    let job = one(&world);
    assert_eq!(job["live"], false, "the lock went with the holder");
}

/// A job with neither a lock nor an outcome is one whose supervisor died. It
/// is reported as that and never as running: the record said it started, which
/// is a claim about the past, and nothing on disk supports the other one.
#[test]
fn a_job_that_holds_no_lock_and_wrote_no_outcome_died() {
    let world = World::new();
    write_job(&world, "20260818T090200.000Z-replay", one_step("true"));

    let job = one(&world);
    assert_eq!(job["live"], false);
    assert_eq!(job["died"], true, "{job:?}");
    assert!(
        job["says"].as_str().unwrap().contains("died"),
        "it says so rather than claiming to run: {job:?}"
    );
}

/// The chain travels with the job, and the checkout's contract is verified
/// rather than trusted (§FS-006-project-interface.8): a step that was to make
/// the workspace and did not ends the job there, naming the step — the replay
/// after it never runs, which is the whole point of checking.
#[test]
fn a_checkout_that_made_nothing_ends_the_job_where_it_stopped() {
    let world = World::new();
    let workspace = world.path().join("branches/GR-42");
    let ran = world.path().join("replay-ran");
    let dir = write_job(
        &world,
        "20260818T090300.000Z-replay",
        serde_json::json!([
            {
                "icon": "⇣",
                "description": "check out branch workspace",
                "command": "true",
                "cwd": "root",
                "creates": workspace,
                "becomes_workspace": true,
            },
            {
                "icon": "⤴",
                "description": "replay",
                "command": format!("touch {}", ran.display()),
            },
        ]),
    );

    assert!(supervise(&world, &dir), "the supervisor ran");

    let job = one(&world);
    assert_eq!(job["outcome"], "failed");
    let says = job["says"].as_str().unwrap();
    assert!(
        says.contains("did not create") && says.contains("check out branch workspace"),
        "the step that stopped it is named: {says}"
    );
    assert!(
        !ran.exists(),
        "the step after a failed checkout never ran, which is what verifying is for"
    );
}

/// A step's own exit code is carried, and the *come back later* code is not
/// reported as a failure (§FS-006-project-interface.3): the three outcomes a
/// summons has are the three a job has.
#[test]
fn a_step_that_parks_is_parked_and_not_failed() {
    let world = World::new();
    let dir = write_job(&world, "20260818T090400.000Z-replay", one_step("exit 75"));

    assert!(supervise(&world, &dir), "the supervisor ran");

    let job = one(&world);
    assert_eq!(job["outcome"], "parked", "{job:?}");
    assert_eq!(job["died"], false);
}

/// Two supervisors on one job would interleave one log and race for one
/// outcome, so the second is refused by the lock rather than allowed to make a
/// mess nothing could read afterwards.
#[test]
fn one_job_has_one_supervisor() {
    let world = World::new();
    let dir = write_job(&world, "20260818T090500.000Z-replay", one_step("true"));
    let held = hold_lock(&dir);

    assert!(
        !supervise(&world, &dir),
        "a second supervisor on one job is refused"
    );
    assert!(
        !dir.join("outcome.json").exists(),
        "the refused supervisor wrote nothing"
    );
    drop(held);
}

/// The log is readable by name, by a process that never saw the one that
/// started the job — which is what "outlives the interface" has to mean to be
/// worth anything (§FS-005-dispatch.17).
#[test]
fn a_job_is_readable_from_the_command_line_alone() {
    let world = World::new();
    let dir = write_job(
        &world,
        "20260818T090600.000Z-replay",
        one_step("echo the whole story"),
    );
    assert!(supervise(&world, &dir), "the supervisor ran");

    world
        .ephor()
        .args(["job", "log", "20260818T090600.000Z-replay"])
        .assert()
        .success()
        .stdout(predicates::str::contains("the whole story"));
}
