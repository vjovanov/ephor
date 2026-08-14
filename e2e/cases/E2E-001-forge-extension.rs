//! E2E-001-forge-extension: a forge ephor never heard of, watched by a script on PATH.
//!
//! The scenario is a person who works on a forge ephor does not implement.
//! They write `ephor-forge-acmeforge`, name it in their configuration, and
//! from that moment the forge is watched like any other: its pull requests are
//! matters in the feed, its gate renders on the row, and the expensive question
//! — what actually failed — is asked of the same script on demand
//! (§FS-001-forge-interface.2). Nothing about the script is compiled, linked,
//! or registered: it is materials and a process, which is the whole of the
//! boundary (§REQ-001-boundary.1).
//!
//! The second half is the degrade: an extension that is not installed, or that
//! answers rubbish, costs its own source and nothing else
//! (§FS-001-forge-interface.4).

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;

use support::*;

/// A forge in twenty lines of bash. It answers each subcommand with JSON on
/// stdout, and decides for itself which messages are the user's — the one
/// question policy above the seam cannot answer for it.
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":true,"gate":true,"issues":true,"failures":true,"review":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/101", "repo": "app", "number": "101",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/101",
        "branch": "you/ABC-42-retry",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open", "cited": false,
        "threads": [ { "messages": [
          { "author": "Ada", "text": "does the window reset per attempt?",
            "when": "2026-07-30T11:00:00Z", "mine": false } ] } ],
        "gate": { "repos": [ { "repo": "app", "passed": 5, "failed": 1, "running": 0 } ] } },
      { "id": "app/102", "repo": "app", "number": "102",
        "title": "Drop the legacy shim",
        "url": "https://acme.example/pr/102",
        "updated_at": "2026-07-30T09:00:00Z",
        "role": "reviewer", "state": "open", "cited": false,
        "review": "approved",
        "threads": [ { "messages": [
          { "author": "You", "text": "looks right to me", "mine": true } ] } ] }
    ]'
    ;;
  failures)
    printf '%s' '[ { "job": "gate / integration", "url": "https://acme.example/job/9",
                     "trace": "retry_window_test: expected 3 attempts, saw 1" } ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// The whole path, from a script on PATH to the row a person reads.
#[test]
fn a_script_on_path_is_a_forge_ephor_watches_like_its_own() {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    // Names no built-in provider, so ephor resolves `ephor-forge-acmeforge`.
    // Every other key is opaque to ephor and handed to the extension
    // (§FS-001-forge-interface.2).
    world.configure(serde_json::json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } }
    }));

    world.ephor().args(["refresh", PROJECT]).assert().success();

    // The pull request is a matter of the feed, attributed to the project the
    // source was configured under (§FS-008-attribution.2).
    let matter = world.matter("acmeforge:app/101");
    assert_eq!(matter["title"], "Widen the retry window");
    assert_eq!(matter["placement"]["on"]["project"], PROJECT);
    assert_eq!(matter["kind"], "pr");
    // Policy ran over out-of-process data exactly as it would in process: the
    // last word in the thread is not the user's, so it awaits an answer
    // (§FS-003-feed-categories.4).
    assert_eq!(matter["role"], "author");
    assert_eq!(matter["needs_response"], true);
    assert_eq!(matter["raw"]["branch"], "you/ABC-42-retry");

    // A review the extension reported is the reader's own verdict, and it
    // leads the row it is on (§FS-001-forge-interface.1): the question a
    // reviewing row has to answer first is what the reader already did about
    // it. Having answered is not the same as owing nothing else, so the
    // verdict arrives as a fact of its own rather than only inside the state.
    let reviewed = world.matter("acmeforge:app/102");
    assert_eq!(reviewed["role"], "reviewer");
    assert_eq!(reviewed["state"], "open:approved");
    assert_eq!(reviewed["raw"]["review"], "approved");
    assert_eq!(reviewed["needs_response"], false);

    // And the gate the extension reported renders on the row like any forge's.
    world
        .ephor()
        .args(["feed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Widen the retry window"))
        .stdout(predicate::str::contains("✓5 ✗1"))
        .stdout(predicate::str::contains(
            "Drop the legacy shim  [open:approved]",
        ));

    // The expensive question, asked on demand rather than on every refresh
    // (§FS-001-forge-interface.1): the same script, a different subcommand.
    world
        .ephor()
        .args([
            "failures",
            "--project",
            PROJECT,
            "--source",
            "acmeforge",
            "--repo",
            "app",
            "--number",
            "101",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("gate / integration"))
        .stdout(predicate::str::contains("expected 3 attempts"));
}

/// An extension that is not installed is a source that could not answer, not a
/// refresh that fell over: what the other sources said still arrives, and the
/// missing one says why (§FS-001-forge-interface.4, §REQ-001-boundary.1).
#[test]
fn an_extension_that_is_not_there_costs_its_own_source_and_nothing_else() {
    let world = World::new();
    world.configure(serde_json::json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } }
    }));

    // Not installed at all: the refresh reports the source as unreachable and
    // names the executable it looked for, which is the difference between an
    // install command and an afternoon.
    world
        .ephor()
        .args(["refresh", PROJECT])
        .assert()
        .stderr(predicate::str::contains("ephor-forge-acmeforge"))
        .stderr(predicate::str::contains("not on PATH"));

    // Installed and answering rubbish: still one source's failure, reported as
    // itself rather than as an empty feed.
    world.stub(
        "ephor-forge-acmeforge",
        "#!/usr/bin/env bash\ncat > /dev/null\nprintf 'not json at all'\n",
    );
    world
        .ephor()
        .args(["refresh", PROJECT])
        .assert()
        .stderr(predicate::str::contains("acmeforge"));
}
