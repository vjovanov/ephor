//! E2E-010-restart: a gate is asked to run again, at the scope the reader meant.
//!
//! The scenario is the move a red gate most often needs, which is not a fix
//! (§FS-004-quick-actions.9). A runner died, a mirror was unreachable, the
//! same flake landed on the same job again: nothing about the change is wrong
//! and what it needs is another run. What this case holds ephor to is that the
//! *scope* survives the whole way down — the cheap re-run and the expensive
//! one are different asks, and only the caller knows which was meant, so
//! nothing between the key and the forge may widen one into the other.
//!
//! It is proven against a forge ephor never heard of, because that is where
//! every part of the path is visible at once (§FS-001-forge-interface.2): the
//! capability is declared or it is not, the scope crosses as a word in the
//! request, and what comes back — how many jobs, and what could not be
//! restarted — is what the reader is told. A gate is minutes away from saying
//! anything itself, so a restart that reported only *done* would be
//! indistinguishable from one that found nothing to do.
//!
//! Which of the two entries a row is offered is a question about a menu rather
//! than about a forge, so it is pinned where menus are — beside the providers
//! that build them (§FS-004-quick-actions.2).

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;

use support::*;

/// A forge that can restart. It records the request it was handed, so the case
/// can read back exactly what crossed the seam rather than inferring it from
/// what happened afterwards.
const RESTARTING_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
request="$(cat)"
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"gate":true,"failures":true,"restart":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/101", "repo": "app", "number": "101",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/101",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open",
        "gate": { "repos": [ { "repo": "app", "passed": 7, "failed": 2, "running": 0 } ] } },
      { "id": "app/102", "repo": "app", "number": "102",
        "title": "Green as grass",
        "url": "https://acme.example/pr/102",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open",
        "gate": { "repos": [ { "repo": "app", "passed": 9, "failed": 0, "running": 0 } ] } },
      { "id": "app/103", "repo": "app", "number": "103",
        "title": "No gate at all",
        "url": "https://acme.example/pr/103",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open" }
    ]'
    ;;
  restart)
    printf '%s' "$request" > "$ACME_ASKED"
    scope="$(printf '%s' "$request" | sed -n 's/.*"scope":"\([a-z]*\)".*/\1/p')"
    if [ "$scope" = all ]; then
      # A forge that takes the whole-gate start and does not count what it
      # scheduled — the ordinary shape for a gate executed asynchronously.
      printf '{"note":"gate start accepted; it runs elsewhere"}'
    else
      printf '{"asked":2,"skipped":["legal-sign-off is not a job this forge runs"]}'
    fi
    ;;
  *) printf '[]' ;;
esac
"#;

/// A forge that reports a gate and cannot re-run it — the ordinary case, not
/// a broken one.
const READ_ONLY_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities) printf '{"pull_requests":true,"gate":true}' ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/101", "repo": "app", "number": "101",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/101",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open",
        "gate": { "repos": [ { "repo": "app", "passed": 7, "failed": 2, "running": 0 } ] } }
    ]'
    ;;
  *) printf '[]' ;;
esac
"#;

fn watching(world: &World, forge: &str) {
    world.stub("ephor-forge-acmeforge", forge);
    world.configure(serde_json::json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } }
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
}

/// The scope crosses the seam as the caller's word, and what comes back is
/// what the reader is told — the count and what could not be restarted, not a
/// bare *done* (§FS-004-quick-actions.9, §FS-001-forge-interface.1).
#[test]
fn the_scope_the_reader_meant_is_the_scope_the_forge_is_asked_for() {
    let world = World::new();
    watching(&world, RESTARTING_FORGE);
    let asked = world.path().join("asked.json");

    world
        .ephor()
        .env("ACME_ASKED", &asked)
        .args([
            "restart",
            "--project",
            PROJECT,
            "--source",
            "acmeforge",
            "--repo",
            "app",
            "--number",
            "101",
            "--scope",
            "failed",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("asked 2 job(s) to run again"))
        // What it could not do is said, not swallowed: a restart that silently
        // did three quarters of the job is worse than one that names the
        // quarter it skipped.
        .stdout(predicates::str::contains("legal-sign-off"));

    let handed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the request")).unwrap();
    assert_eq!(handed["scope"], "failed");
    assert_eq!(handed["repo"], "app");
    assert_eq!(handed["number"], "101");

    // The expensive ask is a different ask, and arrives as one.
    world
        .ephor()
        .env("ACME_ASKED", &asked)
        .args([
            "restart",
            "--project",
            PROJECT,
            "--source",
            "acmeforge",
            "--repo",
            "app",
            "--number",
            "101",
            "--scope",
            "all",
        ])
        .assert()
        .success()
        // A forge that does not count says what it said, rather than
        // reporting a zero that would read as "nothing to do".
        .stdout(predicates::str::contains(
            "gate start accepted; it runs elsewhere",
        ))
        .stdout(predicates::str::contains("asked 0").not());
    let handed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the request")).unwrap();
    assert_eq!(handed["scope"], "all");
}

/// A scope nobody recognizes is refused rather than defaulted: the two differ
/// by an hour of somebody else's machines, and guessing which was meant is the
/// one thing this must not do.
#[test]
fn a_scope_that_is_not_one_of_the_two_is_refused() {
    let world = World::new();
    watching(&world, RESTARTING_FORGE);
    world
        .ephor()
        .args([
            "restart",
            "--project",
            PROJECT,
            "--source",
            "acmeforge",
            "--repo",
            "app",
            "--number",
            "101",
            "--scope",
            "everything",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Unknown scope 'everything'"));
}

/// A forge that reports a gate it cannot re-run is an ordinary
/// implementation: ephor asks nothing of it and says so, rather than offering
/// a key that prints its own refusal (§FS-004-quick-actions.2).
#[test]
fn a_forge_that_does_not_restart_is_asked_for_nothing() {
    let world = World::new();
    watching(&world, READ_ONLY_FORGE);
    world
        .ephor()
        .args([
            "restart",
            "--project",
            PROJECT,
            "--source",
            "acmeforge",
            "--repo",
            "app",
            "--number",
            "101",
            "--scope",
            "failed",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("does not restart a gate"));
}
