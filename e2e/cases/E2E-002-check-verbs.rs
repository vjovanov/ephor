//! E2E-002-check-verbs: a repository says how it is checked, and `ephor check` runs it.
//!
//! The scenario is a repository checking itself, in its own CI or on a laptop:
//! nothing but a forest root, whatever the project put in it, and the manifest
//! sitting there if it wrote one (§FS-006-project-interface.5). No registry, no
//! site configuration, no credentials — which is the property the shipped CI
//! step stands on (§FS-009-shipped-actions.1, §REQ-001-boundary.2).
//!
//! The three verbs are `check` (the aggregate), `style`, and `smoke`, found by
//! well-known name at the root or declared in the manifest under whatever paths
//! the project prefers. What the scenario walks through is the contract around
//! them: which verbs run when none is named, what a verb that parks means, what
//! a verb a workflow named and the project does not have means, and how a smoke
//! that enumerates features answers a matrix.

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;

use support::*;

/// A verb that passes and says one line about itself through the envelope
/// (§FS-006-project-interface.4).
fn passing(summary: &str) -> String {
    answering(serde_json::json!({ "v": 1, "summary": summary }), 0)
}

/// A project that says nothing is checked the way it looks: the well-known
/// names at the forest root are the shipped default binding
/// (§REQ-001-boundary.1, §REQ-001-boundary.2).
#[test]
fn the_well_known_names_are_what_a_project_that_wrote_nothing_down_is_checked_by() {
    let world = World::new();
    world.script("check-style.sh", &passing("42 files, no drift"));
    world.script("smoke-test.sh", &passing("6 scenarios"));

    // No aggregate declared, so everything else the project has runs — and
    // both are named in the output, because which verb answered is the first
    // thing a person reads a gate for.
    world
        .ephor()
        .args(["check", "--root", &world.forest().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ style — 42 files, no drift"))
        .stdout(predicate::str::contains("✓ smoke — 6 scenarios"));

    // With an aggregate on disk, the aggregate is the answer to "am I well":
    // running all three would run the style pass twice, since the aggregate is
    // defined as everything the project considers a check.
    world.script("check.sh", &passing("everything"));
    world
        .ephor()
        .args(["check", "--root", &world.forest().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ check — everything"))
        .stdout(predicate::str::contains("style").not());
}

/// A manifest declares the same three verbs under the paths the project
/// prefers, and outranks the probe (§FS-006-project-interface.1).
#[test]
fn a_manifest_binds_the_verbs_where_the_project_keeps_them() {
    let world = World::new();
    // The probed name exists and is wrong: if it ran, the summary would say so.
    world.script("check.sh", &passing("the probed script"));
    world.script("ci/aggregate.sh", &passing("the declared script"));
    world.script("ci/smoke.sh", &passing("3 features"));
    world.manifest(serde_json::json!({
        "checks": {
            "check": "./ci/aggregate.sh",
            "smoke": { "command": "./ci/smoke.sh", "features": [
                { "id": "retry", "description": "the retry window" },
                { "id": "cache" }
            ] }
        }
    }));

    world
        .ephor()
        .args(["check", "--root", &world.forest().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ check — the declared script"));

    // A smoke that enumerates answers a workflow matrix without being run:
    // the manifest listed the features, so listing them costs nothing
    // (§FS-006-project-interface.5, §FS-009-shipped-actions.1).
    let listed = world
        .ephor()
        .args([
            "check",
            "--root",
            &world.forest().to_string_lossy(),
            "--list-features",
            "--json",
        ])
        .output()
        .expect("list the features");
    assert_eq!(json_of(&listed), serde_json::json!(["retry", "cache"]));

    // And one feature's smoke runs alone, which is what each leg of that
    // matrix does.
    world.script(
        "ci/smoke.sh",
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'smoking feature=%s\\n' \"${1:-all}\"\n",
    );
    world
        .ephor()
        .args([
            "check",
            "--root",
            &world.forest().to_string_lossy(),
            "--verb",
            "smoke",
            "--feature",
            "retry",
        ])
        .assert()
        .success()
        // The verb's own output streams rather than being captured: a gate's
        // log belongs in the log, not in ephor's memory (§AR-002-summons.2).
        .stdout(predicate::str::contains("smoking feature=retry"));
}

/// The two answers a check verb may give that are not "it failed"
/// (§FS-006-project-interface.3): not applicable now, and not there at all.
#[test]
fn a_parked_verb_has_not_failed_and_a_missing_one_is_refused_rather_than_skipped() {
    let world = World::new();
    // Exit 75: not applicable now, ask again later. A CI step that failed on
    // this would be reading the exit code the way the contract exists to stop.
    world.script(
        "check.sh",
        "#!/usr/bin/env bash\necho 'no compiler here today'\nexit 75\n",
    );
    world
        .ephor()
        .args(["check", "--root", &world.forest().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("· check — parked"));

    // A verb a workflow named and the project does not have is refused, not
    // skipped: a check nobody ran that nobody was told about is the failure
    // this rule exists to prevent.
    world
        .ephor()
        .args([
            "check",
            "--root",
            &world.forest().to_string_lossy(),
            "--verb",
            "style",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("declares no 'style' verb"))
        .stderr(predicate::str::contains("check-style.sh"));

    // And a project that declares nothing at all says so as a sentence with
    // the two ways out in it, rather than passing an empty gate.
    let empty = World::new();
    empty
        .ephor()
        .args(["check", "--root", &empty.forest().to_string_lossy()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("declares no check verbs"))
        .stderr(predicate::str::contains("ephor.json"));
}

/// A red verb names itself and hands back what the project said went wrong
/// (§FS-006-project-interface.4).
#[test]
fn a_failing_verb_names_the_job_that_failed() {
    let world = World::new();
    world.script(
        "check.sh",
        &answering(
            serde_json::json!({
                "v": 1,
                "summary": "1 job failed",
                "failures": [{
                    "job": "unit / retry",
                    "trace": "expected 3 attempts, saw 1"
                }]
            }),
            1,
        ),
    );

    world
        .ephor()
        .args(["check", "--root", &world.forest().to_string_lossy()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("✗ check — 1 job failed"))
        .stderr(predicate::str::contains("unit / retry"))
        .stderr(predicate::str::contains("expected 3 attempts, saw 1"))
        .stderr(predicate::str::contains("failed: check"));
}
