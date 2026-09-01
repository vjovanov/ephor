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
mod ranking;
pub mod recipe;
pub mod runtime;
pub mod workflow;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

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
    /// A live run has this ticket in hand right now, read from the run's own
    /// record of itself (§FS-005-dispatch.15.2, §FS-005-dispatch.23). Open and
    /// being worked on are different facts and the row says which.
    pub running: bool,
    /// A run is live on this ticket's root and busy elsewhere: it will get its
    /// turn without anyone doing anything (§FS-005-dispatch.15).
    pub queued: bool,
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
    /// Minutes of silence worth noting on the live run holding this work — the
    /// badge the board carries, on the row the reader is already looking at
    /// (§FS-005-dispatch.23). None where no run is live here, and on one
    /// writing normally.
    pub quiet: Option<u64>,
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
            let mut said = format!(
                "{} · {}",
                ticket.recipe,
                ticket.state.as_deref().unwrap_or("?")
            );
            // Open and being worked on right now are different facts, and the
            // row says which (§FS-005-dispatch.23) — in the board's own words,
            // because this is that reading narrowed to one matter
            // (§FS-005-dispatch.15).
            let (tone, marker) = match (ticket.running, ticket.queued) {
                (true, _) => {
                    // A live run that has gone silent wears the badge it wears
                    // on the board: a long tool call is legitimately quiet, so
                    // it is a badge and never a verdict (§FS-005-dispatch.15).
                    if let Some(minutes) = self.quiet {
                        said = format!("{said} · quiet {minutes}m");
                    }
                    (Tone::Running, "▶")
                }
                (false, true) => {
                    said = format!("{said} · queued");
                    (Tone::Going, "⚙")
                }
                (false, false) => (Tone::Going, "⚙"),
            };
            lines.push(WorkLine::of(tone, marker, said, ticket));
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
    /// A live run has it in hand right now (§FS-005-dispatch.23).
    Running,
    /// Open, and nothing is working it this moment.
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
        // `judge` is the machine that answers for the plan the ticket is in,
        // never assumed to be the root's (§FS-005-dispatch.28).
        let mut consider = |judge: Option<&WorkRoot>,
                            plan_id: &str,
                            path: &std::path::Path,
                            ticket: &plan::PlanTicket| {
            let Some(machine) = judge else {
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
                        // The matter's own plan is one the root holds
                        // directly: the root's machine answers for it.
                        consider(
                            self.machine.as_ref(),
                            &self.entry.plan_id,
                            &self.entry.plan,
                            &ticket,
                        );
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
            // A plan a workflow laid down is a store of its own, and its
            // tasks mean what the machine in force there says they mean
            // (§FS-005-dispatch.28, §FS-006-project-interface.7) — the board
            // reads it the same way, and one row must not disagree with the
            // screen it was narrowed from (§AR-009-surfaces.1). Declaring
            // none, it is the root's machine the runtime resolves it against;
            // with one that will not read, nothing there is judged, rather
            // than judged by a machine that answers for other work.
            let own = plan::own_machine(&laid.path);
            let judge: Option<&WorkRoot> = match &own {
                Ok(Some(store)) => Some(store),
                Ok(None) => self.machine.as_ref(),
                Err(_) => None,
            };
            for ticket in plan.tickets() {
                consider(judge, &laid.plan_id, &laid.path, &ticket);
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
    /// The person's own entries, and the ones they wrote for one project:
    /// two of the three homes a workflow entry may live in
    /// (§FS-005-dispatch.19). Kept because a sweep has to find the entry
    /// that asked to run itself without a menu being open
    /// (§FS-005-dispatch.28).
    actions: Vec<ActionConfig>,
    project_actions: BTreeMap<String, Vec<ActionConfig>>,
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
            actions: config.actions.clone(),
            project_actions: config
                .projects
                .iter()
                .map(|(id, project)| (id.clone(), project.actions.clone()))
                .collect(),
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
    ///
    /// `branch` is the template carried by the entry that would be dispatched,
    /// where it carries one: the root is then inside the workspace that
    /// template names, exactly as [`Dispatcher::site_for`] resolves it. A
    /// caller with no entry in hand passes `None` and gets the matter's own
    /// (§FS-005-dispatch.25).
    pub fn work_root_of(&mut self, item: &Item, branch: Option<&str>) -> Option<PathBuf> {
        if let Some(entry) = self.ledger.entries.get(&item.id) {
            return Some(entry.root.clone());
        }
        let template = self.root_template(&item.project);
        let placement = self.placement(&item.project)?.clone();
        let checkout = crate::branches::placed_through(&placement, item, branch);
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
        self.site_for(item, recipe.needs_checkout, recipe.branch.as_deref())
    }

    /// The same, for an entry that is not a recipe: a workflow says what it
    /// needs on disk through its own `requires_checkout`
    /// (§FS-005-dispatch.19).
    ///
    /// `branch` is the entry's template for the branch its work belongs on,
    /// where it carries one (§FS-005-dispatch.25). It applies only to a matter
    /// with no branch of its own, and saying it means the work needs the
    /// checkout — so it decides where the work goes without resolving anything
    /// the matter already answered.
    fn site_for(
        &mut self,
        item: &Item,
        needs_checkout: bool,
        branch: Option<&str>,
    ) -> Result<Site> {
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
        // Where the matter's code lives right now, main branch included, and
        // the forest root where nothing resolves (§FS-005-dispatch.13,
        // §AR-004-forest.1) — which is what lets work about a conversation
        // run without the checkout-able rung (§FS-006-project-interface.10).
        // This is the read resolution: work that only reads the change (no
        // branch template, no needs_checkout) runs here unchanged, whether
        // or not the matter merely resembles the main branch by word match.
        let mut checkout = placement.checkout(item);
        // Saying which branch the work belongs on says that it needs the
        // checkout: the template is about where the change will be edited
        // (§FS-005-dispatch.25).
        let needs_checkout = needs_checkout || branch.is_some();
        // The matter's own branch always wins — but the project's main
        // branch is never a matter's own (§FS-005-dispatch.25). Only work
        // that edits the change asks this: a template supplies the branch a
        // matter has none of, and never displaces the one the forge recorded
        // or the registry matched; with no template, editing work has
        // nowhere to go. This is [`crate::branches::placed_through`] with
        // the refusal kept: a surface asking where the work would go falls
        // back to the matter's own placement, and the dispatch that would
        // write there refuses instead.
        let mut mint = None;
        if needs_checkout && placement.own_branch(item).is_none() {
            match branch {
                Some(template) => {
                    checkout = crate::branches::minted(&placement, item, template)
                        .map_err(EphorError::Command)?;
                    if let WorkspaceState::Missing(target) = &checkout.state {
                        mint = Some(target.clone());
                    }
                }
                None => checkout = placement.own_checkout(item),
            }
        }
        // Only for work that edits the change. A review or a reply runs in
        // the project's own checkout and fetches what it needs.
        if needs_checkout {
            let wanted = checkout.branch.as_deref().unwrap_or("?");
            match &checkout.state {
                // A workspace this dispatch is about to make is not a missing
                // one: it is named, it is this matter's, and making it is the
                // move that comes after the last refusal (§FS-005-dispatch.25).
                WorkspaceState::Missing(_) if mint.is_some() => {}
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
                // The matter is on no branch and no entry said which one its
                // work belongs on, so there is no workspace for work that
                // edits the change — and the project root of a project whose
                // checkouts are one per branch holds no change to edit. This
                // used to be written there anyway; it is refused now, which is
                // what the menu has always done (§FS-005-dispatch.25). A
                // matter matched only to the project's main branch is on no
                // branch by the same rule, and the refusal names what it
                // declined rather than calling the matter unmatched.
                WorkspaceState::Unmatched => {
                    let clause = match placement
                        .matched(item)
                        .filter(|matched| placement.is_main_branch(&matched.branch))
                    {
                        Some(matched) => format!(
                            "matched {}, this project's main branch — the trunk every workspace \
                             is grown from, not a branch of its own",
                            matched.branch
                        ),
                        None => "is on no branch".to_string(),
                    };
                    return Err(EphorError::Command(format!(
                        "{}: {} {clause}, and this work edits the change. Give the entry a \
                         'branch' template naming the branch it belongs on, so dispatch makes \
                         the workspace:\n  \"branch\": \"fix/issue-{{number}}\"\n\
                         or hand over work that reads the change instead of editing it.",
                        item.project, item.id,
                    )));
                }
                WorkspaceState::Ready => {}
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
            mint,
        })
    }

    /// Make the workspace a `branch` template named, where it named one that is
    /// not on disk (§FS-005-dispatch.25).
    ///
    /// Called after every refusal and before the first write, so a refusal
    /// still leaves nothing behind — and never on a dry run, which is the
    /// caller's to decide because it is the caller that knows it is one.
    /// It is `ephor checkout`'s own operation ([`crate::checkout::make`]), so
    /// the workspace a dispatch makes and the workspace a reader's key makes
    /// are the same thing (§FS-004-quick-actions.7).
    fn mint(&mut self, item: &Item, site: &Site) -> Result<()> {
        if site.mint.is_none() {
            return Ok(());
        }
        let branch = site.checkout.branch.clone().unwrap_or_default();
        let placement = self
            .placement(&item.project)
            .cloned()
            .ok_or_else(|| EphorError::Command(format!("{} cannot be placed", item.project)))?;
        let (made, source) = crate::checkout::make(&placement, &item.project, &branch, None)?;
        // A repository the checkout refused is the checkout's own refusal, in
        // the checkout's own words, and nothing is dispatched behind it.
        if let Some(why) = made.refusal(&source) {
            return Err(EphorError::Command(why));
        }
        if let Some(note) = made.store.as_ref().and_then(|store| store.note.clone()) {
            self.note_once(&note);
        }
        Ok(())
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
            // A dry run makes nothing, so it says what it would have made:
            // the branch, and the workspace the plan path below is inside
            // (§FS-005-dispatch.25).
            if let Some(target) = &site.mint {
                let note = format!(
                    "{} is not checked out — the dispatch would make {} first.",
                    site.checkout.branch.as_deref().unwrap_or("?"),
                    target.display()
                );
                self.note_once(&note);
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

        // The machine answers before the workspace is made, not after. Where
        // the work root is already there it is the machine that root declares;
        // where a `branch` template would mint the workspace there is no root
        // to open — the directory does not exist yet — so what is vetted is
        // the machine `ensure` installs below. Either way the refusal lands on
        // the same side of the mint as the hand and the inputs
        // (§FS-005-dispatch.25).
        match WorkRoot::open(&site.dir)? {
            Some(existing) => vet(&existing)?,
            None => vet(&WorkRoot::proposed(&site.dir, &states)?)?,
        }

        // The workspace a `branch` template named, made now: after the hand,
        // the machine and the opening move have all had their chance to refuse,
        // and before the work root below is the first thing written
        // (§FS-005-dispatch.25).
        self.mint(item, &site)?;

        let root = WorkRoot::ensure(&site.dir, &states)?;
        // Read back rather than assumed: a workspace the mint just made can
        // come with a machine of the runtime's own, which `ensure` leaves
        // standing (§FS-006-project-interface.7), and that is the one the work
        // will actually run under.
        //
        // Which makes this the one refusal that outlives the mint: the machine
        // vetted above was the one ephor would have installed, and the runner
        // installed another inside the workspace this dispatch has just made.
        // The workspace is named rather than left for the reader to find
        // (§FS-005-dispatch.25) — and nothing further is made behind it,
        // because the next dispatch opens this root and refuses on the same
        // machine before minting anything.
        vet(&root).map_err(|why| match &site.mint {
            Some(target) => EphorError::Command(format!(
                "{why} The workspace {} was made before that machine could be read, and is \
                 still there; dispatching here again refuses on it without making anything \
                 further.",
                target.display()
            )),
            None => why,
        })?;
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

    /// Every workflow entry this project has, in the menu's own provenance
    /// order (§FS-005-dispatch.19): what travels with the workflow, then the
    /// project's own, then the person's, an entry a narrower home repeats
    /// replacing the one it displaces.
    ///
    /// The same assembly the menu makes, and deliberately the same one: a
    /// sweep that ranked the three homes differently from the screen would
    /// lay down an entry the reader never saw offered (§AR-009-surfaces.1).
    /// What it leaves out is the menu's gating — which is a question about a
    /// keystroke, and a sweep answers it by laying the entry down and
    /// reporting the refusal (§FS-005-dispatch.28).
    pub fn workflow_actions(&mut self, project: &str) -> Vec<ActionConfig> {
        let beside = self.workflow_entries(project);
        let from = |want: runtime::workflow::Source| -> Vec<ActionConfig> {
            beside
                .iter()
                .filter(|(source, _)| *source == want)
                .map(|(_, entry)| entry.clone())
                .collect()
        };
        let offers: Vec<ActionConfig> = self
            .placement(project)
            .and_then(Placement::manifest)
            .map(|manifest| {
                manifest
                    .offers
                    .iter()
                    .map(crate::manifest::Offer::action)
                    .collect()
            })
            .unwrap_or_default();
        let configured: Vec<ActionConfig> = self
            .actions
            .iter()
            .chain(
                self.project_actions
                    .get(project)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            )
            .cloned()
            .collect();
        let mut entries = crate::api::offers::merge(vec![
            from(runtime::workflow::Source::Runtime),
            [offers, from(runtime::workflow::Source::Project)].concat(),
            [configured, from(runtime::workflow::Source::Person)].concat(),
        ]);
        entries.retain(|entry| entry.workflow.is_some());
        entries
    }

    /// The workflow entries offered on one matter, in that same order
    /// (§FS-005-dispatch.19) — the selector, in the language every other
    /// entry is selected by.
    ///
    /// Finished work is never among them, exactly as it is never among a
    /// recipe's offers ([`recipe::Recipe::matches`]) and never on the menu
    /// (§FS-005-dispatch.6): a merged pull request with a red gate stays in
    /// the feed as news, and handing news to an agent is asking it to invent
    /// something to do.
    pub fn workflow_offers(&mut self, item: &Item) -> Vec<ActionConfig> {
        if item.is_finished() {
            return Vec::new();
        }
        let facts = self.facts(item);
        let mut entries = self.workflow_actions(&item.project);
        entries.retain(|entry| entry.matches(item, &facts));
        entries
    }

    /// The ids of this project's workflow entries that asked to run
    /// themselves (§FS-005-dispatch.28). What the due sweep matches a laid
    /// plan's laying entry against: silence is the key, so an id that is not
    /// here is nobody's to start.
    pub fn autorun_workflows(&mut self, project: &str) -> BTreeSet<String> {
        self.workflow_actions(project)
            .into_iter()
            .filter(|entry| entry.workflow.as_ref().is_some_and(|ask| ask.autorun))
            .map(|entry| entry.id)
            .collect()
    }

    /// Everything one workflow entry would write, with nothing written yet
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
        let site = self.site_for(item, entry.requires_checkout, entry.branch.as_deref())?;
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
        if let Some(why) = laying.refusal() {
            return Err(EphorError::Command(why));
        }
        // A run asked what it would do makes nothing at all. Where the
        // workspace itself is not there yet, that has to include the work root
        // and the files the runtime would be shown: they have nowhere to go
        // until the workspace exists, and a dry run that made the directory
        // tree to report on it would be the thing this refuses to do
        // (§FS-005-dispatch.25).
        if dry_run {
            if let Some(target) = &laying.site.mint {
                return Ok(Laid {
                    outcome: Outcome::Laid {
                        plan: laying.output.clone(),
                        plan_id: laying.plan_id.clone(),
                        workflow: laying.workflow.id.clone(),
                        entry: laying.entry.clone(),
                    },
                    report: format!(
                        "would check out {} at {} first, and lay {} down inside it",
                        laying.site.checkout.branch.as_deref().unwrap_or("?"),
                        target.display(),
                        laying.workflow.id,
                    ),
                });
            }
        }
        let states = self.states_yaml(&item.project)?;
        let values_json = serde_json::to_string_pretty(&laying.answered.values)
            .unwrap_or_else(|_| "{}".to_string());
        if dry_run {
            // The runtime validates the resolved inputs even when it is only
            // reporting what it would render. Its values and carried files
            // are staged in a private temporary directory so asking that
            // question cannot make any part of the destination
            // (§FS-005-dispatch.19).
            let destination = carried(&laying.site.dir, &laying.plan_id);
            let staged = StagedWorkflow::new(
                &laying.answered.values,
                &destination,
                &format!("# {}\n\n{}", item.title, laying.site.dossier),
                &identifiers(&laying.site.metadata),
            )?;
            let report = runtime::workflow::lay(
                &self.global,
                &laying.site.checkout.workspace,
                &laying.workflow,
                &staged.values,
                &laying.output,
                true,
            )
            .map(|report| staged.restore_destination_paths(&report, &destination))
            .map_err(|err| staged.restore_destination_error(err, &destination))?;
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
        // Everything above could still refuse; nothing above has written
        // anything. The workspace goes in here, and the work root is the first
        // thing inside it (§FS-005-dispatch.25).
        self.mint(item, &laying.site)?;
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
        let values = put(VALUES, &values_json)?;
        let report = runtime::workflow::lay(
            &self.global,
            &laying.site.checkout.workspace,
            &laying.workflow,
            &values,
            &laying.output,
            false,
        )?;
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
        ledger_entry.branch = laying.site.checkout.branch.clone();
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
            // And so it mints nothing: what is asked for on the spot is asked
            // about the matter as it stands (§FS-005-dispatch.25).
            branch: None,
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

    /// The same reading, with the per-root probes shared across a run of
    /// items (§FS-005-dispatch.15.1): a caller building every matter's rows
    /// keeps one [`RootLook`] across them, so a root two matters share is
    /// probed once rather than twice.
    pub fn status_seen(&self, item: &Item, look: &mut RootLook) -> Option<WorkStatus> {
        let entry = self.ledger.entries.get(&item.id)?;
        Some(status_of_entry_seen(&self.global, entry, Some(item), look))
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
        self.due_in(&roots, now)
    }

    /// Due roots from one discovered-root snapshot. `start_due` also derives
    /// its live counts from this same snapshot, so discovery cannot disagree
    /// with capacity accounting halfway through a sweep
    /// (§FS-005-dispatch.24).
    fn due_in(&mut self, roots: &[runtime::watch::RootPlans], now: DateTime<Utc>) -> Vec<Due> {
        let projects: BTreeSet<String> = roots
            .iter()
            .flat_map(|group| group.plans.iter().map(|plan| plan.project.clone()))
            .collect();
        // Which recipes asked to run themselves, per project. Resolved once
        // per project: the tables behind it are the same for every root of
        // one of them.
        let autoruns: BTreeMap<String, BTreeSet<String>> = projects
            .iter()
            .map(|project| {
                let asked = self
                    .recipes(project)
                    .into_iter()
                    .filter(|recipe| recipe.autorun)
                    .map(|recipe| recipe.id)
                    .collect();
                (project.clone(), asked)
            })
            .collect();
        // And which workflow entries did (§FS-005-dispatch.28). Only for a
        // project whose record holds a laying: one of the three homes is
        // beside the workflow itself, so resolving this asks the runtime what
        // it offers — a summons per project, and a sweep on a site that has
        // never laid a workflow has no laid plan for an entry to answer for.
        let laying: BTreeSet<&str> = self
            .ledger
            .entries
            .values()
            .filter(|entry| entry.dispatches.iter().any(Dispatch::is_workflow))
            .map(|entry| entry.project.as_str())
            .collect();
        let asking: Vec<String> = projects
            .iter()
            .filter(|project| laying.contains(project.as_str()))
            .cloned()
            .collect();
        let workflow_autoruns: BTreeMap<String, BTreeSet<String>> = asking
            .into_iter()
            .map(|project| {
                let asked = self.autorun_workflows(&project);
                (project, asked)
            })
            .collect();
        let mut due = due_among(
            &self.global,
            roots,
            &autoruns,
            &workflow_autoruns,
            &self.ledger,
            now,
        );
        if let Some(path) = self.global.ranking.as_deref() {
            let reading = ranking::read(&crate::paths::resolve_path(path));
            due = rank_due(due, &reading.order);
        }
        due
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
    workflow_autoruns: &BTreeMap<String, BTreeSet<String>>,
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
    // And which entry laid each plan a workflow wrote (§FS-005-dispatch.28).
    // The record is the only thing that knows: the plan is the runtime's and
    // says nothing about who asked for it
    // (§FS-005-dispatch.4). Keyed by the plan itself, because the ledger
    // names the directory the runtime was pointed at and what landed inside
    // it is the runtime's answer (§AR-007-runtime.1).
    let laid_by: BTreeMap<PathBuf, String> = ledger
        .entries
        .values()
        .flat_map(|entry| {
            entry
                .dispatches
                .iter()
                .filter(|dispatch| dispatch.is_workflow())
                .filter_map(move |dispatch| {
                    let plan_id = dispatch.plan.as_deref()?;
                    let found = runtime::workflow::laid(&entry.root.join(plan_id))?;
                    Some((canonical(&found.path), dispatch.recipe.clone()))
                })
        })
        .collect();
    let nothing: BTreeSet<String> = BTreeSet::new();
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
        let mut items: Vec<String> = Vec::new();
        for plan_ref in &group.plans {
            // Which entry laid this plan down, where a workflow did — and so
            // which set of "asked to run itself" answers for what is inside
            // it (§FS-005-dispatch.28). A plan the record knows nothing
            // about asked for nothing, and is nobody's to start.
            let laid = laid_by.get(&canonical(&plan_ref.path)).map(String::as_str);
            // For a store of its own, the entry is the only way that can be
            // said: its tasks are the runtime's own, and the `<recipe>-<n>`
            // shape below is a fact about the tickets ephor wrote into the
            // root's own plan, so no spelling in there names a recipe
            // (§FS-005-dispatch.28).
            if laid.is_none() && runtime::plan::own_store(&plan_ref.path).is_some() {
                continue;
            }
            let asked = match laid {
                Some(_) => workflow_autoruns.get(&plan_ref.project),
                None => autoruns.get(&plan_ref.project),
            }
            .unwrap_or(&nothing);
            if asked.is_empty() {
                continue;
            }
            // A plan that is a store of its own runs under the machine it
            // declares there, not the root's: a task's state means whatever
            // the machine in force for its own store says it means
            // (§FS-006-project-interface.7) — which is a plan a workflow laid
            // down, and the same question the board asks of the same plan
            // (§AR-009-surfaces.1). Declaring none, it is the root's the
            // runtime resolves it against, as it is for the plans the root
            // holds directly. With one that will not read, nothing in that
            // plan can be judged runnable, and the honest move is to start
            // nothing rather than to fall back on a machine that answers for
            // other work (§FS-005-dispatch.15).
            let own = match runtime::plan::own_machine(&plan_ref.path) {
                Ok(store) => store,
                Err(_) => continue,
            };
            let judge = own.as_ref().unwrap_or(&machine);
            let Ok(Some(plan)) = Plan::read(&plan_ref.path) else {
                continue;
            };
            for ticket in plan.tickets() {
                let state = ticket.state.as_deref();
                // Over, waiting on a person, or somebody's to move: none
                // of them is work a run would advance
                // (§FS-005-dispatch.24, §FS-005-dispatch.15).
                if state.map(|state| judge.is_final(state)).unwrap_or(true)
                    || state.map(|state| judge.is_gating(state)).unwrap_or(false)
                    || ticket.assignee.is_some()
                {
                    continue;
                }
                // What asked for this work. For a plan a workflow laid down
                // it is the entry that laid it, and it answers for every
                // task in the plan — the tasks are the workflow's, not
                // ephor's, and no id of theirs names a recipe
                // (§FS-005-dispatch.28). Otherwise the ledger says which
                // recipe ephor wrote a ticket from; for one a hand appended,
                // the id says it, because ids are `<recipe>-<n>` by
                // construction. Either way it is a fact about the ticket
                // (§FS-005-dispatch.24).
                let asked_for = laid.or_else(|| {
                    dispatched
                        .get(&(group.root.clone(), ticket.id.clone()))
                        .map(String::as_str)
                        .or_else(|| recipe_of_ticket(&ticket.id))
                });
                if !asked_for.is_some_and(|what| asked.contains(what)) {
                    continue;
                }
                if !plans.contains(&plan_ref.plan_id) {
                    plans.push(plan_ref.plan_id.clone());
                }
                if let Some(item) = &plan_ref.item {
                    if !items.contains(item) {
                        items.push(item.clone());
                    }
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
            projects: group
                .plans
                .iter()
                .map(|plan| plan.project.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            root: group.root.clone(),
            checkout,
            plans,
            tickets,
            item: items.first().cloned().or_else(|| {
                known.and_then(|entry| {
                    ledger
                        .entries
                        .iter()
                        .find(|(_, candidate)| candidate.root == entry.root)
                        .map(|(id, _)| id.clone())
                })
            }),
            items,
        });
    }
    due
}

/// Ranked roots first, then every root the file did not distinguish in its
/// existing deterministic order. A root with no matter id is necessarily in
/// the latter group (§FS-005-dispatch.24, §FS-005-dispatch.26).
fn rank_due(due: Vec<Due>, ranked_ids: &[String]) -> Vec<Due> {
    let ranks: BTreeMap<&str, usize> = ranked_ids
        .iter()
        .enumerate()
        .rev()
        .map(|(rank, id)| (id.as_str(), rank))
        .collect();
    let mut due = due;
    due.sort_by(|left, right| {
        let rank = |root: &Due| {
            root.items
                .iter()
                .filter_map(|item| ranks.get(item.as_str()).copied())
                .min()
        };
        rank(left)
            .unwrap_or(usize::MAX)
            .cmp(&rank(right).unwrap_or(usize::MAX))
            .then_with(|| left.root.cmp(&right.root))
    });
    due
}

/// Serialize autorun capacity decisions across every ephor process using this
/// site's state directory. The operating system releases the lock if a sweep
/// exits, so no reservation can outlive the process that made it.
fn autorun_lock() -> Result<fs::File> {
    let state = crate::paths::state_dir();
    fs::create_dir_all(&state).map_err(|err| {
        EphorError::Command(format!(
            "Cannot create the autorun state directory {}: {err}",
            state.display()
        ))
    })?;
    let path = state.join("work.autorun.lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|err| {
            EphorError::Command(format!(
                "Cannot open the autorun capacity lock {}: {err}",
                path.display()
            ))
        })?;
    lock.lock().map_err(|err| {
        EphorError::Command(format!(
            "Cannot take the autorun capacity lock {}: {err}",
            path.display()
        ))
    })?;
    Ok(lock)
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
        max_concurrent: Option<usize>,
    ) -> Result<Vec<Launched>> {
        // The runtime is a rung like any other capacity, and with nothing
        // bound there is no run to start (§AR-005-capabilities.2).
        if runtime::refusal(&self.global).is_some() {
            return Ok(Vec::new());
        }
        let detaches = runtime::can_detach(&self.global);
        // Everything after this point is one site-wide reservation: reload
        // the record another sweep may just have changed, take one fresh root
        // and liveness snapshot, decide capacity, and keep the lock until the
        // launches and their failed-start back-off are committed.
        let _autorun = autorun_lock()?;
        self.ledger = ledger::load()?;
        let roots = self.work_roots();
        let live = LiveRuns::read(&self.global, &roots);
        let global_limit = max_concurrent.or(self.global.max_concurrent);
        let project_limits = self
            .projects
            .iter()
            .filter_map(|(project, work)| work.max_concurrent.map(|limit| (project.clone(), limit)))
            .collect();
        let mut capacity = Capacity::new(global_limit, project_limits, live);
        let launched = self
            .due_in(&roots, now)
            .into_iter()
            .filter(|root| {
                projects.is_empty()
                    || root
                        .projects
                        .iter()
                        .any(|project| projects.contains(project))
            })
            .map(|root| {
                if let Some(why) = capacity.refusal(&root.projects) {
                    return Launched::passed_over(&root, why);
                }
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
                        // The descriptor can already say the run is over, and
                        // a run that died just after publishing it has already
                        // released the lock. Neither occupies a slot needed by
                        // the next ranked root (§FS-005-dispatch.24).
                        let remains_live =
                            !started.finished && runtime::watch::live(&self.global, &root.root);
                        capacity.started(&root.projects, remains_live);
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
            .collect();
        ledger::store(&self.ledger)?;
        Ok(launched)
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
    /// Every project with a plan on this root. Usually one; all of them count
    /// when a deliberately shared root is live.
    projects: Vec<String>,
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
    /// Every matter whose due tickets contribute to this root. Ranking uses
    /// the best configured position among them; plans with no due ticket do
    /// not get to rank the root.
    items: Vec<String>,
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
    /// Why an otherwise eligible root was omitted solely for lack of sweep
    /// capacity. This is a successful, non-launch outcome, not a failure.
    pub passed_over: Option<String>,
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
            passed_over: None,
        }
    }

    fn refused(due: &Due, why: String) -> Launched {
        Launched {
            failed: Some(why),
            ..Launched::of(due)
        }
    }

    fn passed_over(due: &Due, why: String) -> Launched {
        Launched {
            passed_over: Some(why),
            ..Launched::of(due)
        }
    }

    /// The one line this is worth: what started, or what stopped it. The
    /// same sentence wherever it is said, so a command and a screen never
    /// phrase one situation two ways (§AR-009-surfaces.1).
    pub fn says(&self) -> String {
        match (&self.failed, &self.passed_over, &self.id, self.finished) {
            (Some(why), ..) => format!("⚠ no run started on {}: {why}", self.root.display()),
            (None, Some(why), ..) => {
                format!("↷ {} passed over: {why}", self.root.display())
            }
            (None, None, Some(id), false) => format!("▶ run {id} started"),
            (None, None, Some(id), true) => format!("✓ run {id} finished already"),
            (None, None, None, false) => "▶ run started".to_string(),
            (None, None, None, true) => "✓ the run finished already".to_string(),
        }
    }

    /// What a reading calls this (§REQ-002-parity.3).
    pub fn outcome(&self) -> &'static str {
        match (&self.failed, &self.passed_over, self.finished) {
            (Some(_), ..) => "failed",
            (None, Some(_), _) => "passed-over",
            (None, None, true) => "done",
            (None, None, false) => "started",
        }
    }

    pub fn reason(&self) -> Option<&str> {
        self.passed_over.as_deref()
    }
}

/// Live roots at the beginning of a sweep, counted once from the same root
/// snapshot that supplies due candidates (§FS-005-dispatch.24).
#[derive(Debug, Default)]
struct LiveRuns {
    global: usize,
    projects: BTreeMap<String, usize>,
}

impl LiveRuns {
    fn read(global: &WorkConfig, roots: &[runtime::watch::RootPlans]) -> LiveRuns {
        let mut live = LiveRuns::default();
        for root in roots {
            if !runtime::watch::live(global, &root.root) {
                continue;
            }
            live.global += 1;
            let projects: BTreeSet<&str> = root
                .plans
                .iter()
                .map(|plan| plan.project.as_str())
                .collect();
            for project in projects {
                *live.projects.entry(project.to_string()).or_default() += 1;
            }
        }
        live
    }
}

/// Remaining aggregate and per-project slots. Missing ceilings stay missing:
/// omission is unlimited, while `Some(0)` is deliberately no capacity.
#[derive(Debug)]
struct Capacity {
    global_limit: Option<usize>,
    global_live: usize,
    project_limits: BTreeMap<String, usize>,
    project_live: BTreeMap<String, usize>,
}

impl Capacity {
    fn new(
        global_limit: Option<usize>,
        project_limits: BTreeMap<String, usize>,
        live: LiveRuns,
    ) -> Capacity {
        Capacity {
            global_limit,
            global_live: live.global,
            project_limits,
            project_live: live.projects,
        }
    }

    fn refusal(&self, projects: &[String]) -> Option<String> {
        if self
            .global_limit
            .is_some_and(|limit| self.global_live >= limit)
        {
            let limit = self.global_limit.expect("checked");
            return Some(format!(
                "global work.max_concurrent {limit} is full ({} live run(s))",
                self.global_live
            ));
        }
        for project in projects {
            if self
                .project_limits
                .get(project)
                .is_some_and(|limit| self.project_live.get(project).copied().unwrap_or(0) >= *limit)
            {
                let limit = self.project_limits[project];
                let count = self.project_live.get(project).copied().unwrap_or(0);
                return Some(format!(
                    "projects.{project}.work.max_concurrent {limit} is full ({count} live run(s))"
                ));
            }
        }
        None
    }

    fn started(&mut self, projects: &[String], remains_live: bool) {
        if !remains_live {
            return;
        }
        self.global_live += 1;
        for project in projects {
            *self.project_live.entry(project.clone()).or_default() += 1;
        }
    }
}

/// How a work root is named in the ledger's record of starts. The path as
/// written, so the key travels with the same spelling the entry carries.
fn root_key(root: &std::path::Path) -> String {
    root.to_string_lossy().into_owned()
}

/// One path, resolved to the file it names — the spelling a caller happened
/// to use is not the identity of a directory or a plan. A path that cannot
/// be resolved is its own answer: something that is not there yet is still
/// only equal to itself.
fn canonical(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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
    status_of_entry_seen(global, entry, item, &mut RootLook::default())
}

/// The reads one work root answers the same way for every ticket in it —
/// whether a run is live on it, what that run has in hand, and how long it has
/// been silent — taken once and reused (§FS-005-dispatch.15.1).
///
/// A caller reading one matter can let this be built and dropped around the
/// call; a caller reading every matter on the feed keeps one across them, so a
/// root shared by two matters is probed once rather than twice.
#[derive(Default)]
pub struct RootLook {
    seen: BTreeMap<PathBuf, RootRun>,
}

/// One root's run, as the probe found it.
struct RootRun {
    live: bool,
    /// What the run says it has in hand. None where nothing is live: a slot
    /// nobody released under a free lock is a dead run's leavings, which is
    /// the board's business and not a running mark (§FS-005-dispatch.15).
    witness: Option<runtime::watch::Witness>,
    quiet: Option<u64>,
}

impl RootLook {
    fn of(&mut self, global: &WorkConfig, root: &std::path::Path) -> &RootRun {
        self.seen.entry(root.to_path_buf()).or_insert_with(|| {
            // Liveness and the quiet clock in one probe, the same one the
            // board makes (§FS-005-dispatch.15).
            let pulse = runtime::watch::pulse(global, root);
            RootRun {
                witness: pulse.live.then(|| runtime::watch::witness(global, root)),
                quiet: pulse.quiet,
                live: pulse.live,
            }
        })
    }
}

/// An entry's work as it stands, with the per-root reads hoisted out
/// (§FS-005-dispatch.15.1).
pub fn status_of_entry_seen(
    global: &WorkConfig,
    entry: &Entry,
    item: Option<&Item>,
    look: &mut RootLook,
) -> WorkStatus {
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
    // What a run on this root is doing, read once for every ticket asked
    // about (§FS-005-dispatch.15.1).
    let run = look.of(global, &entry.root);
    let (live, quiet) = (run.live, run.quiet);
    let lock_born = live
        .then(|| runtime::watch::lock_born(&entry.root))
        .flatten();
    let holds = |ticket: &plan::PlanTicket| {
        run.witness.as_ref().is_some_and(|witness| {
            witness.holds(
                &entry.root,
                lock_born,
                &entry.plan_id,
                &ticket.id,
                ticket.state.as_deref(),
            )
        })
    };
    let tickets: Vec<TicketStatus> = plan
        .as_ref()
        .map(|plan| {
            plan.tickets()
                .into_iter()
                .map(|ticket| TicketStatus {
                    // A live run holds it, or a live run on this root will
                    // reach it: open and being worked on are different facts
                    // (§FS-005-dispatch.23).
                    running: holds(&ticket),
                    queued: live && !holds(&ticket),
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
        quiet,
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
    let canon = canonical;
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
        self.refusal().is_none()
    }

    /// Why it cannot, where it cannot (§FS-005-dispatch.19). Asked before
    /// anything is written — and asked on its own by a sweep's dry run,
    /// which writes nothing at all and must still refuse everything the real
    /// laying would (§FS-005-dispatch.28).
    pub fn refusal(&self) -> Option<String> {
        if !self.answered.refusals.is_empty() {
            return Some(self.answered.refusals.join("; "));
        }
        if !self.answered.missing.is_empty() {
            return Some(format!(
                "'{}' cannot be laid down: nothing answers {}. Answer them with --set \
                 <input>=<value>, or say them in the entry.",
                self.entry,
                self.answered.missing.join(", ")
            ));
        }
        None
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

/// The files a runtime needs to validate a dry run. They live in a fresh
/// temporary directory, not beside the plan, and the directory is removed
/// when the runtime has answered — including when it refused.
struct StagedWorkflow {
    dir: PathBuf,
    dossier: PathBuf,
    item: PathBuf,
    values: PathBuf,
}

impl StagedWorkflow {
    fn new(
        values: &serde_json::Map<String, Value>,
        destination: &std::path::Path,
        dossier: &str,
        item: &str,
    ) -> Result<StagedWorkflow> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let base = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..64 {
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = base.join(format!("ephor-workflow-values-{pid}-{serial}"));
            match fs::create_dir(&dir) {
                Ok(()) => {
                    let staged = StagedWorkflow {
                        dossier: dir.join(DOSSIER),
                        item: dir.join(ITEM),
                        values: dir.join(VALUES),
                        dir,
                    };
                    let destination_dossier = for_shell(&destination.join(DOSSIER));
                    let destination_item = for_shell(&destination.join(ITEM));
                    let staged_dossier = for_shell(&staged.dossier);
                    let staged_item = for_shell(&staged.item);
                    let mut runtime_values = values.clone();
                    for value in runtime_values.values_mut() {
                        replace_path(value, &destination_dossier, &staged_dossier);
                        replace_path(value, &destination_item, &staged_item);
                    }
                    let values = serde_json::to_string_pretty(&runtime_values)
                        .unwrap_or_else(|_| "{}".to_string());
                    for (path, content) in [
                        (&staged.dossier, dossier),
                        (&staged.item, item),
                        (&staged.values, values.as_str()),
                    ] {
                        fs::write(path, content).map_err(|err| {
                            EphorError::Command(format!(
                                "Cannot write temporary workflow file {}: {err}",
                                path.display()
                            ))
                        })?;
                    }
                    return Ok(staged);
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(EphorError::Command(format!(
                        "Cannot make a temporary place for workflow values in {}: {err}",
                        base.display()
                    )))
                }
            }
        }
        Err(EphorError::Command(format!(
            "Cannot make a temporary place for workflow values in {}",
            base.display()
        )))
    }

    /// Runtime validation reads temporary files, but its account is public.
    /// Restore only the three exact paths ephor substituted before returning
    /// that account, so reports and refusals keep the destination semantics.
    fn restore_destination_paths(&self, text: &str, destination: &std::path::Path) -> String {
        [
            (&self.values, destination.join(VALUES)),
            (&self.dossier, destination.join(DOSSIER)),
            (&self.item, destination.join(ITEM)),
        ]
        .into_iter()
        .fold(text.to_string(), |text, (staged, destination)| {
            text.replace(&for_shell(staged), &for_shell(&destination))
        })
    }

    fn restore_destination_error(
        &self,
        error: EphorError,
        destination: &std::path::Path,
    ) -> EphorError {
        match error {
            EphorError::Registry(message) => {
                EphorError::Registry(self.restore_destination_paths(&message, destination))
            }
            EphorError::Command(message) => {
                EphorError::Command(self.restore_destination_paths(&message, destination))
            }
        }
    }
}

impl Drop for StagedWorkflow {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Replace only the values sent to a dry-run runtime. Public answers retain
/// the eventual destination paths, while every supported carried-file input
/// the runtime validates points at an existing staged equivalent.
fn replace_path(value: &mut Value, destination: &str, staged: &str) {
    match value {
        Value::String(text) if text.contains(destination) => {
            *text = text.replace(destination, staged);
        }
        Value::Array(items) => {
            for item in items {
                replace_path(item, destination, staged);
            }
        }
        Value::Object(fields) => {
            for field in fields.values_mut() {
                replace_path(field, destination, staged);
            }
        }
        _ => {}
    }
}

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
    /// The branch workspace this dispatch has to make before it writes
    /// anything, where a `branch` template named one that is not on disk
    /// (§FS-005-dispatch.25). None for everything else, including a workspace
    /// the template named that is already there.
    mint: Option<PathBuf>,
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
