//! `ephor check` end to end (§FS-006-project-interface.5): a project's own
//! verbs, run from its checkout alone.
//!
//! This is what the shipped CI step stands on
//! (§FS-009-shipped-actions.1), so every case here runs with no registry, no
//! site configuration, and nothing on `PATH` that a repository did not bring
//! — which is the whole claim being made about it.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::json;

use common::*;

fn script(root: &Path, name: &str, body: &str) {
    make_executable(&root.join(name), &format!("#!/bin/sh\n{body}\n"));
}

fn manifest(root: &Path, value: serde_json::Value) {
    fs::write(
        root.join("ephor.json"),
        serde_json::to_string_pretty(&value).unwrap(),
    )
    .unwrap();
}

/// A project that carries the well-known name and nothing else is checkable:
/// no manifest, no registry, no configuration (§REQ-001-boundary.2).
#[test]
fn a_well_known_script_is_the_whole_declaration() {
    let tmp = tempdir();
    script(tmp.path(), "check.sh", "echo 'the aggregate ran'");

    ephor_cmd()
        .args(["check", "--root", &tmp.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ check"));
}

/// A verb that failed fails the step, and says which one: a red gate is read
/// first for what broke.
#[test]
fn a_failing_verb_fails_the_step_and_is_named() {
    let tmp = tempdir();
    script(tmp.path(), "check.sh", "echo 'boom' >&2\nexit 3");

    ephor_cmd()
        .args(["check", "--root", &tmp.path().to_string_lossy()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("✗ check: failed (3)"))
        .stderr(predicate::str::contains("failed: check"));
}

/// The envelope's summary is what a reader sees, rather than the exit code
/// alone (§FS-006-project-interface.4).
#[test]
fn what_a_verb_says_in_the_envelope_is_what_is_reported() {
    let tmp = tempdir();
    script(
        tmp.path(),
        "check.sh",
        "cat > \"$EPHOR_ANSWER\" <<'JSON'\n\
         {\"v\":1,\"summary\":\"3 suites, 1 failed\",\"failures\":[{\"job\":\"style\"}]}\nJSON\nexit 1",
    );

    ephor_cmd()
        .args(["check", "--root", &tmp.path().to_string_lossy()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("✗ check — 3 suites, 1 failed"))
        .stderr(predicate::str::contains("style"));
}

/// Exit 75 is "not applicable now, ask again later"
/// (§FS-006-project-interface.3) — a step that failed on it would read the
/// exit code the way this contract exists to stop.
#[test]
fn a_verb_that_parks_has_not_failed() {
    let tmp = tempdir();
    script(tmp.path(), "check.sh", "exit 75");

    ephor_cmd()
        .args(["check", "--root", &tmp.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("parked"));
}

/// The manifest outranks the probe, and its `cwd` puts the verb in the
/// repository of the forest it named (§FS-006-project-interface.1,
/// §AR-002-summons.1).
#[test]
fn the_manifest_outranks_the_probe_and_places_the_verb() {
    let tmp = tempdir();
    script(tmp.path(), "check.sh", "echo 'the probed one'");
    fs::create_dir_all(tmp.path().join("ce")).unwrap();
    manifest(
        tmp.path(),
        json!({ "checks": { "check": { "command": "pwd", "cwd": "repo:ce" } } }),
    );

    ephor_cmd()
        .args(["check", "--root", &tmp.path().to_string_lossy()])
        .assert()
        .success()
        // The declared command ran, in the repository it named.
        .stdout(predicate::str::contains("/ce"))
        .stdout(predicate::str::contains("the probed one").not());
}

/// The aggregate is everything the project considers a check, so it runs
/// alone; a project that declares no aggregate runs what it does declare.
#[test]
fn the_aggregate_runs_alone_and_the_rest_run_where_there_is_none() {
    let tmp = tempdir();
    script(tmp.path(), "check.sh", "echo 'aggregate'");
    script(tmp.path(), "check-style.sh", "echo 'style'");
    ephor_cmd()
        .args(["check", "--root", &tmp.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("aggregate"))
        .stdout(predicate::str::contains("✓ style").not());

    fs::remove_file(tmp.path().join("check.sh")).unwrap();
    script(tmp.path(), "smoke-test.sh", "echo 'smoke'");
    ephor_cmd()
        .args(["check", "--root", &tmp.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ style"))
        .stdout(predicate::str::contains("✓ smoke"));
}

/// A verb a workflow named and ephor could not fill is refused: a check
/// nobody ran that nobody was told about is worse than a red step.
#[test]
fn a_verb_that_was_asked_for_and_is_not_there_is_refused() {
    let tmp = tempdir();
    script(tmp.path(), "check.sh", "true");

    ephor_cmd()
        .args([
            "check",
            "--root",
            &tmp.path().to_string_lossy(),
            "--verb",
            "smoke",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("declares no 'smoke' verb"));

    ephor_cmd()
        .args([
            "check",
            "--root",
            &tmp.path().to_string_lossy(),
            "--verb",
            "vibes",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown verb 'vibes'"));

    // And a project that declares nothing at all says so, naming both ways to
    // declare one.
    let empty = tempdir();
    ephor_cmd()
        .args(["check", "--root", &empty.path().to_string_lossy()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("declares no check verbs"))
        .stderr(predicate::str::contains("ephor.json"));
}

/// Features enumerate for the matrix a workflow fans out over
/// (§FS-006-project-interface.5): a static list, an asked one, or none —
/// which is a complete answer and prints an empty list.
#[test]
fn features_enumerate_for_a_matrix_or_answer_the_empty_list() {
    let listed = tempdir();
    manifest(
        listed.path(),
        json!({ "checks": { "smoke": { "command": "./smoke-test.sh",
                                       "features": [{ "id": "reflection" },
                                                    { "id": "resources",
                                                      "description": "resource bundles" }] } } }),
    );
    ephor_cmd()
        .args([
            "check",
            "--root",
            &listed.path().to_string_lossy(),
            "--list-features",
            "--json",
        ])
        .assert()
        .success()
        .stdout("[\"reflection\",\"resources\"]\n");

    // Asked: the command prints one id per line, which is a complete answer
    // and needs no artifact.
    let asked = tempdir();
    script(asked.path(), "smoke-test.sh", "printf 'jfr\\nheap\\n'");
    manifest(
        asked.path(),
        json!({ "checks": { "smoke": { "command": "./smoke-test.sh", "features": "list" } } }),
    );
    ephor_cmd()
        .args([
            "check",
            "--root",
            &asked.path().to_string_lossy(),
            "--list-features",
            "--json",
        ])
        .assert()
        .success()
        .stdout("[\"jfr\",\"heap\"]\n");

    // One feature's smoke alone, which is what a matrix leg runs.
    ephor_cmd()
        .args([
            "check",
            "--root",
            &asked.path().to_string_lossy(),
            "--verb",
            "smoke",
            "--feature",
            "jfr",
        ])
        .assert()
        .success();

    // A smoke that is one opaque verb enumerates nothing.
    let opaque = tempdir();
    script(opaque.path(), "smoke-test.sh", "true");
    ephor_cmd()
        .args([
            "check",
            "--root",
            &opaque.path().to_string_lossy(),
            "--list-features",
            "--json",
        ])
        .assert()
        .success()
        .stdout("[]\n");
}

/// A committed registry is held to the published schema and nothing else: the
/// checkouts its rows name are on somebody's machine, not in CI
/// (§FS-009-shipped-actions.1).
#[test]
fn a_committed_registry_is_held_to_the_schema_without_its_checkouts() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let registry = tmp.path().join("workspaces.json");
    write_registry(
        &registry,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [{
                "id": "demo",
                "type": "monorepo",
                "display_name": "Demo",
                // Nowhere on this machine, which is the point: the file is
                // what CI can check.
                "root": "/nowhere/that/exists",
                "main_branch": "main",
                "branches": [{ "id": "demo-main", "branch": "main", "active": true }]
            }]
        }),
    );

    ephor_cmd()
        .args([
            "--registry",
            &registry.to_string_lossy(),
            "validate",
            "--schema-only",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Validated"));

    // Without it, validation is about the machine as well, and says so.
    ephor_cmd()
        .args(["--registry", &registry.to_string_lossy(), "validate"])
        .assert()
        .failure();

    // A registry the schema refuses is refused either way.
    fs::write(&registry, "{\"projects\": [{\"id\": 7}]}").unwrap();
    ephor_cmd()
        .args([
            "--registry",
            &registry.to_string_lossy(),
            "validate",
            "--schema-only",
        ])
        .assert()
        .failure();
}
