//! The budget's own arithmetic and its own words (§FS-015-spend-ceiling).
//!
//! Everything here is asked of buckets built in the test rather than of a
//! store on disk: the end-to-end case owns whether the store is read, and
//! these own what is said about what it held.

use super::*;
use crate::burn::{Key, Tokens};

fn at(when: &str) -> DateTime<Utc> {
    when.parse::<DateTime<Utc>>().expect("an instant")
}

/// One bucket: a span, a project, what it burned, and — only where the log
/// carried one — what it cost.
fn bucket(when: &str, project: &str, tokens: u64, cost: Option<f64>) -> Bucket {
    Bucket {
        at: at(when),
        key: Key {
            project: project.to_string(),
            source: "claude".to_string(),
            provider: "anthropic".to_string(),
            model: "a-model".to_string(),
            session: "a-session".to_string(),
            subagent: false,
        },
        tokens: Tokens {
            output: tokens,
            ..Tokens::default()
        },
        cost_usd: cost,
    }
}

fn dollars(amount: f64, per: Per) -> SpendBudget {
    SpendBudget {
        amount,
        currency: Currency::Usd,
        per,
    }
}

fn refused(budget: &SpendBudget, window: &[Bucket], now: &str) -> Option<Refusal> {
    let mine: Vec<&Bucket> = window.iter().collect();
    dollars_full("global work", budget, &mine, at(now))
}

/// The whole of a refusal in one case (§FS-015-spend-ceiling.9): the key that
/// carries the ceiling, the number, what was measured to two decimals, the
/// tokens no price covered, and the instant the trailing window lets work
/// start again.
#[test]
fn a_full_dollar_ceiling_names_the_key_the_total_the_hole_and_the_instant() {
    let window = vec![
        bucket("2026-09-05T08:00:00Z", "demo", 1_000_000, Some(52.40)),
        bucket("2026-09-05T08:00:00Z", "demo", 12_000_000, None),
    ];
    let full = refused(&dollars(50.0, Per::Day), &window, "2026-09-05T10:00:00Z")
        .expect("the ceiling is full");
    assert_eq!(full.scope, "global work.max_spend");
    assert_eq!(
        full.says,
        "global work.max_spend 50 USD per 24h is full \
         (52.40 USD measured, 12000000 token(s) unpriced); resumes 2026-09-06T08:00:00Z"
    );
    // The span the spend fell in, plus the window: the earliest moment what is
    // measured now is no longer full (§FS-015-spend-ceiling.8).
    assert_eq!(
        full.resumes_at.as_deref(),
        Some("2026-09-06T08:00:00Z"),
        "{full:?}"
    );
}

/// A window nothing priced has measured no dollars, so the dollar ceiling
/// binds nothing there — never a zero it invented
/// (§FS-015-spend-ceiling.7). The token ceiling is unaffected, which is why
/// both denominations ship (§FS-015-spend-ceiling.2).
#[test]
fn a_window_nothing_priced_binds_no_dollars_and_the_token_ceiling_still_does() {
    let window = vec![bucket("2026-09-05T08:00:00Z", "demo", 2_000_000, None)];
    assert_eq!(
        refused(&dollars(1.0, Per::Day), &window, "2026-09-05T10:00:00Z"),
        None,
        "a cap that stopped over an accounting hole would fail over an outage"
    );
    let mine: Vec<&Bucket> = window.iter().collect();
    let full = tokens_full(
        "global work",
        &TokenBudget {
            amount: 1_000_000,
            per: Per::Day,
        },
        &mine,
        at("2026-09-05T10:00:00Z"),
    )
    .expect("the ceiling that measured something is full");
    assert_eq!(
        full.says,
        "global work.max_tokens 1000000 per 24h is full \
         (2000000 token(s) measured); resumes 2026-09-06T08:00:00Z"
    );
}

/// A hole that is not there is not reported as an empty one, and a ceiling
/// with room is not a reason for anything (§FS-015-spend-ceiling.9).
#[test]
fn a_window_with_room_refuses_nobody_and_a_priced_one_reports_no_hole() {
    let window = vec![bucket(
        "2026-09-05T08:00:00Z",
        "demo",
        1_000_000,
        Some(10.00),
    )];
    assert_eq!(
        refused(&dollars(50.0, Per::Day), &window, "2026-09-05T10:00:00Z"),
        None
    );
    let full = refused(&dollars(10.0, Per::Day), &window, "2026-09-05T10:00:00Z")
        .expect("the ceiling is full at exactly its number");
    assert!(!full.says.contains("unpriced"), "{}", full.says);
}

/// A `0` budget is a pause its author meant rather than a window that will
/// pass, so it admits no start and names no instant to wait for
/// (§FS-015-spend-ceiling.4, §FS-015-spend-ceiling.8).
#[test]
fn a_ceiling_of_zero_admits_no_start_and_names_no_instant() {
    let full = refused(&dollars(0.0, Per::Day), &[], "2026-09-05T10:00:00Z")
        .expect("zero refuses on an empty window too");
    assert_eq!(
        full.says,
        "global work.max_spend 0 USD per 24h admits no new autorun starts"
    );
    assert_eq!(full.resumes_at, None);
}

/// The window is the trailing one the reading uses, not a calendar period, so
/// spend older than it is not what a ceiling binds on
/// (§FS-015-spend-ceiling.3).
#[test]
fn spend_older_than_the_window_is_outside_it() {
    let window = vec![bucket(
        "2026-09-04T09:00:00Z",
        "demo",
        1_000_000,
        Some(52.40),
    )];
    assert_eq!(
        refused(&dollars(50.0, Per::Day), &window, "2026-09-05T10:00:00Z"),
        None,
        "a day-old span has aged out of a 24h window"
    );
    assert!(
        refused(&dollars(50.0, Per::Week), &window, "2026-09-05T10:00:00Z").is_some(),
        "and is still inside a 7d one"
    );
}

/// The oldest span leaves first, so the instant is the first one whose leaving
/// clears the ceiling — not the last span's, and not the newest
/// (§FS-015-spend-ceiling.8).
#[test]
fn the_instant_is_the_first_span_whose_leaving_clears_the_ceiling() {
    let window = vec![
        bucket("2026-09-05T08:00:00Z", "demo", 10, Some(30.00)),
        bucket("2026-09-05T09:00:00Z", "demo", 10, Some(30.00)),
    ];
    let full = refused(&dollars(50.0, Per::Day), &window, "2026-09-05T10:00:00Z")
        .expect("sixty dollars is over fifty");
    assert_eq!(
        full.resumes_at.as_deref(),
        Some("2026-09-06T08:00:00Z"),
        "thirty dollars leaving is enough, so the later span is not waited for"
    );
}

/// Every ceiling is evaluated and the outermost full one is the reason
/// (§FS-015-spend-ceiling.5). An organization's measured total contains its
/// projects', which is why the wider number is the one that answers.
#[test]
fn the_outermost_full_scope_is_the_one_a_reader_is_sent_to() {
    let budgets = Budgets {
        bounds: vec![
            Bound {
                scope: Scope::Site,
                full: None,
            },
            Bound {
                scope: Scope::Organization("guild".to_string()),
                full: Some(Refusal {
                    scope: "organizations.guild.work.max_spend".to_string(),
                    says: "the organization's".to_string(),
                    resumes_at: None,
                }),
            },
            Bound {
                scope: Scope::Project("demo".to_string()),
                full: Some(Refusal {
                    scope: "projects.demo.work.max_spend".to_string(),
                    says: "the project's".to_string(),
                    resumes_at: None,
                }),
            },
        ],
        membership: BTreeMap::from([("demo".to_string(), "guild".to_string())]),
    };
    let over = ["demo".to_string()];
    assert_eq!(
        budgets.over(&over).map(|full| full.says.as_str()),
        Some("the organization's")
    );
    // A project in no organization is under the site and its own alone, so a
    // ceiling over an organization it is not in says nothing about it.
    assert_eq!(budgets.over(&["other".to_string()]), None);
    assert_eq!(
        budgets.binding("demo").map(|full| full.scope.as_str()),
        Some("organizations.guild.work.max_spend")
    );
}

/// The machine form of a pause is the scope and the instant and nothing else —
/// the totals and the coverage stay in `burn` (§FS-015-spend-ceiling.10).
#[test]
fn a_pause_carries_the_scope_and_the_instant_and_nothing_else() {
    let full = Refusal {
        scope: "global work.max_spend".to_string(),
        says: "global work.max_spend 50 USD per 24h is full (52.40 USD measured)".to_string(),
        resumes_at: Some("2026-09-06T08:00:00Z".to_string()),
    };
    let pause = serde_json::to_value(full.pause()).expect("the pause is a document");
    assert_eq!(
        pause,
        serde_json::json!({
            "scope": "global work.max_spend",
            "resumes_at": "2026-09-06T08:00:00Z"
        })
    );
    let paused = BTreeMap::from([("demo".to_string(), full.pause())]);
    assert_eq!(
        lines(&paused),
        vec!["Autorun paused: global work.max_spend, resumes 2026-09-06T08:00:00Z"]
    );
    // Under a ceiling of `0` the instant is absent from both, and one line is
    // said however many projects the scope holds.
    let zero = Refusal {
        resumes_at: None,
        ..full
    };
    assert_eq!(
        serde_json::to_value(zero.pause()).expect("the pause is a document"),
        serde_json::json!({ "scope": "global work.max_spend" })
    );
    let paused = BTreeMap::from([
        ("demo".to_string(), zero.pause()),
        ("other".to_string(), zero.pause()),
    ]);
    assert_eq!(
        lines(&paused),
        vec!["Autorun paused: global work.max_spend"]
    );
}

/// What the two keys accept, and what they refuse by name
/// (§FS-015-spend-ceiling.3). A window the store cannot answer is a budget
/// that cannot bind, so it is refused rather than approximated.
#[test]
fn a_currency_or_a_window_the_store_cannot_answer_is_refused_by_name() {
    let read = |value: serde_json::Value| serde_json::from_value::<SpendBudget>(value);
    assert_eq!(
        read(serde_json::json!({ "amount": 50, "currency": "USD", "per": "24h" }))
            .expect("the shape the site writes"),
        dollars(50.0, Per::Day)
    );
    // The two spellings the reading already has names for.
    assert_eq!(
        read(serde_json::json!({ "amount": 50, "currency": "USD", "per": "day" }))
            .expect("day")
            .per,
        Per::Day
    );
    assert_eq!(
        read(serde_json::json!({ "amount": 50, "currency": "USD", "per": "week" }))
            .expect("week")
            .per,
        Per::Week
    );

    let refusal = read(serde_json::json!({ "amount": 50, "currency": "EUR", "per": "24h" }))
        .expect_err("a currency ephor cannot convert to what the logs carry")
        .to_string();
    assert!(
        refusal.contains("EUR") && refusal.contains("USD"),
        "{refusal}"
    );

    let refusal = read(serde_json::json!({ "amount": 50, "currency": "USD", "per": "fortnight" }))
        .expect_err("a window the store cannot answer")
        .to_string();
    for window in ["1h", "6h", "24h", "7d"] {
        assert!(refusal.contains(window), "{refusal}");
    }

    // Every key is required on the block that carries it, and a `max_tokens`
    // with a currency means something its author did not write.
    assert!(read(serde_json::json!({ "amount": 50, "currency": "USD" })).is_err());
    assert!(read(serde_json::json!({ "amount": -1, "currency": "USD", "per": "24h" })).is_err());
    assert!(serde_json::from_value::<TokenBudget>(
        serde_json::json!({ "amount": 1, "per": "24h" })
    )
    .is_ok());
    assert!(serde_json::from_value::<TokenBudget>(
        serde_json::json!({ "amount": 1, "currency": "USD", "per": "24h" })
    )
    .is_err());
}
