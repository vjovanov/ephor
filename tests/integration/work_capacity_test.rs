//! Autorun capacity, end to end (§AR-007-runtime): what the ceilings over a
//! sweep count, which of them refuses a start, and what the reading says about
//! the roots holding the slots.
//!
//! Split out of `work_test.rs`, whose remaining cases are about dispatch
//! rather than about capacity. The cases moved unchanged.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::{json, Value};

use common::*;

/// Copy the dispatched fixture's plan and ledger entry onto another checkout,
/// producing another independently lockable due root without inventing a
/// second dispatch implementation inside the test.
fn duplicate_work_root(tmp: &Path, item: &str, checkout_name: &str) {
    let ledger_path = tmp.join("state/ephor/work.json");
    let mut ledger: Value =
        serde_json::from_str(&fs::read_to_string(&ledger_path).unwrap()).unwrap();
    let original = ledger["entries"]["github-prs:acme/widget#42"].clone();
    let source_root = tmp.join("demo/panta");
    let checkout = tmp.join(checkout_name);
    let root = checkout.join("panta");
    fs::create_dir_all(&root).unwrap();
    fs::copy(source_root.join("states.yaml"), root.join("states.yaml")).unwrap();
    let plan_id = format!("github-prs-acme-widget-{checkout_name}");
    let plan = root.join(format!("{plan_id}.rhei.md"));
    fs::copy(source_root.join("github-prs-acme-widget-42.rhei.md"), &plan).unwrap();

    let mut copied = original;
    copied["root"] = json!(root);
    copied["checkout"] = json!(checkout);
    copied["branch"] = Value::Null;
    copied["plan_id"] = json!(plan_id);
    copied["plan"] = json!(plan);
    ledger["entries"][item] = copied;
    fs::write(&ledger_path, serde_json::to_string_pretty(&ledger).unwrap()).unwrap();
}

/// A detached runner whose `second` root finishes inside the handshake and
/// whose other roots hold their runtime lock long enough for the sweep to
/// account for them. Its child redirects the inherited pipes so the launcher
/// itself can return immediately, as a real detached launcher does.
fn capacity_runner(tmp: &Path, log: &Path) {
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
               if [[ \"$root\" == *second* ]]; then\n\
                 printf '{{\"id\":\"finished\",\"status\":\"finished\",\"exit_code\":0}}\\n'\n\
                 exit 0\n\
               fi\n\
               mkdir -p \"$root/.rhei\"\n\
               ready=\"$root/.rhei/run-lock-ready\"\n\
               python -c 'import fcntl,pathlib,sys,time; lock=open(sys.argv[1],\"w\"); fcntl.flock(lock,fcntl.LOCK_EX); pathlib.Path(sys.argv[2]).touch(); time.sleep(20)' \"$root/.rhei/run.lock\" \"$ready\" >/dev/null 2>&1 &\n\
               for _ in {{1..100}}; do\n\
                 [[ -e \"$ready\" ]] && break\n\
                 sleep 0.01\n\
               done\n\
               [[ -e \"$ready\" ]] || exit 1\n\
               printf '{{\"id\":\"live\",\"status\":\"running\",\"exit_code\":null}}\\n'\n\
               exit 0 ;;\n\
             *) exit 0 ;;\n\
             esac\n",
            log = log.to_string_lossy(),
        ),
    );
}

/// A detached runner whose help probes rendezvous before either command can
/// take the autorun reservation. Once released, a launch holds its root long
/// enough for the other command's authoritative snapshot to see it.
fn overlapping_capacity_runner(tmp: &Path, log: &Path) {
    let rendezvous = tmp.join("autorun-rendezvous");
    fs::create_dir_all(&rendezvous).unwrap();
    fs::create_dir_all(tmp.join("fakebin")).unwrap();
    make_executable(
        &tmp.join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*)\n\
               touch {rendezvous}/$$\n\
               for _ in {{1..500}}; do\n\
                 [[ $(find {rendezvous} -type f | wc -l) -ge 2 ]] && break\n\
                 sleep 0.01\n\
               done\n\
               printf 'Options:\\n      --headless  detach it\\n'\n\
               exit 0 ;;\n\
             *--headless*)\n\
               printf '%s\\n' \"$*\" >> {log}\n\
               root=\"$4\"\n\
               mkdir -p \"$root/.rhei\"\n\
               ready=\"$root/.rhei/run-lock-ready\"\n\
               python -c 'import fcntl,pathlib,sys,time; lock=open(sys.argv[1],\"w\"); fcntl.flock(lock,fcntl.LOCK_EX); pathlib.Path(sys.argv[2]).touch(); time.sleep(20)' \"$root/.rhei/run.lock\" \"$ready\" >/dev/null 2>&1 &\n\
               for _ in {{1..100}}; do\n\
                 [[ -e \"$ready\" ]] && break\n\
                 sleep 0.01\n\
               done\n\
               [[ -e \"$ready\" ]] || exit 1\n\
               printf '{{\"id\":\"live\",\"status\":\"running\",\"exit_code\":null}}\\n'\n\
               exit 0 ;;\n\
             *) exit 0 ;;\n\
             esac\n",
            rendezvous = rendezvous.to_string_lossy(),
            log = log.to_string_lossy(),
        ),
    );
}

/// §FS-005-dispatch.24: filtered sweeps in separate processes reserve the
/// shared aggregate slot before either starts a root.
#[test]
fn review_repro_concurrent_sweeps_share_the_global_ceiling() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
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
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    let config_path = tmp.path().join("status.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["work"]["max_concurrent"] = json!(1);
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    let other_item = "github-prs:acme/widget#other";
    duplicate_work_root(tmp.path(), other_item, "other");
    let ledger_path = tmp.path().join("state/ephor/work.json");
    let mut ledger: Value =
        serde_json::from_str(&fs::read_to_string(&ledger_path).unwrap()).unwrap();
    ledger["entries"][other_item]["project"] = json!("other");
    fs::write(&ledger_path, serde_json::to_string_pretty(&ledger).unwrap()).unwrap();
    overlapping_capacity_runner(tmp.path(), &log);

    let mut demo = ephor(tmp.path());
    demo.args(["work", "run", "--due", "--project", "demo", "--json"]);
    let demo = std::thread::spawn(move || demo.output().unwrap());
    let mut other = ephor(tmp.path());
    other.args(["work", "run", "--due", "--project", "other", "--json"]);
    let other = std::thread::spawn(move || other.output().unwrap());
    let demo = demo.join().unwrap();
    let other = other.join().unwrap();
    assert!(
        demo.status.success(),
        "{}",
        String::from_utf8_lossy(&demo.stderr)
    );
    assert!(
        other.status.success(),
        "{}",
        String::from_utf8_lossy(&other.stderr)
    );
    let readings: [Value; 2] = [
        serde_json::from_slice(&demo.stdout).unwrap(),
        serde_json::from_slice(&other.stdout).unwrap(),
    ];
    let outcomes: Vec<&str> = readings
        .iter()
        .flat_map(|reading| reading["runs"].as_array().unwrap())
        .filter_map(|run| run["outcome"].as_str())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == "started")
            .count(),
        1,
        "only one filtered sweep may consume the shared slot: {readings:?}"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == "passed-over")
            .count(),
        1,
        "the other eligible root is reported without failing: {readings:?}"
    );
    assert_eq!(starts(&log), 1, "only one launcher was invoked");

    let roots = [
        tmp.path().join("demo/panta"),
        tmp.path().join("other/panta"),
    ];
    let live = roots
        .iter()
        .filter(|root| {
            fs::File::open(root.join(".rhei/run.lock")).is_ok_and(|lock| {
                matches!(lock.try_lock_shared(), Err(fs::TryLockError::WouldBlock))
            })
        })
        .count();
    assert_eq!(live, 1, "global cap=1 must leave at most one live root");
}

#[test]
fn review_repro_due_rows_hold_to_the_published_schema() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
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
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert_eq!(reading["failed"], 0, "{reading}");
    let problems = ephor::api::schema::holds("work-run", &reading);
    assert!(problems.is_empty(), "{problems:?}\n{reading}");
}

/// §FS-005-dispatch.24: capacity is a ceiling on live roots, selected in
/// configured rank order. Existing live work consumes it, a completed start
/// returns it, and roots omitted only for capacity are non-failing results.
#[test]
fn the_autorun_sweep_caps_live_roots_and_reports_every_capacity_pass_over() {
    let tmp = tempdir();
    let ranking = tmp.path().join("ranking.txt");
    fs::write(
        &ranking,
        "github-prs:acme/widget#second\ngithub-prs:acme/widget#42\n",
    )
    .unwrap();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 0,
            "ranking": ranking,
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
    let log = tmp.path().join("runner.log");
    capacity_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();

    // The configured zero lets dispatch write the ticket but starts nothing.
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"));
    assert_eq!(starts(&log), 0);
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#second", "demo-second");
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#third", "demo-third");

    // An existing live root fills the CLI's one aggregate slot even though
    // that root is itself excluded by the one-live-run-per-root guarantee.
    let held_root = tmp.path().join("demo-third/panta");
    fs::create_dir_all(held_root.join(".rhei")).unwrap();
    fs::write(held_root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(held_root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();
    let full = ephor(tmp.path())
        .args(["work", "run", "--due", "--max-concurrent", "1", "--json"])
        .output()
        .unwrap();
    let full: Value = serde_json::from_slice(&full.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &full).is_empty(),
        "capacity rows must hold to the published work-run shape: {full}"
    );
    assert_eq!(full["failed"], 0, "{full}");
    assert_eq!(full["runs"].as_array().unwrap().len(), 2, "{full}");
    assert!(full["runs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|run| run["outcome"] == "passed-over"));
    assert_eq!(starts(&log), 0);
    drop(holder);

    // The flag overrides configured zero. Ranking tries `second` first; it
    // finishes immediately and returns the slot, the original root stays
    // live, and the remaining root is explicitly passed over.
    let swept = ephor(tmp.path())
        .args(["work", "run", "--due", "--max-concurrent", "1", "--json"])
        .output()
        .unwrap();
    let swept: Value = serde_json::from_slice(&swept.stdout).unwrap();
    assert_eq!(swept["failed"], 0, "{swept}");
    let runs = swept["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 3, "{swept}");
    assert_eq!(runs[0]["item"], "github-prs:acme/widget#second");
    assert_eq!(runs[0]["outcome"], "done");
    assert_eq!(runs[1]["item"], "github-prs:acme/widget#42");
    assert_eq!(runs[1]["outcome"], "started");
    assert_eq!(runs[2]["item"], "github-prs:acme/widget#third");
    assert_eq!(runs[2]["outcome"], "passed-over");
    assert!(runs[2]["reason"]
        .as_str()
        .unwrap()
        .contains("global work.max_concurrent 1"));
    assert_eq!(starts(&log), 2);

    // Prose carries the same non-failing outcome, and configured zero is
    // still in force on the next invocation because the flag was one-sweep.
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"));
}

/// Park a duplicated work root on a person: teach its machine a poll that
/// declares whose answer it waits for, move its only ticket into that state,
/// and hold the root's run lock. What is left is a live root spending
/// nothing — the shape the ticket measured for six and a half hours.
fn park_on_a_person(root: &Path) -> fs::File {
    let states = root.join("states.yaml");
    let machine = fs::read_to_string(&states).unwrap();
    assert!(machine.contains("\n  done:\n"), "{machine}");
    fs::write(
        &states,
        machine.replace(
            "\n  done:\n",
            "\n  plan-approval:\n    program: [\"bash\", \"await.sh\"]\n    \
             poll:\n      interval: 15m\n      max_attempts: 96\n      \
             waiting_on: author\n\n  done:\n",
        ),
    )
    .unwrap();
    for plan in fs::read_dir(root).unwrap().flatten() {
        let path = plan.path();
        if path.extension().is_some_and(|ext| ext == "md") {
            let text = fs::read_to_string(&path).unwrap();
            fs::write(
                &path,
                text.replace("**State:** fix", "**State:** plan-approval"),
            )
            .unwrap();
        }
    }
    hold_the_run_lock(root)
}

fn hold_the_run_lock(root: &Path) -> fs::File {
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::write(root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();
    holder
}

/// Rewrite the site's two autorun ceilings between sweeps, so one fixture
/// answers for every combination of them.
fn set_ceilings(tmp: &Path, concurrent: Value, active: Value) {
    let path = tmp.join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["work"]["max_concurrent"] = concurrent;
    config["work"]["max_active"] = active;
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// §FS-005-dispatch.24, §AR-007-runtime.1: two ceilings count two different
/// things. A root parked on a person's answer is live — it holds a
/// `max_concurrent` slot — and is not working, so it holds no `max_active`
/// one; a refusal names the key it refused on; and a site that never names
/// the second key is bounded exactly as it was.
#[test]
fn a_root_parked_on_a_person_costs_a_flight_slot_and_no_agent_slot() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            // Zero while the tickets are written, so dispatch starts nothing
            // and every start below is this test's own.
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
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    assert_eq!(starts(&log), 0);
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#second", "demo-second");
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#third", "demo-third");

    let sweep = |tmp: &Path| -> Value {
        let output = ephor(tmp)
            .args(["work", "run", "--due", "--json"])
            .output()
            .unwrap();
        let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
        let problems = ephor::api::schema::holds("work-run", &reading);
        assert!(problems.is_empty(), "{problems:?}\n{reading}");
        reading
    };

    // Nothing live yet, and zero admits nothing under the new key either.
    set_ceilings(tmp.path(), json!(9), json!(0));
    let none = sweep(tmp.path());
    assert_eq!(none["failed"], 0, "{none}");
    assert!(
        none["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|run| run["outcome"] == "passed-over"),
        "{none}"
    );
    assert!(
        none["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("global work.max_active 0"),
        "{none}"
    );
    assert_eq!(none["capacity"]["live"], 0, "{none}");
    assert_eq!(starts(&log), 0);

    // One root working, one parked on its author. Both are live; only one of
    // them is spending anything.
    let working = hold_the_run_lock(&tmp.path().join("demo-second/panta"));
    let parked = park_on_a_person(&tmp.path().join("demo-third/panta"));

    set_ceilings(tmp.path(), json!(9), json!(1));
    let full = sweep(tmp.path());
    assert_eq!(full["capacity"]["live"], 2, "{full}");
    assert_eq!(full["capacity"]["active"], 1, "{full}");
    assert_eq!(full["capacity"]["parked"], 1, "{full}");
    assert_eq!(full["runs"][0]["outcome"], "passed-over", "{full}");
    assert_eq!(full["failed"], 0, "{full}");
    assert_eq!(
        full["runs"][0]["reason"], "global work.max_active 1 is full (1 active run(s), 1 parked)",
        "the refusal names its key and says what the other slot is doing"
    );
    assert_eq!(starts(&log), 0);

    // The parked root still holds a roots-in-flight slot: it exists, and that
    // is what the older key counts. Its wording is untouched.
    set_ceilings(tmp.path(), json!(2), json!(9));
    let flight = sweep(tmp.path());
    assert_eq!(
        flight["runs"][0]["reason"], "global work.max_concurrent 2 is full (2 live run(s))",
        "{flight}"
    );
    assert_eq!(starts(&log), 0);

    // And with room for one more agent, the parked root's freed slot is what
    // the remaining root starts in — which is the whole point of the second
    // number.
    set_ceilings(tmp.path(), json!(9), json!(2));
    let started = sweep(tmp.path());
    assert_eq!(started["runs"][0]["outcome"], "started", "{started}");
    assert_eq!(started["failed"], 0, "{started}");
    assert_eq!(starts(&log), 1);
    drop((working, parked));
}

/// §FS-005-dispatch.24: a site that never names the second key behaves as it
/// did before there was one — same starts, same wording — and its reading
/// still says how much of what is live is a person rather than an agent.
#[test]
fn a_site_that_names_only_the_flight_ceiling_is_bounded_exactly_as_before() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 1,
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
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#second", "demo-second");
    let parked = park_on_a_person(&tmp.path().join("demo-second/panta"));

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    // The parked root fills the one aggregate slot exactly as any live root
    // did before, in the words it always used.
    assert_eq!(reading["failed"], 0, "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert_eq!(
        reading["runs"][0]["reason"], "global work.max_concurrent 1 is full (1 live run(s))",
        "{reading}"
    );
    assert_eq!(reading["capacity"]["max_active"], Value::Null, "{reading}");
    // And the reading says what the old wording could not: the slot holding
    // the sweep back is a person.
    assert_eq!(reading["capacity"]["live"], 1, "{reading}");
    assert_eq!(reading["capacity"]["parked"], 1, "{reading}");

    // The prose carries the same split beside the pass-over it explains.
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"))
        .stdout(predicate::str::contains(
            "capacity: 1 live root(s) — 0 active, 1 parked",
        ));
    drop(parked);
}

#[test]
fn a_project_autorun_ceiling_remains_inside_the_cli_aggregate_override() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!({
            "max_concurrent": 1,
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
    let config_path = tmp.path().join("status.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["projects"]["demo"]["work"] = json!({ "max_concurrent": 0 });
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    let log = tmp.path().join("runner.log");
    detaching_runner(tmp.path(), &log);
    ephor(tmp.path())
        .args(["refresh", "demo"])
        .assert()
        .success();
    ephor(tmp.path())
        .args(["work", "dispatch"])
        .assert()
        .success();

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--max-concurrent", "3", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert!(reading["runs"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("projects.demo.work.max_concurrent 0"));
    assert_eq!(starts(&log), 0, "the project ceiling remains in force");
}

/// Put the fixture's project and one more inside a single organization, and
/// give the second one a work root of its own. The grouping is the registry's
/// to declare and is declared nowhere else (§FS-005-dispatch.24).
fn one_organization(tmp: &Path, organization: &str, sibling: &str, item: &str) {
    let path = tmp.join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    registry["organizations"] = json!([{ "id": organization, "name": "The Guild" }]);
    registry["projects"][0]["organization"] = json!(organization);
    let mut second = registry["projects"][0].clone();
    second["id"] = json!(sibling);
    second["display_name"] = json!(sibling);
    second["root"] = json!(tmp.join(sibling).to_string_lossy());
    second["branches"] = json!([]);
    registry["projects"].as_array_mut().unwrap().push(second);
    write_registry(&path, &registry);
    duplicate_work_root(tmp, item, sibling);
    assign_project(tmp, item, sibling);
}

/// Say which project a duplicated root's plan belongs to. `duplicate_work_root`
/// copies the fixture's entry, and the copy would otherwise keep the fixture's
/// project — and with it that project's organization.
fn assign_project(tmp: &Path, item: &str, project: &str) {
    let ledger_path = tmp.join("state/ephor/work.json");
    let mut ledger: Value =
        serde_json::from_str(&fs::read_to_string(&ledger_path).unwrap()).unwrap();
    ledger["entries"][item]["project"] = json!(project);
    fs::write(&ledger_path, serde_json::to_string_pretty(&ledger).unwrap()).unwrap();
}

/// Take and hold a work root's runtime lock, so the sweep reads it as a root
/// somebody else's run already has.
fn hold(root: &Path) -> fs::File {
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::write(root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();
    holder
}

/// A dispatched fixture with one autorun recipe, whose site ceiling of zero
/// lets the ticket be written and starts nothing — the state every ceiling
/// test below builds its own configuration on top of.
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
}

/// Read the site configuration, let the test rewrite it, and write it back.
fn reconfigure(tmp: &Path, edit: impl Fn(&mut Value)) {
    let path = tmp.join("status.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    edit(&mut config);
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// §FS-005-dispatch.24: the organization ceiling bounds the sum of its
/// projects' live roots, and reaches exactly the projects the registry puts
/// inside it — a project it names no organization for is bound by none.
#[test]
fn an_organization_ceiling_bounds_the_sum_of_its_projects_live_roots() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    one_organization(
        tmp.path(),
        "guild",
        "gadget",
        "github-prs:acme/widget#gadget",
    );
    // A root outside the organization entirely: the registry knows no project
    // by this name, so nothing puts it under the guild's ceiling.
    duplicate_work_root(tmp.path(), "github-prs:acme/widget#outsider", "outsider");
    assign_project(tmp.path(), "github-prs:acme/widget#outsider", "outsider");
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({ "guild": { "work": { "max_concurrent": 1 } } });
    });

    // One of the guild's two projects already has a live run, which spends the
    // guild's only slot.
    let holder = hold(&tmp.path().join("gadget/panta"));
    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        ephor::api::schema::holds("work-run", &reading).is_empty(),
        "the organization tier must hold to the published work-run shape: {reading}"
    );
    assert_eq!(reading["failed"], 0, "{reading}");
    let runs = reading["runs"].as_array().unwrap();
    let demo = runs
        .iter()
        .find(|run| run["root"].as_str().unwrap().contains("/demo/"))
        .unwrap_or_else(|| panic!("{reading}"));
    assert_eq!(demo["outcome"], "passed-over", "{reading}");
    assert!(
        demo["reason"]
            .as_str()
            .unwrap()
            .contains("organizations.guild.work.max_concurrent 1 is full (1 live run(s))"),
        "{reading}"
    );
    // The project the registry left out of every organization is untouched by
    // the guild's ceiling and gets its run.
    let outsider = runs
        .iter()
        .find(|run| run["root"].as_str().unwrap().contains("/outsider/"))
        .unwrap_or_else(|| panic!("{reading}"));
    assert_eq!(outsider["outcome"], "started", "{reading}");
    assert_eq!(starts(&log), 1);
    drop(holder);
}

/// §FS-005-dispatch.24: an absent `organizations` map is an omitted ceiling
/// for every organization, so a configuration written before the tier existed
/// starts exactly what it started before.
#[test]
fn an_absent_organizations_map_leaves_every_organization_unbounded() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    one_organization(
        tmp.path(),
        "guild",
        "gadget",
        "github-prs:acme/widget#gadget",
    );
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    let runs = reading["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{reading}");
    assert!(
        runs.iter().all(|run| run["outcome"] == "started"),
        "{reading}"
    );
    assert!(
        reading["notes"].as_array().unwrap().is_empty(),
        "nothing is wrong with a configuration that omits the tier: {reading}"
    );
    assert_eq!(starts(&log), 2);
}

/// §FS-005-dispatch.24: a ceiling keyed on an organization no registry row
/// places a project inside bounds nobody, so the sweep that would have read it
/// says so by name — a key may not quietly remove the bound its author meant
/// to set — and starts exactly what it would have started without the key. An
/// organization the registry declares that no project has joined is as empty
/// as one it never declared, and is named the same way.
#[test]
fn a_ceiling_over_an_organization_holding_no_project_is_noted() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    let path = tmp.path().join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    registry["organizations"] = json!([{ "id": "emptyguild", "name": "The Empty Guild" }]);
    write_registry(&path, &registry);
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({
            "emptyguild": { "work": { "max_concurrent": 0 } },
            "nosuchorg": { "work": { "max_concurrent": 0 } }
        });
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    // The declared-but-empty id and the undeclared one are one condition, and
    // the sweep says the same sentence about each.
    assert_eq!(
        reading["notes"],
        json!([
            "organizations.emptyguild: no registry row places a project in it, so the ceiling written there bounds nothing",
            "organizations.nosuchorg: no registry row places a project in it, so the ceiling written there bounds nothing"
        ]),
        "{reading}"
    );
    assert_eq!(reading["runs"][0]["outcome"], "started", "{reading}");
    assert_eq!(starts(&log), 1, "a ceiling over nobody refuses nobody");
}

/// §FS-005-dispatch.24: membership is the project row's `organization` field
/// and nothing else, so a registry that declares no `organizations` array at
/// all — which the validator permits, and which this fixture writes — still
/// puts its project inside the organization its row names — the fixture this
/// case starts from writes no such array, so the shape is the repository's own
/// default. The ceiling binds there, and nothing announces it as bounding
/// nobody: one reading answers both, so no document can refuse by a key it
/// calls empty in the same breath.
#[test]
fn a_ceiling_binds_where_a_row_joins_it_though_the_registry_declares_no_array() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    let path = tmp.path().join("workspaces.json");
    let mut registry: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    registry["projects"][0]["organization"] = json!("acme");
    write_registry(&path, &registry);
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({ "acme": { "work": { "max_concurrent": 0 } } });
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    // A ceiling that is refusing starts bounds somebody.
    assert_eq!(reading["notes"], json!([]), "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    let why = reading["runs"][0]["reason"].as_str().unwrap_or_default();
    let full = "organizations.acme.work.max_concurrent 0 is full";
    assert!(why.contains(full), "{reading}");
    assert_eq!(starts(&log), 0, "{reading}");
}

/// §FS-005-dispatch.24: a project ceiling above its organization's is named
/// at the sweep, in prose and `--json`, and changes nothing — the project's
/// number stands and the organization's total still refuses the next start.
#[test]
fn an_inverted_project_ceiling_is_named_and_the_organization_total_still_binds() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    one_organization(
        tmp.path(),
        "guild",
        "gadget",
        "github-prs:acme/widget#gadget",
    );
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = Value::Null;
        config["organizations"] = json!({ "guild": { "work": { "max_concurrent": 1 } } });
        config["projects"]["demo"]["work"] = json!({ "max_concurrent": 4 });
    });

    let holder = hold(&tmp.path().join("gadget/panta"));
    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    let notes = reading["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1, "{reading}");
    let note = notes[0].as_str().unwrap();
    assert!(
        note.contains("projects.demo.work.max_concurrent 4"),
        "{note}"
    );
    assert!(
        note.contains("organizations.guild.work.max_concurrent 1"),
        "{note}"
    );
    // Warned about, not corrected: the guild's total still refuses the start
    // the project's own number would have allowed four times over.
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert!(
        reading["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("organizations.guild.work.max_concurrent 1 is full"),
        "{reading}"
    );
    assert_eq!(starts(&log), 0);

    // The same sentence in prose, where the reader who did not ask for JSON is
    // (§AR-009-surfaces.1).
    ephor(tmp.path())
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "projects.demo.work.max_concurrent 4 is above \
             organizations.guild.work.max_concurrent 1",
        ));
    drop(holder);
}

/// §FS-005-dispatch.24: the site half of the same rule — a project ceiling
/// above the site's aggregate is named the same way, and is still not
/// rewritten.
#[test]
fn an_inverted_project_ceiling_is_named_against_the_site_ceiling_too() {
    let tmp = tempdir();
    let log = tmp.path().join("runner.log");
    dispatched_but_unstarted(tmp.path(), &log);
    reconfigure(tmp.path(), |config| {
        config["work"]["max_concurrent"] = json!(2);
        config["projects"]["demo"]["work"] = json!({ "max_concurrent": 5 });
    });

    let output = ephor(tmp.path())
        .args(["work", "run", "--due", "--json"])
        .output()
        .unwrap();
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["failed"], 0, "{reading}");
    let notes = reading["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1, "{reading}");
    assert!(
        notes[0]
            .as_str()
            .unwrap()
            .contains("projects.demo.work.max_concurrent 5 is above global work.max_concurrent 2"),
        "{reading}"
    );
    // Named, not refused: the root the project's number allows still starts.
    assert_eq!(reading["runs"][0]["outcome"], "started", "{reading}");
    assert_eq!(starts(&log), 1);
}
