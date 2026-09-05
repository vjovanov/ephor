//! What a provider has left, as ephor is *told* it (§FS-005-dispatch.29).
//!
//! A list of alternates is only worth writing if something can say that the
//! first of them cannot be had right now. That something is a quota, and a
//! quota is the provider's fact rather than ephor's — ephor observes and
//! summons, and it never governs (§REQ-001-boundary). So everything here
//! consumes a *report* and nothing here derives one
//! (§DA-009-headroom-vetoes).
//!
//! Two channels report, and one of them costs nothing. The **ledger** is the
//! shipped default: a start that failed carrying an instant ephor can read is
//! a refusal on that hand's pool until the instant. The **bound verb** is
//! richer and optional: `work.headroom` names one command per pool, summoned
//! exactly as every other command this interface names and answering in the
//! standard envelope. Neither is a number ephor worked out for itself, which
//! is the whole of why either may be believed.
//!
//! The rule the two feed is a **veto**, never a ranking: [`Evidence::spent`]
//! answers about one pool alone, and [`choose`] walks the author's order
//! passing over what is known spent. There is nowhere in it for an unknown to
//! be sorted against anything, which is what makes "unknown never demotes"
//! structural rather than a branch somebody has to remember to write.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::work::ledger::{Ledger, PoolRecord, Window};
use crate::work::recipe::WorkConfig;
use crate::work::runtime::roster::{Hand, Roster};

/// The verb this seam binds, as configuration names it — for messages, so the
/// key a reader has to go and write appears in the sentence telling them to.
pub const VERB: &str = "work.headroom";

/// How long a probe is given before it is abandoned as unknown. A headroom
/// report is one small question to a credential the person is already signed
/// in to; a verb that takes longer than this is stuck, and a refresh must not
/// be held by it.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Who serves the model a hand carries (§FS-005-dispatch.29): its provider
/// where the roster gives it one, and its agent id where it does not.
///
/// That is the smallest thing a window is actually bought against — two hands
/// served by one provider spend one allowance whichever of them is asked — so
/// keying evidence any finer would make a refusal that has already happened
/// look like it happened to somebody else. A hand naming neither falls back to
/// its own id, which is the only name left that anything could be bought
/// against.
pub fn pool_of(hand: &Hand) -> String {
    hand.provider
        .clone()
        .or_else(|| hand.agent.clone())
        .unwrap_or_else(|| hand.id.clone())
}

/// Every pool the roster reaches, in the roster's own order.
pub fn pools_of(roster: &Roster) -> Vec<String> {
    let mut seen = BTreeSet::new();
    roster
        .hands
        .iter()
        .map(pool_of)
        .filter(|pool| seen.insert(pool.clone()))
        .collect()
}

/// What is known about one pool right now: the two channels folded into the
/// three things a decision and a report both need — how much is left, when
/// something about it lifts, and why a number is missing.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Standing {
    pub pool: String,
    /// The least of the *known* windows (§FS-005-dispatch.29). None is
    /// unknown, and unknown is never zero: a window that reported no number is
    /// simply not among them, so one unreadable window never lowers a pool
    /// another window reported healthy.
    pub remaining: Option<f64>,
    /// Why there is no number, or what was refused — the degrade rule's second
    /// half, which is as much the requirement as the first
    /// (§REQ-001-boundary.1). The reason alone: the instants live in the two
    /// fields below, so a surface composes the sentence once rather than
    /// finding a date already inside the words. None only where a number is
    /// known and nothing was refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// An unexpired refusal ephor recorded, and the instant it lifts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused_until: Option<DateTime<Utc>>,
    /// The earliest instant anything known about this pool resets — what the
    /// note names when every member of a list is vetoed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,
    /// How many runs ephor started on this pool. Shown, and never read into
    /// the rule (§FS-005-dispatch.29): counting one's own spawns is deriving a
    /// quota under another name, and it is wrong the first moment anything
    /// else spends from the same credential.
    #[serde(skip_serializing_if = "is_zero")]
    pub spawns: u64,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

/// Why a pool is passed over, and until when — the sentence a ticket carries
/// about the hand it did not go to.
#[derive(Debug, Clone, PartialEq)]
pub struct Spent {
    pub why: String,
    pub until: Option<DateTime<Utc>>,
}

/// What every pool this site can reach says about itself, with the floor the
/// site set. Read once per decision and handed to [`choose`], so that the rule
/// is a pure function of evidence rather than of when the disk was last read.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// The fraction at or below which a pool counts as spent, `0` unless the
    /// site named another (§FS-005-dispatch.29).
    pub floor: f64,
    pub pools: BTreeMap<String, Standing>,
    now: Option<DateTime<Utc>>,
}

impl Evidence {
    /// What the ledger and the site's bindings say, as of `now`. Reads what
    /// the last refresh left; nothing here fetches, because probing is
    /// fetching and fetching runs under `refresh` (§FS-005-dispatch.29).
    pub fn read(config: &WorkConfig, ledger: &Ledger, now: DateTime<Utc>) -> Evidence {
        let mut pools: BTreeMap<String, Standing> = BTreeMap::new();
        for (pool, record) in &ledger.pools {
            pools.insert(pool.clone(), standing(pool, record, now));
        }
        // A pool whose verb is bound but which nothing has reported on yet is
        // still a pool, and it is unknown for a reason worth saying.
        for pool in config.headroom.keys() {
            pools.entry(pool.clone()).or_insert_with(|| Standing {
                pool: pool.clone(),
                why: Some(format!(
                    "{VERB} names a command for this pool and no refresh has run it yet"
                )),
                ..Standing::default()
            });
        }
        Evidence {
            floor: config.headroom_floor.unwrap_or(0.0),
            pools,
            now: Some(now),
        }
    }

    /// What is known about one pool, including the pool nothing was ever said
    /// about — which is unknown, with the reason being that nothing reports on
    /// it (§REQ-001-boundary.1).
    pub fn standing(&self, pool: &str, bound: bool) -> Standing {
        self.pools.get(pool).cloned().unwrap_or_else(|| Standing {
            pool: pool.to_string(),
            why: Some(match bound {
                true => {
                    format!("{VERB} names a command for this pool and no refresh has run it yet")
                }
                false => format!(
                    "nothing reports on this pool — bind a command for it under {VERB} \
                         to have a number here"
                ),
            }),
            ..Standing::default()
        })
    }

    /// Whether this pool is **known spent** — the only thing that may pass a
    /// member over (§FS-005-dispatch.29). An unexpired refusal, or an
    /// effective remaining at or under the floor. Everything else, unknown
    /// included, answers None: there is no branch here for a missing number
    /// because a missing number is not evidence of anything.
    pub fn spent(&self, pool: &str) -> Option<Spent> {
        let standing = self.pools.get(pool)?;
        let now = self.now?;
        if let Some(until) = standing.refused_until.filter(|until| now < *until) {
            return Some(Spent {
                why: match &standing.why {
                    Some(says) => format!("{pool} refused a start ephor made: {says}"),
                    None => format!("{pool} refused a start ephor made"),
                },
                until: Some(until),
            });
        }
        let remaining = standing.remaining?;
        (remaining <= self.floor).then(|| Spent {
            why: format!("{pool} reports {remaining} of its window left"),
            until: standing.resets_at,
        })
    }
}

/// One pool's record, read into what a decision and a report both need.
fn standing(pool: &str, record: &PoolRecord, now: DateTime<Utc>) -> Standing {
    let refused_until = record.refused_until.filter(|until| now < *until);
    // The least of the *known* windows: the unknown ones are skipped rather
    // than folded in as a zero or a one, so neither can move the answer.
    let remaining = record
        .windows
        .iter()
        .filter_map(|window| window.remaining)
        .fold(None, |least: Option<f64>, value| {
            Some(least.map_or(value, |least| least.min(value)))
        });
    let mut resets: Vec<DateTime<Utc>> = record
        .windows
        .iter()
        .filter_map(|window| window.resets_at)
        .collect();
    resets.extend(record.refused_until);
    // The reason, and only the reason: the instants are fields of their own, so
    // a surface writes "unknown until <instant> — <reason>" once instead of
    // every reason carrying a date inside its words.
    let why = match (&refused_until, remaining) {
        (Some(_), _) => Some(
            record
                .says
                .clone()
                .unwrap_or_else(|| "a start ephor made here was refused".to_string()),
        ),
        // Unknown, and the degrade rule asks why out loud.
        (None, None) => Some(
            record
                .unreadable
                .clone()
                .unwrap_or_else(|| format!("nothing readable has been reported about {pool} yet")),
        ),
        (None, Some(_)) => record.unreadable.clone(),
    };
    Standing {
        pool: pool.to_string(),
        remaining,
        why,
        refused_until,
        resets_at: resets.into_iter().min(),
        spawns: record.spawns,
    }
}

/// One member of a pin, settled against the roster and carrying the pool its
/// work would be bought against.
#[derive(Debug, Clone)]
pub struct Member {
    pub hand: Hand,
    pub effort: Option<String>,
    /// What settling the effort had to say, where it had anything.
    pub note: Option<String>,
    pub pool: String,
}

/// Which member of a list gets the work, and what the choosing had to say.
///
/// **The rule is a veto over the known spent, never a ranking over the known
/// remaining.** The list is walked in the author's order and a member is
/// passed over exactly when its pool is known spent; nothing is sorted and no
/// two members' numbers are ever compared, because the order inside a list is
/// the author's judgment about fitness and a rule that reordered it would be
/// overruling the only judgment in it (§FS-005-dispatch.14).
///
/// Where every member is vetoed the **first** still gets the ticket, carrying
/// the earliest instant any of their pools resets: a ticket that is written and
/// waits is work a person can see and start by hand, and work that silently
/// never dispatched is neither (§FS-005-dispatch.29). The fallback lives inside
/// the list; nothing here reaches past it.
pub fn choose(members: &[Member], evidence: &Evidence) -> (usize, Option<String>) {
    let mut passed: Vec<String> = Vec::new();
    let mut resets: Vec<DateTime<Utc>> = Vec::new();
    for (index, member) in members.iter().enumerate() {
        let Some(spent) = evidence.spent(&member.pool) else {
            let said = (!passed.is_empty()).then(|| {
                format!(
                    "'{}' takes this, and it is not the first hand named here. {}",
                    member.hand.id,
                    passed.join(". ")
                )
            });
            return (index, said);
        };
        passed.push(format!(
            "'{}' was passed over: {}{}",
            member.hand.id,
            spent.why,
            match spent.until {
                Some(until) => format!(", which lifts at {}", instant(&until)),
                None => String::new(),
            }
        ));
        resets.extend(spent.until);
    }
    // Every member vetoed. The first still gets it, and the note names the
    // earliest instant any of their pools resets.
    let said = members.first().map(|first| {
        format!(
            "every hand named here is spent, so '{}' — the first — takes the ticket and it \
             waits. {}. The earliest of their windows lifts at {}",
            first.hand.id,
            passed.join(". "),
            match resets.iter().min() {
                Some(earliest) => instant(earliest),
                None => "an instant none of them named".to_string(),
            }
        )
    });
    (0, said)
}

/// The words a report and a ticket both name an instant with. RFC 3339 to the
/// second in UTC, which is the spelling the envelope's own `resets_at` uses —
/// one instant should not read two ways depending on which surface printed it.
pub fn instant(at: &DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// An instant out of a refusal's own words, where they carry one ephor can
/// read (§FS-005-dispatch.29).
///
/// A refusal names the instant it lifts, and that is the whole of what makes a
/// failed start evidence about a pool rather than about one root. Where the
/// words carry no readable instant this answers None and the caller claims
/// nothing: a failure ephor cannot date is not a window it may guess at, and
/// the start stays exactly what it was — one root's own doubling back-off
/// (§FS-005-dispatch.24).
///
/// Nothing here knows any vendor's phrasing (§REQ-001-boundary.5). It looks for
/// a timestamp, in the one spelling a machine-readable instant is written in,
/// and takes the earliest it finds: a message naming several is naming the
/// soonest thing that could lift.
pub fn instant_in(says: &str) -> Option<DateTime<Utc>> {
    says.split(|character: char| character.is_whitespace())
        .filter_map(|word| {
            let word = word.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '+' && character != '-'
            });
            DateTime::parse_from_rfc3339(word).ok()
        })
        .map(|at| at.with_timezone(&Utc))
        .min()
}

/// The payload a bound verb writes on `data` (§FS-005-dispatch.29): a list of
/// windows, each with a name, how much of it is left as a fraction, and when it
/// resets. `remaining` absent or null is unknown, and it stays unknown all the
/// way through — it is never filled in with a number.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct Payload {
    #[serde(default)]
    windows: Vec<Window>,
}

/// Ask every bound pool what it has left, and write what it said into the
/// ledger.
///
/// Probing is fetching, so this runs where fetching runs and never in front of
/// a dispatch (§FS-005-dispatch.29): a network call between a reader and a
/// ticket is the thing this placement exists to avoid. Nothing here can fail
/// the caller — a verb that is unbound, that exits non-zero, that writes
/// nothing, or that writes something unreadable all leave the pool *unknown*
/// with the reason beside it, which is the degrade rule
/// (§REQ-001-boundary.1).
/// `fresh_within` is the freshness discipline: a pool reported on more
/// recently than that is left alone, so a reading that only refetches what has
/// aged does not ask every provider again. None asks every bound pool, which
/// is what an explicit `refresh` does for every other source.
pub fn probe(
    config: &WorkConfig,
    ledger: &mut Ledger,
    now: DateTime<Utc>,
    fresh_within: Option<chrono::TimeDelta>,
) {
    if config.headroom.is_empty() {
        return;
    }
    let place = crate::paths::state_dir();
    if std::fs::create_dir_all(&place).is_err() {
        return;
    }
    let site = crate::seams::summons::Site::root(&place);
    for (pool, binding) in &config.headroom {
        let asked_lately = fresh_within.is_some_and(|within| {
            ledger.pools.get(pool).is_some_and(|record| {
                record
                    .reported_at
                    .is_some_and(|reported| now - reported < within)
            })
        });
        if asked_lately {
            continue;
        }
        let summons =
            crate::seams::summons::Summons::new(format!("{VERB}.{pool}"), binding.clone())
                .at(crate::seams::summons::Place::Root);
        let record = ledger.pools.entry(pool.clone()).or_default();
        record.reported_at = Some(now);
        match reading(&summons, &site) {
            Ok(windows) => {
                record.windows = windows;
                record.unreadable = None;
                if record
                    .windows
                    .iter()
                    .all(|window| window.remaining.is_none())
                {
                    record.unreadable = Some(format!(
                        "'{binding}' answered, and named no window with a number in it"
                    ));
                }
            }
            Err(why) => {
                // The last report is dropped rather than kept: a number nobody
                // has re-confirmed is a claim about a moment that has passed,
                // and unknown is the honest answer for it.
                record.windows.clear();
                record.unreadable = Some(why);
            }
        }
    }
}

/// One pool's report, or the one sentence saying why there is none.
fn reading(
    summons: &crate::seams::summons::Summons,
    site: &crate::seams::summons::Site,
) -> std::result::Result<Vec<Window>, String> {
    let answer = crate::seams::summons::run(
        summons,
        site,
        crate::seams::summons::Mode::Captured(PROBE_TIMEOUT),
    )
    .map_err(|err| format!("'{}' could not be run: {err}", summons.binding))?;
    if !answer.is_done() {
        return Err(format!(
            "'{}' {}",
            summons.binding,
            match answer.exit_code {
                Some(code) => format!("exited {code}"),
                None => "was killed".to_string(),
            }
        ));
    }
    // Not standard output: stdout is the command's own, and a contract that
    // parsed it would make an honest log a protocol violation
    // (§FS-006-project-interface.3).
    let answered = answer.answer.ok_or_else(|| {
        format!(
            "'{}' wrote nothing to the file $EPHOR_ANSWER named",
            summons.binding
        )
    })?;
    let payload =
        serde_json::from_value::<Payload>(serde_json::Value::Object(answered.facts.data.clone()))
            .map_err(|err| format!("'{}' answered something unreadable: {err}", summons.binding))?;
    match payload.windows.is_empty() {
        true => Err(format!(
            "'{}' answered with no windows on 'data'",
            summons.binding
        )),
        false => Ok(payload.windows),
    }
}

#[cfg(test)]
#[path = "headroom_tests.rs"]
mod tests;
