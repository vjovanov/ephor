//! The selection rule, held apart from everything that reaches a disk
//! (§FS-005-dispatch.29). These are about the two refusals the rule is made
//! of: it may veto and it may not reorder, and it may not treat silence as a
//! number.

use super::*;
use crate::work::runtime::roster::Hand;

fn hand(id: &str, provider: Option<&str>) -> Hand {
    Hand {
        id: id.to_string(),
        agent: Some("an-agent".to_string()),
        model: Some(format!("m-{id}")),
        provider: provider.map(str::to_string),
        efforts: Vec::new(),
        available: None,
    }
}

fn member(id: &str, provider: &str) -> Member {
    let hand = hand(id, Some(provider));
    Member {
        pool: pool_of(&hand),
        hand,
        effort: None,
        note: None,
    }
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("an instant")
        .with_timezone(&Utc)
}

fn evidence(pools: &[Standing], now: &str) -> Evidence {
    Evidence {
        floor: 0.0,
        pools: pools
            .iter()
            .map(|standing| (standing.pool.clone(), standing.clone()))
            .collect(),
        now: Some(at(now)),
    }
}

fn reporting(pool: &str, remaining: Option<f64>, resets_at: Option<&str>) -> Standing {
    Standing {
        pool: pool.to_string(),
        remaining,
        why: None,
        refused_until: None,
        resets_at: resets_at.map(at),
        spawns: 0,
    }
}

/// A pool is the provider that serves the model, and the agent that carries it
/// only where the profile names no provider: two hands behind one provider
/// spend one allowance, so evidence about either is evidence about both.
#[test]
fn a_pool_is_the_provider_and_falls_back_to_the_agent() {
    assert_eq!(pool_of(&hand("fast", Some("north"))), "north");
    assert_eq!(pool_of(&hand("fast", None)), "an-agent");
    let mut orphan = hand("fast", None);
    orphan.agent = None;
    assert_eq!(pool_of(&orphan), "fast");
}

/// The load-bearing half. A pool nobody reported a number for is unknown, and
/// unknown is not a low number: the first member still takes the work, and the
/// rule says nothing about it because there is nothing in the rule an unknown
/// could be sorted against.
#[test]
fn unknown_demotes_nobody() {
    let members = [member("north-fast", "north"), member("south-fast", "south")];
    // Nothing recorded at all.
    let (index, said) = choose(&members, &evidence(&[], "2026-09-05T09:00:00Z"));
    assert_eq!(index, 0);
    assert_eq!(said, None);

    // A window that reported no number, and one that reported a healthy one.
    let known = evidence(
        &[
            reporting("north", None, Some("2026-09-05T18:30:00Z")),
            reporting("south", Some(0.9), None),
        ],
        "2026-09-05T09:00:00Z",
    );
    assert_eq!(choose(&members, &known).0, 0);
    assert_eq!(known.spent("north"), None);
}

/// The other half. A member is passed over exactly when its pool is known
/// spent, the survivors keep the order their author wrote, and the ticket says
/// which member was passed over and until when.
#[test]
fn a_known_spent_pool_is_passed_over_and_the_next_member_keeps_its_place() {
    let members = [
        member("north-fast", "north"),
        member("south-fast", "south"),
        member("east-fast", "east"),
    ];
    let known = evidence(
        &[
            reporting("north", Some(0.0), Some("2026-09-05T18:30:00Z")),
            reporting("south", Some(0.42), None),
            // Plenty left, and it still does not overtake south: the rule is a
            // veto and never a ranking, so the author's order stands.
            reporting("east", Some(0.99), None),
        ],
        "2026-09-05T09:00:00Z",
    );
    let (index, said) = choose(&members, &known);
    assert_eq!(index, 1);
    let said = said.expect("the choosing says why");
    assert!(said.contains("north-fast"), "{said}");
    assert!(said.contains("2026-09-05T18:30:00Z"), "{said}");
}

/// Where every member is vetoed the first still gets the ticket, and the note
/// names the earliest instant any of their pools resets — not the first one
/// asked. A ticket that waits is work a person can see; work that silently
/// never dispatched is not. Each member it passed over names its own instant
/// beside it, which the rule neither asks for nor forbids: what it requires is
/// that the earliest is the one the sentence closes on.
#[test]
fn every_member_spent_writes_to_the_first_with_the_earliest_reset() {
    let members = [member("north-fast", "north"), member("south-fast", "south")];
    let known = evidence(
        &[
            reporting("north", Some(0.0), Some("2026-09-05T18:30:00Z")),
            reporting("south", Some(0.0), Some("2026-09-05T12:00:00Z")),
        ],
        "2026-09-05T09:00:00Z",
    );
    let (index, said) = choose(&members, &known);
    assert_eq!(index, 0);
    let said = said.expect("the choosing says why");
    assert!(said.contains("2026-09-05T12:00:00Z"), "{said}");
}

/// An unexpired refusal vetoes, and the same refusal after its instant does
/// not: the record says until when, and after that the pool is simply a pool
/// nobody has reported a number for.
#[test]
fn a_refusal_vetoes_until_its_instant_and_not_after_it() {
    let record = PoolRecord {
        refused_until: Some(at("2026-09-05T18:30:00Z")),
        says: Some("rate limit reached".to_string()),
        ..PoolRecord::default()
    };
    let before = standing("north", &record, at("2026-09-05T09:00:00Z"));
    assert_eq!(before.refused_until, Some(at("2026-09-05T18:30:00Z")));
    let after = standing("north", &record, at("2026-09-05T19:00:00Z"));
    assert_eq!(after.refused_until, None);
    assert_eq!(after.remaining, None);
}

/// A pool's effective remaining is the least of the windows it reported a
/// number for. An unknown window is not among them, so it can neither lower a
/// pool another window called healthy nor raise one another called spent.
#[test]
fn effective_remaining_is_the_least_of_the_known_windows() {
    let record = PoolRecord {
        windows: vec![
            Window {
                name: Some("session".to_string()),
                remaining: Some(0.5),
                resets_at: Some(at("2026-09-05T18:30:00Z")),
            },
            Window {
                name: Some("weekly".to_string()),
                remaining: None,
                resets_at: Some(at("2026-09-08T00:00:00Z")),
            },
            Window {
                name: Some("daily".to_string()),
                remaining: Some(0.2),
                resets_at: None,
            },
        ],
        ..PoolRecord::default()
    };
    let known = standing("north", &record, at("2026-09-05T09:00:00Z"));
    assert_eq!(known.remaining, Some(0.2));
    // The earliest instant anything about the pool resets, across every window
    // that named one.
    assert_eq!(known.resets_at, Some(at("2026-09-05T18:30:00Z")));

    // Every window unknown is a pool that is unknown, and it says so.
    let silent = standing(
        "south",
        &PoolRecord {
            windows: vec![Window::default()],
            ..PoolRecord::default()
        },
        at("2026-09-05T09:00:00Z"),
    );
    assert_eq!(silent.remaining, None);
    assert!(silent.why.is_some());
}

/// The floor the site names is what "spent" measures against, and `0` is what
/// it is unless the site names another.
#[test]
fn the_floor_is_what_spent_measures_against() {
    let members = [member("north-fast", "north"), member("south-fast", "south")];
    let mut known = evidence(
        &[
            reporting("north", Some(0.05), None),
            reporting("south", Some(0.9), None),
        ],
        "2026-09-05T09:00:00Z",
    );
    assert_eq!(choose(&members, &known).0, 0);
    known.floor = 0.1;
    assert_eq!(choose(&members, &known).0, 1);
}

/// A refusal is evidence about a pool only where its words carry an instant
/// ephor can read. Nothing here knows any vendor's phrasing: it reads a
/// timestamp, and it reads the earliest one where a message names several.
#[test]
fn an_instant_is_read_out_of_a_refusals_own_words_or_not_at_all() {
    let now = at("2026-09-05T09:00:00Z");
    assert_eq!(
        instant_in(
            "rate limit reached for this account; resets at 2026-09-05T18:30:00Z",
            now
        ),
        Some(at("2026-09-05T18:30:00Z"))
    );
    // Punctuation around the instant is the sentence's, not the instant's.
    assert_eq!(
        instant_in("too many requests (until 2026-09-05T18:30:00Z).", now),
        Some(at("2026-09-05T18:30:00Z"))
    );
    // The soonest thing that could lift.
    assert_eq!(
        instant_in(
            "2026-09-08T00:00:00Z weekly, 2026-09-05T12:00:00Z session",
            now
        ),
        Some(at("2026-09-05T12:00:00Z"))
    );
    // An offset is an instant too, read into the one spelling everything else
    // here uses.
    assert_eq!(
        instant_in("back at 2026-09-05T20:30:00+02:00", now),
        Some(at("2026-09-05T18:30:00Z"))
    );
    // And a failure ephor cannot date claims nothing at all.
    assert_eq!(instant_in("the runner exited 1", now), None);
    assert_eq!(instant_in("no space left on device", now), None);
    assert_eq!(instant_in("failed on 2026-09-05", now), None);
}

/// The soonest thing that *could lift* is not simply the soonest thing written
/// down. What a failed start records is the run's whole merged output, so a
/// refusal ordinarily arrives stamped with the hour it was printed, and an
/// instant already past is a log line rather than a window. Reading one as the
/// reset would date the refusal into the past, where `standing` drops it — and
/// the veto the seam's free channel exists to cast would be silently lost.
#[test]
fn an_instant_already_past_is_a_log_line_and_never_the_window() {
    let now = at("2026-09-05T09:00:00Z");
    assert_eq!(
        instant_in(
            "2026-09-05T06:00:00Z request failed: rate limit reached for this \
             account; resets at 2026-09-05T23:30:00Z",
            now
        ),
        Some(at("2026-09-05T23:30:00Z"))
    );
    // And where every instant it names has already gone by, the words date
    // nothing: the failure stays one root's own back-off and claims nothing
    // about a pool.
    assert_eq!(
        instant_in("2026-09-05T06:00:00Z the runner exited 1", now),
        None
    );
}

/// A refusal that has lifted names no instant on any surface. The veto reads
/// the refusal filtered against now, and the reported reset reads the same
/// filtered value — so the text line and the JSON one say the same thing about
/// the pool rather than the reading suppressing what the document still
/// carries (§FS-011-command-line.7).
#[test]
fn a_lifted_refusal_stops_naming_its_instant_in_the_report_too() {
    let record = PoolRecord {
        refused_until: Some(at("2026-09-05T18:30:00Z")),
        says: Some("rate limit reached".to_string()),
        ..PoolRecord::default()
    };
    // Still standing: the instant is both the veto's and the report's.
    let before = standing("north", &record, at("2026-09-05T09:00:00Z"));
    assert_eq!(before.refused_until, Some(at("2026-09-05T18:30:00Z")));
    assert_eq!(before.resets_at, Some(at("2026-09-05T18:30:00Z")));
    // Lifted: neither field names it, and nothing else about the pool is known.
    let after = standing("north", &record, at("2026-09-05T19:00:00Z"));
    assert_eq!(after.refused_until, None);
    assert_eq!(after.resets_at, None);
    assert_eq!(after.remaining, None);
    // A window of its own still reports its reset: only the refusal's instant
    // goes with the refusal.
    let with_window = standing(
        "north",
        &PoolRecord {
            windows: vec![Window {
                name: Some("weekly".to_string()),
                remaining: None,
                resets_at: Some(at("2026-09-08T00:00:00Z")),
            }],
            ..record.clone()
        },
        at("2026-09-05T19:00:00Z"),
    );
    assert_eq!(with_window.resets_at, Some(at("2026-09-08T00:00:00Z")));
}
