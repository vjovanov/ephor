//! Dispatch: what ephor watches, it can hand to an agent runtime
//! (§FS-005-dispatch).
//!
//! The feed says what is happening; this says what is being done about it.
//! An item plus a recipe becomes a ticket in a plan, written into the
//! checkout the item's branch resolves to, carrying the dossier of everything
//! ephor already knew. Afterwards ephor keeps the ledger and reads the work's
//! state back out of the plan — never out of its own memory.

pub mod commands;
pub mod dossier;
pub mod ledger;
pub mod recipe;
pub mod runtime;
pub mod workflow;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::branches::{Placement, WorkspaceState};
use crate::capabilities::{CapabilitySet, Rung};
use crate::error::{EphorError, Result};
use crate::feed::config::{ActionConfig, StatusConfig};
use crate::feed::model::Item;
use crate::paths::for_shell;

use dossier::Subject;
use ledger::{Dispatch, Entry, Ledger, Snapshot};
use recipe::{ProjectWorkConfig, Recipe, WorkConfig};
use runtime::plan::{self, Plan, Ticket, WorkRoot};

/// What one dispatch did.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// A plan was created and the first ticket written into it.
    Opened {
        plan: PathBuf,
        ticket: String,
        recipe: String,
    },
    /// The item had moved, so a ticket was appended after the last one
    /// (§FS-005-dispatch.5).
    Reopened {
        plan: PathBuf,
        ticket: String,
        recipe: String,
        changes: Vec<String>,
    },
    /// The work still answers the item as it is.
    Current,
    /// A recipe's deterministic opening move finished, so there was nothing
    /// left to hand over (§FS-005-dispatch.12): a clean rebase is a done
    /// thing, not a ticket.
    Settled { move_name: String, report: String },
    /// The item moved, but nothing applies to it any more — it was merged,
    /// closed, or answered. The work is over; the ledger keeps saying so.
    Dormant { changes: Vec<String> },
    /// A workflow the runtime offers laid down a plan of its own beside the
    /// item's (§FS-005-dispatch.19).
    Laid {
        plan: PathBuf,
        plan_id: String,
        workflow: String,
        entry: String,
    },
}

impl Outcome {
    /// What was written, without saying whether it happened: the caller knows
    /// whether this was a dry run and a second "opened" from here would
    /// contradict it.
    pub fn describe(&self) -> String {
        match self {
            Outcome::Opened {
                plan,
                ticket,
                recipe,
            } => format!("{recipe} → {}#{ticket}", plan.display()),
            // A ticket appended to a plan that exists is "reopened" whether or
            // not the item moved — asking for something else is one way to
            // reopen work. With nothing to say about the item, say nothing.
            Outcome::Reopened {
                plan,
                ticket,
                recipe,
                changes,
            } if changes.is_empty() => format!("{recipe} → {}#{ticket}", plan.display()),
            Outcome::Reopened {
                plan,
                ticket,
                recipe,
                changes,
            } => format!(
                "{recipe} → {}#{ticket} ({})",
                plan.display(),
                changes.join("; ")
            ),
            Outcome::Current => "already current".to_string(),
            Outcome::Settled { move_name, .. } => {
                format!("{move_name} finished — nothing to hand over")
            }
            Outcome::Dormant { changes } => {
                format!("{} — no recipe applies to it now", changes.join("; "))
            }
            Outcome::Laid {
                plan,
                workflow,
                entry,
                ..
            } => format!("{entry} ({workflow}) → {}", plan.display()),
        }
    }
}

/// One ticket of an item's work, as the plan currently has it.
#[derive(Debug, Clone)]
pub struct TicketStatus {
    pub id: String,
    pub recipe: String,
    pub title: String,
    pub state: Option<String>,
    pub finished: bool,
    /// Taken back rather than finished: the ticket sits in the machine's
    /// abandonment state (§FS-005-dispatch.16). Finished too, since that
    /// state is final — this says which kind of over it is.
    pub cancelled: bool,
    /// The runtime has stopped on this ticket and a person has to answer it
    /// (§FS-005-dispatch.9).
    pub waiting: bool,
    /// Who claimed the ticket, where anyone has — a claimed ticket is not a
    /// run's to advance (§FS-005-dispatch.15).
    pub assignee: Option<String>,
    /// The execution line the ticket carries, where it carries one
    /// (§FS-005-dispatch.14).
    pub pinned: Option<plan::Pin>,
    /// What the review left behind, where the work reached one — or, for a
    /// ticket taken back, the reason the reader gave (§FS-005-dispatch.16).
    pub verdict: Option<String>,
    /// When ephor asked for it, from the ledger's record of the dispatch
    /// (§FS-005-dispatch.18). None for a ticket ephor did not dispatch — one
    /// written into the plan by hand, or by the machine for itself: nothing
    /// knows when that was asked for, so nothing claims to.
    pub asked: Option<DateTime<Utc>>,
}

/// What one cancel did (§FS-005-dispatch.16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cancelled {
    pub ticket: String,
    /// The state it was taken back from.
    pub from: String,
    pub plan: PathBuf,
    /// Open tickets ordered after it, which will not start while it stands
    /// cancelled — named, never moved: cancelling them too is the reader's
    /// call.
    pub left_waiting: Vec<String>,
}

impl Cancelled {
    /// One line for a message: what was cancelled, and what that leaves
    /// waiting.
    pub fn describe(&self) -> String {
        match self.left_waiting.is_empty() {
            true => format!("⊘ {} cancelled", self.ticket),
            false => format!(
                "⊘ {} cancelled — {} ordered after it and will not start while it stands cancelled",
                self.ticket,
                match self.left_waiting.len() {
                    1 => format!("{} is", self.left_waiting[0]),
                    _ => format!("{} are", self.left_waiting.join(", ")),
                }
            ),
        }
    }
}

/// What a cancel says when the reader says nothing: the runtime records why
/// a ticket ended where it did and refuses a terminal move that says
/// nothing, and this is the truth of a reason left blank — never a reason
/// invented on the reader's behalf.
pub const CANCELLED_UNSAID: &str = "Cancelled from ephor; no reason was given.";

/// An item's work as it stands: read from the plan every time
/// (§FS-005-dispatch.4).
#[derive(Debug, Clone)]
pub struct WorkStatus {
    pub project: String,
    pub root: PathBuf,
    /// The plan's id inside the root, for naming it to the runtime.
    pub plan_id: String,
    /// The checkout the runtime is run from.
    pub checkout: PathBuf,
    pub plan: PathBuf,
    /// The plan the ledger points at is gone — reported, never repaired.
    pub missing: bool,
    pub tickets: Vec<TicketStatus>,
    /// How many plans a workflow laid down beside this matter's own
    /// (§FS-005-dispatch.19). A count rather than the plans themselves: what
    /// each one is doing is the operations board's answer, read from the plan
    /// files there like every other operation (§FS-005-dispatch.15).
    pub workflows: usize,
    /// What has happened to the item since the last dispatch.
    pub changes: Vec<String>,
    /// How to move the waiting ticket on by hand, where one is waiting. Built
    /// by the runtime module, since the words are the runner's
    /// (§REQ-001-boundary.5).
    pub advance: Option<String>,
}

impl WorkStatus {
    pub fn stale(&self) -> bool {
        !self.changes.is_empty()
    }

    pub fn open_tickets(&self) -> usize {
        self.tickets.iter().filter(|t| !t.finished).count()
    }

    /// Work that has stopped and is waiting on a person. The one thing in here
    /// that is nobody else's to move (§FS-005-dispatch.9).
    pub fn waiting(&self) -> Option<&TicketStatus> {
        self.tickets.iter().find(|ticket| ticket.waiting)
    }

    /// One line for a row that has room for one: what the work is doing, or
    /// what it decided, and whether the item has moved under it. `verdict` is
    /// how much of the verdict's own sentence fits where this is going.
    pub fn badge(&self, verdict_width: usize) -> String {
        if self.missing {
            return "⚠ plan missing".to_string();
        }
        // Work that is entirely workflows has no ticket to badge; what it has
        // is said on the rows beneath (§FS-005-dispatch.19).
        if self.tickets.is_empty() && self.workflows > 0 {
            return match self.workflows {
                1 => "⛬ 1 workflow".to_string(),
                many => format!("⛬ {many} workflows"),
            };
        }
        // A question for a person leads: everything else in the badge is the
        // runtime telling you what it is doing, and this is it telling you it
        // has stopped.
        if let Some(waiting) = self.waiting() {
            // A ticket the machine opened for itself has no recipe — its
            // "recipe" falls back to its own id, and saying that twice is
            // noise where the point is the question.
            return match waiting.recipe == waiting.id {
                true => format!("⚠ waiting on you · {}", waiting.id),
                false => format!("⚠ {} · waiting on you · {}", waiting.recipe, waiting.id),
            };
        }
        let mut badge = match self.tickets.iter().rev().find(|ticket| !ticket.finished) {
            Some(open) => format!(
                "⚙ {} · {}",
                open.recipe,
                open.state.as_deref().unwrap_or("?")
            ),
            None => match self.tickets.last() {
                // Taken back is a different kind of over from finished, and
                // the row says which (§FS-005-dispatch.16).
                Some(last) if last.cancelled => format!("⊘ {} · cancelled", last.recipe),
                Some(last) => match &last.verdict {
                    // The verdict's own sentence, cut where a row ends: the
                    // rest of it is in the artifact, one keystroke away.
                    Some(verdict) => {
                        format!("✓ {} · {}", last.recipe, clamp(verdict, verdict_width))
                    }
                    None => format!("✓ {}", last.recipe),
                },
                None => "· no tickets".to_string(),
            },
        };
        if self.stale() {
            badge.push_str(&format!("  ⟳ {}", self.changes.join("; ")));
        }
        badge
    }

    /// The rows this work stands on beneath the matter it is about
    /// (§FS-005-dispatch.23): one per open ticket, the parked one first
    /// (§FS-005-dispatch.9), and — where nothing is open — one for what the
    /// last ticket decided. `verdict` is how much of a verdict's own sentence
    /// fits on a row.
    pub fn lines(&self, verdict_width: usize) -> Vec<WorkLine> {
        if self.missing {
            return vec![WorkLine::said(Tone::Waiting, "⚠", "plan missing")];
        }
        // Work that is entirely workflows has no ticket of its own; what it
        // has is said on the rows beneath (§FS-005-dispatch.19).
        if self.tickets.is_empty() && self.workflows > 0 {
            let said = match self.workflows {
                1 => "1 workflow".to_string(),
                many => format!("{many} workflows"),
            };
            return vec![WorkLine::said(Tone::Going, "⛬", said)];
        }
        let mut lines: Vec<WorkLine> = Vec::new();
        let open = self.tickets.iter().filter(|ticket| !ticket.finished);
        for ticket in open.clone().filter(|ticket| ticket.waiting) {
            // A ticket the machine opened for itself has no recipe — its
            // "recipe" falls back to its own id, and saying that twice is
            // noise where the point is the question.
            let said = match ticket.recipe == ticket.id {
                true => format!("waiting on you · {}", ticket.id),
                false => format!("{} · waiting on you · {}", ticket.recipe, ticket.id),
            };
            lines.push(WorkLine::of(Tone::Waiting, "⚠", said, ticket));
        }
        for ticket in open.filter(|ticket| !ticket.waiting) {
            let said = format!(
                "{} · {}",
                ticket.recipe,
                ticket.state.as_deref().unwrap_or("?")
            );
            lines.push(WorkLine::of(Tone::Going, "⚙", said, ticket));
        }
        // Nothing open: what the last one decided, on one line. The rest of
        // the record is the work screen's (§FS-005-dispatch.18).
        if lines.is_empty() {
            lines.push(match self.tickets.last() {
                // Taken back is a different kind of over from finished, and
                // the row says which (§FS-005-dispatch.16).
                Some(last) if last.cancelled => WorkLine::of(
                    Tone::Over,
                    "⊘",
                    format!("{} · cancelled", last.recipe),
                    last,
                ),
                Some(last) => {
                    // The verdict's own sentence, cut where a row ends: the
                    // rest of it is in the artifact, one keystroke away.
                    let said = match &last.verdict {
                        Some(verdict) => {
                            format!("{} · {}", last.recipe, clamp(verdict, verdict_width))
                        }
                        None => last.recipe.clone(),
                    };
                    WorkLine::of(Tone::Over, "✓", said, last)
                }
                None => WorkLine::said(Tone::Over, "·", "no tickets"),
            });
        }
        if self.stale() {
            lines.push(WorkLine::said(
                Tone::Stale,
                "⟳",
                format!("since that was asked: {}", self.changes.join("; ")),
            ));
        }
        lines
    }
}

/// How a work line reads at a glance (§FS-005-dispatch.23). The tones the work
/// screen already spells its tickets in, so the tree and the screen behind `w`
/// cannot say the same ticket two different ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The runtime is on it.
    Going,
    /// It has stopped and a person has to answer it (§FS-005-dispatch.9).
    Waiting,
    /// Finished, or taken back (§FS-005-dispatch.16).
    Over,
    /// The item has moved under the work (§FS-005-dispatch.5).
    Stale,
}

/// One row of a matter's work, beneath the row the matter is on
/// (§FS-005-dispatch.23).
#[derive(Debug, Clone)]
pub struct WorkLine {
    pub tone: Tone,
    pub marker: &'static str,
    pub said: String,
    /// The ticket this row *is*, where it is one — what cancelling here takes
    /// back (§FS-005-dispatch.16). None on a row that is a summary rather than
    /// a ticket, and on one whose ticket is already over.
    pub ticket: Option<String>,
    /// When ephor asked for it, where the ledger knows (§FS-005-dispatch.18).
    pub asked: Option<DateTime<Utc>>,
}

impl WorkLine {
    fn of(tone: Tone, marker: &'static str, said: String, ticket: &TicketStatus) -> WorkLine {
        WorkLine {
            tone,
            marker,
            said,
            ticket: (!ticket.finished).then(|| ticket.id.clone()),
            asked: ticket.asked,
        }
    }

    fn said(tone: Tone, marker: &'static str, said: impl Into<String>) -> WorkLine {
        WorkLine {
            tone,
            marker,
            said: said.into(),
            ticket: None,
            asked: None,
        }
    }
}

/// What the work behind one menu entry is doing, for the row that could start
/// it again (§FS-005-dispatch.21). Three answers, because the runtime schedules
/// one run per execution root: the run holds this entry's work, it parked a
/// question this entry opened, or it is live on the root and will reach it
/// (§FS-005-dispatch.15).
#[derive(Debug, Clone)]
pub enum WorkGoing {
    Running {
        root: PathBuf,
        /// The ticket the run holds and the state it is in, in the words the
        /// board already uses.
        doing: String,
    },
    /// A ticket this entry opened that the machine parks for a person: it is
    /// *waiting on you* (§FS-005-dispatch.9, §FS-005-dispatch.20), and §21's
    /// word for it is §15's, never *queued*, which would promise a turn that
    /// never comes.
    ///
    /// Marked whether or not a run still holds the root. A run with nobody at
    /// its terminal waits at a human gate rather than exiting, and one that
    /// exited leaves the question standing all the same — either way this is
    /// open work about this subject, and a second dispatch laid beside it is
    /// exactly the mistake §21 exists to prevent.
    Waiting {
        root: PathBuf,
        /// The ticket the question is in, and the state the machine parked it
        /// in — the plan is where the answer belongs (§FS-005-dispatch.9).
        ticket: String,
        state: String,
        plan: PathBuf,
    },
    Queued {
        root: PathBuf,
    },
}

impl WorkGoing {
    pub fn root(&self) -> &std::path::Path {
        match self {
            WorkGoing::Running { root, .. }
            | WorkGoing::Waiting { root, .. }
            | WorkGoing::Queued { root } => root,
        }
    }
}

/// One item's work root, read once and asked about many times
/// (§FS-005-dispatch.15.1).
///
/// The lock probe, the states document, the plan and the journal answer the
/// same way for every row of one menu, so they are read here and handed to
/// each — the rule that an answer is resolved once and reused
/// (§AR-005-capabilities.1). Nothing is remembered past the menu: this is a
/// reading of the world, taken when the menu was assembled.
pub struct WorkAt<'a> {
    entry: &'a Entry,
    /// The run lock is held: something is running on this root right now.
    live: bool,
    /// The root's own state machine, where it has a readable one. Finality and
    /// gating are its words: with none to say them nothing here is judged over,
    /// and nothing is judged a question for a person either
    /// (§FS-005-dispatch.15).
    machine: Option<WorkRoot>,
    /// The matter's own plan.
    plan: Option<Plan>,
    /// What answers for which tickets the run has in hand — the run's own
    /// stream where the binding writes one, the journal otherwise
    /// (§FS-005-dispatch.15.2) — and when the root's lock was born, which
    /// only the journal's reading needs. Read once for every ticket asked
    /// about (§FS-005-dispatch.15).
    witness: Option<runtime::watch::Witness>,
    lock_born: Option<std::time::SystemTime>,
    /// What the live run calls itself, from the descriptor beside its lock
    /// (§FS-005-dispatch.20).
    pub identity: Option<runtime::watch::RunIdentity>,
}

impl WorkAt<'_> {
    pub fn root(&self) -> &std::path::Path {
        &self.entry.root
    }

    pub fn live(&self) -> bool {
        self.live
    }

    /// What the work one entry hands over is doing right now
    /// (§FS-005-dispatch.21), off the reading already taken.
    ///
    /// Three answers, ranked as the board ranks them: what waits on the reader
    /// stands ahead of anything else its work is doing (§FS-005-dispatch.9),
    /// then the ticket a run holds, then the queue the root's run will reach.
    /// This is the board's reading narrowed to one row, not a second reading.
    pub fn going(&self, action: &str) -> Option<WorkGoing> {
        let mut waiting: Option<WorkGoing> = None;
        let mut running: Option<WorkGoing> = None;
        let mut queued = false;
        let mut consider = |plan_id: &str, path: &std::path::Path, ticket: &plan::PlanTicket| {
            let Some(machine) = self.machine.as_ref() else {
                // With no machine nothing here is judged over and nothing is
                // judged a question: the ticket is open work, and whether a run
                // holds it is still the journal's to say.
                return self.hold(plan_id, ticket, &mut running, &mut queued);
            };
            let state = ticket.state.as_deref().unwrap_or("?");
            if ticket
                .state
                .as_deref()
                .is_some_and(|at| machine.is_final(at))
            {
                return;
            }
            // A ticket the machine parks is waiting on the reader
            // (§FS-005-dispatch.9, §FS-005-dispatch.20). It is *not* queued:
            // §15 is explicit that calling it that would promise a turn that
            // never comes, and it is not the run's to advance either.
            if ticket
                .state
                .as_deref()
                .is_some_and(|at| machine.is_gating(at))
            {
                if waiting.is_none() {
                    waiting = Some(WorkGoing::Waiting {
                        root: self.entry.root.clone(),
                        ticket: format!("{plan_id}.{}", ticket.id),
                        state: state.to_string(),
                        plan: path.to_path_buf(),
                    });
                }
                return;
            }
            self.hold(plan_id, ticket, &mut running, &mut queued);
        };
        // The tickets this entry wrote into the matter's own plan.
        let mine: std::collections::BTreeSet<&str> = self
            .entry
            .dispatches
            .iter()
            .filter(|dispatch| dispatch.recipe == action && !dispatch.is_workflow())
            .map(|dispatch| dispatch.ticket.as_str())
            .collect();
        if !mine.is_empty() {
            if let Some(plan) = self.plan.as_ref() {
                for ticket in plan.tickets() {
                    if mine.contains(ticket.id.as_str()) {
                        consider(&self.entry.plan_id, &self.entry.plan, &ticket);
                    }
                }
            }
        }
        // And the plans this entry laid down of its own
        // (§FS-005-dispatch.19), which are operations exactly as tickets are.
        // One plan per dispatch and one dispatch per entry, so nothing here is
        // read twice by a menu asking about every row.
        for dispatch in self
            .entry
            .dispatches
            .iter()
            .filter(|dispatch| dispatch.recipe == action && dispatch.is_workflow())
        {
            let Some(name) = dispatch.plan.as_deref() else {
                continue;
            };
            let Some(laid) = runtime::workflow::laid(&self.entry.root.join(name)) else {
                continue;
            };
            let Ok(Some(plan)) = Plan::read(&laid.path) else {
                continue;
            };
            for ticket in plan.tickets() {
                consider(&laid.plan_id, &laid.path, &ticket);
            }
        }
        waiting.or(running).or_else(|| {
            queued.then(|| WorkGoing::Queued {
                root: self.entry.root.clone(),
            })
        })
    }

    /// Whether the live run holds this open ticket, off the journal already
    /// read — and the queue where it does not. A root nothing holds is not an
    /// operation: an open ticket there is waiting work, which is the work
    /// screen's business (§FS-005-dispatch.15).
    fn hold(
        &self,
        plan_id: &str,
        ticket: &plan::PlanTicket,
        running: &mut Option<WorkGoing>,
        queued: &mut bool,
    ) {
        if !self.live {
            return;
        }
        let state = ticket.state.as_deref().unwrap_or("?");
        if self.witness.as_ref().is_some_and(|witness| {
            witness.holds(
                &self.entry.root,
                self.lock_born,
                plan_id,
                &ticket.id,
                Some(state),
            )
        }) {
            if running.is_none() {
                // The board's own phrasing for a held ticket, narrowed to one
                // row (§FS-005-dispatch.15).
                *running = Some(WorkGoing::Running {
                    root: self.entry.root.clone(),
                    doing: format!("{plan_id}.{} [{state}]", ticket.id),
                });
            }
            return;
        }
        *queued = true;
    }
}

/// Reads the work configuration, offers recipes, writes tickets, and keeps the
/// ledger.
pub struct Dispatcher {
    registry_doc: Value,
    global: WorkConfig,
    projects: BTreeMap<String, ProjectWorkConfig>,
    placements: BTreeMap<String, Option<Placement>>,
    /// What the checkout of each (project, branch) says about itself —
    /// both distances, from one fold — measured on demand.
    behind: BTreeMap<(String, String), recipe::Facts>,
    /// Who can be asked, per work root — the roster is read from the runtime's
    /// merged settings, and a sweep asks about the same handful of roots over
    /// and over (§FS-005-dispatch.14).
    rosters: BTreeMap<PathBuf, runtime::roster::Roster>,
    /// What the runtime offers, per place asked — a sweep asks the same
    /// handful of roots over and over (§FS-005-dispatch.19).
    workflows: BTreeMap<PathBuf, runtime::workflow::Offered>,
    /// What the reader should know about the hands this dispatcher resolved,
    /// each said once (§FS-006-project-interface.9).
    notes: Vec<String>,
    pub ledger: Ledger,
}

impl Dispatcher {
    pub fn load(config: &StatusConfig) -> Result<Dispatcher> {
        Ok(Dispatcher {
            registry_doc: crate::feed::commands::load_registry_doc()?,
            global: config.work.clone(),
            projects: config
                .projects
                .iter()
                .map(|(id, project)| (id.clone(), project.work.clone()))
                .collect(),
            placements: BTreeMap::new(),
            behind: BTreeMap::new(),
            rosters: BTreeMap::new(),
            workflows: BTreeMap::new(),
            notes: Vec::new(),
            ledger: ledger::load()?,
        })
    }

    /// What the reader should know about who got this work, each note once.
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// The recipes offered on a project: shipped, then configured
    /// (§FS-005-dispatch.1).
    pub fn recipes(&self, project: &str) -> Vec<Recipe> {
        let per_project = self
            .projects
            .get(project)
            .map(|work| work.recipes.as_slice())
            .unwrap_or_default();
        recipe::resolve(&self.global.recipes, per_project)
    }

    /// The recipes that apply to one item.
    pub fn offers(&mut self, item: &Item) -> Vec<Recipe> {
        let facts = self.facts(item);
        recipe::applicable(&self.recipes(&item.project), item, &facts)
    }

    /// What a selector needs to know about the checkout
    /// (§FS-004-quick-actions.6). Measured once per branch and remembered for
    /// the sweep: a dispatch over a whole feed asks about the same handful of
    /// branches over and over, and each answer is several git calls.
    pub fn facts(&mut self, item: &Item) -> recipe::Facts {
        let Some(placement) = self.placement(&item.project).cloned() else {
            return recipe::Facts::default();
        };
        let checkout = placement.checkout(item);
        let Some(branch) = checkout.branch.clone() else {
            return recipe::Facts::default();
        };
        // A branch nobody has checked out cannot be measured, and guessing
        // would offer work about a tree that is not on the machine.
        if !matches!(checkout.state, WorkspaceState::Ready) {
            return recipe::Facts::default();
        }
        let key = (item.project.clone(), branch);
        if let Some(facts) = self.behind.get(&key) {
            return *facts;
        }
        // Summed across the workspace's forest, like the inbox's own count:
        // one repository trailing is the workspace trailing
        // (§AR-004-forest.1). Both distances come off the one standing fold,
        // so a recipe asking about either is answered from one measurement
        // (§FS-004-quick-actions.8). A distance to a base nobody named is not
        // a fact anything acts on (§FS-004-quick-actions.6), so on a project
        // with no main branch only `behind` is nulled — the other distance is
        // to the branch's own copy, and needs no base — the same split the
        // rows and their menus make of the same measurement.
        let standing = placement.forest(&checkout.workspace).standing();
        let facts = recipe::Facts {
            behind: placement
                .main_branch
                .is_some()
                .then(|| standing.staleness().total())
                .flatten(),
            behind_upstream: standing.behind_upstream(),
        };
        self.behind.insert(key, facts);
        facts
    }

    /// What a recipe would actually ask for about this item — the brief with
    /// the item's own words in it, which is what a reader has to see before
    /// pressing the key, not the template it came from. Falls back to the
    /// template where the item cannot be placed; the refusal that follows
    /// says why better than a blank line would.
    pub fn brief(&mut self, item: &Item, recipe: &Recipe) -> String {
        match self.site(item, recipe) {
            Ok(site) => dossier::render(&recipe.brief, &site.values),
            Err(_) => recipe.brief.clone(),
        }
    }

    /// Who does one action on one project, in the order §FS-005-dispatch.14
    /// sets (§FS-006-project-interface.9). `picked` is what the reader chose
    /// for this dispatch alone — the first of the seven steps, which nothing
    /// feeds yet: choosing at the moment of asking belongs to the picker, and
    /// this is the signature it will call.
    pub fn hand(
        &mut self,
        project: &str,
        action: &str,
        picked: Option<&recipe::HandPin>,
        pinned: Option<&recipe::HandPin>,
        root: &std::path::Path,
    ) -> runtime::roster::Choice {
        self.ensure_roster(root);
        runtime::roster::resolve(
            &self.rosters[root],
            &self.global,
            self.projects.get(project),
            action,
            picked,
            pinned,
        )
    }

    /// The hands a picker may offer about this project's work, against the
    /// work root the dispatch will use (§FS-005-dispatch.14): the roster's,
    /// already without what the project's narrowing excludes. Empty where
    /// the roster is — with no runtime bound there is nothing to pick from,
    /// and the picker is simply not offered.
    pub fn pickable(
        &mut self,
        project: &str,
        root: &std::path::Path,
    ) -> Vec<runtime::roster::Hand> {
        self.ensure_roster(root);
        runtime::roster::pickable(&self.rosters[root], self.projects.get(project))
    }

    fn ensure_roster(&mut self, root: &std::path::Path) {
        if !self.rosters.contains_key(root) {
            let roster = runtime::roster::roster(&self.global, Some(root));
            self.rosters.insert(root.to_path_buf(), roster);
        }
    }

    /// Where an item's work root is, without making it: the ledger's answer
    /// where the item has work, and the same template `site` resolves at
    /// dispatch otherwise (§FS-006-project-interface.7) — so a surface asking
    /// "who would get this" resolves against the root the dispatch will use.
    pub fn work_root_of(&mut self, item: &Item) -> Option<PathBuf> {
        if let Some(entry) = self.ledger.entries.get(&item.id) {
            return Some(entry.root.clone());
        }
        let template = self.root_template(&item.project);
        let placement = self.placement(&item.project)?.clone();
        let checkout = placement.checkout(item);
        let subject = Subject {
            item,
            checkout: &checkout,
            root: &placement.root,
        };
        Some(crate::paths::resolve_path(&dossier::render(
            &template,
            &subject.placeholders(),
        )))
    }

    /// Every execution root beneath the watch, with the plans it holds
    /// (§FS-005-dispatch.15): the ledger's dispatches — those plans carry the
    /// matter behind them — merged with what enumerating the work roots
    /// finds, so a plan ephor never wrote is watched exactly as one it did.
    /// The walk is bounded by the registry, never by the disk: each project's
    /// checkout and each branch workspace already resolved
    /// (§FS-005-dispatch.15.1).
    pub fn work_roots(&mut self) -> Vec<runtime::watch::RootPlans> {
        let ids: Vec<String> = crate::registry::array_field(&self.registry_doc, "projects")
            .iter()
            .map(|project| crate::registry::id_of(project).to_string())
            .collect();
        let placements: Vec<Placement> = ids
            .iter()
            .filter_map(|id| self.placement(id).cloned())
            .collect();
        enumerate_roots(&self.global, &self.projects, &placements, &self.ledger)
    }

    /// What this ticket pins, and what the reader is told about it. Refuses
    /// where the choice cannot stand, so nothing is written and no opening
    /// move is made under a hand that may not have it
    /// (§FS-006-project-interface.9).
    fn pin(
        &mut self,
        item: &Item,
        recipe: &Recipe,
        picked: Option<&recipe::HandPin>,
        root: &std::path::Path,
    ) -> Result<(Option<String>, Option<String>)> {
        // One spelling per recipe: a hand is the checkable name for exactly
        // what `target`/`model` spell raw, and a recipe carrying both would
        // have one of them silently lose (§FS-006-project-interface.9).
        if recipe.hand.is_some() && (recipe.target.is_some() || recipe.model.is_some()) {
            return Err(EphorError::Command(format!(
                "recipe '{}' names both a hand and the runtime's own execution identity \
                 (target/model) — say one or the other",
                recipe.id
            )));
        }
        // A recipe spelling the runtime's own execution identity has pinned
        // itself — the second step — and the tables below do not displace it;
        // only the reader's own pick, the first step, does
        // (§FS-005-dispatch.14). A project that narrows the roster binds it
        // all the same: a selector no hand named is not authorized by a list
        // of names.
        if picked.is_none()
            && recipe.hand.is_none()
            && (recipe.target.is_some() || recipe.model.is_some())
        {
            if let Some(why) = runtime::roster::refuse_unnamed(
                self.projects.get(&item.project),
                &format!(
                    "recipe '{}' pins the runtime's own execution identity",
                    recipe.id
                ),
            ) {
                return Err(EphorError::Command(why));
            }
            return Ok((recipe.target.clone(), recipe.model.clone()));
        }
        let choice = self.hand(
            &item.project,
            &recipe.id,
            picked,
            recipe.hand.as_ref(),
            root,
        );
        if let runtime::roster::Choice::Refused(why) = choice {
            return Err(EphorError::Command(why));
        }
        if let Some(note) = choice.note() {
            self.note_once(note);
        }
        Ok(choice.pin())
    }

    /// Say one thing about who got the work once, however many tickets raise
    /// it (§FS-006-project-interface.9).
    fn note_once(&mut self, note: &str) {
        if !self.notes.iter().any(|have| have == note) {
            self.notes.push(note.to_string());
        }
    }

    /// The one flag spelling a run over this entry's plan may carry
    /// (§FS-005-dispatch.14): the chosen hand the plan language could not
    /// spell — an agent and no model — resolved again at the moment the run
    /// is invoked, the same moment the runtime reads its own configuration.
    /// `status` is the same entry's, as [`Dispatcher::status_of`] just read
    /// it — the plan is not read twice for one run.
    ///
    /// Flags are per-run, and what they can touch differs by what a ticket
    /// carries (§FS-005-dispatch.14): one with the full execution line is
    /// resolved from that line alone, the flags invisible to it, while one
    /// pinning a model alone would take its carrier from them; a claimed
    /// ticket is not the run's to advance at all (§FS-005-dispatch.15). So
    /// the flags ride only where they can re-aim nothing: every ticket the
    /// run would advance resolves to the same spelling, and none pins a
    /// model. None otherwise — and where a hand wanted flags the run cannot
    /// carry, the reader is told it went unbound.
    pub fn run_hand(
        &mut self,
        entry: &Entry,
        status: &WorkStatus,
    ) -> Option<runtime::roster::HandFlags> {
        if status.missing {
            return None;
        }
        let recipes = self.recipes(&entry.project);
        let mut wants: Vec<Option<runtime::roster::HandFlags>> = Vec::new();
        for ticket in &status.tickets {
            // Not this run's to advance: finished, or claimed by somebody
            // (§FS-005-dispatch.15) — flags can re-aim neither.
            if ticket.finished || ticket.assignee.is_some() {
                continue;
            }
            match ticket.pinned {
                // Its own full line: the runtime resolves it from the line
                // alone, and a run's flags are invisible to it.
                Some(plan::Pin::Target) => continue,
                // A model alone would take its carrier from the flags, so its
                // presence keeps them off this run.
                Some(plan::Pin::Model) => {
                    wants.push(None);
                    continue;
                }
                None => {}
            }
            // The recipe the ticket was dispatched under, already resolved by
            // `status_of` — its own id where no dispatch recorded one.
            let action = &ticket.recipe;
            let pinned = recipes
                .iter()
                .find(|recipe| &recipe.id == action)
                .and_then(|recipe| recipe.hand.clone());
            let choice = self.hand(&entry.project, action, None, pinned.as_ref(), &entry.root);
            // At dispatch a refusal blocks the ticket; here the ticket
            // already exists, so the run goes unflagged and the reason is
            // said rather than swallowed (§FS-006-project-interface.9).
            if let runtime::roster::Choice::Refused(why) = &choice {
                self.note_once(why);
            }
            if let Some(note) = choice.note() {
                let note = note.to_string();
                self.note_once(&note);
            }
            wants.push(choice.flags());
        }
        let first = wants.iter().flatten().next()?.clone();
        if wants.iter().all(|want| want.as_ref() == Some(&first)) {
            return Some(first);
        }
        self.note_once(&format!(
            "the open tickets of {} do not agree on one hand — the run carries no \
             agent flags, and each ticket runs as it stands",
            entry.plan.display()
        ));
        None
    }

    /// The hand riding a run of one item's plan, and what the reader should
    /// be told about it — everything a surface that runs a single plan needs
    /// to know before it cedes the terminal (§FS-005-dispatch.14). The same
    /// resolution `work run` makes, over this item's entry and the plan as it
    /// stands right now, so the key and the command line cannot come apart
    /// (§FS-005-dispatch.12). One plan is one group: such a run advances no
    /// other plan, so there is nothing for its flags to contradict and no
    /// grouping to do.
    ///
    /// The notes come back rather than piling up: what an earlier keystroke
    /// was told does not silence this run's own answer, and does not travel
    /// with it either.
    pub fn run_hand_for(
        &mut self,
        item: &str,
    ) -> (Option<runtime::roster::HandFlags>, Vec<String>) {
        let said = std::mem::take(&mut self.notes);
        let hand = self.ledger.entries.get(item).cloned().and_then(|entry| {
            let status = self.status_of(&entry, None);
            self.run_hand(&entry, &status)
        });
        let notes = std::mem::replace(&mut self.notes, said);
        (hand, notes)
    }

    fn placement(&mut self, project: &str) -> Option<&Placement> {
        self.placements
            .entry(project.to_string())
            .or_insert_with(|| Placement::load(&self.registry_doc, project))
            .as_ref()
    }

    /// The states YAML installed into a work root that has none: the project's
    /// own, the global one, or the machine ephor ships.
    fn states_yaml(&self, project: &str) -> Result<String> {
        states_yaml(&self.global, self.projects.get(project))
    }

    fn root_template(&self, project: &str) -> String {
        root_template(&self.global, self.projects.get(project))
    }

    /// Where an item's work belongs, refusing where it would not run
    /// (§FS-005-dispatch.6).
    fn site(&mut self, item: &Item, recipe: &Recipe) -> Result<Site> {
        self.site_for(item, recipe.needs_checkout)
    }

    /// The same, for an entry that is not a recipe: a workflow says what it
    /// needs on disk through its own `requires_checkout`
    /// (§FS-005-dispatch.19).
    fn site_for(&mut self, item: &Item, needs_checkout: bool) -> Result<Site> {
        let template = self.root_template(&item.project);
        // A project ephor cannot place has nowhere to put the work, and the
        // ladder owns that sentence (§AR-005-capabilities.2).
        let placement = self.placement(&item.project).cloned().ok_or_else(|| {
            EphorError::Command(
                CapabilitySet::unknown(&item.project)
                    .refusal(&[Rung::Observable])
                    .unwrap_or_else(|| format!("{} cannot be placed", item.project)),
            )
        })?;
        // The workspace where the matter resolves, and the forest root where
        // none does (§FS-005-dispatch.13, §AR-004-forest.1) — which is what
        // lets work about a conversation run without the checkout-able rung
        // (§FS-006-project-interface.10).
        let checkout = placement.checkout(item);
        // Only for work that edits the change. A review or a reply runs in
        // the project's own checkout and fetches what it needs.
        if needs_checkout {
            let wanted = checkout.branch.as_deref().unwrap_or("?");
            match &checkout.state {
                WorkspaceState::Missing(target) => {
                    return Err(EphorError::Command(format!(
                        "{}: branch {} is not checked out ({} is missing). Make it with:\n  \
                         ephor checkout --item {}",
                        item.project,
                        wanted,
                        target.display(),
                        item.id
                    )));
                }
                // The one checkout is standing on other code. There is no
                // workspace to make, so the refusal names both ways out
                // rather than offering a checkout that cannot be made
                // (§FS-005-dispatch.3).
                WorkspaceState::Elsewhere(head) => {
                    return Err(EphorError::Command(format!(
                        "{}: branch {} is not checked out — {} is standing on {}, and it is \
                         the only checkout this project has. Put the branch there:\n  \
                         git -C {} switch {}\n\
                         or give '{}' a branch_root_template in the registry, so its branches \
                         get workspaces of their own.",
                        item.project,
                        wanted,
                        placement.root.display(),
                        head,
                        placement.root.display(),
                        wanted,
                        item.project,
                    )));
                }
                WorkspaceState::Ready | WorkspaceState::Unmatched => {}
            }
        }
        let subject = Subject {
            item,
            checkout: &checkout,
            root: &placement.root,
        };
        let mut values = subject.placeholders();
        let dir = crate::paths::resolve_path(&dossier::render(&template, &values));
        // Where a proposed reply belongs, named absolutely: the runtime runs
        // from the checkout, not from the work root, so a brief that asks for
        // a file has to say which one (§FS-005-dispatch.13).
        values.insert(
            "reply",
            runtime::results::reply_path(&dir, &plan::plan_id(&item.id))
                .to_string_lossy()
                .into_owned(),
        );
        Ok(Site {
            dir,
            dossier: subject.dossier(),
            metadata: subject.metadata(),
            values,
            checkout: checkout.clone(),
        })
    }

    /// The deterministic opening move a recipe declares, made before the
    /// ticket costs a model (§FS-005-dispatch.12).
    ///
    /// It is the same implementation the reader's key runs
    /// (§FS-004-quick-actions.6) — two of them would eventually disagree about
    /// what a clean rebase is. Where it finishes there is nothing to dispatch;
    /// where it stops, the repository is left standing in the conflict and the
    /// report of where it got to becomes the ticket's opening.
    fn opening(&mut self, item: &Item, recipe: &Recipe) -> Result<Opening> {
        let Some(name) = recipe.opens_with.as_deref() else {
            return Ok(Opening::None);
        };
        if name != recipe::OPENING_REBASE {
            return Err(EphorError::Command(format!(
                "recipe '{}' opens with '{name}', which ephor does not know (it knows: {}).",
                recipe.id,
                recipe::OPENING_REBASE
            )));
        }
        let placement = self
            .placement(&item.project)
            .cloned()
            .ok_or_else(|| EphorError::Command(format!("{} cannot be placed", item.project)))?;
        let checkout = placement.checkout(item);
        // Nothing on disk to replay, or nothing to replay onto: the move does
        // not apply, and the ticket is written as it would have been.
        if !matches!(checkout.state, WorkspaceState::Ready) {
            return Ok(Opening::None);
        }
        let Some(base) = placement.main_branch.clone() else {
            return Ok(Opening::None);
        };
        let forest = placement.forest(&checkout.workspace);
        if forest.repos.is_empty() {
            return Ok(Opening::None);
        }
        // The opening move a recipe declares is the rebase onto the project's
        // main branch; a replay onto the branch's own copy is the reader's
        // move and has its own entry (§FS-004-quick-actions.8).
        let outcome = crate::git::rebase(&forest, &crate::git::Onto::Base(base));
        // What the replay measured is now stale: the branch it was offered for
        // has moved under the cached answer.
        if let Some(branch) = checkout.branch.clone() {
            self.behind.remove(&(item.project.clone(), branch));
        }
        let report = outcome.report();
        if outcome.conflicted().is_empty() && outcome.stuck().is_empty() {
            return Ok(Opening::Finished(report));
        }
        Ok(Opening::Stopped(report))
    }

    /// Hand an item to the runtime under one recipe. Opens the plan when the
    /// item has none, and appends to it when it has. `picked` is what the
    /// reader chose for this dispatch alone — the first of the seven steps,
    /// made at the moment of dispatch and spent by it: nothing records it,
    /// and the next dispatch resolves from the second step down
    /// (§FS-005-dispatch.14).
    pub fn dispatch(
        &mut self,
        item: &Item,
        recipe: &Recipe,
        picked: Option<&recipe::HandPin>,
        dry_run: bool,
    ) -> Result<Outcome> {
        let site = self.site(item, recipe)?;
        // Who does it, before anything is written and before the opening move
        // is made: a refusal leaves nothing behind
        // (§FS-006-project-interface.9).
        let (target, model) = self.pin(item, recipe, picked, &site.dir)?;
        let states = self.states_yaml(&item.project)?;
        let plan_id = plan::plan_id(&item.id);

        // Where a machine is already in force, it answers before anything is
        // written: a recipe naming a state it does not have is refused, and a
        // refusal should leave nothing behind.
        let undeclared = |root: &WorkRoot| {
            EphorError::Command(format!(
                "recipe '{}' starts in state '{}', which the machine '{}' in {} does not declare (it has: {}).",
                recipe.id,
                recipe.state,
                root.machine,
                root.dir.display(),
                root.state_names().join(", ")
            ))
        };
        // A state that waits on files an earlier state writes is not one a
        // fresh ticket can start in: there is no earlier state, so the work
        // would be written and then sit there unrunnable — which is the thing
        // this whole check exists to prevent (§FS-005-dispatch.6).
        let unopenable = |root: &WorkRoot| {
            let openable = root.openable_states();
            let instead = match openable.is_empty() {
                true => "no state of it opens without one".to_string(),
                false => format!("states that open without one: {}", openable.join(", ")),
            };
            EphorError::Command(format!(
                "recipe '{}' starts in state '{}', which the machine '{}' in {} declares inputs for. \
                 A fresh ticket has no earlier state to have written them, so it would never run \
                 — {instead}. Give the recipe a 'state' in the work configuration.",
                recipe.id,
                recipe.state,
                root.machine,
                root.dir.display(),
            ))
        };
        let vet = |root: &WorkRoot| match () {
            _ if !root.declares(&recipe.state) => Err(undeclared(root)),
            _ if root.needs_input(&recipe.state) => Err(unopenable(root)),
            _ => Ok(()),
        };
        if dry_run {
            // A machine already in force is read without creating anything, and
            // it answers here too: a dry run that promises a ticket the real
            // dispatch would refuse is the most misleading promise of the set
            // (§FS-005-dispatch.6). Where no root exists yet there is nothing
            // to consult, and what the run promises is where the ticket would
            // go.
            if let Some(existing) = WorkRoot::open(&site.dir)? {
                vet(&existing)?;
            }
            let path = plan::plan_path_in(&site.dir, &plan_id);
            let existing = Plan::read(&path)?;
            let ticket = existing
                .as_ref()
                .map(|plan| plan.next_ticket_id(&recipe.id))
                .unwrap_or_else(|| format!("{}-1", recipe.id));
            return Ok(match existing {
                Some(_) => Outcome::Reopened {
                    plan: path,
                    ticket,
                    recipe: recipe.id.clone(),
                    changes: self
                        .ledger
                        .entries
                        .get(&item.id)
                        .map(|entry| entry.changes_since(item))
                        .unwrap_or_default(),
                },
                None => Outcome::Opened {
                    plan: path,
                    ticket,
                    recipe: recipe.id.clone(),
                },
            });
        }

        // The deterministic move first, and the work starts where it stopped
        // (§FS-005-dispatch.12). Before the machine is consulted and before
        // anything is written, so a clean move leaves no plan behind either.
        let opening = self.opening(item, recipe)?;
        if let Opening::Finished(report) = opening {
            return Ok(Outcome::Settled {
                move_name: recipe
                    .opens_with
                    .clone()
                    .unwrap_or_else(|| recipe.id.clone()),
                report,
            });
        }

        if let Some(existing) = WorkRoot::open(&site.dir)? {
            vet(&existing)?;
        }
        let root = WorkRoot::ensure(&site.dir, &states)?;
        vet(&root)?;
        let path = root.plan_path(&plan_id);
        let mut brief = dossier::render(&recipe.brief, &site.values);
        // What is handed over is the situation rather than the request to
        // reproduce it: the repository is standing in what this report
        // describes (§FS-005-dispatch.12).
        if let Opening::Stopped(report) = &opening {
            brief = format!("{brief}\n\n{report}");
        }
        let changes = self
            .ledger
            .entries
            .get(&item.id)
            .map(|entry| entry.changes_since(item))
            .unwrap_or_default();

        let (outcome, ticket_id) = match Plan::read(&path)? {
            None => {
                let ticket_id = format!("{}-1", recipe.id);
                let ticket = Ticket {
                    id: ticket_id.clone(),
                    title: format!("{} — {}", recipe.description, item.title),
                    state: recipe.state.clone(),
                    prior: None,
                    target: target.clone(),
                    model: model.clone(),
                    body: brief,
                };
                let mut plan =
                    Plan::create(&path, &root.machine, &item.title, &site.dossier, &ticket);
                plan.set_metadata(&ticket_id, &site.metadata);
                plan.save()?;
                (
                    Outcome::Opened {
                        plan: path.clone(),
                        ticket: ticket_id.clone(),
                        recipe: recipe.id.clone(),
                    },
                    ticket_id,
                )
            }
            Some(mut existing) => {
                let ticket_id = existing.next_ticket_id(&recipe.id);
                let prior = existing.last_ticket().map(|ticket| ticket.id);
                let mut body = String::new();
                if !changes.is_empty() {
                    body.push_str(&format!(
                        "Since the previous ticket: {}. The item above has been \
                         rewritten to what it is now.\n\n",
                        changes.join("; ")
                    ));
                }
                body.push_str(&brief);
                existing.set_dossier(&site.dossier);
                existing.append(&Ticket {
                    id: ticket_id.clone(),
                    title: format!("{} — {}", recipe.description, item.title),
                    state: recipe.state.clone(),
                    prior,
                    target: target.clone(),
                    model: model.clone(),
                    body,
                });
                existing.set_metadata(&ticket_id, &site.metadata);
                existing.save()?;
                (
                    Outcome::Reopened {
                        plan: path.clone(),
                        ticket: ticket_id.clone(),
                        recipe: recipe.id.clone(),
                        changes: changes.clone(),
                    },
                    ticket_id,
                )
            }
        };

        let entry = self.ledger.entries.entry(item.id.clone()).or_insert(Entry {
            project: item.project.clone(),
            title: item.title.clone(),
            url: item.url.clone(),
            root: root.dir.clone(),
            checkout: site.checkout.workspace.clone(),
            branch: site.checkout.branch.clone(),
            plan_id: plan_id.clone(),
            plan: path.clone(),
            dispatches: Vec::new(),
        });
        entry.title = item.title.clone();
        entry.url = item.url.clone();
        entry.root = root.dir.clone();
        entry.checkout = site.checkout.workspace.clone();
        entry.branch = site.checkout.branch.clone();
        entry.plan = path;
        entry.dispatches.push(Dispatch {
            ticket: ticket_id,
            recipe: recipe.id.clone(),
            at: Utc::now(),
            // A ticket goes into the item's own plan, which the entry already
            // names (§FS-005-dispatch.3).
            plan: None,
            snapshot: Snapshot::of(item),
        });
        Ok(outcome)
    }

    /// What the runtime offers about this project's work
    /// (§FS-005-dispatch.19). Asked at the project's own root, because a
    /// project keeps workflows of its own beside its checkout — and asked
    /// once per root, since a sweep asks about the same handful over and over.
    pub fn workflows(&mut self, project: &str) -> runtime::workflow::Offered {
        let Some(at) = self
            .placement(project)
            .map(|placement| placement.root.clone())
        else {
            return runtime::workflow::Offered::default();
        };
        if let Some(offered) = self.workflows.get(&at) {
            return offered.clone();
        }
        let offered = runtime::workflow::offered(&self.global, &at);
        self.workflows.insert(at, offered.clone());
        offered
    }

    /// The entries written beside the workflows this project can reach — the
    /// third home an entry may live in (§FS-005-dispatch.19), and the only
    /// one that travels with the workflow itself. Each comes back with where
    /// its workflow was found, because that is where the entry ranks in the
    /// menu. An entry that would not parse is reported rather than dropped
    /// (§FS-004-quick-actions.3).
    pub fn workflow_entries(
        &mut self,
        project: &str,
    ) -> Vec<(runtime::workflow::Source, ActionConfig)> {
        let offered = self.workflows(project);
        let mut entries = Vec::new();
        for workflow in &offered.workflows {
            match crate::work::workflow::beside(workflow) {
                Ok(Some(entry)) => entries.push((workflow.source, entry)),
                Ok(None) => {}
                Err(why) => self.note_once(&why),
            }
        }
        entries
    }

    /// Everything one workflow entry would write, with nothing written yet    /// Everything one workflow entry would write, with nothing written yet
    /// (§FS-005-dispatch.19): which workflow, every input answered and where
    /// its answer came from, and the plan it would lay down. A refusal here
    /// leaves nothing behind, which is the point of resolving before writing.
    pub fn laying(
        &mut self,
        item: &Item,
        entry: &ActionConfig,
        typed: &BTreeMap<String, String>,
        picked: Option<&recipe::HandPin>,
    ) -> Result<Laying> {
        let ask = entry.workflow.clone().ok_or_else(|| {
            EphorError::Command(format!("action '{}' lays down no workflow", entry.id))
        })?;
        let offered = self.workflows(&item.project);
        let workflow = offered.find(&ask.name).cloned().ok_or_else(|| {
            EphorError::Command(match &offered.refusal {
                Some(why) => format!("'{}' lays down '{}', and {why}", entry.id, ask.name),
                None => format!(
                    "'{}' names the workflow '{}', which this runtime does not offer{}",
                    entry.id,
                    ask.name,
                    match offered.workflows.is_empty() {
                        true => String::new(),
                        false => format!(
                            " (it offers: {})",
                            offered
                                .workflows
                                .iter()
                                .map(|workflow| workflow.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    }
                ),
            })
        })?;
        let site = self.site_for(item, entry.requires_checkout)?;
        // Where the plan goes: named after the matter and the entry, and
        // named apart from what an earlier run of the same entry left, since
        // two runs of one workflow about one item are two records and not a
        // correction of the first (§FS-005-dispatch.19).
        let plan_id = free_plan_id(
            &site.dir,
            &plan::plan_id(&format!("{}-{}", item.id, entry.id)),
        );
        let output = site.dir.join(&plan_id);
        // What ephor knows reaches a workflow as files: the paths are fixed
        // here so an answer may name them, and the files themselves are
        // written before the workflow is (§FS-005-dispatch.19).
        let carried = carried(&site.dir, &plan_id);
        let mut values = site.values.clone();
        values.insert("dossier", for_shell(&carried.join(DOSSIER)));
        values.insert("item", for_shell(&carried.join(ITEM)));
        // Who does the work, before anything is answered: a refusal leaves
        // nothing behind (§FS-006-project-interface.9).
        let choice = self.hand(&item.project, &entry.id, picked, None, &site.dir);
        if let runtime::roster::Choice::Refused(why) = choice {
            return Err(EphorError::Command(why));
        }
        if let Some(note) = choice.note() {
            self.note_once(note);
        }
        let hand = choice.pin().0;
        let named = self.named_hands(item, entry, &ask, &workflow, typed, &site.dir);
        let answered = crate::work::workflow::answer(
            &workflow,
            &ask,
            typed,
            &values,
            hand.as_deref(),
            &|name: &str| {
                named
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| Err(format!("'{name}' is not a hand this runtime knows")))
            },
        );
        Ok(Laying {
            entry: entry.id.clone(),
            workflow,
            answered,
            plan_id,
            output,
            site,
        })
    }

    /// Every hand the entry or the reader named for this workflow, resolved
    /// through the seven steps before any of them is used
    /// (§DA-006-hands-fill-a-workflows-targets). Resolved up front because
    /// one refusal is the whole entry's refusal, and because answering the
    /// inputs must not need the roster again halfway through.
    fn named_hands(
        &mut self,
        item: &Item,
        entry: &ActionConfig,
        ask: &crate::feed::config::WorkflowAsk,
        workflow: &runtime::workflow::Workflow,
        typed: &BTreeMap<String, String>,
        root: &std::path::Path,
    ) -> BTreeMap<String, std::result::Result<Option<String>, String>> {
        let is_hand = |name: &str| {
            workflow
                .input(name)
                .map(|input| input.hand)
                .unwrap_or(false)
                || ask.hands.iter().any(|listed| listed == name)
        };
        let mut names: Vec<String> = Vec::new();
        for (input, value) in &ask.inputs {
            if is_hand(input) {
                collect_names(value, &mut names);
            }
        }
        for (input, word) in typed {
            if is_hand(input) {
                // Read as the input's own type, exactly as the answering will
                // read it: a line naming several hands is several names here
                // too, or the list would be looked up as one hand called
                // `["a","b"]` (§DA-006-hands-fill-a-workflows-targets).
                let kind = workflow
                    .input(input)
                    .map(|input| input.kind)
                    .unwrap_or(runtime::workflow::Kind::Text);
                collect_names(&crate::work::workflow::coerce(word, kind), &mut names);
            }
        }
        names.sort();
        names.dedup();
        names
            .into_iter()
            .map(|name| {
                let rendered = match recipe::HandPin::parse(&name) {
                    Err(why) => Err(why),
                    Ok(pin) => {
                        match self.hand(&item.project, &entry.id, None, Some(&pin), root) {
                            runtime::roster::Choice::Refused(why) => Err(why),
                            runtime::roster::Choice::Unasked { .. } => Ok(None),
                            chosen => match chosen.pin().0 {
                                Some(target) => Ok(Some(target)),
                                // A hand with no model of its own rides a run
                                // as flags and has no selector to write into
                                // an input (§FS-005-dispatch.14).
                                None => Err(format!(
                                    "hand '{name}' names an agent with no model of its own, and \
                                     an input naming who does the work needs the full spelling"
                                )),
                            },
                        }
                    }
                };
                (name, rendered)
            })
            .collect()
    }

    /// Lay a workflow's plan down beside the item's (§FS-005-dispatch.19).
    /// Writes files and nothing else: what runs the plan is the reader, from
    /// the board where every other operation is run
    /// ([§FS-005-dispatch.7](crate)).
    pub fn lay(&mut self, item: &Item, laying: &Laying, dry_run: bool) -> Result<Laid> {
        if !laying.answered.refusals.is_empty() {
            return Err(EphorError::Command(laying.answered.refusals.join("; ")));
        }
        if !laying.answered.missing.is_empty() {
            return Err(EphorError::Command(format!(
                "'{}' cannot be laid down: nothing answers {}. Answer them with --set \
                 <input>=<value>, or say them in the entry.",
                laying.entry,
                laying.answered.missing.join(", ")
            )));
        }
        let states = self.states_yaml(&item.project)?;
        let root = WorkRoot::ensure(&laying.site.dir, &states)?;
        let carried = carried(&root.dir, &laying.plan_id);
        std::fs::create_dir_all(&carried).map_err(|err| {
            EphorError::Command(format!("Cannot make {}: {err}", carried.display()))
        })?;
        let put = |name: &str, content: &str| -> Result<PathBuf> {
            let path = carried.join(name);
            std::fs::write(&path, content).map_err(|err| {
                EphorError::Command(format!("Cannot write {}: {err}", path.display()))
            })?;
            Ok(path)
        };
        // The matter's own name leads it: a ticket carries the title in the
        // plan's heading, and a workflow's plan is not ephor's to write — so
        // without this the one thing every reader looks for first would be
        // the one thing missing (§FS-005-dispatch.2).
        put(
            DOSSIER,
            &format!("# {}\n\n{}", item.title, laying.site.dossier),
        )?;
        put(ITEM, &identifiers(&laying.site.metadata))?;
        let values = put(
            VALUES,
            &serde_json::to_string_pretty(&laying.answered.values)
                .unwrap_or_else(|_| "{}".to_string()),
        )?;
        let report = runtime::workflow::lay(
            &self.global,
            &laying.site.checkout.workspace,
            &laying.workflow,
            &values,
            &laying.output,
            dry_run,
        )?;
        if dry_run {
            return Ok(Laid {
                outcome: Outcome::Laid {
                    plan: laying.output.clone(),
                    plan_id: laying.plan_id.clone(),
                    workflow: laying.workflow.id.clone(),
                    entry: laying.entry.clone(),
                },
                report,
            });
        }
        // Where the plan actually landed is the binding's answer, not a path
        // ephor composed (§AR-007-runtime.1).
        let plan = runtime::workflow::laid(&laying.output)
            .map(|found| found.path)
            .unwrap_or_else(|| laying.output.clone());
        let entry_id = laying.entry.clone();
        let ledger_entry = self.ledger.entries.entry(item.id.clone()).or_insert(Entry {
            project: item.project.clone(),
            title: item.title.clone(),
            url: item.url.clone(),
            root: root.dir.clone(),
            checkout: laying.site.checkout.workspace.clone(),
            branch: laying.site.checkout.branch.clone(),
            // The item's own plan, whether or not it has one yet: a workflow
            // lays down a plan beside it and never replaces it
            // (§FS-005-dispatch.3).
            plan_id: plan::plan_id(&item.id),
            plan: plan::plan_path_in(&root.dir, &plan::plan_id(&item.id)),
            dispatches: Vec::new(),
        });
        ledger_entry.title = item.title.clone();
        ledger_entry.url = item.url.clone();
        ledger_entry.root = root.dir.clone();
        ledger_entry.checkout = laying.site.checkout.workspace.clone();
        ledger_entry.dispatches.push(Dispatch {
            ticket: String::new(),
            recipe: entry_id.clone(),
            at: Utc::now(),
            plan: Some(laying.plan_id.clone()),
            snapshot: Snapshot::of(item),
        });
        Ok(Laid {
            outcome: Outcome::Laid {
                plan,
                plan_id: laying.plan_id.clone(),
                workflow: laying.workflow.id.clone(),
                entry: entry_id,
            },
            report,
        })
    }

    /// Ask an item for something no recipe covers, in the reader's own words
    /// (§FS-005-dispatch.10). An ordinary ticket in every other respect — the
    /// same dossier, the same plan, the same order — and refused for nothing
    /// but being unrunnable: what is asked for is asked for.
    pub fn ask(
        &mut self,
        item: &Item,
        words: &str,
        state: Option<&str>,
        dry_run: bool,
    ) -> Result<Outcome> {
        let words = words.trim();
        if words.is_empty() {
            return Err(EphorError::Command("Nothing was asked for.".to_string()));
        }
        let recipe = Recipe {
            id: "ask".to_string(),
            icon: "✎".to_string(),
            // The first line names the ticket; the whole of it is the brief.
            description: summarize(words),
            state: state.unwrap_or(WORKING_STATE).to_string(),
            when: Default::default(),
            // Asked for on the spot, about whatever is on screen: a branch
            // that is not here is a fact for the dossier to state, not a
            // reason to refuse the reader.
            needs_checkout: false,
            // Typed on the spot by somebody who is right there: the reader
            // starts it, as they always did (§FS-005-dispatch.24).
            autorun: false,
            brief: words.to_string(),
            // What was asked for is what is written down: ephor does not make
            // a move of its own in front of somebody's own words.
            opens_with: None,
            // An ask pins nobody of its own, so `hands` answers for it under
            // the id 'ask' like any other action (§FS-006-project-interface.9).
            hand: None,
            target: None,
            model: None,
        };
        self.dispatch(item, &recipe, None, dry_run)
    }

    /// Reopen an item's work when the item has moved under it
    /// (§FS-005-dispatch.5). Work whose item is unchanged is left alone.
    pub fn sync(&mut self, item: &Item, dry_run: bool) -> Result<Outcome> {
        let Some(entry) = self.ledger.entries.get(&item.id) else {
            return Ok(Outcome::Current);
        };
        let changes = entry.changes_since(item);
        if changes.is_empty() {
            return Ok(Outcome::Current);
        }
        // Under the recipe that fits the item as it is now, preferring the one
        // last asked for while it still fits. What moved may have moved the
        // item out of its old category: a pull request whose gate went green
        // and whose author asked a question is not a red gate any more, and
        // reopening it as one would hand the work a ticket about a problem
        // that is no longer there.
        let last = entry
            .last()
            .map(|dispatch| dispatch.recipe.clone())
            .unwrap_or_default();
        let offers = self.offers(item);
        let Some(recipe) = offers
            .iter()
            .find(|candidate| candidate.id == last)
            .or_else(|| offers.first())
            .cloned()
        else {
            return Ok(Outcome::Dormant { changes });
        };
        self.dispatch(item, &recipe, None, dry_run)
    }

    /// Take one of an item's tickets back (§FS-005-dispatch.16): the runtime's
    /// own move into the abandonment state, asked in its own words, refused
    /// beforehand on what ephor can see for itself. The ledger is untouched —
    /// it records what was asked, and this was asked.
    pub fn cancel(&self, item: &str, ticket: &str, why: &str, dry_run: bool) -> Result<Cancelled> {
        let entry = self.ledger.entries.get(item).ok_or_else(|| {
            EphorError::Command(format!(
                "{item} has no work to cancel — nothing was dispatched for it"
            ))
        })?;
        cancel_ticket(
            &self.global,
            &entry.root,
            &entry.plan_id,
            &entry.plan,
            ticket,
            why,
            dry_run,
        )
    }

    /// The reply a run drafted about this matter and did not send, where one
    /// was drafted (§FS-005-dispatch.13). Attached to the matter here rather
    /// than stored on it: it is what the runtime left on disk, and reading it
    /// every time is what keeps ephor from reporting on itself
    /// (§FS-005-dispatch.4).
    pub fn proposal(&self, item: &Item) -> Option<runtime::results::Proposal> {
        let entry = self.ledger.entries.get(&item.id)?;
        runtime::results::proposal(&entry.root, &entry.plan_id)
    }

    /// Record that this matter's proposed reply was posted, so it is offered
    /// once (§FS-005-dispatch.13).
    pub fn proposal_posted(&self, item: &Item) -> Result<()> {
        let Some(entry) = self.ledger.entries.get(&item.id) else {
            return Ok(());
        };
        runtime::results::mark_posted(&entry.root, &entry.plan_id)
    }

    /// Everything one item's work root has to say about what is going there,
    /// read once (§FS-005-dispatch.15.1, §AR-005-capabilities.1).
    ///
    /// Found by looking, never remembered from the keypress: the ledger says
    /// which tickets this entry opened and which plans it laid, the plans say
    /// what state each is in, the machine says which of those states are over
    /// and which are questions for a person, and the lock says whether a run
    /// holds the root at all (§FS-005-dispatch.15).
    ///
    /// Read here rather than in [`WorkAt::going`] because a menu asks about
    /// every row of one subject, and every row of one subject shares this root:
    /// six recipe entries used to mean six states-document parses, six plan
    /// parses, a dozen lock probes and six descriptor reads for one keypress.
    /// [`WorkAt::going`] then answers each row off what is already in hand
    /// (§FS-005-dispatch.21).
    pub fn work_at(&self, item: &Item) -> Option<WorkAt<'_>> {
        let entry = self.ledger.entries.get(&item.id)?;
        let live = runtime::watch::live(&self.global, &entry.root);
        Some(WorkAt {
            machine: WorkRoot::open(&entry.root).ok().flatten(),
            plan: Plan::read(&entry.plan).ok().flatten(),
            // Neither the witness nor the lock's birth is worth reading on a
            // root nothing holds: a slot nobody released under a free lock is
            // a dead run's leavings, which is the board's business and not a
            // running mark (§FS-005-dispatch.15).
            witness: live.then(|| runtime::watch::witness(&self.global, &entry.root)),
            lock_born: live
                .then(|| runtime::watch::lock_born(&entry.root))
                .flatten(),
            // Who the run says it is, read from the descriptor beside the lock
            // and gated on it: a descriptor outlives the run that wrote it
            // (§FS-005-dispatch.20).
            identity: live
                .then(|| runtime::watch::identity(&self.global, &entry.root))
                .flatten(),
            live,
            entry,
        })
    }

    /// What an item's work is doing, read from the plan.
    pub fn status(&self, item: &Item) -> Option<WorkStatus> {
        let entry = self.ledger.entries.get(&item.id)?;
        Some(self.status_of(entry, Some(item)))
    }

    pub fn status_of(&self, entry: &Entry, item: Option<&Item>) -> WorkStatus {
        status_of_entry(&self.global, entry, item)
    }

    /// Every work root that should have a run and has none
    /// (§FS-005-dispatch.24).
    ///
    /// The whole of the sweep, and it reads the world rather than a memory of
    /// what was dispatched: the roots are the ones every other reading walks
    /// (§FS-005-dispatch.15), a ticket counts by what it is rather than by
    /// who appended it, and whether a run is already there is the runtime's
    /// own lock. So this is idempotent — running it twice in a second starts
    /// one run — and asks nothing about what it did last time, apart from the
    /// one thing it must remember: a root whose start failed rests before it
    /// is tried again.
    pub fn due(&mut self, now: DateTime<Utc>) -> Vec<Due> {
        let roots = self.work_roots();
        // Which recipes asked to run themselves, per project. Resolved once
        // per project: the tables behind it are the same for every root of
        // one of them.
        let autoruns: BTreeMap<String, BTreeSet<String>> = roots
            .iter()
            .flat_map(|group| group.plans.iter().map(|plan| plan.project.clone()))
            .collect::<BTreeSet<String>>()
            .into_iter()
            .map(|project| {
                let asked = self
                    .recipes(&project)
                    .into_iter()
                    .filter(|recipe| recipe.autorun)
                    .map(|recipe| recipe.id)
                    .collect();
                (project, asked)
            })
            .collect();
        due_among(&self.global, &roots, &autoruns, &self.ledger, now)
    }
}

/// The sweep's own reading, with the roots and the recipes already gathered
/// (§FS-005-dispatch.24). A free function so a test can ask the question the
/// way every surface does, without a registry behind it — the shape
/// [`status_of_entry`] and [`enumerate_roots`] already take.
pub fn due_among(
    global: &WorkConfig,
    roots: &[runtime::watch::RootPlans],
    autoruns: &BTreeMap<String, BTreeSet<String>>,
    ledger: &Ledger,
    now: DateTime<Utc>,
) -> Vec<Due> {
    // What ephor dispatched, so a ticket it wrote is judged by the recipe it
    // was written from rather than by the shape of its id.
    let dispatched: BTreeMap<(PathBuf, String), String> = ledger
        .entries
        .values()
        .flat_map(|entry| {
            entry.dispatches.iter().map(move |dispatch| {
                (
                    (entry.root.clone(), dispatch.ticket.clone()),
                    dispatch.recipe.clone(),
                )
            })
        })
        .collect();
    let mut due = Vec::new();
    for group in roots {
        // A root a run already holds is left alone: the live run reaches
        // a ticket written beneath it, and a second run there would only
        // wait for the first (§FS-005-dispatch.24).
        if runtime::watch::live(global, &group.root) {
            continue;
        }
        // A root that could not be started is passed over for a while,
        // longer each time — a runner that refuses must not turn every
        // sweep into a spawn.
        if ledger
            .starts
            .get(&root_key(&group.root))
            .is_some_and(|start| start.resting(now))
        {
            continue;
        }
        // Finality and gating are the machine's words. With none to say
        // them, nothing here can be judged runnable, and the honest move
        // is to start nothing rather than to guess (§FS-005-dispatch.15).
        let Some(machine) = WorkRoot::open(&group.root).ok().flatten() else {
            continue;
        };
        let mut plans: Vec<String> = Vec::new();
        let mut tickets: Vec<String> = Vec::new();
        for plan_ref in &group.plans {
            let Some(asked) = autoruns.get(&plan_ref.project) else {
                continue;
            };
            if asked.is_empty() {
                continue;
            }
            let Ok(Some(plan)) = Plan::read(&plan_ref.path) else {
                continue;
            };
            for ticket in plan.tickets() {
                let state = ticket.state.as_deref();
                // Over, waiting on a person, or somebody's to move: none
                // of them is work a run would advance
                // (§FS-005-dispatch.24, §FS-005-dispatch.15).
                if state.map(|state| machine.is_final(state)).unwrap_or(true)
                    || state.map(|state| machine.is_gating(state)).unwrap_or(false)
                    || ticket.assignee.is_some()
                {
                    continue;
                }
                // The ledger says which recipe ephor wrote a ticket from;
                // for one a hand appended, the id says it, because ids
                // are `<recipe>-<n>` by construction. Either way the
                // recipe is a fact about the ticket
                // (§FS-005-dispatch.24).
                let recipe = dispatched
                    .get(&(group.root.clone(), ticket.id.clone()))
                    .map(String::as_str)
                    .or_else(|| recipe_of_ticket(&ticket.id));
                if !recipe.is_some_and(|recipe| asked.contains(recipe)) {
                    continue;
                }
                if !plans.contains(&plan_ref.plan_id) {
                    plans.push(plan_ref.plan_id.clone());
                }
                tickets.push(format!("{}.{}", plan_ref.plan_id, ticket.id));
            }
        }
        if tickets.is_empty() {
            continue;
        }
        // Where the run is made from, and whether it may be made there at
        // all: work about a branch belongs in that branch's working tree,
        // and a tree standing on another branch holds different code
        // (§FS-005-dispatch.3). Dispatch refuses on this and so does a
        // start, because with nobody watching there is no one to notice
        // (§FS-005-dispatch.24).
        let known = ledger
            .entries
            .values()
            .find(|entry| entry.root == group.root);
        let checkout = known
            .map(Entry::checkout)
            .unwrap_or_else(|| root_checkout(&group.root));
        if let Some(wanted) = known.and_then(|entry| entry.branch.as_deref()) {
            // Only a branch that can be read and disagrees refuses: an
            // unreadable or detached HEAD is a fact nobody can establish,
            // and refusing on one is worse than the run — the same
            // latitude dispatch takes (§FS-005-dispatch.3).
            if crate::git::head_branch(&checkout).is_some_and(|head| head != wanted) {
                continue;
            }
        }
        due.push(Due {
            project: group
                .plans
                .first()
                .map(|plan| plan.project.clone())
                .unwrap_or_default(),
            root: group.root.clone(),
            checkout,
            plans,
            tickets,
            item: known.and_then(|entry| {
                ledger
                    .entries
                    .iter()
                    .find(|(_, candidate)| candidate.root == entry.root)
                    .map(|(id, _)| id.clone())
            }),
        });
    }
    due
}

impl Dispatcher {
    /// Start a run on every root the sweep says is due, and say what each
    /// came to (§FS-005-dispatch.24).
    ///
    /// The one implementation of autorun's *act*, so the timer, the command,
    /// and the dispatch that just wrote a ticket all start runs the same way
    /// and cannot drift into three of them (§AR-009-surfaces.1). Runs go
    /// beneath the screen and only beneath it: a sweep has nobody at a
    /// terminal by definition, so where the binding has no detached shape
    /// this starts nothing and says so rather than seizing the terminal of
    /// whatever invoked it (§FS-005-dispatch.24, §FS-005-dispatch.20).
    pub fn start_due(
        &mut self,
        now: DateTime<Utc>,
        projects: &[String],
        runner_args: &[String],
    ) -> Vec<Launched> {
        // The runtime is a rung like any other capacity, and with nothing
        // bound there is no run to start (§AR-005-capabilities.2).
        if runtime::refusal(&self.global).is_some() {
            return Vec::new();
        }
        let detaches = runtime::can_detach(&self.global);
        self.due(now)
            .into_iter()
            .filter(|root| projects.is_empty() || projects.contains(&root.project))
            .map(|root| {
                if !detaches {
                    return Launched::refused(
                        &root,
                        format!(
                            "{} cannot start a run detached here, and a run nobody asked for \
                             must not take a terminal",
                            runtime::runner(&self.global)
                        ),
                    );
                }
                let hand = self
                    .ledger
                    .entries
                    .values()
                    .find(|entry| entry.root == root.root)
                    .cloned()
                    .and_then(|entry| {
                        let status = self.status_of(&entry, None);
                        self.run_hand(&entry, &status)
                    });
                match runtime::start_detached(
                    &self.global,
                    &root.root,
                    &root.checkout,
                    &root.plans,
                    hand.as_ref(),
                    runner_args,
                ) {
                    Ok(started) => {
                        self.start_worked(&root.root);
                        Launched {
                            id: started.id,
                            finished: started.finished,
                            failed: None,
                            ..Launched::of(&root)
                        }
                    }
                    Err(err) => {
                        self.start_failed(&root.root, &err.to_string(), now);
                        Launched::refused(&root, err.to_string())
                    }
                }
            })
            .collect()
    }

    /// Remember that starting a run on this root did not work, so the next
    /// sweep passes it over for a while (§FS-005-dispatch.24). Ephor's record
    /// of ephor's own act; the work's state is untouched and stays the plan's
    /// (§FS-005-dispatch.4).
    pub fn start_failed(&mut self, root: &std::path::Path, says: &str, now: DateTime<Utc>) {
        let key = root_key(root);
        let failures = self
            .ledger
            .starts
            .get(&key)
            .map(|start| start.failures.saturating_add(1))
            .unwrap_or(1);
        self.ledger.starts.insert(
            key,
            ledger::Start {
                at: now,
                failures,
                says: says.to_string(),
            },
        );
    }

    /// Forget a root's failed starts: a run began there, so whatever was
    /// wrong is not wrong now. Nothing is remembered about a start that
    /// worked — the run leaves a lock, and the lock is what every later sweep
    /// reads (§FS-005-dispatch.15).
    pub fn start_worked(&mut self, root: &std::path::Path) {
        self.ledger.starts.remove(&root_key(root));
    }

    pub fn save(&self) -> Result<()> {
        ledger::store(&self.ledger)
    }
}

/// One work root a sweep should start a run on (§FS-005-dispatch.24).
#[derive(Debug, Clone)]
pub struct Due {
    pub project: String,
    pub root: PathBuf,
    /// The checkout the run is made from.
    pub checkout: PathBuf,
    /// The plans holding what made this root due — what the run is narrowed
    /// to, so a runtime project the reader keeps in the same root for their
    /// own work is not swept up by ephor's.
    pub plans: Vec<String>,
    /// The tickets themselves, plan-qualified: what the line saying a run
    /// started names as the reason.
    pub tickets: Vec<String>,
    /// The matter this root's work is about, where the ledger knows one —
    /// where a failure's news lands (§FS-005-dispatch.24).
    pub item: Option<String>,
}

/// What starting one due root came to (§FS-005-dispatch.24).
#[derive(Debug, Clone)]
pub struct Launched {
    pub project: String,
    pub root: PathBuf,
    /// The matter the work is about, where the ledger knows one — where the
    /// news of a failure lands (§FS-005-dispatch.24).
    pub item: Option<String>,
    /// The tickets that made this root due, plan-qualified.
    pub tickets: Vec<String>,
    /// What the run calls itself, where it named itself.
    pub id: Option<String>,
    /// The run was over before the launcher returned — nothing was left to
    /// do. Reported as over rather than as started, so nobody is sent to a
    /// board with nothing on it (§FS-005-dispatch.20).
    pub finished: bool,
    /// Why no run was started, where none was.
    pub failed: Option<String>,
}

impl Launched {
    fn of(due: &Due) -> Launched {
        Launched {
            project: due.project.clone(),
            root: due.root.clone(),
            item: due.item.clone(),
            tickets: due.tickets.clone(),
            id: None,
            finished: false,
            failed: None,
        }
    }

    fn refused(due: &Due, why: String) -> Launched {
        Launched {
            failed: Some(why),
            ..Launched::of(due)
        }
    }

    /// The one line this is worth: what started, or what stopped it. The
    /// same sentence wherever it is said, so a command and a screen never
    /// phrase one situation two ways (§AR-009-surfaces.1).
    pub fn says(&self) -> String {
        match (&self.failed, &self.id, self.finished) {
            (Some(why), ..) => format!("⚠ no run started on {}: {why}", self.root.display()),
            (None, Some(id), false) => format!("▶ run {id} started"),
            (None, Some(id), true) => format!("✓ run {id} finished already"),
            (None, None, false) => "▶ run started".to_string(),
            (None, None, true) => "✓ the run finished already".to_string(),
        }
    }

    /// What a reading calls this (§REQ-002-parity.3).
    pub fn outcome(&self) -> &'static str {
        match (&self.failed, self.finished) {
            (Some(_), _) => "failed",
            (None, true) => "done",
            (None, false) => "started",
        }
    }
}

/// How a work root is named in the ledger's record of starts. The path as
/// written, so the key travels with the same spelling the entry carries.
fn root_key(root: &std::path::Path) -> String {
    root.to_string_lossy().into_owned()
}

/// The checkout a work root belongs to, where nothing in the ledger says: the
/// directory holding it, which is what a work root's own template renders
/// under.
fn root_checkout(root: &std::path::Path) -> PathBuf {
    root.parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf())
}

/// The recipe a ticket id was written from. Ids are `<recipe>-<n>`
/// ([`plan::Plan::next_ticket_id`]), so the recipe is readable off a ticket
/// nobody recorded a dispatch for (§FS-005-dispatch.24).
fn recipe_of_ticket(id: &str) -> Option<&str> {
    let (recipe, number) = id.rsplit_once('-')?;
    (!recipe.is_empty() && !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
        .then_some(recipe)
}

/// An entry's work as it stands, read from the plan (§FS-005-dispatch.4). A
/// free function so a test can read a plan back the way every surface does.
pub fn status_of_entry(global: &WorkConfig, entry: &Entry, item: Option<&Item>) -> WorkStatus {
    let recipes: BTreeMap<&str, &str> = entry
        .dispatches
        .iter()
        .map(|dispatch| (dispatch.ticket.as_str(), dispatch.recipe.as_str()))
        .collect();
    // When each ticket was asked for (§FS-005-dispatch.18). The ledger is the
    // only thing that knows: the plan tracks what the work reached, never
    // when it was handed over ([§4]). Dispatched twice under one id, the
    // later ask stands — it is the one the reader last pressed.
    let asked: BTreeMap<&str, DateTime<Utc>> = entry
        .dispatches
        .iter()
        .map(|dispatch| (dispatch.ticket.as_str(), dispatch.at))
        .collect();
    let root = WorkRoot::open(&entry.root).ok().flatten();
    let plan = Plan::read(&entry.plan).ok().flatten();
    let tickets: Vec<TicketStatus> = plan
        .as_ref()
        .map(|plan| {
            plan.tickets()
                .into_iter()
                .map(|ticket| TicketStatus {
                    asked: asked.get(ticket.id.as_str()).copied(),
                    recipe: recipes
                        .get(ticket.id.as_str())
                        .map(|recipe| recipe.to_string())
                        .unwrap_or_else(|| ticket.id.clone()),
                    finished: ticket
                        .state
                        .as_deref()
                        .map(|state| {
                            root.as_ref()
                                .map(|root| root.is_final(state))
                                // With no machine to ask, a ticket is
                                // finished when the work left a verdict.
                                .unwrap_or(false)
                        })
                        .unwrap_or(false),
                    // The abandonment state is the machine's word too:
                    // judged only where the machine is there to say it is
                    // final (§FS-005-dispatch.16).
                    cancelled: ticket.cancelled()
                        && root
                            .as_ref()
                            .is_some_and(|root| root.cancel_state().is_some()),
                    waiting: ticket
                        .state
                        .as_deref()
                        .map(|state| {
                            root.as_ref()
                                .map(|root| root.is_gating(state))
                                .unwrap_or(false)
                        })
                        .unwrap_or(false),
                    // The review's line where the work reached one; for a
                    // ticket taken back, the reason the reader gave, read
                    // out of the runtime's own result (§FS-005-dispatch.16).
                    verdict: runtime::results::verdict(&entry.root, &entry.plan_id, &ticket.id)
                        .or_else(|| {
                            ticket
                                .cancelled()
                                .then(|| {
                                    runtime::results::result(
                                        &entry.root,
                                        &entry.plan_id,
                                        &ticket.id,
                                    )
                                })
                                .flatten()
                        }),
                    assignee: ticket.assignee,
                    pinned: ticket.pinned,
                    id: ticket.id,
                    title: ticket.title,
                    state: ticket.state,
                })
                .collect()
        })
        .unwrap_or_default();
    let advance = tickets.iter().find(|ticket| ticket.waiting).map(|ticket| {
        runtime::advance_command(global, &ticket.id, ticket.state.as_deref().unwrap_or("?"))
    });
    WorkStatus {
        workflows: entry
            .dispatches
            .iter()
            .filter(|dispatch| dispatch.is_workflow())
            .count(),
        project: entry.project.clone(),
        root: entry.root.clone(),
        plan_id: entry.plan_id.clone(),
        checkout: entry.checkout(),
        plan: entry.plan.clone(),
        // The matter's own plan is missing only where something was meant
        // to be in it. An entry whose every dispatch laid a plan of its own
        // never wrote a ticket here (§FS-005-dispatch.19), so there is no
        // plan to be missing — and reporting one would be ephor alarming a
        // reader about a file it never said it would write.
        missing: plan.is_none()
            && entry
                .dispatches
                .iter()
                .any(|dispatch| !dispatch.is_workflow()),
        tickets,
        advance,
        changes: item
            .map(|item| entry.changes_since(item))
            .unwrap_or_default(),
    }
}

/// Cancel one ticket of one plan (§FS-005-dispatch.16): refuse on what the
/// artifacts already say — no runner bound, no such ticket, a ticket already
/// over, a machine with no abandonment state, a ticket a live run holds — and
/// otherwise ask the runtime for the move in its own words, and report what
/// that leaves waiting. A free function so the command line and the interface
/// make one and the same set of refusals in one and the same order
/// (§DA-005-cancel-is-the-runtimes-move). `why` empty is the reason left
/// unsaid, recorded as exactly that.
pub fn cancel_ticket(
    config: &WorkConfig,
    root: &std::path::Path,
    plan_id: &str,
    plan_path: &std::path::Path,
    ticket: &str,
    why: &str,
    dry_run: bool,
) -> Result<Cancelled> {
    // The move is the runtime's; with nobody to make it the plan is left as
    // it is, and the refusal is the workable rung's sentence
    // (§AR-005-capabilities.2).
    if let Some(refusal) = runtime::refusal(config) {
        return Err(EphorError::Command(refusal));
    }
    let plan = Plan::read(plan_path)?.ok_or_else(|| {
        EphorError::Command(format!(
            "the plan {} is gone — nothing there to cancel",
            plan_path.display()
        ))
    })?;
    let found = plan.ticket(ticket).ok_or_else(|| {
        let known: Vec<String> = plan.tickets().into_iter().map(|t| t.id).collect();
        EphorError::Command(format!(
            "{} holds no ticket '{ticket}' (it has: {})",
            plan_path.display(),
            match known.is_empty() {
                true => "none".to_string(),
                false => known.join(", "),
            }
        ))
    })?;
    let from = found.state.clone().ok_or_else(|| {
        EphorError::Command(format!(
            "{ticket} declares no state, so there is nothing to move it from"
        ))
    })?;
    // A machine that cannot say what final means cannot say what cancelled
    // means either; and one that declares no abandonment state has nowhere to
    // put the ticket (§FS-005-dispatch.6).
    let machine = WorkRoot::open(root)?.ok_or_else(|| {
        EphorError::Command(format!(
            "{} declares no state machine, so no state there is one to cancel into",
            root.display()
        ))
    })?;
    if found.cancelled() && machine.cancel_state().is_some() {
        return Err(EphorError::Command(format!(
            "{ticket} is already cancelled"
        )));
    }
    if machine.is_final(&from) {
        return Err(EphorError::Command(format!(
            "{ticket} is already over — it sits in '{from}', which is final"
        )));
    }
    if machine.cancel_state().is_none() {
        return Err(EphorError::Command(format!(
            "the machine '{}' in {} declares no final '{}' state to cancel into (it has: {}) — \
             add one and a transition into it from anywhere (`ephor work states` prints ephor's, \
             which has both), or move the ticket by hand",
            machine.machine,
            root.display(),
            plan::CANCELLED,
            machine.state_names().join(", ")
        )));
    }
    // A ticket a live run holds is that run's to finish (§FS-005-dispatch.16):
    // moving it out from under the agent is interfering with the run, which
    // is not this key's to do (§FS-005-dispatch.15).
    if runtime::watch::held_by_live_run(config, root, plan_id, ticket, &from) {
        return Err(EphorError::Command(format!(
            "{ticket} is held by a live run in '{from}' — the run is its to finish; wait for it, \
             or stop the run where it is running, then cancel"
        )));
    }
    let left_waiting: Vec<String> = plan
        .tickets()
        .into_iter()
        .filter(|other| {
            other.id != ticket
                && other.prior.iter().any(|prior| prior == ticket)
                && !other
                    .state
                    .as_deref()
                    .is_some_and(|state| machine.is_final(state))
        })
        .map(|other| other.id)
        .collect();
    let why = match why.trim().is_empty() {
        true => CANCELLED_UNSAID,
        false => why.trim(),
    };
    if !dry_run {
        runtime::cancel(config, root, plan_path, ticket, &from, why)?;
    }
    Ok(Cancelled {
        ticket: ticket.to_string(),
        from,
        plan: plan_path.to_path_buf(),
        left_waiting,
    })
}

/// Where an item's work goes for a project: its own template, or the global
/// one. A free function because `ephor checkout` resolves the same place
/// (§FS-006-project-interface.7) and two answers to "where does work live"
/// would eventually disagree.
pub fn root_template(global: &WorkConfig, project: Option<&ProjectWorkConfig>) -> String {
    project
        .and_then(|work| work.root.clone())
        .unwrap_or_else(|| global.root.clone())
}

/// The board's universe (§FS-005-dispatch.15): one group per execution root —
/// the ledger's plans first, since they carry the matter behind them, and
/// every plan enumeration finds after, item-less where nothing dispatched it.
/// Enumeration resolves the work-root template at each configured place: the
/// project's own checkout and each branch workspace on disk, because a
/// branch-addressable project keeps a work root per branch workspace and each
/// one is its own execution root. A template naming a placeholder only an
/// item can fill is skipped rather than guessed — work written through it is
/// the ledger's to know. Reading only, and bounded: one directory listing per
/// candidate work root, no plan opened, no repository entered, no runner
/// asked (§FS-005-dispatch.15.1, §AR-007-runtime.3).
pub fn enumerate_roots(
    global: &WorkConfig,
    projects: &BTreeMap<String, ProjectWorkConfig>,
    placements: &[Placement],
    ledger: &Ledger,
) -> Vec<runtime::watch::RootPlans> {
    use runtime::watch::{PlanRef, RootPlans};
    // One row per execution root means one per *directory*, not one per
    // spelling: a workspace template that symlinks back to the checkout
    // renders the same root under two names, and the runtime's lock is on
    // the directory, so two groups here would be one operation shown twice.
    let canon =
        |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut groups: BTreeMap<PathBuf, RootPlans> = BTreeMap::new();
    for (item_id, entry) in &ledger.entries {
        let root = canon(&entry.root);
        let group = groups.entry(root.clone()).or_insert_with(|| RootPlans {
            root,
            plans: Vec::new(),
        });
        // The matter's own plan, where a ticket was ever written into it. An
        // entry whose every dispatch laid a plan of its own never wrote one
        // (§FS-005-dispatch.19), and a row for a file ephor never promised
        // would be the board reporting on itself.
        if entry.plan.is_file() || entry.dispatches.iter().any(|d| !d.is_workflow()) {
            group.plans.push(PlanRef {
                project: entry.project.clone(),
                plan_id: entry.plan_id.clone(),
                path: entry.plan.clone(),
                item: Some(item_id.clone()),
                title: entry.title.clone(),
            });
        }
        // And the plans workflows laid down beside it. The listing below finds
        // these anyway — they are plans in a work root — but only the ledger
        // knows which matter they are about, which is what `Enter` needs
        // (§FS-005-dispatch.15).
        for dispatch in entry.dispatches.iter().filter(|d| d.is_workflow()) {
            let Some(plan_id) = dispatch.plan.as_deref() else {
                continue;
            };
            let Some(found) = runtime::workflow::laid(&entry.root.join(plan_id)) else {
                continue;
            };
            group.plans.push(PlanRef {
                project: entry.project.clone(),
                plan_id: found.plan_id,
                path: found.path,
                item: Some(item_id.clone()),
                title: entry.title.clone(),
            });
        }
    }
    for placement in placements {
        let template = root_template(global, projects.get(&placement.project));
        let mut places = vec![placement.root.clone()];
        for branch in &placement.branches {
            places.extend(placement.workspace_for(&branch.branch));
        }
        places.sort();
        places.dedup();
        // A template that ignores the workspace renders every place to one
        // root; listing it once is enough.
        let mut listed: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
        for place in places {
            if !place.is_dir() {
                continue;
            }
            let values = BTreeMap::from([
                ("workspace", place.to_string_lossy().into_owned()),
                ("root", placement.root.to_string_lossy().into_owned()),
                ("project", placement.project.clone()),
            ]);
            let rendered = dossier::render(&template, &values);
            if rendered.contains('{') {
                continue;
            }
            let root = canon(&crate::paths::resolve_path(&rendered));
            if !listed.insert(root.clone()) {
                continue;
            }
            for found in plan::plans_in(&root) {
                let group = groups.entry(root.clone()).or_insert_with(|| RootPlans {
                    root: root.clone(),
                    plans: Vec::new(),
                });
                // The ledger may spell the same plan through a different
                // alias; a plan is the file, not the spelling.
                if group
                    .plans
                    .iter()
                    .any(|plan| canon(&plan.path) == found.path)
                {
                    continue;
                }
                group.plans.push(PlanRef {
                    project: placement.project.clone(),
                    plan_id: found.plan_id,
                    path: found.path,
                    item: None,
                    title: String::new(),
                });
            }
        }
    }
    groups.into_values().collect()
}

/// The states YAML installed into a work root that has none: the project's
/// own, the global one, or the machine ephor ships.
pub fn states_yaml(global: &WorkConfig, project: Option<&ProjectWorkConfig>) -> Result<String> {
    let configured = project
        .and_then(|work| work.states.clone())
        .or_else(|| global.states.clone());
    match configured {
        Some(path) => {
            let path = crate::paths::resolve_path(&path);
            std::fs::read_to_string(&path).map_err(|err| {
                EphorError::Command(format!(
                    "Cannot read the configured state machine {}: {err}",
                    path.display()
                ))
            })
        }
        None => Ok(plan::SHIPPED_STATES.to_string()),
    }
}

/// A work root ephor made or found, and what it could not do on the way
/// (§FS-006-project-interface.7).
pub struct Store {
    pub dir: PathBuf,
    /// Whether this call is what put it there. False where it was already on
    /// disk, which is what lets a checkout asked for twice say what it did
    /// rather than say the same thing twice (§FS-004-quick-actions.7.1).
    pub made: bool,
    /// What the runtime said when it could not make its own project there.
    /// None where it did — and never an error, because the workspace around
    /// this one directory is whole either way (§FS-004-quick-actions.7).
    pub note: Option<String>,
}

/// Make the work root for a workspace, so the first dispatch into that branch
/// has somewhere to land and what is under way is visible from the moment the
/// tree exists (§FS-006-project-interface.7).
///
/// The runtime makes its own project and ephor says where: the directory is
/// the work root resolved here, and what a project in it consists of is the
/// runner's answer rather than a copy of that answer kept in ephor. Ephor's
/// own state machine goes in beside what the runner wrote, and the store's
/// self-ignore is added whatever the runner's project says about version
/// control — that is what keeps this from being an artifact required of the
/// project (§REQ-001-boundary.3): what it holds is ephor's own planning state
/// that happens to live in a checkout.
pub fn ensure_store(
    global: &WorkConfig,
    project: Option<&ProjectWorkConfig>,
    project_id: &str,
    workspace: &std::path::Path,
    root: &std::path::Path,
) -> Result<Store> {
    let values = BTreeMap::from([
        ("workspace", workspace.to_string_lossy().into_owned()),
        ("root", root.to_string_lossy().into_owned()),
        ("project", project_id.to_string()),
    ]);
    let dir =
        crate::paths::resolve_path(&dossier::render(&root_template(global, project), &values));
    let states = states_yaml(global, project)?;
    let made = !dir.is_dir();
    // The directory first: the runner is asked to make a place that is there,
    // and what ephor installs afterwards reads what the runner left rather than
    // racing it.
    plan::create_dir(&dir)?;
    let note = match runtime::init(global, &dir) {
        runtime::Initialized::Project => None,
        runtime::Initialized::Refused(why) => Some(why),
    };
    WorkRoot::ensure(&dir, &states)?;
    Ok(Store { dir, made, note })
}

/// The state a hand-written ask starts in when the reader names none: the
/// shipped machine's working state, which is also the shipped recipes'.
const WORKING_STATE: &str = "fix";

/// A ticket title out of what was asked: its first line, cut at a word so it
/// reads in a list beside the item's own title. The whole of what was asked is
/// in the ticket body regardless; this is only the label.
fn summarize(words: &str) -> String {
    const LABEL: usize = 56;
    let first = words.lines().next().unwrap_or("").trim();
    if first.chars().count() <= LABEL {
        return first.to_string();
    }
    let kept: String = first.chars().take(LABEL).collect();
    let cut = kept.rfind(' ').unwrap_or(kept.len());
    format!("{}…", kept[..cut].trim_end())
}

fn clamp(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit - 1).collect::<String>() + "…"
}

/// What a recipe's deterministic opening move reached
/// (§FS-005-dispatch.12).
enum Opening {
    /// The recipe declares none, or there was nothing here for it to do.
    None,
    /// It finished: there is nothing left to hand over.
    Finished(String),
    /// It stopped, and this is the situation the ticket is about.
    Stopped(String),
}

/// What one workflow entry would write, with nothing written yet
/// (§FS-005-dispatch.19).
pub struct Laying {
    /// The entry that names the workflow — the menu's own id, and the key a
    /// hands table answers by (§FS-006-project-interface.9).
    pub entry: String,
    pub workflow: runtime::workflow::Workflow,
    /// Every input, answered, and where each answer came from.
    pub answered: crate::work::workflow::Answered,
    /// The plan this would lay down, by the id the runtime will know it by.
    pub plan_id: String,
    /// Where it goes, under the item's own work root.
    pub output: PathBuf,
    site: Site,
}

impl Laying {
    /// The work root the plan lands in.
    pub fn root(&self) -> &std::path::Path {
        &self.site.dir
    }

    /// Whether this can be laid down as it stands.
    pub fn ready(&self) -> bool {
        self.answered.missing.is_empty() && self.answered.refusals.is_empty()
    }
}

/// A workflow's plan, written (§FS-005-dispatch.19).
pub struct Laid {
    pub outcome: Outcome,
    /// What the binding said as it wrote — its own account of what it made,
    /// which is what a reader is shown before and after.
    pub report: String,
}

/// The files ephor writes for a workflow to read, under the work root's own
/// hidden corner: a dotted name is not a plan, so enumerating the root steps
/// over it (§FS-005-dispatch.15).
fn carried(root: &std::path::Path, plan_id: &str) -> PathBuf {
    root.join(CARRIED).join(plan_id)
}

const CARRIED: &str = ".ephor";
const DOSSIER: &str = "dossier.md";
const ITEM: &str = "item.json";
const VALUES: &str = "values.json";

/// The item as data, as a workflow's own programs can read it — the same
/// names a shell action gets in its environment (§FS-005-dispatch.8).
fn identifiers(metadata: &[(&'static str, String)]) -> String {
    let fields: serde_json::Map<String, Value> = metadata
        .iter()
        .map(|(key, value)| ((*key).to_string(), Value::String(value.clone())))
        .collect();
    serde_json::to_string_pretty(&fields).unwrap_or_else(|_| "{}".to_string())
}

/// A plan id nothing has taken in this work root: the name the entry earns,
/// then the same name counted, so a second run of one workflow about one item
/// is a second record rather than a correction of the first
/// (§FS-005-dispatch.19).
fn free_plan_id(root: &std::path::Path, base: &str) -> String {
    if !root.join(base).exists() {
        return base.to_string();
    }
    (2..)
        .map(|nth| format!("{base}-{nth}"))
        .find(|id| !root.join(id).exists())
        .unwrap_or_else(|| base.to_string())
}

/// Every hand named in one written answer, at whatever depth it was written.
fn collect_names(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(name) => out.push(name.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_names(item, out)),
        _ => {}
    }
}

/// Where one item's work goes, and what the ticket will say.
struct Site {
    dir: PathBuf,
    dossier: String,
    /// The item as data, for the state machine's programs
    /// (§FS-005-dispatch.8).
    metadata: Vec<(&'static str, String)>,
    values: BTreeMap<&'static str, String>,
    #[allow(dead_code)] // kept for callers that report where work landed
    checkout: crate::branches::Checkout,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branches::BranchInfo;
    use std::fs;
    use std::path::Path;

    fn placement(project: &str, root: &Path, template: Option<&str>) -> Placement {
        Placement {
            project: project.to_string(),
            root: root.to_path_buf(),
            template: template.map(String::from),
            branches: Vec::new(),
            main_branch: None,
            repos: Vec::new(),
            aliases: Vec::new(),
            territory: Vec::new(),
            trust: Default::default(),
        }
    }

    fn branch(name: &str) -> BranchInfo {
        BranchInfo {
            branch: name.to_string(),
            ticket: None,
            active: false,
            is_release: false,
            declared: true,
        }
    }

    fn plant(dir: &Path, plan: &str, title: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(plan), format!("# Rhei: {title}\n")).unwrap();
    }

    /// The work roots are enumerated from the configured places
    /// (§FS-005-dispatch.15): the project's own checkout and each branch
    /// workspace on disk — the work root is per branch workspace, and each
    /// is its own execution root. A declared branch with no workspace yet is
    /// skipped, not guessed at; a plan the ledger knows keeps its matter and
    /// its title, and one found beside it arrives item-less.
    #[test]
    fn the_roots_are_the_configured_places_and_the_ledger_keeps_its_matter() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("widget");
        let mut widget = placement("widget", &root, Some("{project_root}/branches/{branch}"));
        widget.branches = vec![branch("you/ABC-1"), branch("you/ABC-2-unmade")];
        // The project's own tickets, and a branch workspace holding a
        // dispatched plan beside a hand-written one.
        plant(&root.join("panta"), "housekeeping.rhei.md", "Housekeeping");
        let workspace = root.join("branches/you/ABC-1");
        plant(
            &workspace.join("panta"),
            "forge-widget-7.rhei.md",
            "ledger's",
        );
        plant(&workspace.join("panta"), "audit.rhei.md", "Audit the paths");

        let mut ledger = Ledger {
            version: 1,
            entries: BTreeMap::new(),
            starts: BTreeMap::new(),
        };
        ledger.entries.insert(
            "forge:widget/7".to_string(),
            Entry {
                project: "widget".to_string(),
                title: "Widen the retry window".to_string(),
                url: None,
                root: workspace.join("panta"),
                checkout: workspace.clone(),
                branch: None,
                plan_id: "forge-widget-7".to_string(),
                plan: workspace.join("panta/forge-widget-7.rhei.md"),
                dispatches: Vec::new(),
            },
        );

        let groups = enumerate_roots(
            &WorkConfig::default(),
            &BTreeMap::new(),
            std::slice::from_ref(&widget),
            &ledger,
        );
        assert_eq!(groups.len(), 2, "{:?}", roots_of(&groups));
        let by_root = |dir: &Path| {
            let dir = fs::canonicalize(dir).unwrap();
            groups
                .iter()
                .find(|group| group.root == dir)
                .unwrap_or_else(|| panic!("{} should be a root", dir.display()))
        };
        let own = by_root(&root.join("panta"));
        assert_eq!(own.plans.len(), 1);
        assert_eq!(own.plans[0].plan_id, "housekeeping");
        assert_eq!(own.plans[0].item, None);

        let dispatched = by_root(&workspace.join("panta"));
        assert_eq!(dispatched.plans.len(), 2);
        // The ledger's plan first, with its matter and its title; the
        // hand-written one beside it, item-less.
        assert_eq!(dispatched.plans[0].item.as_deref(), Some("forge:widget/7"));
        assert_eq!(dispatched.plans[0].title, "Widen the retry window");
        assert_eq!(dispatched.plans[1].plan_id, "audit");
        assert_eq!(dispatched.plans[1].item, None);
        assert_eq!(dispatched.plans[1].title, "");
    }

    fn roots_of(groups: &[runtime::watch::RootPlans]) -> Vec<&Path> {
        groups.iter().map(|group| group.root.as_path()).collect()
    }

    /// A stand-in runtime with the transition verb the cancel asks for
    /// (§FS-005-dispatch.16): it moves the named ticket's state line and
    /// records the result the way the shipped binding does — the plan's own
    /// state line, and `runtime/results/<plan>.<ticket>.md`. Anything else
    /// it is asked, it refuses, in a sentence of its own.
    const STAND_IN_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="${1:-}"; shift || true
[ "$verb" = transition ] || { echo "  × stand-in: no verb '$verb'" >&2; exit 2; }
plan="$1"; shift
task=""; from=""; to=""; result=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --task) task="$2"; shift 2 ;;
    --from) from="$2"; shift 2 ;;
    --to) to="$2"; shift 2 ;;
    --result) result="$2"; shift 2 ;;
    *) shift ;;
  esac
done
grep -q "^### Task $task:" "$plan" || { echo "  × stand-in: no task '$task'" >&2; exit 1; }
if [ "$from" = "fix" ]; then
  echo "  × Task $task cannot leave state fix." >&2
  echo "  │ Missing required output artifact: report" >&2
  exit 1
fi
awk -v task="$task" -v to="$to" '
  /^### Task / { current = $3; sub(":", "", current) }
  /^\*\*State:\*\*/ && current == task { print "**State:** " to; next }
  { print }
' "$plan" > "$plan.tmp" && mv "$plan.tmp" "$plan"
stem="$(basename "$plan" .rhei.md)"
mkdir -p "$(dirname "$plan")/runtime/results"
printf '## Result\n\n%s\n' "$result" >> "$(dirname "$plan")/runtime/results/$stem.$task.md"
echo "Task $stem.$task transitioned: '$from' → '$to'"
"#;

    /// A work root under the shipped machine, holding one plan with three
    /// tickets — one done, two open, the third ordered after the second — and
    /// the config binding the stand-in runtime as the runner.
    fn root_with_plan(tmp: &Path) -> (WorkConfig, PathBuf, PathBuf) {
        let root = tmp.join("panta");
        WorkRoot::ensure(&root, plan::SHIPPED_STATES).unwrap();
        let plan_path = root.join("forge-demo-17.rhei.md");
        fs::write(
            &plan_path,
            concat!(
                "# Rhei: demo\n**States:** ephor-work\n\n## Tasks\n\n",
                "### Task fix-gate-1: one\n**State:** done\n\nbody\n\n",
                "### Task fix-gate-2: two\n**State:** review\n**Prior:** Task fix-gate-1\n\nbody\n\n",
                "### Task fix-gate-3: three\n**State:** review\n**Prior:** Task fix-gate-2\n\nbody\n\n",
                "### Task ask-1: four\n**State:** fix\n\nbody\n",
            ),
        )
        .unwrap();
        let runner = tmp.join("stand-in-runtime");
        fs::write(&runner, STAND_IN_RUNTIME).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
        }
        // The runner is named by path and looked up as one word, so unlike the
        // other stand-ins it has to be executed rather than read. Settle it
        // before any test runs it: `exec` on a file this process just wrote
        // fails with `ETXTBSY` while a child another thread forked still holds
        // it open, and the `exec` that trips is the shell's, where nothing can
        // wait it out.
        crate::seams::summons::settle_executable(&runner);
        let config = WorkConfig {
            runner: Some(runner.to_string_lossy().into_owned()),
            ..WorkConfig::default()
        };
        (config, root, plan_path)
    }

    fn entry_for(root: &Path, plan_path: &Path) -> Entry {
        Entry {
            project: "demo".to_string(),
            title: "demo".to_string(),
            url: None,
            root: root.to_path_buf(),
            checkout: root.parent().unwrap().to_path_buf(),
            branch: None,
            plan_id: "forge-demo-17".to_string(),
            plan: plan_path.to_path_buf(),
            dispatches: Vec::new(),
        }
    }

    /// A cancel is the runtime's move: the stand-in moves the state line and
    /// records the reason, ephor reads both back — the ticket taken back,
    /// with its reason as its line — and names what was ordered after it.
    /// A reason left blank is recorded as exactly that; a dry run moves
    /// nothing; and a second cancel of the same ticket is refused
    /// (§FS-005-dispatch.16).
    #[test]
    fn a_cancel_asks_the_runtime_names_what_it_leaves_waiting_and_is_read_back() {
        let tmp = tempfile::tempdir().unwrap();
        let (config, root, plan_path) = root_with_plan(tmp.path());

        let dry = cancel_ticket(
            &config,
            &root,
            "forge-demo-17",
            &plan_path,
            "fix-gate-2",
            "",
            true,
        )
        .expect("a dry run answers");
        assert_eq!(dry.left_waiting, vec!["fix-gate-3"]);
        assert!(fs::read_to_string(&plan_path)
            .unwrap()
            .contains("**State:** review\n**Prior:** Task fix-gate-1"));

        let done = cancel_ticket(
            &config,
            &root,
            "forge-demo-17",
            &plan_path,
            "fix-gate-2",
            "asked twice",
            false,
        )
        .expect("the stand-in agrees");
        assert_eq!(done.from, "review");
        assert_eq!(done.left_waiting, vec!["fix-gate-3"]);
        assert!(
            done.describe().contains("fix-gate-3 is ordered after it"),
            "{}",
            done.describe()
        );
        let text = fs::read_to_string(&plan_path).unwrap();
        assert!(
            text.contains("### Task fix-gate-2: two\n**State:** cancelled"),
            "{text}"
        );
        assert!(
            text.contains("### Task fix-gate-3: three\n**State:** review"),
            "{text}"
        );

        // Read back: taken back, with the reason as its line; the badge says
        // taken back where that is the last word (§FS-005-dispatch.16).
        let status = status_of_entry(&config, &entry_for(&root, &plan_path), None);
        let two = status
            .tickets
            .iter()
            .find(|t| t.id == "fix-gate-2")
            .unwrap();
        assert!(two.cancelled && two.finished);
        assert_eq!(two.verdict.as_deref(), Some("asked twice"));
        assert_eq!(status.open_tickets(), 2);

        // Blank is recorded as blank, and said so on the row.
        cancel_ticket(
            &config,
            &root,
            "forge-demo-17",
            &plan_path,
            "fix-gate-3",
            "   ",
            false,
        )
        .unwrap();
        let status = status_of_entry(&config, &entry_for(&root, &plan_path), None);
        let three = status
            .tickets
            .iter()
            .find(|t| t.id == "fix-gate-3")
            .unwrap();
        assert_eq!(three.verdict.as_deref(), Some(CANCELLED_UNSAID));

        // Already taken back: nothing to do, said rather than asked again.
        let again = cancel_ticket(
            &config,
            &root,
            "forge-demo-17",
            &plan_path,
            "fix-gate-2",
            "",
            false,
        )
        .expect_err("already cancelled");
        assert!(again.to_string().contains("already cancelled"), "{again}");

        // The next ticket ephor writes follows the last one not taken back.
        let plan = Plan::read(&plan_path).unwrap().unwrap();
        assert_eq!(plan.last_ticket().map(|t| t.id), Some("ask-1".to_string()));
    }

    /// Everything ephor can see for itself is refused before the runtime is
    /// asked, in one sentence each; and what the runtime refuses comes back
    /// in its own words (§FS-005-dispatch.16, §DA-005-cancel-is-the-runtimes-move).
    #[test]
    fn a_cancel_refuses_on_what_the_artifacts_say_and_relays_what_the_runtime_says() {
        let tmp = tempfile::tempdir().unwrap();
        let (config, root, plan_path) = root_with_plan(tmp.path());
        let refuse = |config: &WorkConfig, ticket: &str| {
            cancel_ticket(
                config,
                &root,
                "forge-demo-17",
                &plan_path,
                ticket,
                "",
                false,
            )
            .expect_err("refused")
            .to_string()
        };

        // No runner: the workable rung's own sentence, and the plan untouched.
        let unbound = WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            ..WorkConfig::default()
        };
        assert!(refuse(&unbound, "fix-gate-2").contains("is not on PATH"));

        // No such ticket, and one already over.
        assert!(refuse(&config, "fix-gate-9").contains("holds no ticket 'fix-gate-9'"));
        assert!(refuse(&config, "fix-gate-1").contains("already over"));

        // The runtime's own refusal, relayed: the stand-in will not leave `fix`.
        let said = refuse(&config, "ask-1");
        assert!(said.contains("refused: Task ask-1 cannot leave state fix. Missing required output artifact: report"), "{said}");
        assert!(fs::read_to_string(&plan_path)
            .unwrap()
            .contains("### Task ask-1: four\n**State:** fix"));

        // A machine with no abandonment state: refused by name, with what to add.
        let bare = tmp.path().join("bare");
        fs::create_dir_all(&bare).unwrap();
        fs::write(
            bare.join("states.yaml"),
            "name: bare\nstates:\n  fix:\n    agent: x\n  done:\n    final: true\n",
        )
        .unwrap();
        let bare_plan = bare.join("p.rhei.md");
        fs::write(
            &bare_plan,
            "# Rhei: p\n**States:** bare\n\n## Tasks\n\n### Task a-1: a\n**State:** fix\n\nbody\n",
        )
        .unwrap();
        let said = cancel_ticket(&config, &bare, "p", &bare_plan, "a-1", "", false)
            .expect_err("nowhere to put it")
            .to_string();
        assert!(said.contains("the machine 'bare' in"), "{said}");
        assert!(
            said.contains("declares no final 'cancelled' state"),
            "{said}"
        );
        assert!(said.contains("ephor work states"), "{said}");
    }

    /// A ticket a live run holds is the run's to finish (§FS-005-dispatch.16):
    /// with the root's lock held and the journal naming the ticket where the
    /// plan has it, the cancel refuses and names the run; with the lock free
    /// the same journal line is a dead run's, and the cancel goes ahead.
    #[cfg(unix)]
    #[test]
    fn a_ticket_a_live_run_holds_is_not_cancelled_from_under_it() {
        let tmp = tempfile::tempdir().unwrap();
        let (config, root, plan_path) = root_with_plan(tmp.path());
        fs::create_dir_all(root.join(".rhei")).unwrap();
        fs::create_dir_all(root.join("runtime/logs")).unwrap();
        let lock_path = root.join(".rhei/run.lock");
        fs::write(&lock_path, "").unwrap();
        let log = root.join("runtime/logs/task-fix-gate-2-review.log");
        fs::write(&log, "working").unwrap();
        fs::write(
            root.join("runtime/transitions.log"),
            "2026-08-15T10:00:00Z  fix-gate-2  start@review  runtime/logs/task-fix-gate-2-review.log\n",
        )
        .unwrap();

        // The lock held, as a live run holds it.
        let held = fs::File::open(&lock_path).unwrap();
        held.lock().unwrap();
        let said = cancel_ticket(
            &config,
            &root,
            "forge-demo-17",
            &plan_path,
            "fix-gate-2",
            "",
            false,
        )
        .expect_err("a live run holds it")
        .to_string();
        assert!(said.contains("held by a live run in 'review'"), "{said}");
        // Another ticket in the same root, not held, is fair.
        cancel_ticket(
            &config,
            &root,
            "forge-demo-17",
            &plan_path,
            "fix-gate-3",
            "",
            false,
        )
        .expect("queued, not held");
        held.unlock().unwrap();
        drop(held);

        // The lock free: the run died, the journal line is history, and the
        // reader may take the ticket back.
        cancel_ticket(
            &config,
            &root,
            "forge-demo-17",
            &plan_path,
            "fix-gate-2",
            "the run died",
            false,
        )
        .expect("nobody holds it now");
    }

    /// The walk is bounded by what can resolve without an item: a work-root
    /// template that ignores the workspace is listed once however many
    /// places render to it, and one naming a placeholder only an item can
    /// fill is skipped rather than guessed (§FS-005-dispatch.15.1).
    #[test]
    fn a_shared_root_is_listed_once_and_an_item_template_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("widget");
        let mut widget = placement("widget", &root, Some("{project_root}/branches/{branch}"));
        widget.branches = vec![branch("a"), branch("b")];
        fs::create_dir_all(root.join("branches/a")).unwrap();
        fs::create_dir_all(root.join("branches/b")).unwrap();
        plant(&root.join("work"), "shared.rhei.md", "Shared");

        let mut projects = BTreeMap::new();
        projects.insert(
            "widget".to_string(),
            ProjectWorkConfig {
                root: Some("{root}/work".to_string()),
                ..ProjectWorkConfig::default()
            },
        );
        let ledger = Ledger {
            version: 1,
            entries: BTreeMap::new(),
            starts: BTreeMap::new(),
        };
        let groups = enumerate_roots(
            &WorkConfig::default(),
            &projects,
            std::slice::from_ref(&widget),
            &ledger,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].root, fs::canonicalize(root.join("work")).unwrap());
        assert_eq!(groups[0].plans.len(), 1, "one listing, one plan, no dupes");

        // A placeholder only an item can fill cannot resolve here.
        projects.insert(
            "widget".to_string(),
            ProjectWorkConfig {
                root: Some("{root}/work/{ticket}".to_string()),
                ..ProjectWorkConfig::default()
            },
        );
        let groups = enumerate_roots(
            &WorkConfig::default(),
            &projects,
            std::slice::from_ref(&widget),
            &ledger,
        );
        assert!(groups.is_empty(), "{:?}", roots_of(&groups));
    }

    /// A branch workspace that is a symlink back to the checkout renders the
    /// same work root under two names, and one directory is one operation —
    /// the runtime's lock is on the directory, so two rows here would show
    /// one run twice (§FS-005-dispatch.15). Groups collapse on the
    /// directory, never on the spelling.
    #[cfg(unix)]
    #[test]
    fn an_aliased_workspace_is_one_root_not_two() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("widget");
        let mut widget = placement("widget", &root, Some("{project_root}/branches/{branch}"));
        widget.branches = vec![branch("main")];
        plant(&root.join("panta"), "housekeeping.rhei.md", "Housekeeping");
        fs::create_dir_all(root.join("branches")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("branches/main")).unwrap();

        let ledger = Ledger {
            version: 1,
            entries: BTreeMap::new(),
            starts: BTreeMap::new(),
        };
        let groups = enumerate_roots(
            &WorkConfig::default(),
            &BTreeMap::new(),
            std::slice::from_ref(&widget),
            &ledger,
        );
        assert_eq!(groups.len(), 1, "{:?}", roots_of(&groups));
        assert_eq!(groups[0].plans.len(), 1);
    }

    fn ticket(id: &str, state: &str) -> TicketStatus {
        TicketStatus {
            id: id.to_string(),
            recipe: "fix-gate".to_string(),
            title: "fix the red gate".to_string(),
            state: Some(state.to_string()),
            finished: false,
            cancelled: false,
            waiting: false,
            assignee: None,
            pinned: None,
            verdict: None,
            asked: None,
        }
    }

    fn work_status(tickets: Vec<TicketStatus>) -> WorkStatus {
        WorkStatus {
            project: "widget".to_string(),
            root: PathBuf::from("/w/widget/panta"),
            plan_id: "forge-widget-42".to_string(),
            checkout: PathBuf::from("/w/widget"),
            plan: PathBuf::from("/w/widget/panta/forge-widget-42.rhei.md"),
            missing: false,
            tickets,
            workflows: 0,
            changes: Vec::new(),
            advance: None,
        }
    }

    /// The work stands on a row per open ticket, and the one the runtime
    /// parked stands first: it is the one part nobody else will move
    /// (§FS-005-dispatch.9, §FS-005-dispatch.23). Each row names the ticket it
    /// is, so cancelling on it takes back that one and not the plan's newest
    /// (§FS-005-dispatch.16).
    #[test]
    fn every_open_ticket_gets_a_row_and_the_parked_one_leads() {
        let parked = TicketStatus {
            waiting: true,
            ..ticket("fix-gate-2", "ask")
        };
        let status = work_status(vec![ticket("fix-gate-1", "collect"), parked]);
        let lines = status.lines(60);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0].tone, Tone::Waiting);
        assert!(lines[0].said.contains("waiting on you"), "{lines:?}");
        assert_eq!(lines[0].ticket.as_deref(), Some("fix-gate-2"));
        assert_eq!(lines[1].tone, Tone::Going);
        assert_eq!(lines[1].said, "fix-gate · collect");
        assert_eq!(lines[1].ticket.as_deref(), Some("fix-gate-1"));
    }

    /// What is over is one row and not many: the whole record is the work
    /// screen's, and a tree that grew a row per finished ticket would bury the
    /// matters between them (§FS-005-dispatch.18, §FS-005-dispatch.23). It
    /// carries no ticket, because there is nothing there to take back.
    #[test]
    fn a_plan_with_nothing_open_stands_on_one_row_for_what_it_decided() {
        let finished = |id: &str, verdict: &str| TicketStatus {
            finished: true,
            verdict: Some(verdict.to_string()),
            state: Some("done".to_string()),
            ..ticket(id, "done")
        };
        let status = work_status(vec![
            finished("fix-gate-1", "an older one"),
            finished("fix-gate-2", "the gate is green"),
        ]);
        let lines = status.lines(60);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].tone, Tone::Over);
        assert_eq!(lines[0].said, "fix-gate · the gate is green");
        assert_eq!(lines[0].ticket, None);

        // Taken back is a different kind of over, and the row says which.
        let taken_back = TicketStatus {
            finished: true,
            cancelled: true,
            ..ticket("fix-gate-1", "cancelled")
        };
        let cancelled = work_status(vec![taken_back]).lines(60);
        assert_eq!(cancelled[0].said, "fix-gate · cancelled");
        assert_eq!(cancelled[0].marker, "⊘");
    }

    /// An item that moved under its work says so on a row of its own: it is a
    /// fact about the work and not about the matter (§FS-005-dispatch.5,
    /// §FS-005-dispatch.23).
    #[test]
    fn an_item_that_moved_under_its_work_says_so_on_a_row_of_its_own() {
        let mut status = work_status(vec![ticket("fix-gate-1", "collect")]);
        status.changes = vec!["1 new message".to_string()];
        let lines = status.lines(60);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[1].tone, Tone::Stale);
        assert!(lines[1].said.contains("1 new message"), "{lines:?}");
        assert_eq!(lines[1].ticket, None);
    }
    // ---- the sweep behind autorun (§FS-005-dispatch.24) ----

    /// A work root holding a plan, a machine, and whatever tickets are asked
    /// for. `fix` runs, `needs-human` gates, `done` is over — the shipped
    /// shape, narrowed to what the sweep has to tell apart.
    fn due_root(root: &Path, tickets: &str) -> runtime::watch::RootPlans {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("states.yaml"),
            concat!(
                "name: m\n",
                "states:\n",
                "  collect:\n    agent: x\n",
                "  fix:\n    agent: x\n",
                "  needs-human:\n    gating: true\n",
                "  done:\n    final: true\n",
            ),
        )
        .unwrap();
        fs::write(
            root.join("widget-42.rhei.md"),
            format!("# Rhei: t\n**States:** m\n\n## Tasks\n\n{tickets}"),
        )
        .unwrap();
        runtime::watch::RootPlans {
            root: root.to_path_buf(),
            plans: vec![runtime::watch::PlanRef {
                project: "widget".to_string(),
                plan_id: "widget-42".to_string(),
                path: root.join("widget-42.rhei.md"),
                item: Some("forge:widget/42".to_string()),
                title: "Widen the retry window".to_string(),
            }],
        }
    }

    fn ticket_at(id: &str, state: &str) -> String {
        format!("### Task {id}: do it\n**State:** {state}\n\nwork\n\n")
    }

    fn asking(recipes: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
        BTreeMap::from([(
            "widget".to_string(),
            recipes.iter().map(|id| id.to_string()).collect(),
        )])
    }

    /// A runner every machine has, so nothing here is refused for want of one.
    fn work_config() -> WorkConfig {
        WorkConfig {
            runner: Some("sh".to_string()),
            ..WorkConfig::default()
        }
    }

    fn empty_ledger() -> Ledger {
        Ledger {
            version: 1,
            entries: BTreeMap::new(),
            starts: BTreeMap::new(),
        }
    }

    /// The plain case: an open ticket from a recipe that asked to run itself,
    /// on a root nothing is running on, is due — and it says which ticket
    /// made it so (§FS-005-dispatch.24).
    #[test]
    fn an_open_ticket_from_an_autorun_recipe_makes_its_root_due() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
        let due = due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &empty_ledger(),
            Utc::now(),
        );
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].root, root);
        assert_eq!(due[0].plans, vec!["widget-42".to_string()]);
        assert_eq!(due[0].tickets, vec!["widget-42.fix-gate-1".to_string()]);
        // The checkout is the directory the work root sits in — where the
        // runtime is run from (§FS-005-dispatch.3).
        assert_eq!(due[0].checkout, tmp.path());
    }

    /// Silence means the key: a recipe that never asked to run itself is
    /// started by the reader, as everything always was
    /// (§FS-005-dispatch.24).
    #[test]
    fn a_recipe_that_did_not_ask_is_never_due() {
        let tmp = tempfile::tempdir().unwrap();
        let group = due_root(&tmp.path().join("panta"), &ticket_at("review-1", "fix"));
        assert!(due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &empty_ledger(),
            Utc::now(),
        )
        .is_empty());
    }

    /// What a run would not advance is not what makes a root due: work that
    /// is over, work parked on a question for a person, and work somebody
    /// has claimed (§FS-005-dispatch.24, §FS-005-dispatch.15).
    #[test]
    fn finished_parked_and_claimed_tickets_make_nothing_due() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        let group = due_root(
            &root,
            &format!(
                "{}{}{}",
                ticket_at("fix-gate-1", "done"),
                ticket_at("fix-gate-2", "needs-human"),
                "### Task fix-gate-3: do it\n**State:** fix\n**Assignee:** luna\n\nwork\n\n",
            ),
        );
        assert!(due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &empty_ledger(),
            Utc::now(),
        )
        .is_empty());
    }

    /// A root a run already holds gets nothing: the runtime schedules one run
    /// per root, and the live run reaches a ticket written beneath it
    /// (§FS-005-dispatch.24).
    #[test]
    fn a_root_a_run_already_holds_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
        fs::create_dir_all(root.join(".rhei")).unwrap();
        fs::write(root.join(".rhei/run.lock"), "").unwrap();
        let holder = fs::File::open(root.join(".rhei/run.lock")).unwrap();
        holder.lock().unwrap();

        assert!(
            due_among(
                &work_config(),
                std::slice::from_ref(&group),
                &asking(&["fix-gate"]),
                &empty_ledger(),
                Utc::now(),
            )
            .is_empty(),
            "a second run there would only wait for the first"
        );
        // And once that run lets go, the root is due again. Closing the
        // holder is not always the instant the kernel releases the lock —
        // the release rides on the last reference to the open file going
        // away, which can be deferred by a millisecond under load — so this
        // waits for the world to agree rather than assuming it already does.
        // Everything ephor does here reads the lock as it is at the moment it
        // asks (§FS-005-dispatch.15), which is exactly what is being checked.
        drop(holder);
        let freed = std::time::Instant::now();
        while runtime::watch::live(&work_config(), &root) {
            assert!(
                freed.elapsed() < std::time::Duration::from_secs(5),
                "the run let go and the lock never came free"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            due_among(
                &work_config(),
                std::slice::from_ref(&group),
                &asking(&["fix-gate"]),
                &empty_ledger(),
                Utc::now(),
            )
            .len(),
            1
        );
    }

    /// Finality and gating are the machine's words. With no machine to say
    /// them, nothing can be judged runnable, and the sweep starts nothing
    /// rather than guessing (§FS-005-dispatch.15).
    #[test]
    fn a_root_with_no_machine_starts_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
        fs::remove_file(root.join("states.yaml")).unwrap();
        assert!(due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &empty_ledger(),
            Utc::now(),
        )
        .is_empty());
    }

    /// A root whose start failed rests, and is tried again once it has
    /// (§FS-005-dispatch.24) — otherwise a runner that refuses turns every
    /// sweep into another spawn.
    #[test]
    fn a_root_whose_start_failed_rests_before_it_is_tried_again() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
        let now = Utc::now();
        let mut ledger = empty_ledger();
        ledger.starts.insert(
            root.to_string_lossy().into_owned(),
            ledger::Start {
                at: now,
                failures: 1,
                says: "the runner refused".to_string(),
            },
        );
        let sweep = |ledger: &Ledger, at| {
            due_among(
                &work_config(),
                std::slice::from_ref(&group),
                &asking(&["fix-gate"]),
                ledger,
                at,
            )
        };
        assert!(sweep(&ledger, now).is_empty(), "it has just failed");
        assert_eq!(
            sweep(&ledger, now + chrono::Duration::minutes(6)).len(),
            1,
            "and it is tried again once the interval is out"
        );
        // Two failures in a row and it waits longer than one did.
        ledger
            .starts
            .get_mut(&root.to_string_lossy().into_owned())
            .unwrap()
            .failures = 3;
        assert!(
            sweep(&ledger, now + chrono::Duration::minutes(6)).is_empty(),
            "the interval grows with each consecutive failure"
        );
    }

    /// A ticket a hand appended is due exactly as a dispatched one: the
    /// recipe is a fact about the ticket, read off its id where no dispatch
    /// recorded one (§FS-005-dispatch.24).
    #[test]
    fn a_ticket_nobody_dispatched_is_due_by_the_recipe_its_id_names() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-7", "fix"));
        let due = due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &empty_ledger(),
            Utc::now(),
        );
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].tickets, vec!["widget-42.fix-gate-7".to_string()]);
    }

    /// An id that is not `<recipe>-<n>` names no recipe, and nothing is
    /// guessed from it.
    #[test]
    fn a_ticket_id_that_names_no_recipe_makes_nothing_due() {
        assert_eq!(recipe_of_ticket("fix-gate-1"), Some("fix-gate"));
        assert_eq!(recipe_of_ticket("fix-gate-1-2"), Some("fix-gate-1"));
        assert_eq!(recipe_of_ticket("housekeeping"), None);
        assert_eq!(recipe_of_ticket("-1"), None);
        assert_eq!(recipe_of_ticket("fix-gate-"), None);
    }

    /// Work about a branch belongs in that branch's working tree. A root
    /// whose checkout has since moved to another branch holds different
    /// code, and a start with nobody watching refuses exactly where dispatch
    /// does (§FS-005-dispatch.24, §FS-005-dispatch.3).
    #[test]
    fn a_checkout_standing_on_another_branch_is_not_run_in() {
        let tmp = tempfile::tempdir().unwrap();
        let checkout = tmp.path().join("widget");
        let root = checkout.join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
        fs::create_dir_all(checkout.join(".git")).unwrap();
        fs::write(checkout.join(".git/HEAD"), "ref: refs/heads/other\n").unwrap();

        let mut ledger = empty_ledger();
        ledger.entries.insert(
            "forge:widget/42".to_string(),
            Entry {
                project: "widget".to_string(),
                title: "t".to_string(),
                url: None,
                root: root.clone(),
                checkout: checkout.clone(),
                branch: Some("you/retry-window".to_string()),
                plan_id: "widget-42".to_string(),
                plan: root.join("widget-42.rhei.md"),
                dispatches: Vec::new(),
            },
        );
        let sweep = |ledger: &Ledger| {
            due_among(
                &work_config(),
                std::slice::from_ref(&group),
                &asking(&["fix-gate"]),
                ledger,
                Utc::now(),
            )
        };
        assert!(
            sweep(&ledger).is_empty(),
            "the tree is standing on another branch"
        );
        // Back on the branch the work is about, and it runs.
        fs::write(
            checkout.join(".git/HEAD"),
            "ref: refs/heads/you/retry-window\n",
        )
        .unwrap();
        assert_eq!(sweep(&ledger).len(), 1);
    }

    /// A branch nobody recorded refuses nothing: an entry written before the
    /// branch was kept, or work that matched no branch at all, is run where
    /// it always was (§FS-005-dispatch.24).
    #[test]
    fn a_branch_nobody_recorded_refuses_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let checkout = tmp.path().join("widget");
        let root = checkout.join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
        fs::create_dir_all(checkout.join(".git")).unwrap();
        fs::write(checkout.join(".git/HEAD"), "ref: refs/heads/other\n").unwrap();
        let mut ledger = empty_ledger();
        ledger.entries.insert(
            "forge:widget/42".to_string(),
            Entry {
                project: "widget".to_string(),
                title: "t".to_string(),
                url: None,
                root: root.clone(),
                checkout: checkout.clone(),
                branch: None,
                plan_id: "widget-42".to_string(),
                plan: root.join("widget-42.rhei.md"),
                dispatches: Vec::new(),
            },
        );
        assert_eq!(
            due_among(
                &work_config(),
                std::slice::from_ref(&group),
                &asking(&["fix-gate"]),
                &ledger,
                Utc::now(),
            )
            .len(),
            1
        );
    }

    /// The ledger says which recipe ephor wrote a ticket from, and that beats
    /// the id's own shape: a ticket dispatched from a recipe that does not
    /// autorun is not made due by an id that happens to look like one that
    /// does (§FS-005-dispatch.24).
    #[test]
    fn the_ledgers_recipe_answers_for_a_ticket_ephor_dispatched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
        let mut ledger = empty_ledger();
        ledger.entries.insert(
            "forge:widget/42".to_string(),
            Entry {
                project: "widget".to_string(),
                title: "t".to_string(),
                url: None,
                root: root.clone(),
                checkout: tmp.path().to_path_buf(),
                branch: None,
                plan_id: "widget-42".to_string(),
                plan: root.join("widget-42.rhei.md"),
                dispatches: vec![ledger::Dispatch {
                    ticket: "fix-gate-1".to_string(),
                    // Written from a recipe that asks nobody to run it, under
                    // an id another recipe's tickets would carry.
                    recipe: "review".to_string(),
                    at: Utc::now(),
                    plan: None,
                    snapshot: Default::default(),
                }],
            },
        );
        assert!(due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &ledger,
            Utc::now(),
        )
        .is_empty());
    }

    /// The back-off's own arithmetic: it doubles, and it stops growing
    /// (§FS-005-dispatch.24).
    #[test]
    fn the_back_off_doubles_and_is_capped() {
        let at = Utc::now();
        let rest = |failures| {
            ledger::Start {
                at,
                failures,
                says: String::new(),
            }
            .ready_at()
                - at
        };
        assert_eq!(rest(1), chrono::Duration::minutes(5));
        assert_eq!(rest(2), chrono::Duration::minutes(10));
        assert_eq!(rest(3), chrono::Duration::minutes(20));
        // However long it has been failing, it is always tried again.
        assert_eq!(rest(99), chrono::Duration::hours(2));
    }
}
