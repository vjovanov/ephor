//! E2E-017-provider-headroom: a pin naming alternates, and the evidence that
//! decides which of them the ticket goes to.
//!
//! The scenario is a machine with two providers authenticated and one window
//! spent. A person writes both hands on the action, best first
//! (§FS-005-dispatch.14), and ephor has to answer one question at every write
//! it makes: which of those names can be reached right now
//! (§FS-005-dispatch.29). It answers from evidence it was *given* — a refusal
//! it recorded when its own spawn failed, or a bound `work.headroom` verb
//! reporting what a provider has left — and never from a number it worked out
//! itself.
//!
//! The two halves of the rule are what this case is about, and both are
//! refusals to do something. Evidence may veto a member and may never reorder
//! the survivors, so the hand a person chose for the work stays the hand that
//! does it until its pool is *known* spent. And a pool nobody reported on is
//! unknown rather than empty — an unbound verb, one that fails, one that
//! answers `null` — so silence can never demote anything
//! (§FS-006-project-interface.9, §REQ-001-boundary.1).

#[path = "../support.rs"]
mod support;

use chrono::{Duration, SecondsFormat, Utc};
use predicates::prelude::*;
use serde_json::{json, Value};

use support::*;

/// The plan id ephor derives from the item's key.
const PLAN: &str = "acmeforge-app-101";
const ITEM: &str = "acmeforge:app/101";

/// A forge with one pull request of the reader's own, so there is a matter to
/// hand over.
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

/// The runtime's own registry: one agent, and two model profiles served by two
/// different providers. The providers are what make two pools — a pool is the
/// hand's provider where it names one (§FS-005-dispatch.29) — and the agent
/// declares no modes, so both hands are asked plainly and their targets are
/// short enough to read in an assertion.
const RUNTIME_SETTINGS: &str = r#"{
  "agents": { "acme-agent": { "command": ["sh"] } },
  "models": {
    "north-fast": { "provider": "north", "model": "m-north", "default_agent": "acme-agent" },
    "south-fast": { "provider": "south", "model": "m-south", "default_agent": "acme-agent" }
  }
}"#;

/// A headroom verb: it writes the envelope to the file `$EPHOR_ANSWER` names,
/// with the windows on `data` (§FS-006-project-interface.4). Never on stdout —
/// stdout is the command's own (§FS-006-project-interface.3).
fn reporting(windows: Value) -> String {
    answering(json!({ "v": 1, "data": { "windows": windows } }), 0)
}

/// One window, as the payload spells it. `remaining` is a fraction of the
/// window left, `None` where the provider said nothing about it.
fn window(name: &str, remaining: Option<f64>, resets_at: &str) -> Value {
    json!({
        "name": name,
        "remaining": remaining,
        "resets_at": resets_at,
    })
}

/// A world watching the forge, with the runtime bound, both model profiles on
/// the roster, and one recipe that turns the pull request into a ticket.
/// `work` is merged over that, so a case states only the hands and the
/// headroom bindings its scenario is about.
fn watching(work: Value) -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    world.stub("acme-runtime", "#!/bin/sh\nexit 0\n");
    let settings = world.path().join(".config/rhei/settings.json");
    std::fs::create_dir_all(settings.parent().expect("a parent")).expect("the settings directory");
    std::fs::write(&settings, RUNTIME_SETTINGS).expect("the runtime's registry");

    let mut configured = json!({
        "runner": "acme-runtime",
        "recipes": [{
            "id": "fix-gate",
            "icon": "🛠",
            "description": "fix the red gate",
            "state": "fix",
            "needs_checkout": false,
            "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
            "brief": "Fix the gate on {title}."
        }]
    });
    merge_into(&mut configured, work);
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": configured,
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world
}

/// The same, plus a project narrowing — which lives on the project rather than
/// beside the site's work configuration (§FS-006-project-interface.9).
fn watching_narrowed(work: Value, permitted: &[&str]) -> World {
    let world = watching(work.clone());
    let mut configured = json!({
        "runner": "acme-runtime",
        "recipes": [{
            "id": "fix-gate",
            "icon": "🛠",
            "description": "fix the red gate",
            "state": "fix",
            "needs_checkout": false,
            "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
            "brief": "Fix the gate on {title}."
        }]
    });
    merge_into(&mut configured, work);
    world.configure(json!({
        "projects": { PROJECT: {
            "providers": [{ "provider": "acmeforge", "user": "you", "repos": ["app"] }],
            "work": { "permitted_hands": permitted },
        } },
        "work": configured,
    }));
    world
}

fn merge_into(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => merge_into(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn plan_path(world: &World) -> std::path::PathBuf {
    world.forest().join("panta").join(format!("{PLAN}.rhei.md"))
}

fn plan(world: &World) -> String {
    std::fs::read_to_string(plan_path(world)).expect("the plan the dispatch wrote")
}

/// Both hands are written on the action, best first, and nothing anywhere says
/// either pool is spent. The list is an ordered list of hands and never one
/// hand spelled out positionally (§FS-006-project-interface.9), so the first
/// member is resolved against the roster like any other name and takes the
/// ticket — and the ticket says which member it went to, in ephor's own words
/// beside the dossier rather than in a field of the runtime's language
/// (§FS-005-dispatch.29).
#[test]
fn an_ordered_list_hands_the_ticket_to_its_first_member() {
    let world = watching(json!({ "hands": { "default": ["north-fast", "south-fast"] } }));

    world
        .ephor()
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** acme-agent:north:m-north"),
        "{plan}"
    );
    // Never the positional reading of an array, which would take the second
    // member for the model the first one carries.
    assert!(!plan.contains("north-fast:south-fast"), "{plan}");
    // And the plan itself says who is doing this and can be read by somebody
    // who was not there when it was written.
    assert!(plan.contains("north-fast"), "{plan}");
}

/// A narrowing is checked against every member of a list, and one name it does
/// not permit refuses the whole list with that name in the message
/// (§FS-006-project-interface.9). Never filtered down to the permitted member:
/// a policy that quietly used the second choice is indistinguishable from one
/// that was never asked.
#[test]
fn one_unpermitted_member_refuses_the_whole_list() {
    let world = watching_narrowed(
        json!({ "hands": { "default": ["north-fast", "south-fast"] } }),
        &["north-fast"],
    );

    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM])
        .assert()
        .failure()
        .stderr(predicate::str::contains("south-fast"))
        .stderr(predicate::str::contains("does not permit"));
    assert!(
        !plan_path(&world).exists(),
        "a refused list wrote a plan anyway"
    );
}

/// The bound verb reports one pool spent, so that member is passed over and
/// the next survivor takes the ticket — and the ticket records which member
/// was passed over and until when, so the plan says why it is not going to the
/// hand its author wrote first (§FS-005-dispatch.29).
#[test]
fn a_pool_reported_spent_is_passed_over_and_the_next_member_takes_the_ticket() {
    let world = watching(json!({
        "hands": { "default": ["north-fast", "south-fast"] },
        "headroom": { "north": "north-headroom", "south": "south-headroom" },
    }));
    world.stub(
        "north-headroom",
        &reporting(json!([window(
            "session",
            Some(0.0),
            "2026-09-05T18:30:00Z"
        )])),
    );
    world.stub(
        "south-headroom",
        &reporting(json!([window(
            "session",
            Some(0.42),
            "2026-09-05T18:30:00Z"
        )])),
    );
    // Probing is fetching, so it happens where fetching happens
    // (§FS-001-forge-interface.7) rather than in front of the dispatch.
    world.ephor().args(["refresh", PROJECT]).assert().success();

    world.ephor().args(["work", "dispatch"]).assert().success();
    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** acme-agent:south:m-south"),
        "{plan}"
    );
    assert!(plan.contains("north-fast"), "{plan}");
    assert!(plan.contains("2026-09-05T18:30:00Z"), "{plan}");
}

/// Every member's pool is spent, and the ticket is written all the same — to
/// the first member, carrying the earliest instant any of their pools resets.
/// A ticket that waits is work a person can see and start by hand; work that
/// silently never dispatched is neither (§FS-005-dispatch.29).
#[test]
fn every_pool_spent_still_writes_the_ticket_to_the_first_member() {
    let world = watching(json!({
        "hands": { "default": ["north-fast", "south-fast"] },
        "headroom": { "north": "north-headroom", "south": "south-headroom" },
    }));
    world.stub(
        "north-headroom",
        &reporting(json!([window(
            "session",
            Some(0.0),
            "2026-09-05T18:30:00Z"
        )])),
    );
    world.stub(
        "south-headroom",
        &reporting(json!([window(
            "session",
            Some(0.0),
            "2026-09-05T12:00:00Z"
        )])),
    );
    world.ephor().args(["refresh", PROJECT]).assert().success();

    world.ephor().args(["work", "dispatch"]).assert().success();
    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** acme-agent:north:m-north"),
        "{plan}"
    );
    // The earliest reset among the vetoed pools, not the first one asked.
    assert!(plan.contains("2026-09-05T12:00:00Z"), "{plan}");
}

/// Nobody reported a number, and nothing is demoted. `remaining: null` is
/// unknown, a verb that exits non-zero is unknown, and a pool with no verb
/// bound is unknown — none of them is zero, none is an error that stops the
/// dispatch, and each says beside its pool why it is unknown
/// (§FS-005-dispatch.29, §REQ-001-boundary.1).
#[test]
fn unknown_never_demotes_a_member_and_says_why_beside_the_pool() {
    let world = watching(json!({
        "hands": { "default": ["north-fast", "south-fast"] },
        "headroom": { "north": "north-headroom" },
    }));
    world.stub(
        "north-headroom",
        &reporting(json!([window("session", None, "2026-09-05T18:30:00Z")])),
    );
    world.ephor().args(["refresh", PROJECT]).assert().success();

    // The first member still gets the work: no number is not a low number.
    world.ephor().args(["work", "dispatch"]).assert().success();
    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** acme-agent:north:m-north"),
        "{plan}"
    );

    // And both pools are reported, unknown, each with the reason it is
    // unknown: the one that answered without a number, and the one nothing is
    // bound to at all.
    let reported = world
        .ephor()
        .args(["caps", "--json"])
        .output()
        .expect("capabilities answers");
    assert!(reported.status.success());
    let pools = json_of(&reported)["pools"].clone();
    let pool = |name: &str| {
        pools
            .as_array()
            .unwrap_or_else(|| panic!("no pools in {pools}"))
            .iter()
            .find(|entry| entry["pool"] == name)
            .unwrap_or_else(|| panic!("no pool {name} in {pools}"))
            .clone()
    };
    for name in ["north", "south"] {
        let entry = pool(name);
        assert!(entry["remaining"].is_null(), "{entry}");
        assert!(
            entry["why"].as_str().is_some_and(|why| !why.is_empty()),
            "pool {name} is unknown and does not say why: {entry}"
        );
    }
}

/// A pool's effective remaining is the least of the windows it reported a
/// number for, and an unknown window is not among them
/// (§FS-005-dispatch.29). So a pool with one spent window is spent whatever
/// its others say, and a pool with one unreadable window is judged on the ones
/// that were readable.
#[test]
fn effective_remaining_is_the_least_of_the_known_windows() {
    let world = watching(json!({
        "hands": { "default": ["north-fast", "south-fast"] },
        "headroom": { "north": "north-headroom", "south": "south-headroom" },
    }));
    world.stub(
        "north-headroom",
        &reporting(json!([
            window("session", Some(0.5), "2026-09-05T18:30:00Z"),
            window("weekly", Some(0.0), "2026-09-08T00:00:00Z"),
        ])),
    );
    world.stub(
        "south-headroom",
        &reporting(json!([
            window("session", Some(0.4), "2026-09-05T18:30:00Z"),
            window("weekly", None, "2026-09-08T00:00:00Z"),
        ])),
    );
    world.ephor().args(["refresh", PROJECT]).assert().success();

    world.ephor().args(["work", "dispatch"]).assert().success();
    let plan = plan(&world);
    assert!(
        plan.contains("**Target:** acme-agent:south:m-south"),
        "{plan}"
    );
}

/// The free channel, which needs nothing bound. A start ephor made failed, and
/// the words it failed with name the instant the window lifts — so that is a
/// refusal on the hand's pool, and the next write passes that pool over
/// (§FS-005-dispatch.29). The record is ephor's own account of ephor's own
/// act and never the truth about the work (§FS-005-dispatch.4).
#[test]
fn a_refusal_ephor_recorded_passes_that_pool_over_until_it_lifts() {
    let world = watching(json!({
        "hands": { "default": ["north-fast", "south-fast"] },
        "recipes": [{
            "id": "fix-gate",
            "icon": "🛠",
            "description": "fix the red gate",
            "state": "fix",
            "needs_checkout": false,
            "autorun": true,
            "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
            "brief": "Fix the gate on {title}."
        }],
    }));
    // The window lifts six hours from now rather than at an hour written down
    // here: a refusal is evidence only while the instant it names is still
    // ahead (§FS-005-dispatch.29), so a literal would quietly stop being a
    // refusal the moment the clock passed it.
    let lifts = (Utc::now() + Duration::hours(6)).to_rfc3339_opts(SecondsFormat::Secs, true);
    // A runtime that answers the detach probe every start makes, and then
    // refuses in its provider's words, naming when the window lifts. Nothing
    // here is a vendor ephor knows: it is a failed start whose message happens
    // to carry an instant.
    world.stub(
        "acme-runtime",
        &format!(
            r#"#!/usr/bin/env bash
case "$*" in
  *--help*)
    printf 'Options:\n      --headless  Detach the run into its own session\n'
    exit 0
    ;;
esac
printf 'rate limit reached for this account; resets at {lifts}\n' >&2
exit 1
"#
        ),
    );

    world.ephor().args(["work", "dispatch"]).assert().success();
    assert!(plan(&world).contains("**Target:** acme-agent:north:m-north"));

    // The sweep starts the run, the run refuses, and the refusal is recorded.
    world
        .ephor()
        .args(["work", "run", "--due", "--json"])
        .output()
        .expect("the sweep runs");

    // The next write ephor makes asks again, and north is now known spent.
    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM, "--again"])
        .assert()
        .success();
    let plan = plan(&world);
    assert_eq!(plan.matches("### Task").count(), 2, "{plan}");
    assert!(
        plan.contains("**Target:** acme-agent:south:m-south"),
        "{plan}"
    );
    assert!(plan.contains(&lifts), "{plan}");
}
