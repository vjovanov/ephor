//! `ephor doctor` and `ephor capabilities` (§FS-010-doctor): the ladder made
//! answerable, and the site's health said in an exit code.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::{json, Value};

use common::*;

/// A forge as an executable on PATH, answering the capability set
/// (§FS-001-forge-interface.2).
const FORGE: &str = r#"#!/bin/sh
case "$1" in
  capabilities) printf '{"pull_requests":true}' ;;
  pull-requests)
    printf '[{"id":"acme/widget/1","repo":"acme/widget","number":"1","title":"A change",'
    printf '"url":null,"branch":null,"updated_at":"2026-01-01T00:00:00Z","role":"author",'
    printf '"state":"open","reasons":["authored"],"cited":false,"threads":[]}]' ;;
  *) printf '[]' ;;
esac
"#;

const UNREACHABLE: &str = "#!/bin/sh\necho 'could not resolve host: forge.invalid' >&2\nexit 1\n";

fn ephor(tmp: &Path) -> assert_cmd::Command {
    let mut cmd = ephor_cmd();
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            tmp.join("bin").to_string_lossy(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    cmd.env("XDG_STATE_HOME", tmp.join("state"));
    cmd.env("EPHOR_REGISTRY", tmp.join("workspaces.json"));
    cmd.env("EPHOR_STATUS_CONFIG", tmp.join("status.json"));
    cmd
}

/// A site with two projects: one whose root is on disk, one whose registry row
/// points at a directory that is not there.
fn fixture(tmp: &Path, providers: Value) {
    fixture_with_work(tmp, providers, json!({}));
}

/// The same site, with the work configuration said explicitly — the roster
/// tests bind their own runner so the machine's does not answer for them.
fn fixture_with_work(tmp: &Path, providers: Value, work: Value) {
    let bin = tmp.join("bin");
    fs::create_dir_all(&bin).unwrap();
    make_executable(&bin.join("ephor-forge-demo"), FORGE);
    make_executable(&bin.join("ephor-forge-dead"), UNREACHABLE);
    fs::create_dir_all(tmp.join("project")).unwrap();

    let template = write_template(tmp);
    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                {
                    "id": "widget",
                    "type": "monorepo",
                    "display_name": "Widget",
                    "root": tmp.join("project").to_string_lossy(),
                    "main_branch": "main"
                },
                {
                    "id": "ghost",
                    "type": "monorepo",
                    "display_name": "Ghost",
                    "root": tmp.join("nowhere").to_string_lossy(),
                    "main_branch": "main"
                }
            ]
        }),
    );
    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 0, "provider_timeout_seconds": 10 },
            "work": work,
            "projects": {
                "widget": { "providers": providers },
                "ghost": { "providers": [{ "provider": "demo" }] }
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

/// The ladder is answerable on its own, without a sweep and without asking a
/// forge anything (§FS-010-doctor.2).
#[test]
fn capabilities_prints_one_projects_rungs_and_why_the_missing_ones_are_missing() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "demo" }]));

    ephor(tmp.path())
        .args(["capabilities", "widget"])
        .assert()
        .success()
        // Held, and missing with the sentence the menu refuses with.
        .stdout(predicate::str::contains("observable"))
        .stdout(predicate::str::contains("branch_root_template"));

    // The alias is the short one a reader types.
    ephor(tmp.path())
        .args(["caps", "widget"])
        .assert()
        .success();

    // An unknown project is refused by name rather than answering emptily.
    ephor(tmp.path())
        .args(["capabilities", "nonesuch"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nonesuch"));
}

/// `--registry` names the registry the answer is about, on every subcommand
/// that offers it (§FS-001-forge-interface.5).
///
/// It is a global flag, but the subcommands that resolve the registry for
/// themselves used to read the configured one regardless — so the answer came
/// back about a file the reader had not named, wearing the label of the one
/// they had. Every test in this tree drives ephor through `EPHOR_REGISTRY`,
/// which is exactly why nobody caught it; this one uses the flag, and points
/// the environment somewhere else so that agreeing by accident is not
/// available.
#[test]
fn the_registry_flag_names_the_registry_the_answer_is_about() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "demo" }]));

    // A second registry, differing in one visible rung: `widget` here has the
    // branch workspaces it does not have in the first.
    let template = write_template(tmp.path());
    let other = tmp.path().join("other.json");
    write_registry(
        &other,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                {
                    "id": "widget",
                    "type": "monorepo",
                    "display_name": "Widget",
                    "root": tmp.path().join("project").to_string_lossy(),
                    "branch_root_template": "{project_root}/{branch}",
                    "main_branch": "main"
                }
            ]
        }),
    );

    // The environment still names the first registry, where the rung is
    // missing; the flag names the second, where it is held. The reason
    // sentence is what tells them apart — it is printed only when the rung is
    // missing, so its absence is the answer coming from the file that was
    // named.
    let named = other.to_string_lossy().into_owned();
    ephor(tmp.path())
        .args(["capabilities", "widget", "--registry", &named])
        .assert()
        .success()
        .stdout(predicate::str::contains("branch-addressable"))
        .stdout(predicate::str::contains("branch_root_template").not());

    // Before the subcommand is the same statement, and clap accepts both.
    ephor(tmp.path())
        .args(["--registry", &named, "capabilities", "widget"])
        .assert()
        .success()
        .stdout(predicate::str::contains("branch_root_template").not());

    // And with no flag, the configured registry is still what answers.
    ephor(tmp.path())
        .args(["capabilities", "widget"])
        .assert()
        .success()
        .stdout(predicate::str::contains("branch_root_template"));
}

/// The rung reasons `capabilities` prints are the capability table's own —
/// doctor composes and never judges (§FS-010-doctor.1). If these two ever
/// disagree, a reader holding both has neither.
#[test]
fn the_ladder_and_the_diagnosis_give_the_same_sentence() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "demo" }]));
    // Against the same world: `doctor` refreshes and `capabilities` reads the
    // cache, so a project nobody has refreshed is a different world rather
    // than a different opinion.
    ephor(tmp.path())
        .args(["refresh", "widget"])
        .assert()
        .success();

    let ladder = ephor(tmp.path())
        .args(["capabilities", "widget", "--json"])
        .output()
        .unwrap();
    let ladder: Value = serde_json::from_slice(&ladder.stdout).unwrap();

    let diagnosed = ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "widget", "--json"])
        .output()
        .unwrap();
    let diagnosed: Value = serde_json::from_slice(&diagnosed.stdout).unwrap();

    assert_eq!(
        ladder["projects"][0]["missing"],
        diagnosed["projects"][0]["missing"]
    );
    assert_eq!(
        ladder["projects"][0]["held"],
        diagnosed["projects"][0]["held"]
    );
    // And the roster both print is the same one (§FS-005-dispatch.14).
    assert_eq!(ladder["roster"], diagnosed["roster"]);
}

/// A refresh nobody has run is not every source having failed. The two ask
/// different things of the reader — one is a command to run, the other is a
/// configuration or a network to look at — so the ladder says which
/// (§FS-006-project-interface.10).
#[test]
fn a_project_nobody_has_refreshed_says_so_rather_than_claiming_silence() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "demo" }]));

    let out = ephor(tmp.path())
        .args(["capabilities", "widget", "--json"])
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows["projects"][0]["sources"]["answering"], Value::Null);
    let why = rows["projects"][0]["missing"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["rung"] == "observable")
        .expect("observable is missing before anything has been asked");
    assert!(
        why["why"]
            .as_str()
            .unwrap()
            .contains("no refresh has produced a cache"),
        "{why}"
    );

    // Once one has, the rung holds and the sentence is gone.
    ephor(tmp.path())
        .args(["refresh", "widget"])
        .assert()
        .success();
    let out = ephor(tmp.path())
        .args(["capabilities", "widget", "--json"])
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows["projects"][0]["sources"]["answering"], 1);
    assert!(rows["projects"][0]["held"]
        .as_array()
        .unwrap()
        .contains(&json!("observable")));
}

/// Asking every source of every project takes as long as the slowest forge,
/// and a diagnostic that prints nothing until it finishes is one a reader
/// kills half way through and reports as hung (§FS-010-doctor.3). It narrates
/// on the error stream, so what a program reads stays the report.
#[test]
fn it_says_what_it_is_doing_while_it_does_it_without_saying_it_to_a_parser() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "demo" }]));

    let out = ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "widget"])
        .output()
        .unwrap();
    let narration = String::from_utf8_lossy(&out.stderr);
    // The project is announced before it is asked, and answered when it comes
    // back — so a slow forge is a line already on screen rather than silence.
    assert!(narration.contains("widget"), "{narration}");
    assert!(narration.contains("item(s)"), "{narration}");

    // And under --json none of it reaches stdout, which is the contract a
    // timer parses.
    let out = ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "widget", "--json"])
        .output()
        .unwrap();
    serde_json::from_slice::<Value>(&out.stdout).expect("stdout is the report and nothing else");
    assert!(
        out.stderr.is_empty(),
        "narration under --json: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The self pass narrates by being incremental rather than by describing
/// itself twice: each check is its own line as it finishes, so the progress
/// and the report are one thing (§FS-010-doctor.3).
#[test]
fn the_self_pass_reports_each_check_as_it_finishes() {
    let out = ephor_cmd()
        .args(["doctor", "--self-only"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&out.stdout);
    assert!(report.contains("ephor itself"), "{report}");
    // Ten checks, each on its own line, and none of them said twice.
    let ticked = report.matches("  ✓ ").count();
    assert!(ticked >= 10, "only {ticked} checks reported: {report}");
    assert!(
        !report.contains("walking the seams"),
        "the report narrated itself as well: {report}"
    );
}

/// A source that did not answer is named, split into the kind of not-answered
/// it was, and makes the run exit `4` (§FS-001-forge-interface.6,
/// §FS-010-doctor.5).
#[test]
fn a_lost_source_is_named_with_its_kind_and_the_run_exits_degraded() {
    let tmp = tempdir();
    fixture(
        tmp.path(),
        json!([{ "provider": "demo" }, { "provider": "dead" }]),
    );

    ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "widget"])
        .assert()
        .code(4)
        .stdout(predicate::str::contains("dead: unreachable"))
        .stdout(predicate::str::contains("degraded"));

    let out = ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "widget", "--json"])
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows["projects"][0]["health"], "degraded");
    assert_eq!(rows["projects"][0]["sources"]["answering"], 1);
    assert_eq!(rows["projects"][0]["sources"]["configured"], 2);
    // Asked, not configured: a project's sources are what wrote a slot, which
    // includes a probed task store and a site-level shared source. Reporting
    // the configured count as the denominator said more answered than were
    // asked (§FS-006-project-interface.7).
    assert_eq!(rows["projects"][0]["sources"]["asked"], 2);
    // The distinction is the whole point: a network to wait out asks nothing
    // of the reader, where every other failure sends them to change something.
    assert_eq!(rows["projects"][0]["silent"][0]["unreachable"], true);
}

/// Losing every source is its own condition, not a worse degrade
/// (§FS-010-doctor.5).
#[test]
fn losing_every_source_is_unreachable_rather_than_degraded() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "dead" }]));

    ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "widget"])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("unreachable"));
}

/// A registry row naming a root that is not there is a fault, not a shape:
/// it is a row nobody updated after a directory moved, and it is exactly the
/// quiet rot a weekly run exists to find (§FS-010-doctor).
#[test]
fn a_root_that_is_not_on_disk_is_a_fault_and_leads_the_reasons() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "demo" }]));

    ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "ghost"])
        .assert()
        .code(4)
        .stdout(predicate::str::contains("is not on disk"));

    // And the project whose root is there, watched by a source that answers,
    // is well — the rungs it simply does not have are not faults.
    ephor(tmp.path())
        .args(["doctor", "--skip-self", "--project", "widget"])
        .assert()
        .success()
        .stdout(predicate::str::contains("well"));
}

/// The self pass reaches no forge and reads nothing of the reader's, so it is
/// the half that works on a machine with no site at all (§FS-010-doctor.3).
/// This is also the check that catches ephor being wrong rather than the world
/// being away, which is why it is its own exit code.
#[test]
fn the_self_pass_walks_the_seams_against_a_site_it_made_up() {
    // No registry, no configuration, no PATH of ours: what `--self-only`
    // promises is that none of that is read.
    let mut cmd = ephor_cmd();
    cmd.env_remove("EPHOR_REGISTRY");
    cmd.env_remove("EPHOR_STATUS_CONFIG");
    cmd.env("XDG_STATE_HOME", "/nonexistent-doctor-state");

    let out = cmd
        .args(["doctor", "--self-only", "--json"])
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["passed"],
        true,
        "the self pass failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(out.status.code(), Some(0));

    // Every seam the spec names is walked, not just the cheap ones.
    let names: Vec<String> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| check["check"].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "the published schemas print",
        "a forge out of process answers a refresh",
        "a source that fails is reported, not silent",
        "a branch workspace is checked out",
        "a trailing branch is replayed",
        "an item becomes a ticket, and the ledger reads it back",
        "a task store is read where it lives",
    ] {
        assert!(names.iter().any(|name| name == expected), "{names:?}");
    }
}

/// Nothing of the reader's is written (§FS-010-doctor.4): the self pass makes
/// a temporary place and takes it away again.
#[test]
fn the_self_pass_leaves_nothing_behind() {
    let tmp = tempdir();
    fixture(tmp.path(), json!([{ "provider": "demo" }]));
    let state = tmp.path().join("state");
    // The scratch site goes wherever temporary things go, so this run gets a
    // temporary place of its own — otherwise the assertion below reads every
    // other test's scratch site as this one's leftovers.
    let scratch = tmp.path().join("tmpdir");
    fs::create_dir_all(&scratch).unwrap();

    ephor(tmp.path())
        .env("TMPDIR", &scratch)
        .args(["doctor", "--self-only"])
        .assert()
        .success();

    // The reader's state directory was never touched: no feed cache, no
    // ledger, nothing.
    assert!(
        !state.exists(),
        "the self pass wrote into the reader's state"
    );
    // And what it made for itself is gone.
    let leftovers: Vec<_> = fs::read_dir(&scratch).unwrap().flatten().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// The roster is reportable before anything depends on it
/// (§FS-005-dispatch.14): each hand, what it resolves to, and why an
/// unavailable one is unavailable — shown with its reason, never hidden.
#[test]
fn the_roster_prints_each_hand_and_why_an_unavailable_one_is_unavailable() {
    let tmp = tempdir();
    fixture_with_work(
        tmp.path(),
        json!([{ "provider": "demo" }]),
        json!({ "runner": "runner-of-ours" }),
    );
    // A PATH of exactly this test's making: the runner and one agent's
    // command are there, every other agent's is not — so which hand is
    // available is this fixture's fact, not the machine's.
    let bin = tmp.path().join("bin");
    make_executable(&bin.join("runner-of-ours"), "#!/bin/sh\nexit 0\n");
    make_executable(&bin.join("claude"), "#!/bin/sh\nexit 0\n");
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();

    let out = ephor(tmp.path())
        .env("PATH", bin.to_string_lossy().to_string())
        .env("HOME", &home)
        .args(["capabilities", "widget"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report = String::from_utf8_lossy(&out.stdout);
    // The section names the bound runner, since the roster is its knowledge.
    assert!(
        report.contains("who can be asked (runner-of-ours)"),
        "{report}"
    );
    // A hand whose agent is there resolves, with its efforts beside it.
    assert!(report.contains("✓ claude-code"), "{report}");
    assert!(report.contains("its own default model"), "{report}");
    assert!(report.contains("efforts: yolo"), "{report}");
    // One whose agent is not stays on the list with the computed reason.
    assert!(report.contains("✗ codex"), "{report}");
    assert!(report.contains("codex is not on PATH"), "{report}");

    // The same roster, as the JSON a program reads.
    let out = ephor(tmp.path())
        .env("PATH", bin.to_string_lossy().to_string())
        .env("HOME", &home)
        .args(["capabilities", "widget", "--json"])
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows["roster"]["runner"], "runner-of-ours");
    assert_eq!(rows["roster"]["refusal"], Value::Null);
    let hands = rows["roster"]["hands"].as_array().unwrap();
    let by_id = |id: &str| {
        hands
            .iter()
            .find(|hand| hand["id"] == id)
            .unwrap_or_else(|| panic!("no hand '{id}': {hands:?}"))
    };
    assert_eq!(by_id("claude-code")["available"], true);
    assert_eq!(by_id("claude-code")["efforts"], json!(["yolo"]));
    assert_eq!(by_id("codex")["available"], false);
    assert_eq!(by_id("codex")["why"], "codex is not on PATH");
}

/// The degrade (§FS-005-dispatch.14): with the bound runtime not on `PATH`
/// the roster is empty and says so in the workable rung's own words — the
/// very sentence the rung shows — and every other rung still resolves, so a
/// site nobody hands work to is diagnosed, not broken.
#[test]
fn a_site_with_no_runtime_still_resolves_every_other_rung() {
    let tmp = tempdir();
    fixture_with_work(
        tmp.path(),
        json!([{ "provider": "demo" }]),
        json!({ "runner": "no-such-runtime-anywhere" }),
    );

    let out = ephor(tmp.path())
        .args(["capabilities", "widget"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report = String::from_utf8_lossy(&out.stdout);
    // The empty roster is said, in the workable rung's words, naming the
    // bound runner rather than the shipped one.
    assert!(report.contains("who can be asked"), "{report}");
    assert!(
        report.contains("nobody — no-such-runtime-anywhere is not on PATH"),
        "{report}"
    );
    // And the ladder is still the ladder: every other rung resolved.
    assert!(report.contains("observable"), "{report}");

    let out = ephor(tmp.path())
        .args(["capabilities", "widget", "--json"])
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows["roster"]["hands"], json!([]));
    // One sentence, twice shown: the roster's refusal is the workable rung's
    // reason, because two sentences would drift (§FS-010-doctor.1).
    let workable = rows["projects"][0]["missing"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["rung"] == "workable")
        .expect("the workable rung is missing with the runner gone");
    assert_eq!(rows["roster"]["refusal"], workable["why"]);
    // Rungs that owe nothing to the runtime still hold.
    assert!(rows["projects"][0]["held"]
        .as_array()
        .unwrap()
        .contains(&json!("placed")));
}
