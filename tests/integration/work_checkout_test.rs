//! One live run per checkout, end to end (§AR-007-runtime): what the sweep
//! does with two work roots over one working tree, and what the hand-started
//! key says when a run is already in that tree.
//!
//! A sibling of `work_capacity_test.rs` because the guard here is not a
//! ceiling: capacity is a budget the reader sets, and this is an invariant
//! about the tree the run edits.

mod common;

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use common::*;

/// A detached runner that holds every root it is given long enough for the
/// rest of the sweep to see the run, and logs what it was asked to start. Its
/// child redirects the inherited pipes so the launcher itself returns at once,
/// as a real detached launcher does.
fn holding_runner(tmp: &Path, log: &Path) {
    fs::create_dir_all(tmp.join("fakebin")).unwrap();
    make_executable(
        &tmp.join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*) printf 'Options:\\n      --headless  detach it\\n'; exit 0 ;;\n\
             *--headless*)\n\
               printf '%s\\n' \"$*\" >> {log}\n\
               root=\"$4\"\n\
               mkdir -p \"$root/.rhei\" \"$root/runtime\"\n\
               printf '{{\"id\":\"live-run\"}}\\n' > \"$root/runtime/run.json\"\n\
               ready=\"$root/.rhei/run-lock-ready\"\n\
               python -c 'import fcntl,pathlib,sys,time; lock=open(sys.argv[1],\"w\"); fcntl.flock(lock,fcntl.LOCK_EX); pathlib.Path(sys.argv[2]).touch(); time.sleep(20)' \"$root/.rhei/run.lock\" \"$ready\" >/dev/null 2>&1 &\n\
               for _ in {{1..200}}; do\n\
                 [[ -e \"$ready\" ]] && break\n\
                 sleep 0.01\n\
               done\n\
               [[ -e \"$ready\" ]] || exit 1\n\
               printf '{{\"id\":\"live-run\",\"status\":\"running\",\"exit_code\":null}}\\n'\n\
               exit 0 ;;\n\
             *) exit 0 ;;\n\
             esac\n",
            log = log.to_string_lossy(),
        ),
    );
}

/// A dispatched fixture with one autorun recipe whose site ceiling of zero
/// lets the ticket be written and starts nothing, so each case below decides
/// for itself what may run.
fn dispatched_but_unstarted(tmp: &Path, log: &Path) {
    fixture(
        tmp,
        json!({
            "max_concurrent": 0,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🔧",
                "description": "fix the red gate",
                "brief": "fix {title}",
                "autorun": true,
                "when": { "kinds": ["pr"], "gate": "red" }
            }]
        }),
    );
    detaching_runner(tmp, log);
    ephor(tmp).args(["refresh", "demo"]).assert().success();
    ephor(tmp).args(["work", "dispatch"]).assert().success();
    let path = tmp.join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["work"]["max_concurrent"] = Value::Null;
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// A second work root beside the fixture's own, in the same checkout: two
/// pantas over one working tree.
fn second_root_in_the_same_checkout(tmp: &Path, item: &str) {
    duplicate_root_at(tmp, item, &tmp.join("demo"), "panta2", "second");
}

/// §FS-005-dispatch.24: a run live on one root stops the sweep starting
/// anything on another root over the same working tree. The live root holds no
/// due ticket of its own, which is exactly the case a per-root guard misses.
#[test]
fn a_sweep_starts_nothing_in_a_checkout_a_run_already_holds() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    second_root_in_the_same_checkout(tmp.path(), "github-prs:acme/widget#second");
    let holder = hold(&tmp.path().join("demo/panta"));

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &reading).is_empty(),
        "the sweep must hold to the published work-run shape: {reading}"
    );
    assert!(
        output.status.success(),
        "passing a root over is not a failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(starts(&log), 0, "no run may be started in a busy checkout");
    // Nothing started, and the sweep says which run has the tree rather than
    // reading as a quiet machine. One row: the root holding the run itself is
    // said nothing about, because it has its run.
    let runs = reading["runs"].as_array().unwrap();
    assert_eq!(
        runs.len(),
        1,
        "one root is passed over, one is running: {reading}"
    );
    assert_eq!(runs[0]["outcome"], "passed-over", "{reading}");
    assert!(
        runs[0]["root"].as_str().unwrap().ends_with("demo/panta2"),
        "the root held back is the one beside the live run: {reading}"
    );
    assert!(
        runs[0]["reason"]
            .as_str()
            .unwrap()
            .contains("a run is live in this checkout"),
        "the reason is the tree, not a ceiling: {reading}"
    );
    drop(holder);
}

/// §FS-005-dispatch.24: one invocation, two work roots over one tree, nothing
/// live to begin with — the run this very command starts holds the tree for
/// the rest of it, so the second group is refused by the id of the first's
/// run rather than started beside it.
#[test]
fn one_command_starts_one_run_in_one_checkout() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    second_root_in_the_same_checkout(tmp.path(), "github-prs:acme/widget#second");
    holding_runner(tmp.path(), &log);

    let output = ephor(tmp.path())
        .args(["work", "run", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &reading).is_empty(),
        "a refusal must hold to the published work-run shape: {reading}"
    );
    assert!(
        !output.status.success(),
        "the reader asked for two runs and got one: {reading}"
    );
    assert_eq!(reading["refused"], 1, "{reading}");
    assert_eq!(reading["failed"], 0, "{reading}");
    let runs = reading["runs"].as_array().unwrap();
    let outcomes: Vec<&str> = runs
        .iter()
        .filter_map(|run| run["outcome"].as_str())
        .collect();
    assert_eq!(outcomes, vec!["started", "refused"], "{reading}");
    assert_eq!(
        runs[1]["says"], "a run is live in this checkout: live-run",
        "the refusal names the run this command just started: {reading}"
    );
    assert_eq!(starts(&log), 1, "one working tree takes one run");
}

/// §FS-005-dispatch.24: `--force` lifts that refusal too — the reader who
/// says they know what the other run is doing means the run made a second ago
/// as much as one that was already there.
#[test]
fn force_starts_both_groups_in_one_checkout() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    second_root_in_the_same_checkout(tmp.path(), "github-prs:acme/widget#second");
    holding_runner(tmp.path(), &log);

    let output = ephor(tmp.path())
        .args(["work", "run", "--force", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        output.status.success(),
        "--force starts them anyway: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(reading["refused"], 0, "{reading}");
    let outcomes: Vec<&str> = reading["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|run| run["outcome"].as_str())
        .collect();
    assert_eq!(outcomes, vec!["started", "started"], "{reading}");
    assert_eq!(starts(&log), 2, "both roots were launched");
}

/// §FS-005-dispatch.24: two roots over one tree, both due, in one sweep. The
/// snapshot the due list was read from is older than the first launch, so the
/// second is passed over with the run that took the tree — a successful
/// non-launch outcome, not a failed start.
#[test]
fn one_sweep_starts_one_of_two_due_roots_over_one_checkout() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    second_root_in_the_same_checkout(tmp.path(), "github-prs:acme/widget#second");
    holding_runner(tmp.path(), &log);

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &reading).is_empty(),
        "the sweep must hold to the published work-run shape: {reading}"
    );
    assert!(
        output.status.success(),
        "passing a root over is not a failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let runs = reading["runs"].as_array().unwrap();
    let outcomes: Vec<&str> = runs
        .iter()
        .filter_map(|run| run["outcome"].as_str())
        .collect();
    assert_eq!(
        outcomes.iter().filter(|of| **of == "started").count(),
        1,
        "one working tree takes one run: {reading}"
    );
    let over = runs
        .iter()
        .find(|run| run["outcome"] == "passed-over")
        .unwrap_or_else(|| panic!("{reading}"));
    assert!(
        over["reason"]
            .as_str()
            .unwrap()
            .contains("a run is live in this checkout: live-run"),
        "the reason names the run that took the tree: {reading}"
    );
    assert_eq!(reading["failed"], 0, "{reading}");
    assert_eq!(starts(&log), 1, "only one root may be launched");
}

/// §FS-005-dispatch.24: the key the reader presses is refused by name when a
/// run holds the plan's tree, and `--force` lifts exactly that refusal.
#[test]
fn a_named_plan_in_a_busy_checkout_is_refused_and_forced() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    second_root_in_the_same_checkout(tmp.path(), "github-prs:acme/widget#second");
    // A run in the fixture's own root, with a descriptor naming it, so the
    // refusal has a run id to give rather than only a root.
    let root = tmp.path().join("demo/panta");
    fs::create_dir_all(root.join("runtime")).unwrap();
    fs::write(root.join("runtime/run.json"), r#"{"id":"live-run"}"#).unwrap();
    let holder = hold(&root);

    let refused = ephor(tmp.path())
        .args([
            "work",
            "run",
            "--item",
            "github-prs:acme/widget#second",
            "--json",
        ])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &reading).is_empty(),
        "a refusal must hold to the published work-run shape: {reading}"
    );
    assert!(
        !refused.status.success(),
        "the reader asked for a run and got none: {reading}"
    );
    assert_eq!(reading["refused"], 1, "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "refused", "{reading}");
    assert_eq!(
        reading["runs"][0]["says"], "a run is live in this checkout: live-run",
        "{reading}"
    );
    assert_eq!(starts(&log), 0, "nothing may be started into a busy tree");

    let forced = ephor(tmp.path())
        .args([
            "work",
            "run",
            "--item",
            "github-prs:acme/widget#second",
            "--force",
            "--json",
        ])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&forced.stdout).unwrap();
    assert!(
        forced.status.success(),
        "--force starts it anyway: {}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert_eq!(reading["runs"][0]["outcome"], "started", "{reading}");
    assert_eq!(starts(&log), 1, "the forced run is the only one started");
    drop(holder);
}

/// §FS-005-dispatch.24: the guard is where runs start, never where plans are
/// written. A busy checkout still takes a ticket — that is what makes handing
/// one down to a tree somebody is working in safe.
#[test]
fn a_busy_checkout_still_takes_a_dispatched_ticket() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    let holder = hold(&tmp.path().join("demo/panta"));

    // Forget what the fixture already dispatched, so this dispatch has a
    // ticket to write rather than an item it has already answered.
    ephor(tmp.path())
        .args(["work", "forget", "--item", "github-prs:acme/widget#42"])
        .assert()
        .success();
    let output = ephor(tmp.path())
        .args(["work", "dispatch", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "writing a file into a busy checkout is not a start: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["opened"], 1, "{reading}");
    assert_eq!(
        reading["refused"], 0,
        "nothing refuses where plans are written: {reading}"
    );
    // Dispatch starts what it just wrote where nobody has to press a key
    // (§FS-005-dispatch.24), and that continuation is the sweep — so this is
    // also the guard holding on the path a hand-down would take.
    assert_eq!(
        starts(&log),
        0,
        "the ticket waits in the plan until the tree is free"
    );
    drop(holder);
}
