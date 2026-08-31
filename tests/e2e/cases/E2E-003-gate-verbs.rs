//! E2E-003-gate-verbs: a project with its own CI answers status, failures, and restart.
//!
//! The scenario is a project whose gate is not the forge's: an internal CI that
//! only its own commands can ask. It writes three of them into its manifest,
//! and from there nothing above the seam can tell the difference between it and
//! a forge-hosted gate — the same counts per repository, the same failures, the
//! same restart (§FS-006-project-interface.6). A forge-hosted gate, meanwhile,
//! needs no manifest at all: the provider's own gate capability is the shipped
//! default binding (§REQ-001-boundary.1).
//!
//! This case is written at the seam rather than at a command, because the seam
//! is where the gate verbs are reached today: `ephor failures` asks the source
//! that reported the item (§FS-004-quick-actions.4), and a manifest-bound gate
//! has no command line of its own yet. Everything below runs the project's real
//! scripts in a real forest — what a surface would do, one call earlier.

#[path = "../support.rs"]
mod support;

use std::time::Duration;

use ephor::capabilities::{Bindings, CapabilitySet, Rung};
use ephor::manifest::{Manifest, Trust};
use ephor::seams::gate::{self, Bound, Restarted, Verb};
use ephor::seams::summons::Site;

use support::*;

const TIMEOUT: Duration = Duration::from_secs(30);

/// The manifest a project with an internal gate writes: three commands, one
/// per verb, the failures one running in a repository of the forest.
fn gated(world: &World) {
    world.manifest(serde_json::json!({
        "ci": {
            "status": "./ci/gate-status.sh",
            "failures": "./ci/gate-failures.sh",
            "restart": "./ci/gate-restart.sh"
        }
    }));
    // status: the gate's counts per repository of the forest — one change may
    // gate across a tree, and a single number could not say which repository
    // went red (§AR-004-forest.1).
    world.script(
        "ci/gate-status.sh",
        &answering(
            serde_json::json!({
                "v": 1,
                "gate": {
                    "repos": [
                        { "repo": "app", "passed": 12, "failed": 1, "running": 0 },
                        { "repo": "plugins", "passed": 4, "failed": 0, "running": 2 }
                    ],
                    "blocked": true,
                    "blockers": ["awaiting release manager"]
                }
            }),
            0,
        ),
    );
    // failures: the expensive question, and the one that carries the dossier
    // the caller handed over — here, the pull request number.
    world.script(
        "ci/gate-failures.sh",
        "#!/usr/bin/env bash\nset -euo pipefail\ncat > \"$EPHOR_ANSWER\" <<ENVELOPE\n\
         { \"v\": 1, \"failures\": [ { \"job\": \"integration #$EPHOR_NUMBER\", \"repo\": \"app\",\n\
         \"trace\": \"retry_window_test: expected 3 attempts, saw 1\" } ] }\nENVELOPE\n",
    );
    // restart: exit 75 is "still running, ask again later"; the default here
    // is a restart that was accepted.
    world.script(
        "ci/gate-restart.sh",
        "#!/usr/bin/env bash\nexit \"${GATE_RESTART_EXIT:-0}\"\n",
    );
}

fn manifest_of(world: &World) -> Manifest {
    Manifest::read(&world.forest(), Trust::Full)
        .expect("the manifest reads")
        .expect("the project wrote one")
}

#[test]
fn a_project_with_an_internal_gate_answers_all_three_verbs_from_its_own_commands() {
    let world = World::new();
    gated(&world);
    let manifest = manifest_of(&world);
    let root = world.forest();

    // A bound command outranks the forge, which is what lets an internal gate
    // be indistinguishable from a hosted one above the seam
    // (§FS-006-project-interface.1).
    let status = gate::bind(Verb::Status, Some(&manifest), None, true).expect("status is bound");
    assert!(matches!(status, Bound::Command { .. }));

    let answer = gate::run(
        &status,
        Verb::Status,
        &Site::root(&root),
        Vec::new(),
        TIMEOUT,
    )
    .expect("the status verb runs")
    .expect("a command answers by running");
    let reported = gate::status_of(&answer).expect("the answer carried a gate");
    assert_eq!(reported.repos.len(), 2);
    assert_eq!(reported.repos[0].repo, "app");
    assert_eq!(reported.repos[0].failed, 1);
    assert_eq!(reported.repos[1].running, 2);
    // What is blocking is the project's to say, and it says it in words.
    assert!(reported.blocked);
    assert_eq!(
        reported.blockers,
        vec!["awaiting release manager".to_string()]
    );

    // failures: asked on demand, and carrying the matter it is about
    // (§FS-006-project-interface.3 — environment in, envelope out).
    let failures = gate::bind(Verb::Failures, Some(&manifest), None, true).expect("failures bound");
    let answer = gate::run(
        &failures,
        Verb::Failures,
        &Site::root(&root),
        vec![("EPHOR_NUMBER".to_string(), "101".to_string())],
        TIMEOUT,
    )
    .expect("the failures verb runs")
    .expect("a command answers by running");
    let found = gate::failures_of(&answer);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].job, "integration #101");
    assert_eq!(found[0].repo.as_deref(), Some("app"));
    assert!(found[0]
        .trace
        .as_deref()
        .expect("the failure carried a trace")
        .contains("expected 3 attempts"));
}

/// Restarting commits nothing — the change was never the problem — so the only
/// answers are "asked for", "still running", and "refused"
/// (§FS-005-dispatch.11).
#[test]
fn a_restart_is_asked_for_parked_or_refused_and_never_endless() {
    let world = World::new();
    gated(&world);
    let manifest = manifest_of(&world);
    let root = world.forest();
    let restart = gate::bind(Verb::Restart, Some(&manifest), None, true).expect("restart is bound");

    let ask = |exit: &str| {
        gate::run(
            &restart,
            Verb::Restart,
            &Site::root(&root),
            vec![("GATE_RESTART_EXIT".to_string(), exit.to_string())],
            TIMEOUT,
        )
        .expect("the restart verb runs")
        .expect("a command answers by running")
    };

    assert_eq!(gate::restarted(&ask("0")), Restarted::Asked);
    // 75 is the one outcome a retry loop must not treat as a failure.
    assert_eq!(gate::restarted(&ask("75")), Restarted::Parked);
    match gate::restarted(&ask("2")) {
        Restarted::Refused(reason) => assert!(reason.contains("failed"), "{reason}"),
        other => panic!("a refusing gate is not {other:?}"),
    }

    // And the asking is bounded: past the limit the infrastructure itself is
    // the thing that is wrong, and the work stops for a person.
    assert!(gate::may_restart(0));
    assert!(!gate::may_restart(gate::RESTART_LIMIT));
}

/// The shipped default and the rung it buys (§FS-006-project-interface.6,
/// §FS-006-project-interface.10): a forge that reports a gate needs no manifest,
/// and a manifest that binds one needs no forge.
#[test]
fn a_forge_gate_needs_no_manifest_and_a_bound_gate_needs_no_forge() {
    let world = World::new();

    // Nothing written down, and the source reports a gate: that is the binding.
    assert_eq!(
        gate::bind(Verb::Status, None, None, true),
        Some(Bound::Forge)
    );
    // Nothing written down and no source reporting one: nothing fills it —
    // which is the *gated* rung not holding, not an error.
    assert_eq!(gate::bind(Verb::Status, None, None, false), None);

    let placement = ephor::branches::Placement::load(&world.registry_doc(), PROJECT)
        .expect("the registry describes the project");
    let ungated = CapabilitySet::resolve(
        PROJECT,
        Some(&placement),
        &Bindings {
            sources: 1,
            gate_reported: false,
            ..Bindings::default()
        },
    );
    assert!(!ungated.holds(Rung::Gated));
    assert!(ungated
        .reason(Rung::Gated)
        .expect("a missing rung says why")
        .contains("no gate verbs are bound"));

    // The project writes its three commands down, and the rung holds without
    // any source having reported anything.
    gated(&world);
    let manifest = manifest_of(&world);
    let bound = CapabilitySet::resolve(
        PROJECT,
        Some(&placement),
        &Bindings {
            sources: 1,
            gate_reported: false,
            manifest: Some(&manifest),
            ..Bindings::default()
        },
    );
    assert!(bound.holds(Rung::Gated));
}

/// A row that trusts the checkout to describe itself but not to run anything
/// keeps the identity and drops the commands (§FS-006-project-interface.2).
#[test]
fn a_narrowed_row_reads_the_manifest_and_runs_none_of_it() {
    let world = World::new();
    gated(&world);
    let narrowed = Manifest::read(&world.forest(), Trust::Descriptions)
        .expect("the manifest reads")
        .expect("the project wrote one");
    assert_eq!(gate::bind(Verb::Status, Some(&narrowed), None, false), None);
    // And with the forge reporting one, the gate is still answered — by the
    // forge, which no checkout had to be trusted for.
    assert_eq!(
        gate::bind(Verb::Status, Some(&narrowed), None, true),
        Some(Bound::Forge)
    );
}
