//! What unattended work may spend, as the person who pays wrote it down
//! (§FS-015-spend-ceiling).
//!
//! A second dimension beside the concurrency ceilings, at the same three
//! nested scopes and read by the same sweep. Everything here is a
//! **comparison against a reading somebody else took**: the number a ceiling
//! is measured against is the one `burn` publishes for the same window and the
//! same project, read out of the store `burn` keeps. Nothing in this module
//! prices a token, holds a rate, or knows what a model costs — a price ephor
//! computed would be a price book outside its adapter
//! (§REQ-001-boundary.5), and a second accounting of ephor's own would be a
//! ceiling nobody could check against the reading it quotes.
//!
//! Two denominations ship because they fail differently
//! (§FS-015-spend-ceiling.2). Tokens are always measured; a dollar figure is
//! only ever the one a log carried already, so a window nothing priced binds
//! nothing in dollars — [`dollars_full`] answers `None` there rather than
//! reading an absent price as a zero, which is the rule this module exists to
//! get right.
//!
//! It lives beside [`crate::work::headroom`] rather than inside
//! [`crate::work`] for the reason that one does: a ceiling dimension is a
//! module, and the file the ceilings are *evaluated* in is already at its
//! measured budget (§FS-012-file-size).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::branches::Placement;
use crate::burn::attribution;
use crate::burn::{store, Bucket};
use crate::feed::config::StatusConfig;
use crate::work::recipe::{OrganizationWorkConfig, ProjectWorkConfig, WorkConfig};

/// The one currency a ceiling may be written in (§FS-015-spend-ceiling.3).
///
/// Present from the start so the shape never has to grow a key later, and
/// accepting nothing else because converting is pricing. An enum rather than a
/// string so the refusal is serde's own — it names what was written and what
/// is accepted, which is the whole requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Currency {
    #[serde(rename = "USD")]
    Usd,
}

impl Currency {
    fn label(self) -> &'static str {
        match self {
            Currency::Usd => "USD",
        }
    }
}

/// The trailing window a budget is measured over (§FS-015-spend-ceiling.3).
///
/// Exactly the windows the store can answer, because a window the store cannot
/// answer is a budget that cannot bind. `day` and `week` are spellings of the
/// last two, written as aliases, and serde lists an alias beside the variant it
/// spells — so an unreadable window is refused naming all six accepted
/// spellings rather than only the four windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum Per {
    #[serde(rename = "1h")]
    Hour,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "24h", alias = "day")]
    Day,
    #[serde(rename = "7d", alias = "week")]
    Week,
}

impl Per {
    /// The reading's own window, so a ceiling and `burn` measure one span
    /// rather than two (§FS-013-burn.6).
    pub fn window(self) -> crate::burn::query::Window {
        match self {
            Per::Hour => crate::burn::query::Window::Hour,
            Per::SixHours => crate::burn::query::Window::SixHours,
            Per::Day => crate::burn::query::Window::Day,
            Per::Week => crate::burn::query::Window::Week,
        }
    }

    fn label(self) -> &'static str {
        self.window().label()
    }

    fn span(self) -> Duration {
        self.window().span()
    }
}

/// A ceiling on dollars (§FS-015-spend-ceiling.1).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpendBudget {
    #[serde(deserialize_with = "amount")]
    pub amount: f64,
    pub currency: Currency,
    pub per: Per,
}

/// A ceiling on tokens, which carries no currency because tokens are not
/// denominated in anything (§FS-015-spend-ceiling.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenBudget {
    pub amount: u64,
    pub per: Per,
}

/// A budget amount is a number and is not negative (§FS-015-spend-ceiling.3).
/// Refused where a site's own typos already land, which is the only outcome
/// that cannot be mistaken for a ceiling quietly doing nothing.
fn amount<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let amount = f64::deserialize(deserializer)?;
    if !amount.is_finite() || amount < 0.0 {
        return Err(serde::de::Error::custom(format!(
            "a budget amount is a number and is not negative, and {amount} is neither"
        )));
    }
    Ok(amount)
}

/// One place a budget may be written, spelled as the key that carries it
/// (§FS-015-spend-ceiling.9) — the spelling the concurrency ceilings already
/// refuse in, so a reader who has read one reason can read this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Site,
    Organization(String),
    Project(String),
}

impl Scope {
    fn key(&self) -> String {
        match self {
            Scope::Site => "global work".to_string(),
            Scope::Organization(id) => format!("organizations.{id}.work"),
            Scope::Project(id) => format!("projects.{id}.work"),
        }
    }

    /// Whether this scope's ceiling counts what one project spent. An
    /// organization's total includes every project the registry places in it,
    /// which is what makes the outer number the wider one. The site counts
    /// every bucket the reading carries, `other` included — the site total is
    /// the total `burn` prints, so spend no watched project claimed is still
    /// spend this machine made (§FS-015-spend-ceiling.1).
    fn holds(&self, project: &str, membership: &BTreeMap<String, String>) -> bool {
        match self {
            Scope::Site => true,
            Scope::Organization(id) => membership.get(project) == Some(id),
            Scope::Project(id) => id == project,
        }
    }

    /// Whether a root touching these projects is under this scope at all.
    fn over(&self, projects: &[String], membership: &BTreeMap<String, String>) -> bool {
        match self {
            Scope::Site => true,
            _ => projects
                .iter()
                .any(|project| self.holds(project, membership)),
        }
    }
}

/// Whether a full budget stops this sweep or is only said to it
/// (§FS-015-spend-ceiling.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Budget {
    /// Nobody is present — a timer asked for this. A full ceiling passes the
    /// root over.
    Binds,
    /// A person typed the command. A full ceiling is said on the error stream
    /// and the run starts anyway: the cap is the human's leash on the machine,
    /// not on the human.
    Warns,
}

/// A ceiling that is full, and everything a refusal and a pause both need
/// (§FS-015-spend-ceiling.9, §FS-015-spend-ceiling.10).
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// The key that carries it: `global work.max_spend` and its two siblings.
    pub scope: String,
    /// The whole sentence, which is what a passed-over root's reason is.
    pub says: String,
    /// When the trailing window lets work start again, where anything ages out
    /// into room. Absent under a ceiling of `0`, which is a pause its author
    /// meant rather than a window that will pass.
    pub resumes_at: Option<String>,
}

impl Refusal {
    /// The machine form of the pause: the scope and the instant, and nothing
    /// else — the totals and the coverage stay in `burn`
    /// (§FS-015-spend-ceiling.10).
    pub fn pause(&self) -> Paused {
        Paused {
            scope: self.scope.clone(),
            resumes_at: self.resumes_at.clone(),
        }
    }
}

/// What `ephor status --json` carries on every project a full ceiling holds
/// (§FS-015-spend-ceiling.10).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Paused {
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resumes_at: Option<String>,
}

/// What the store says one scope spent in one window.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Measured {
    /// The dollars the window's records actually carried. `None` where nothing
    /// in the window carried a price at all — which is not zero and is not a
    /// full ceiling either (§FS-015-spend-ceiling.7).
    cost_usd: Option<f64>,
    tokens: u64,
    /// The tokens of records nothing priced: real spend nothing could put a
    /// figure on, shown beside the dollar total rather than summed into it.
    unpriced_tokens: u64,
}

/// The budgets a site declared, with the reading each is measured against
/// already taken.
///
/// Taken once and kept, because a sweep asks this of every due root and the
/// answer is about the window rather than about the root.
#[derive(Debug, Default)]
pub struct Budgets {
    /// The scopes that declared a budget, outermost first — site, then
    /// organizations, then projects — so walking this is walking the
    /// resolution order (§FS-015-spend-ceiling.5).
    bounds: Vec<Bound>,
    membership: BTreeMap<String, String>,
}

#[derive(Debug)]
struct Bound {
    scope: Scope,
    /// Why this scope refuses a start, where it does. Money before tokens,
    /// which is the order within a scope.
    full: Option<Refusal>,
}

impl Budgets {
    /// Read every declared budget against the store, refreshing it first where
    /// it has gone stale (§FS-015-spend-ceiling.1).
    ///
    /// A site that declared none reads nothing and refuses nobody: omission is
    /// unlimited, and a configuration that names no budget behaves exactly as
    /// it did before this existed (§FS-015-spend-ceiling.4).
    pub fn read(
        global: &WorkConfig,
        organizations: &BTreeMap<String, OrganizationWorkConfig>,
        projects: &BTreeMap<String, ProjectWorkConfig>,
        membership: BTreeMap<String, String>,
        roots: &attribution::Roots,
        now: DateTime<Utc>,
    ) -> Budgets {
        let mut declared: Vec<(Scope, Option<SpendBudget>, Option<TokenBudget>)> =
            vec![(Scope::Site, global.max_spend, global.max_tokens)];
        for (id, work) in organizations {
            declared.push((
                Scope::Organization(id.clone()),
                work.max_spend,
                work.max_tokens,
            ));
        }
        for (id, work) in projects {
            declared.push((Scope::Project(id.clone()), work.max_spend, work.max_tokens));
        }
        declared.retain(|(_, spend, tokens)| spend.is_some() || tokens.is_some());
        if declared.is_empty() {
            return Budgets::default();
        }
        // The sweep runs from a timer with nobody present, so it takes the
        // reading itself rather than depending on somebody having opened one
        // first. A local file read of logs the agent tools were writing
        // anyway, never the fetch `refresh` owns (§FS-015-spend-ceiling.1).
        crate::burn::refreshed(roots, false);
        let longest = declared
            .iter()
            .flat_map(|(_, spend, tokens)| {
                [
                    spend.map(|budget| budget.per),
                    tokens.map(|budget| budget.per),
                ]
            })
            .flatten()
            .map(Per::span)
            .max()
            .unwrap_or_else(|| Per::Hour.span());
        let read = store::since(&store::dir(), now - longest, now);
        let bounds = declared
            .into_iter()
            .map(|(scope, spend, tokens)| {
                let mine: Vec<&Bucket> = read
                    .iter()
                    .filter(|bucket| scope.holds(&bucket.key.project, &membership))
                    .collect();
                let full = refusal(&scope, spend.as_ref(), tokens.as_ref(), &mine, now);
                Bound { scope, full }
            })
            .collect();
        Budgets { bounds, membership }
    }

    /// Why one scope refuses a start, where it does — asked by the sweep at
    /// the point that scope's other ceilings are asked, so roots in flight and
    /// working roots keep answering first (§FS-015-spend-ceiling.5).
    pub fn at(&self, scope: &Scope) -> Option<&Refusal> {
        self.bounds
            .iter()
            .find(|bound| bound.scope == *scope)
            .and_then(|bound| bound.full.as_ref())
    }

    /// The outermost budget that is full over these projects, which is the one
    /// a reader is sent to.
    pub fn over(&self, projects: &[String]) -> Option<&Refusal> {
        self.bounds
            .iter()
            .filter(|bound| bound.scope.over(projects, &self.membership))
            .find_map(|bound| bound.full.as_ref())
    }

    /// The pause one project is under, where a ceiling holding it is currently
    /// full. A fact about the ceiling, not about the queue: nothing has to be
    /// due for this to be true (§FS-015-spend-ceiling.10).
    pub fn binding(&self, project: &str) -> Option<&Refusal> {
        let one = [project.to_string()];
        self.over(&one)
    }
}

/// Whether one `work` block wrote a budget at all.
fn named(spend: Option<SpendBudget>, tokens: Option<TokenBudget>) -> bool {
    spend.is_some() || tokens.is_some()
}

/// Which of a scope's two ceilings refuses a start, in the order the scope
/// asks them: money, then tokens (§FS-015-spend-ceiling.5).
fn refusal(
    scope: &Scope,
    spend: Option<&SpendBudget>,
    tokens: Option<&TokenBudget>,
    mine: &[&Bucket],
    now: DateTime<Utc>,
) -> Option<Refusal> {
    let key = scope.key();
    spend
        .and_then(|budget| dollars_full(&key, budget, mine, now))
        .or_else(|| tokens.and_then(|budget| tokens_full(&key, budget, mine, now)))
}

/// The dollar ceiling, where it is full.
///
/// **A window nothing priced binds nothing here** (§FS-015-spend-ceiling.7):
/// `Measured::cost_usd` is `None` when no record in the window carried a
/// figure, and this answers `None` with it rather than comparing a zero it
/// invented. A cap that stopped the machine because accounting was absent
/// would fail over an extractor outage rather than over money.
fn dollars_full(
    key: &str,
    budget: &SpendBudget,
    mine: &[&Bucket],
    now: DateTime<Utc>,
) -> Option<Refusal> {
    let scope = format!("{key}.max_spend");
    let money = budget.currency.label();
    let per = budget.per.label();
    if budget.amount == 0.0 {
        return Some(Refusal {
            says: format!("{scope} 0 {money} per {per} admits no new autorun starts"),
            scope,
            resumes_at: None,
        });
    }
    let window = within(mine, budget.per, now);
    let measured = measure(&window);
    let spent = measured.cost_usd?;
    if spent < budget.amount {
        return None;
    }
    let lifts = resumes(
        &spans(&window, |bucket| bucket.cost_usd.unwrap_or(0.0)),
        spent,
        budget.amount,
        budget.per.span(),
    );
    // A hole that is not there is not reported as an empty one
    // (§FS-015-spend-ceiling.9).
    let unpriced = match measured.unpriced_tokens {
        0 => String::new(),
        held => format!(", {held} token(s) unpriced"),
    };
    Some(Refusal {
        says: format!(
            "{scope} {} {money} per {per} is full ({spent:.2} {money} measured{unpriced}){}",
            budget.amount,
            resumed(lifts)
        ),
        scope,
        resumes_at: lifts.map(instant),
    })
}

/// The token ceiling, where it is full — the one that still binds when the
/// pricing is missing, which is what §FS-015-spend-ceiling.2 is about.
fn tokens_full(
    key: &str,
    budget: &TokenBudget,
    mine: &[&Bucket],
    now: DateTime<Utc>,
) -> Option<Refusal> {
    let scope = format!("{key}.max_tokens");
    let per = budget.per.label();
    if budget.amount == 0 {
        return Some(Refusal {
            says: format!("{scope} 0 per {per} admits no new autorun starts"),
            scope,
            resumes_at: None,
        });
    }
    let window = within(mine, budget.per, now);
    let measured = measure(&window);
    if measured.tokens < budget.amount {
        return None;
    }
    let lifts = resumes(
        &spans(&window, |bucket| bucket.tokens.total() as f64),
        measured.tokens as f64,
        budget.amount as f64,
        budget.per.span(),
    );
    Some(Refusal {
        says: format!(
            "{scope} {} per {per} is full ({} token(s) measured){}",
            budget.amount,
            measured.tokens,
            resumed(lifts)
        ),
        scope,
        resumes_at: lifts.map(instant),
    })
}

/// The buckets of one trailing window, which is the span the reading itself
/// uses rather than a calendar period.
fn within<'a>(mine: &[&'a Bucket], per: Per, now: DateTime<Utc>) -> Vec<&'a Bucket> {
    let from = now - per.span();
    mine.iter()
        .copied()
        .filter(|bucket| bucket.at >= from)
        .collect()
}

/// What a window holds, with unknown kept apart from zero (§FS-013-burn.7).
fn measure(window: &[&Bucket]) -> Measured {
    let mut measured = Measured::default();
    for bucket in window {
        let tokens = bucket.tokens.total();
        measured.tokens = measured.tokens.saturating_add(tokens);
        match bucket.cost_usd {
            Some(cost) => measured.cost_usd = Some(measured.cost_usd.unwrap_or(0.0) + cost),
            None => {
                measured.unpriced_tokens = measured.unpriced_tokens.saturating_add(tokens);
            }
        }
    }
    measured
}

/// The window folded into its spans, oldest first — the order they age out in.
fn spans(window: &[&Bucket], of: impl Fn(&Bucket) -> f64) -> Vec<(DateTime<Utc>, f64)> {
    let mut folded: BTreeMap<DateTime<Utc>, f64> = BTreeMap::new();
    for bucket in window {
        *folded.entry(bucket.at).or_insert(0.0) += of(bucket);
    }
    folded.into_iter().collect()
}

/// The earliest instant the trailing window is no longer full, computed from
/// what is already measured (§FS-015-spend-ceiling.8).
///
/// An earliest rather than a promise: spend recorded between now and then
/// moves it later, which is why it is recomputed from the reading every time
/// it is asked for rather than remembered. The window empties oldest span
/// first, and a span leaves it one window after the span began.
fn resumes(
    spans: &[(DateTime<Utc>, f64)],
    mut total: f64,
    ceiling: f64,
    window: Duration,
) -> Option<DateTime<Utc>> {
    for (at, spent) in spans {
        total -= spent;
        if total < ceiling {
            return Some(*at + window);
        }
    }
    None
}

fn resumed(lifts: Option<DateTime<Utc>>) -> String {
    match lifts {
        Some(at) => format!("; resumes {}", instant(at)),
        None => String::new(),
    }
}

/// RFC 3339, to the second, in UTC, wherever an instant is written — a time
/// somebody has to compare against `date` is not a place for a second format
/// (§FS-015-spend-ceiling.8).
fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Whether this site wrote a budget anywhere.
///
/// Asked before anything is read, so a site that named none pays nothing for
/// the dimension existing — no registry walk, no store refresh, no reading
/// (§FS-015-spend-ceiling.4).
pub fn declared(config: &StatusConfig) -> bool {
    named(config.work.max_spend, config.work.max_tokens)
        || config
            .organizations
            .values()
            .any(|organization| named(organization.work.max_spend, organization.work.max_tokens))
        || config
            .projects
            .values()
            .any(|project| named(project.work.max_spend, project.work.max_tokens))
}

/// Which of these projects are under a ceiling that is currently binding, and
/// what to say about each (§FS-015-spend-ceiling.10).
///
/// Empty where no budget is written and empty where none is full, so on both
/// surfaces the answer's presence is the signal.
pub fn paused(config: &StatusConfig, projects: &[String]) -> BTreeMap<String, Paused> {
    if !declared(config) {
        return BTreeMap::new();
    }
    let Ok(mut dispatcher) = crate::work::Dispatcher::load(config) else {
        return BTreeMap::new();
    };
    let budgets = dispatcher.budgets(Utc::now());
    projects
        .iter()
        .filter_map(|project| {
            budgets
                .binding(project)
                .map(|full| (project.clone(), full.pause()))
        })
        .collect()
}

/// The pause said in prose: one line per distinct full ceiling, which is one
/// line in every ordinary case (§FS-015-spend-ceiling.10).
pub fn lines(paused: &BTreeMap<String, Paused>) -> Vec<String> {
    let said: BTreeSet<String> = paused
        .values()
        .map(|pause| match &pause.resumes_at {
            Some(at) => format!("Autorun paused: {}, resumes {at}", pause.scope),
            None => format!("Autorun paused: {}", pause.scope),
        })
        .collect();
    said.into_iter().collect()
}

impl crate::work::Dispatcher {
    /// The budgets in force, with what each has spent already measured
    /// (§FS-015-spend-ceiling.1).
    ///
    /// Here rather than beside the concurrency ceilings so the new dimension
    /// is a module of its own, which is the shape `headroom` took for the same
    /// reason — and so that the attribution the ceiling reads is the one
    /// `burn` reads, by construction rather than by comment.
    pub(crate) fn budgets(&mut self, now: DateTime<Utc>) -> Budgets {
        let anywhere = named(self.global.max_spend, self.global.max_tokens)
            || self
                .organizations
                .values()
                .any(|work| named(work.max_spend, work.max_tokens))
            || self
                .projects
                .values()
                .any(|work| named(work.max_spend, work.max_tokens));
        if !anywhere {
            return Budgets::default();
        }
        let ids: Vec<String> = crate::registry::array_field(&self.registry_doc, "projects")
            .iter()
            .map(|project| crate::registry::id_of(project).to_string())
            .collect();
        let places: Vec<Placement> = ids
            .iter()
            .filter_map(|id| self.placement(id).cloned())
            .collect();
        Budgets::read(
            &self.global,
            &self.organizations,
            &self.projects,
            self.organization_of_each_project(),
            &attribution::roots_of(&places),
            now,
        )
    }
}

#[cfg(test)]
#[path = "spend_tests.rs"]
mod tests;
