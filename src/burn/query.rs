//! Windows, groupings, and the current rate — every one of them a sum over
//! buckets rather than a second reading of the transcripts (§FS-013-burn.6).

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

use super::{Bucket, Tokens};

/// How far back a reading looks (§FS-013-burn.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Window {
    #[default]
    #[value(name = "1h")]
    Hour,
    #[value(name = "6h")]
    SixHours,
    #[value(name = "24h")]
    Day,
    #[value(name = "7d")]
    Week,
}

impl Window {
    pub fn label(self) -> &'static str {
        match self {
            Window::Hour => "1h",
            Window::SixHours => "6h",
            Window::Day => "24h",
            Window::Week => "7d",
        }
    }

    pub fn span(self) -> Duration {
        match self {
            Window::Hour => Duration::hours(1),
            Window::SixHours => Duration::hours(6),
            Window::Day => Duration::hours(24),
            Window::Week => Duration::days(7),
        }
    }

    /// The next window round the cycle — what the key does on the screen and
    /// nothing more (§FS-013-burn.8).
    pub fn next(self) -> Window {
        match self {
            Window::Hour => Window::SixHours,
            Window::SixHours => Window::Day,
            Window::Day => Window::Week,
            Window::Week => Window::Hour,
        }
    }
}

/// What a reading groups by, and — because only one lens can answer each —
/// which lens it selects (§FS-013-burn.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum By {
    #[default]
    Project,
    Model,
    Session,
    Plan,
    Matter,
}

impl By {
    pub fn label(self) -> &'static str {
        match self {
            By::Project => "project",
            By::Model => "model",
            By::Session => "session",
            By::Plan => "plan",
            By::Matter => "matter",
        }
    }

    /// Which lens answers this grouping. The two are never added together, so
    /// asking for a grouping is asking one of them (§FS-013-burn.1).
    pub fn lens(self) -> Lens {
        match self {
            By::Project | By::Model | By::Session => Lens::Machine,
            By::Plan | By::Matter => Lens::Work,
        }
    }

    pub fn next(self) -> By {
        match self {
            By::Project => By::Model,
            By::Model => By::Session,
            By::Session => By::Plan,
            By::Plan => By::Matter,
            By::Matter => By::Project,
        }
    }
}

/// Which record a reading was built from (§FS-013-burn.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lens {
    /// What this machine burned, from the agent tools' own transcripts.
    Machine,
    /// What a piece of work cost, from the runtime's accounting records.
    Work,
}

impl Lens {
    pub fn label(self) -> &'static str {
        match self {
            Lens::Machine => "machine",
            Lens::Work => "work",
        }
    }
}

/// One row of a grouped reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// What the row is: a project id, a model, a session id, a plan, a matter.
    pub key: String,
    /// The second thing worth knowing about it — a model's provider, a
    /// session's project — where there is one.
    pub detail: Option<String>,
    pub tokens: Tokens,
    /// Unknown where nothing priced it, and never rendered as zero
    /// (§FS-013-burn.7).
    pub cost_usd: Option<f64>,
}

/// The window's buckets grouped, biggest first.
///
/// Only the three groupings the machine lens answers; the other two are the
/// work lens's and are read from the runtime's own records
/// (§FS-013-burn.1).
pub fn group(buckets: &[Bucket], by: By) -> Vec<Group> {
    let mut rows: BTreeMap<(String, Option<String>), (Tokens, Option<f64>)> = BTreeMap::new();
    for bucket in buckets {
        let named = match by {
            By::Model => (bucket.key.model.clone(), Some(bucket.key.provider.clone())),
            By::Session => (bucket.key.session.clone(), Some(bucket.key.project.clone())),
            // Every other grouping falls back to the project, which is what a
            // machine-lens bucket can always answer.
            _ => (bucket.key.project.clone(), None),
        };
        let entry = rows.entry(named).or_insert((Tokens::default(), None));
        entry.0.add(&bucket.tokens);
        if let Some(cost) = bucket.cost_usd {
            entry.1 = Some(entry.1.unwrap_or(0.0) + cost);
        }
    }
    let mut found: Vec<Group> = rows
        .into_iter()
        .map(|((key, detail), (tokens, cost_usd))| Group {
            key,
            detail,
            tokens,
            cost_usd,
        })
        .collect();
    found.sort_by(|left, right| {
        right
            .tokens
            .total()
            .cmp(&left.tokens.total())
            .then_with(|| left.key.cmp(&right.key))
    });
    found
}

/// Everything the window holds, as one row.
pub fn totals(buckets: &[Bucket]) -> (Tokens, Option<f64>) {
    let mut tokens = Tokens::default();
    let mut cost: Option<f64> = None;
    for bucket in buckets {
        tokens.add(&bucket.tokens);
        if let Some(spent) = bucket.cost_usd {
            cost = Some(cost.unwrap_or(0.0) + spent);
        }
    }
    (tokens, cost)
}

/// Tokens a minute right now (§FS-013-burn.6).
///
/// Over the span in progress and the one before it, divided by the time those
/// actually cover. The span before is in because the one in progress may have
/// started a second ago, and a rate that read nearly zero on every span
/// boundary would be a number nobody could use.
pub fn rate(buckets: &[Bucket], now: DateTime<Utc>) -> u64 {
    let from = super::store::floor(now) - Duration::seconds(super::store::SPAN);
    let spent: u64 = buckets
        .iter()
        .filter(|bucket| bucket.at >= from)
        .map(|bucket| bucket.tokens.total())
        .sum();
    let minutes = ((now - from).num_seconds() / 60).max(1) as u64;
    spent / minutes
}

/// One bar of the sparkline: a span and what it cost.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub at: DateTime<Utc>,
    pub tokens: u64,
}

/// The window as a row of spans, oldest first, with the empty ones present:
/// a gap is a fact, and a sparkline that closed its gaps up would draw a
/// steady burn over an hour nothing ran in.
pub fn bars(buckets: &[Bucket], from: DateTime<Utc>, now: DateTime<Utc>) -> Vec<Bar> {
    let mut spans: BTreeMap<DateTime<Utc>, u64> = BTreeMap::new();
    let mut at = super::store::floor(from);
    let last = super::store::floor(now);
    while at <= last {
        spans.insert(at, 0);
        at += Duration::seconds(super::store::SPAN);
    }
    for bucket in buckets {
        if let Some(span) = spans.get_mut(&bucket.at) {
            *span = span.saturating_add(bucket.tokens.total());
        }
    }
    spans
        .into_iter()
        .map(|(at, tokens)| Bar { at, tokens })
        .collect()
}

/// A session that is still going, and what it is spending (§FS-013-burn.6).
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    pub session: String,
    pub project: String,
    pub model: String,
    pub source: String,
    pub subagent: bool,
    /// What it has spent over the spans it has been seen in.
    pub tokens: u64,
    /// Tokens a minute over those spans.
    pub rate: u64,
}

/// How far back a session counts as still going: the current span and the two
/// before it. Shorter and a session that pauses to think vanishes off the
/// strip and comes back.
const LIVE_SPANS: i64 = 3;

/// The sessions written to in the last few spans, busiest first.
pub fn live(buckets: &[Bucket], now: DateTime<Utc>) -> Vec<Live> {
    let from = super::store::floor(now) - Duration::seconds(super::store::SPAN * (LIVE_SPANS - 1));
    // A sub-agent's transcript carries the session id of the agent that
    // spawned it, so the flag is part of what tells two rows apart: without
    // it a session and its sub-agents merge into one row wearing whichever
    // flag arrived first (§FS-013-burn.3).
    let mut rows: BTreeMap<(String, String, bool), (Live, u64)> = BTreeMap::new();
    for bucket in buckets.iter().filter(|bucket| bucket.at >= from) {
        let entry = rows
            .entry((
                bucket.key.session.clone(),
                bucket.key.model.clone(),
                bucket.key.subagent,
            ))
            .or_insert_with(|| {
                (
                    Live {
                        session: bucket.key.session.clone(),
                        project: bucket.key.project.clone(),
                        model: bucket.key.model.clone(),
                        source: bucket.key.source.clone(),
                        subagent: bucket.key.subagent,
                        tokens: 0,
                        rate: 0,
                    },
                    0,
                )
            });
        entry.1 = entry.1.saturating_add(bucket.tokens.total());
    }
    let minutes = (super::store::SPAN * LIVE_SPANS / 60) as u64;
    let mut found: Vec<Live> = rows
        .into_values()
        .map(|(mut row, spent)| {
            row.tokens = spent;
            row.rate = spent / minutes.max(1);
            row
        })
        // On what it spent, never on the rounded rate: a session that spent a
        // little over fifteen minutes rounds to nought a minute and is still
        // very much going.
        .filter(|row| row.tokens > 0)
        .collect();
    found.sort_by(|left, right| {
        right
            .tokens
            .cmp(&left.tokens)
            .then_with(|| left.session.cmp(&right.session))
            .then_with(|| left.subagent.cmp(&right.subagent))
            .then_with(|| left.model.cmp(&right.model))
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::burn::Key;

    fn bucket(at: &str, project: &str, model: &str, session: &str, tokens: u64) -> Bucket {
        Bucket {
            at: at.parse::<DateTime<Utc>>().expect("a time"),
            key: Key {
                project: project.to_string(),
                source: "claude".to_string(),
                provider: "anthropic".to_string(),
                model: model.to_string(),
                session: session.to_string(),
                subagent: false,
            },
            tokens: Tokens {
                output: tokens,
                ..Tokens::default()
            },
            cost_usd: None,
        }
    }

    fn window() -> Vec<Bucket> {
        vec![
            bucket("2026-09-03T10:00:00Z", "app", "opus", "s1", 100),
            bucket("2026-09-03T10:05:00Z", "app", "haiku", "s1", 10),
            bucket("2026-09-03T10:05:00Z", "lib", "opus", "s2", 40),
        ]
    }

    #[test]
    fn a_grouping_sums_the_window_biggest_first() {
        let by_project = group(&window(), By::Project);
        assert_eq!(by_project.len(), 2);
        assert_eq!(by_project[0].key, "app");
        assert_eq!(by_project[0].tokens.output, 110);
        assert_eq!(by_project[1].key, "lib");

        let by_model = group(&window(), By::Model);
        assert_eq!(by_model[0].key, "opus");
        assert_eq!(by_model[0].tokens.output, 140);
        assert_eq!(by_model[0].detail.as_deref(), Some("anthropic"));

        let by_session = group(&window(), By::Session);
        assert_eq!(by_session[0].key, "s1");
        assert_eq!(by_session[0].tokens.output, 110);
    }

    /// A grouping picks the lens that can answer it, and the two are never
    /// mixed (§FS-013-burn.1).
    #[test]
    fn a_grouping_selects_its_lens() {
        assert_eq!(By::Project.lens(), Lens::Machine);
        assert_eq!(By::Model.lens(), Lens::Machine);
        assert_eq!(By::Session.lens(), Lens::Machine);
        assert_eq!(By::Plan.lens(), Lens::Work);
        assert_eq!(By::Matter.lens(), Lens::Work);
    }

    /// Empty spans stay in the row: a gap is a fact about the window.
    #[test]
    fn the_spans_of_a_window_keep_their_gaps() {
        let now = "2026-09-03T10:12:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        let drawn = bars(&window(), now - Duration::minutes(20), now);
        assert_eq!(drawn.len(), 5);
        assert_eq!(drawn[0].tokens, 0);
        assert_eq!(drawn[2].tokens, 100);
        assert_eq!(drawn[3].tokens, 50);
    }

    /// The rate is the last span only, and a session that has stopped drops
    /// off the live strip rather than being reported as still going.
    #[test]
    fn the_rate_and_the_strip_are_about_now() {
        let now = "2026-09-03T10:07:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        assert_eq!(
            rate(&window(), now),
            21,
            "150 tokens over the seven minutes since 10:00"
        );
        let going = live(&window(), now);
        assert_eq!(going.len(), 3, "{going:?}");

        let later = "2026-09-03T11:00:00Z"
            .parse::<DateTime<Utc>>()
            .expect("later");
        assert_eq!(rate(&window(), later), 0);
        assert!(live(&window(), later).is_empty());
    }

    /// A sub-agent's transcript carries the session id of whatever spawned
    /// it, so a strip keyed on the session alone shows one row holding both
    /// and tagged for whichever arrived first (§FS-013-burn.3).
    #[test]
    fn a_session_and_its_sub_agents_are_two_rows() {
        let now = "2026-09-03T10:07:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        let mut own = bucket("2026-09-03T10:05:00Z", "app", "opus", "s1", 100);
        let mut theirs = bucket("2026-09-03T10:05:00Z", "app", "opus", "s1", 30);
        theirs.key.subagent = true;
        own.key.subagent = false;
        let going = live(&[own, theirs], now);
        assert_eq!(going.len(), 2, "they were added together: {going:?}");
        let session = going
            .iter()
            .find(|row| !row.subagent)
            .expect("the session's own row");
        let sub = going
            .iter()
            .find(|row| row.subagent)
            .expect("the sub-agents' row");
        assert_eq!(session.tokens, 100);
        assert_eq!(sub.tokens, 30);
    }

    /// Both cycles come back round, so a key held down never wedges
    /// (§FS-013-burn.8).
    #[test]
    fn the_cycles_come_back_round() {
        let mut at = Window::default();
        for _ in 0..4 {
            at = at.next();
        }
        assert_eq!(at, Window::default());
        let mut by = By::default();
        for _ in 0..5 {
            by = by.next();
        }
        assert_eq!(by, By::default());
    }
}
