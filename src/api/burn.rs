//! The burn reading, below both surfaces (§FS-013-burn.8, §AR-009-surfaces.1).
//!
//! One derivation, so the page a key opens and the document `ephor burn
//! --json` prints cannot come apart. Which lens answers is decided by the
//! grouping and by nothing else, and neither lens is ever added into the
//! other (§FS-013-burn.1).

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use super::views;
use crate::burn::query::{self, By, Lens, Window};
use crate::burn::{attribution, store, Bucket, Tokens};
use crate::work::runtime::accounting::{self, Invocation};

impl super::Session {
    /// Where every project is on disk: its checkout, and each branch
    /// workspace its template names (§FS-013-burn.4).
    ///
    /// Built from the placements the session already resolved, so the screen
    /// and the command attribute a directory the same way they attribute a
    /// matter.
    pub fn burn_roots(&self) -> attribution::Roots {
        let places: Vec<crate::branches::Placement> = self.placements.values().cloned().collect();
        attribution::roots_of(&places)
    }

    /// Read the transcripts, but only where the store has gone stale
    /// (§FS-013-burn.8).
    ///
    /// This is local file reading, not a fetch: `refresh` remains the only
    /// verb that asks the world (§FS-001-forge-interface.7). It is never
    /// called while drawing — a command calls it before it reads, and the
    /// interface calls it from the tick.
    pub fn burn_refresh(&self, force: bool) -> Option<crate::burn::Scan> {
        crate::burn::refreshed(&self.burn_roots(), force)
    }

    /// What was spent over `window`, grouped by `by` (§FS-013-burn.6).
    pub fn burn(&mut self, window: Window, by: By) -> views::Burn {
        let now = Utc::now();
        let from = now - window.span();
        let buckets = store::since(&store::dir(), from, now);
        let (groups, totals, says) = match by.lens() {
            Lens::Machine => machine(&buckets, by),
            Lens::Work => self.work_lens(by, from),
        };
        views::Burn {
            window: window.label().to_string(),
            by: by.label().to_string(),
            lens: by.lens().label().to_string(),
            from,
            to: now,
            totals,
            groups,
            now: views::BurnNow {
                rate: query::rate(&buckets, now),
                live: query::live(&buckets, now)
                    .into_iter()
                    .map(|row| views::BurnLive {
                        session: row.session,
                        project: row.project,
                        model: row.model,
                        source: row.source,
                        subagent: row.subagent,
                        tokens: row.tokens,
                        rate: row.rate,
                    })
                    .collect(),
                spans: query::bars(&buckets, from, now)
                    .into_iter()
                    .map(|bar| views::BurnSpan {
                        at: bar.at,
                        tokens: bar.tokens,
                    })
                    .collect(),
            },
            says,
        }
    }
}

/// The machine lens: the window's buckets, grouped (§FS-013-burn.1).
fn machine(buckets: &[Bucket], by: By) -> (Vec<views::BurnGroup>, views::Spend, Option<String>) {
    let rows = query::group(buckets, by);
    let (tokens, cost) = query::totals(buckets);
    let says = rows
        .is_empty()
        .then(|| "Nothing recorded in this window yet.".to_string());
    (
        rows.into_iter()
            .map(|row| views::BurnGroup {
                key: row.key,
                detail: row.detail,
                spend: spend(&row.tokens, row.cost_usd),
            })
            .collect(),
        spend(&tokens, cost),
        says,
    )
}

impl super::Session {
    /// The work lens: what the runtime metered under each work root, grouped
    /// by the plan that spent it or by the matter the plan was dispatched for
    /// (§FS-013-burn.1).
    fn work_lens(
        &mut self,
        by: By,
        from: DateTime<Utc>,
    ) -> (Vec<views::BurnGroup>, views::Spend, Option<String>) {
        let roots = self.work_roots();
        let mut found: Vec<Invocation> = Vec::new();
        for group in &roots {
            found.extend(
                accounting::under(&group.root)
                    .into_iter()
                    // An invocation the runtime never dated cannot be put in
                    // a window, and guessing would put it in every one.
                    .filter(|one| one.at.is_some_and(|at| at >= from)),
            );
        }
        // A plan was dispatched *for* a matter, and the ledger is where that
        // is written down (§FS-005-dispatch.4). Nothing else joins the two.
        let matters = dispatched();
        let mut rows: BTreeMap<String, (Tokens, Option<f64>, Option<String>)> = BTreeMap::new();
        let mut skipped = 0usize;
        for one in &found {
            let named = match by {
                By::Matter => matters
                    .get(&(one.root.clone(), one.plan.clone()))
                    .map(|(id, title)| (id.clone(), Some(title.clone()))),
                _ => Some((one.plan.clone(), Some(one.identity()))),
            };
            // A plan nothing dispatched belongs to no matter, and inventing
            // one for it would put another matter's burn on this row.
            let Some((key, detail)) = named else {
                skipped += 1;
                continue;
            };
            let entry = rows.entry(key).or_insert((Tokens::default(), None, detail));
            entry.0.add(&one.tokens);
            if let Some(cost) = one.cost_usd {
                entry.1 = Some(entry.1.unwrap_or(0.0) + cost);
            }
        }
        let mut groups: Vec<views::BurnGroup> = rows
            .into_iter()
            .map(|(key, (tokens, cost, detail))| views::BurnGroup {
                key,
                detail,
                spend: spend(&tokens, cost),
            })
            .collect();
        groups.sort_by(|left, right| {
            right
                .spend
                .tokens
                .cmp(&left.spend.tokens)
                .then_with(|| left.key.cmp(&right.key))
        });
        let mut tokens = Tokens::default();
        let mut cost: Option<f64> = None;
        for one in &found {
            tokens.add(&one.tokens);
            if let Some(spent) = one.cost_usd {
                cost = Some(cost.unwrap_or(0.0) + spent);
            }
        }
        (groups, spend(&tokens, cost), says(&found, skipped, by))
    }
}

/// Every plan the ledger says was dispatched for a matter, keyed the way an
/// invocation is found — by the root it sits under and the plan that spent it
/// (§FS-013-burn.1).
///
/// An entry names *two* kinds of plan: the one it wrote its own ticket into,
/// and the one each workflow dispatch laid down beside it
/// (§FS-005-dispatch.19). The second is where a supervised fix spends
/// everything, and its accounting records are qualified by it — so a join on
/// the entry's own plan alone reaches no matter at all for the very shape
/// most work has. `enumerate_roots` already walks both.
fn dispatched() -> BTreeMap<(PathBuf, String), (String, String)> {
    crate::work::ledger::load()
        .map(|ledger| matters_of(&ledger.entries))
        .unwrap_or_default()
}

fn matters_of(
    entries: &BTreeMap<String, crate::work::ledger::Entry>,
) -> BTreeMap<(PathBuf, String), (String, String)> {
    let mut found: BTreeMap<(PathBuf, String), (String, String)> = BTreeMap::new();
    for (id, entry) in entries {
        let plans = std::iter::once(entry.plan_id.clone())
            .chain(entry.dispatches.iter().filter_map(|one| one.plan.clone()));
        for plan in plans {
            found.insert(
                (entry.root.clone(), plan),
                (id.clone(), entry.title.clone()),
            );
        }
    }
    found
}

/// What the work lens has to say about itself: what it could not measure
/// (§FS-013-burn.2), and what it could not attribute.
fn says(found: &[Invocation], skipped: usize, by: By) -> Option<String> {
    let mut said = Vec::new();
    if found.is_empty() {
        said.push("The runtime has metered nothing in this window.".to_string());
    }
    if let Some(missing) = accounting::unmeasured(found) {
        said.push(missing);
    }
    if skipped > 0 && by == By::Matter {
        said.push(format!(
            "{skipped} invocation{} belong to plans no matter was dispatched for",
            if skipped == 1 { "" } else { "s" }
        ));
    }
    (!said.is_empty()).then(|| said.join(" · "))
}

/// The four counters as a reading prints them. `priced` is what tells a zero
/// nobody knew from a zero that was actually spent (§FS-013-burn.7).
fn spend(tokens: &Tokens, cost_usd: Option<f64>) -> views::Spend {
    views::Spend {
        input: tokens.input,
        output: tokens.output,
        cache_read: tokens.cache_read,
        cache_write: tokens.cache_write,
        tokens: tokens.total(),
        cost_usd,
        priced: cost_usd.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::burn::Key;

    fn bucket(project: &str, model: &str, tokens: u64, cost: Option<f64>) -> Bucket {
        Bucket {
            at: store::floor(Utc::now()),
            key: Key {
                project: project.to_string(),
                source: "claude".to_string(),
                provider: "anthropic".to_string(),
                model: model.to_string(),
                session: "s1".to_string(),
                subagent: false,
            },
            tokens: Tokens {
                output: tokens,
                ..Tokens::default()
            },
            cost_usd: cost,
        }
    }

    /// Unknown and zero stay two facts all the way out to the document a
    /// program reads (§FS-013-burn.7).
    #[test]
    fn a_reading_tells_unpriced_from_priced_at_nothing() {
        let (groups, totals, _) = machine(
            &[
                bucket("app", "m", 10, None),
                bucket("lib", "n", 5, Some(0.0)),
            ],
            By::Project,
        );
        let app = groups.iter().find(|row| row.key == "app").expect("app");
        assert_eq!(app.spend.cost_usd, None);
        assert!(!app.spend.priced);
        let lib = groups.iter().find(|row| row.key == "lib").expect("lib");
        assert_eq!(lib.spend.cost_usd, Some(0.0));
        assert!(lib.spend.priced, "a priced nothing is not an unknown");
        // The total of a known and an unknown is the known part, and it says
        // it is priced — which is the only honest answer either way round.
        assert_eq!(totals.cost_usd, Some(0.0));
        assert_eq!(totals.tokens, 15);
    }

    /// The plan a workflow laid down is the one the invocations name, and it
    /// has to reach the matter it was laid down for — otherwise every
    /// supervised fix, which is the shape most work has, is reported as
    /// belonging to no matter at all (§FS-013-burn.1, §FS-005-dispatch.19).
    #[test]
    fn a_plan_a_workflow_laid_down_reaches_the_matter_it_was_for() {
        let ledger = r#"{
          "github-issues:vjovanov/ephor#39": {
            "project": "ephor", "title": "Burn: a token-burn page in the TUI",
            "root": "/w/ephor/panta",
            "plan_id": "github-issues-vjovanov-ephor-39",
            "plan": "/w/ephor/panta/github-issues-vjovanov-ephor-39.rhei.md",
            "dispatches": [{
              "ticket": "", "recipe": "supervised-ticket-fix",
              "at": "2026-09-02T22:00:00Z",
              "plan": "github-issues-vjovanov-ephor-39-fix-issue",
              "snapshot": { "updated_at": "2026-09-02T22:00:00Z" }
            }]
          }
        }"#;
        let entries: BTreeMap<String, crate::work::ledger::Entry> =
            serde_json::from_str(ledger).expect("a ledger");
        let matters = matters_of(&entries);
        let root = PathBuf::from("/w/ephor/panta");
        // What the accounting records under that root are actually qualified
        // by: the plan the workflow laid down, not the entry's own.
        let laid = matters
            .get(&(
                root.clone(),
                "github-issues-vjovanov-ephor-39-fix-issue".to_string(),
            ))
            .expect("the plan the workflow laid down reaches no matter");
        assert_eq!(laid.0, "github-issues:vjovanov/ephor#39");
        // And the entry's own plan still does, for work dispatched as a
        // ticket rather than as a workflow.
        assert!(matters.contains_key(&(root, "github-issues-vjovanov-ephor-39".to_string())));
    }

    /// An empty window says so rather than printing an empty table that reads
    /// like nothing was spent (§REQ-001-boundary.1).
    #[test]
    fn an_empty_window_says_so() {
        let (groups, totals, says) = machine(&[], By::Project);
        assert!(groups.is_empty());
        assert_eq!(totals.tokens, 0);
        assert!(says.is_some());
    }
}
