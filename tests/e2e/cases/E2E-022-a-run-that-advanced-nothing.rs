//! E2E-022-a-run-that-advanced-nothing: the sweep stops starting runs on a root
//! whose runs keep finishing without moving anything (§FS-005-dispatch.24).
//!
//! The scenario is a machine left sweeping every three minutes with ten plans
//! laid down. One root's runs keep coming back having advanced nothing — a
//! provider overloaded, a workspace the agent cannot get anywhere in — and
//! because the *start* worked every time, nothing was remembered and the next
//! sweep started another one. Measured from the outside that was nine restarts
//! in thirty-three minutes, eighteen agent attempts spent on nothing, and one of
//! two slots held; and the row said `started` each time, so the batch read as
//! healthy while the stalled root hid in it.
//!
//! What this case pins is that the sweep now reads what the last run there
//! actually did, out of the run's own stream and nothing else (§FS-005-dispatch.15.2):
//! no pass reporting progress and no slot released in a completing outcome is a
//! run that advanced nothing, and that root is passed over — loudly, with the
//! reason in the row — rather than restarted. A run the reader asks for by name
//! is never refused for it, and a run that does advance drops the memory at
//! once.
//!
//! Beside it is the same skip with no judgement in it at all: `--except`, for a
//! driver carrying its own back-off. It excludes work roots from one sweep and
//! it may not narrow the width the `--act` gate is counted over
//! (§FS-011-command-line.10) — otherwise the flag that says *skip this one*
//! would also be the flag that walks past the gate.

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::json;

use support::*;

/// The matter the recipe turns into a ticket.
const ITEM: &str = "acmeforge:app/101";

/// The second project, watched beside the first so a bare sweep is a sweep over
/// two — which is what makes the `--act` gate fire. It has no providers and no
/// matters: its only job in this case is to be a project, because the width the
/// gate counts is the resolved project set and not the roots inside it.
const OTHER: &str = "far";

/// A forge with one pull request of the reader's own and a red gate, so there
/// is a matter the recipe picks up and hands over.
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
        "gate": { "repos": [ { "repo": "app", "passed": 5, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime that detaches, writes the record of the run it just made, and
/// reports itself finished inside the handshake — so it leaves no lock and the
/// root is a candidate again on the very next sweep, which is the shape the
/// reproduction had.
///
/// What the run *did* is the fixture. Two files in the world say it: one holds
/// the outcome every slot is released in, the other whether the pass reported
/// progress. Written this way round on purpose — a run that released its slots
/// `failed` and reported no pass progressing is a run that finished, reached its
/// own end, and advanced nothing, which is exactly the case the ticket reports
/// and is indistinguishable from a healthy run by the exit status alone.
///
/// It also appends one line per start, which is the measurement this whole case
/// is about: the reproduction counted nine of them where there should have been
/// one.
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="$1"; shift
if [ "$verb" = run ] && [ "${1:-}" = --help ]; then
  printf '%s\n' '      --headless  Detach the run'
  exit 0
fi
while [ "${1:-}" = --headless ] || [ "${1:-}" = --json ]; do
  shift
done
root="$1"; shift
plan=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --rhei) plan="$2"; shift 2 ;;
    *) shift ;;
  esac
done
printf '%s\n' "$root" >> "$HOME/starts"
id="acme-run-$(wc -l < "$HOME/starts" | tr -d ' ')"
outcome="$(cat "$HOME/slot-outcome" 2>/dev/null || printf failed)"
progressed="$(cat "$HOME/pass-progressed" 2>/dev/null || printf false)"
task="$plan.fix-gate-1"
log="runtime/logs/$task.log"
mkdir -p "$root/runtime"
{
  printf '{"seq":1,"ts":"2026-09-11T09:00:00Z","event":"run_started","schema":1,"run_id":"%s","workspace":"%s","parallel":1,"total_tasks":1}\n' "$id" "$root"
  printf '{"seq":2,"ts":"2026-09-11T09:00:01Z","event":"pass_started","pass":1,"ready":["%s"]}\n' "$task"
  printf '{"seq":3,"ts":"2026-09-11T09:00:02Z","event":"slot_assigned","slot":0,"task":"%s","from":"fix","to":"fix","agent":"acme","log_path":"%s"}\n' "$task" "$log"
  printf '{"seq":4,"ts":"2026-09-11T09:05:00Z","event":"slot_released","slot":0,"task":"%s","from":"fix","to":"fix","log_path":"%s","outcome":"%s","exit_code":1,"duration_ms":298000}\n' "$task" "$log" "$outcome"
  printf '{"seq":5,"ts":"2026-09-11T09:05:01Z","event":"pass_ended","pass":1,"progressed":%s}\n' "$progressed"
  printf '{"seq":6,"ts":"2026-09-11T09:05:02Z","event":"run_finished","summary":{"agents":1,"terminal":0}}\n'
} > "$root/runtime/events.jsonl"
printf '{"id":"%s","status":"finished"}\n' "$id"
"#;

/// A world watching the forge, with the runtime bound and one recipe. `autorun`
/// is the argument because the ticket has to be written before any sweep runs:
/// `work dispatch` sweeps on its way past, so a case that wants to count the
/// first start writes the ticket with autorun off and turns it on afterwards.
/// Whether a recipe asked to run itself is read from configuration at the sweep,
/// never off the ticket, which is what makes that legal (§FS-005-dispatch.24).
fn watching(autorun: bool) -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    world.stub("acme-runtime", ACME_RUNTIME);
    world.configure(configured(false));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world.ephor().args(["work", "dispatch"]).assert().success();
    assert_eq!(
        starts(&world),
        0,
        "the ticket is written before any run: nothing has swept yet"
    );
    world.configure(configured(autorun));
    world
}

/// The site configuration: one forge-watching project, the runtime bound, and
/// the one recipe every case here uses.
fn configured(autorun: bool) -> serde_json::Value {
    json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": {
            "runner": "acme-runtime",
            "recipes": [{
                "id": "fix-gate",
                "icon": "🛠",
                "description": "fix the red gate",
                "state": "fix",
                "needs_checkout": false,
                "autorun": autorun,
                "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
                "brief": "Fix the gate on {title}."
            }]
        }
    })
}

/// The work root the sweep is about.
fn work_root(world: &World) -> std::path::PathBuf {
    world.forest().join("panta")
}

/// How many runs the runtime has actually been asked to make. The one number
/// the ticket measured, and the one this case measures back.
fn starts(world: &World) -> usize {
    std::fs::read_to_string(world.path().join("starts"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// What the last run on this root did, as the run itself would have said it:
/// `advancing` releases its slot `completed`, the other releases it `failed` and
/// reports no pass progressing.
fn the_next_run_advances(world: &World, advancing: bool) {
    let (outcome, progressed) = match advancing {
        true => ("completed", "true"),
        false => ("failed", "false"),
    };
    std::fs::write(world.path().join("slot-outcome"), outcome).expect("the outcome fixture");
    std::fs::write(world.path().join("pass-progressed"), progressed).expect("the pass fixture");
}

/// One due sweep, as a reading. A sweep that started nothing is still a
/// successful sweep, so the exit code is not what this case reads.
fn sweep(world: &World, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["work", "run", "--due"];
    args.extend_from_slice(extra);
    args.push("--json");
    world
        .ephor_raw()
        .args(&args)
        .output()
        .expect("the sweep runs")
}

/// The reproduction, from the outside. A root whose last run advanced nothing
/// is passed over with the reason in its own row instead of being started again,
/// the rest never refuses a run asked for by name, and a run that advances drops
/// the memory at once (§FS-005-dispatch.24).
#[test]
fn a_root_whose_last_run_advanced_nothing_is_passed_over_and_says_so() {
    let world = watching(true);
    the_next_run_advances(&world, false);

    // One sweep, one run. It reports itself over inside the handshake, so the
    // row is `done` rather than `started` — a run was made either way, which is
    // what the runtime's own record of starts says.
    let first = sweep(&world, &[]);
    assert!(first.status.success(), "{first:?}");
    let first = json_of(&first);
    assert_eq!(first["failed"], 0, "{first:#}");
    assert_eq!(first["runs"][0]["outcome"], "done", "{first:#}");
    assert_eq!(starts(&world), 1, "the first sweep starts a run");

    // The second sweep is the whole ticket. The ticket is still open so the root
    // is still due, the start worked so nothing refused it, and the run it made
    // moved nothing — which is the one fact the sweep now reads.
    let second = sweep(&world, &[]);
    assert!(second.status.success(), "{second:?}");
    let second = json_of(&second);
    assert_eq!(
        starts(&world),
        1,
        "the second sweep started another run on a root whose last run advanced \
         nothing — which is the loop this case exists to end"
    );
    assert_eq!(second["failed"], 0, "{second:#}");
    assert_eq!(
        second["runs"][0]["outcome"], "passed-over",
        "a root passed over for a rest is not a failed launch: {second:#}"
    );
    let why = second["runs"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        why.contains("advanced nothing"),
        "the reason says what was read, not that something was withheld: {why}"
    );
    assert!(
        why.contains("acme-run-1"),
        "the reason names the run it judged, so the reader can go and read it: {why}"
    );

    // And it is said in prose in the same row and the same words, because a
    // reader told only that nothing started goes looking for a full ceiling.
    world
        .ephor()
        .args(["work", "run", "--due"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"))
        .stdout(predicate::str::contains("advanced nothing"));
    assert_eq!(starts(&world), 1, "a reported sweep is still not a sweep");

    // The reader keeps the key. A run asked for by name is never refused
    // because a sweep gave up on the root — and this one advances, which is what
    // the next sweep reads.
    the_next_run_advances(&world, true);
    let named = world
        .ephor_raw()
        .args(["work", "run", "--item", ITEM, "--json"])
        .output()
        .expect("the named run runs");
    assert!(named.status.success(), "{named:?}");
    assert_eq!(
        starts(&world),
        2,
        "the rest refused a run the reader asked for by name"
    );

    // A run that moved something drops the memory whole, so the root is
    // admitted again at once rather than at the end of an interval it no longer
    // deserves.
    let after = sweep(&world, &[]);
    assert!(after.status.success(), "{after:?}");
    let after = json_of(&after);
    assert_eq!(
        after["runs"][0]["outcome"], "done",
        "a run that advanced should have dropped the rest: {after:#}"
    );
    assert_eq!(starts(&world), 3, "the root is admitted again");
}

/// The same skip with no judgement in it: a driver that has worked out for
/// itself which root to leave alone says so, and the sweep names the
/// instruction back rather than quietly doing less
/// (§FS-005-dispatch.24, §FS-011-command-line.9).
#[test]
fn a_root_the_driver_excluded_is_passed_over_and_the_exclusion_is_named() {
    let world = watching(true);
    the_next_run_advances(&world, true);

    let root = work_root(&world).to_string_lossy().into_owned();
    let excluded = sweep(&world, &["--except", &root]);
    assert!(excluded.status.success(), "{excluded:?}");
    let excluded = json_of(&excluded);
    assert_eq!(starts(&world), 0, "an excluded root was started anyway");
    assert_eq!(excluded["failed"], 0, "{excluded:#}");
    assert_eq!(
        excluded["runs"][0]["outcome"], "passed-over",
        "an exclusion is a successful non-launch, not a failure: {excluded:#}"
    );
    let why = excluded["runs"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        why.contains("--except"),
        "naming the instruction back is what tells the driver the flag took \
         effect: {why}"
    );
    assert!(
        why.contains(&root),
        "and it names the value it was given: {why}"
    );

    // A matter id resolves the same way, against the ledger, so a driver that
    // knows which item is stuck need not know where its work root is.
    let by_item = sweep(&world, &["--except", ITEM]);
    assert!(by_item.status.success(), "{by_item:?}");
    let by_item = json_of(&by_item);
    assert_eq!(starts(&world), 0, "an item exclusion started a run anyway");
    assert_eq!(by_item["runs"][0]["outcome"], "passed-over", "{by_item:#}");

    // And with nothing excluded the sweep is the sweep it always was.
    let plain = sweep(&world, &[]);
    assert!(plain.status.success(), "{plain:?}");
    assert_eq!(starts(&world), 1, "the unexcluded sweep started nothing");
}

/// Every value is honoured or refused naming the input it came in on, and a
/// flag that binds one other flag says so rather than parsing and meaning
/// nothing (§FS-011-command-line.9).
#[test]
fn an_exclusion_that_binds_nothing_and_one_that_resolves_to_nothing_are_refused() {
    let world = watching(true);
    let root = work_root(&world).to_string_lossy().into_owned();

    // `--except` narrows a sweep. Without `--due` there is no sweep to narrow,
    // and a flag that parsed and changed nothing would be indistinguishable
    // from one that worked.
    world
        .ephor()
        .args(["work", "run", "--except", &root])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--due"));

    // A value that is neither a work root on disk nor a matter the ledger knows
    // is refused quoting what it held, so the driver learns which of its
    // exclusions was the typo.
    let unknown = world
        .ephor_raw()
        .args(["work", "run", "--due", "--except", "nowhere-at-all"])
        .output()
        .expect("ran");
    assert_eq!(unknown.status.code(), Some(2), "{unknown:?}");
    let said = String::from_utf8_lossy(&unknown.stderr).into_owned();
    assert!(
        said.contains("nowhere-at-all"),
        "a refused value is quoted: {said}"
    );
    assert_eq!(starts(&world), 0, "a refusal happens instead of the work");
}

/// An exclusion is not a narrowing. A sweep that reaches two projects and
/// excludes every root but none of the projects is still a sweep at that width,
/// so it reports and writes nothing without `--act` — and the report says the
/// excluded root would be passed over rather than run
/// (§FS-011-command-line.10).
#[test]
fn an_exclusion_never_narrows_the_width_the_act_gate_is_counted_over() {
    let world = watching(true);
    watch_a_second_project(&world);

    let root = work_root(&world).to_string_lossy().into_owned();
    let held = sweep(&world, &["--except", &root]);
    assert!(held.status.success(), "{held:?}");
    let held = json_of(&held);
    assert_eq!(
        held["gated"],
        json!(true),
        "excluding a root narrowed the width the gate is counted over: {held:#}"
    );
    assert!(
        held["says"].as_str().unwrap_or_default().contains("--act"),
        "a gated report must say the word that acts: {held:#}"
    );
    assert_eq!(
        held["runs"][0]["outcome"], "passed-over",
        "the report says what the sweep would say, exclusions included, rather \
         than promising a run it would not make: {held:#}"
    );
    assert_eq!(starts(&world), 0, "a gated sweep started a run");
    assert!(
        !world.path().join("state/ephor/work.json").exists() || !ledger_names_a_rest(&world),
        "a gated sweep wrote a verdict into the ledger"
    );
}

/// A second watched project, so a bare sweep is a sweep over two. It has no
/// providers and no work: the width the `--act` gate counts is the resolved
/// project set, not the roots inside it (§FS-011-command-line.10).
fn watch_a_second_project(world: &World) {
    let far = world.path().join(OTHER);
    std::fs::create_dir_all(&far).expect("the other forest");

    let mut registry = world.registry_doc();
    let mut row = registry["projects"][0].clone();
    row["id"] = json!(OTHER);
    row["display_name"] = json!("Far");
    row["root"] = json!(far.to_string_lossy());
    registry["projects"]
        .as_array_mut()
        .expect("projects")
        .push(row);
    write_json(&world.registry_path(), &registry);

    let mut settings = configured(true);
    settings["projects"][OTHER] = json!({ "providers": [] });
    world.configure(settings);
}

/// Whether ephor's own record holds a no-advance verdict about any root. Read
/// as text rather than by field name: what the record is called is the
/// implementation's, and what this case is about is that a held report wrote no
/// verdict at all (§FS-011-command-line.10).
fn ledger_names_a_rest(world: &World) -> bool {
    std::fs::read_to_string(world.path().join("state/ephor/work.json"))
        .unwrap_or_default()
        .contains("acme-run-")
}
