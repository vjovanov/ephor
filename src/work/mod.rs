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

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;
use serde_json::Value;

use crate::branches::{Placement, WorkspaceState};
use crate::capabilities::{CapabilitySet, Rung};
use crate::error::{EphorError, Result};
use crate::feed::config::StatusConfig;
use crate::feed::model::Item;

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
    /// The runtime has stopped on this ticket and a person has to answer it
    /// (§FS-005-dispatch.9).
    pub waiting: bool,
    /// Who claimed the ticket, where anyone has — a claimed ticket is not a
    /// run's to advance (§FS-005-dispatch.15).
    pub assignee: Option<String>,
    /// The execution line the ticket carries, where it carries one
    /// (§FS-005-dispatch.14).
    pub pinned: Option<plan::Pin>,
    /// What the review left behind, where the work reached one.
    pub verdict: Option<String>,
}

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
        if let WorkspaceState::Missing(target) = &checkout.state {
            // Only for work that edits the change. A review or a reply runs in
            // the project's own checkout and fetches what it needs.
            if recipe.needs_checkout {
                return Err(EphorError::Command(format!(
                    "{}: branch {} is not checked out ({} is missing). Make it with:\n  \
                     ephor checkout --item {}",
                    item.project,
                    checkout.branch.as_deref().unwrap_or("?"),
                    target.display(),
                    item.id
                )));
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

        if dry_run {
            // Nothing is created, so the machine cannot be consulted; what a
            // dry run promises is where the ticket would go.
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
        if let Some(existing) = WorkRoot::open(&site.dir)? {
            if !existing.declares(&recipe.state) {
                return Err(undeclared(&existing));
            }
        }
        let root = WorkRoot::ensure(&site.dir, &states)?;
        if !root.declares(&recipe.state) {
            return Err(undeclared(&root));
        }
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
            plan_id: plan_id.clone(),
            plan: path.clone(),
            dispatches: Vec::new(),
        });
        entry.title = item.title.clone();
        entry.url = item.url.clone();
        entry.root = root.dir.clone();
        entry.checkout = site.checkout.workspace.clone();
        entry.plan = path;
        entry.dispatches.push(Dispatch {
            ticket: ticket_id,
            recipe: recipe.id.clone(),
            at: Utc::now(),
            snapshot: Snapshot::of(item),
        });
        Ok(outcome)
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

    /// What an item's work is doing, read from the plan.
    pub fn status(&self, item: &Item) -> Option<WorkStatus> {
        let entry = self.ledger.entries.get(&item.id)?;
        Some(self.status_of(entry, Some(item)))
    }

    pub fn status_of(&self, entry: &Entry, item: Option<&Item>) -> WorkStatus {
        let recipes: BTreeMap<&str, &str> = entry
            .dispatches
            .iter()
            .map(|dispatch| (dispatch.ticket.as_str(), dispatch.recipe.as_str()))
            .collect();
        let root = WorkRoot::open(&entry.root).ok().flatten();
        let plan = Plan::read(&entry.plan).ok().flatten();
        let tickets: Vec<TicketStatus> = plan
            .as_ref()
            .map(|plan| {
                plan.tickets()
                    .into_iter()
                    .map(|ticket| TicketStatus {
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
                        waiting: ticket
                            .state
                            .as_deref()
                            .map(|state| {
                                root.as_ref()
                                    .map(|root| root.is_gating(state))
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false),
                        verdict: runtime::results::verdict(&entry.root, &entry.plan_id, &ticket.id),
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
            runtime::advance_command(
                &self.global,
                &ticket.id,
                ticket.state.as_deref().unwrap_or("?"),
            )
        });
        WorkStatus {
            project: entry.project.clone(),
            root: entry.root.clone(),
            plan_id: entry.plan_id.clone(),
            checkout: entry.checkout(),
            plan: entry.plan.clone(),
            missing: plan.is_none(),
            tickets,
            advance,
            changes: item
                .map(|item| entry.changes_since(item))
                .unwrap_or_default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        ledger::store(&self.ledger)
    }
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

/// Make the work root for a workspace, so the first dispatch into that branch
/// has somewhere to land and what is under way is visible from the moment the
/// tree exists (§FS-006-project-interface.7).
///
/// The store ignores itself, which is what keeps this from being an artifact
/// required of the project (§REQ-001-boundary.3): what it holds is ephor's own
/// planning state that happens to live in a checkout.
pub fn ensure_store(
    global: &WorkConfig,
    project: Option<&ProjectWorkConfig>,
    project_id: &str,
    workspace: &std::path::Path,
    root: &std::path::Path,
) -> Result<PathBuf> {
    let values = BTreeMap::from([
        ("workspace", workspace.to_string_lossy().into_owned()),
        ("root", root.to_string_lossy().into_owned()),
        ("project", project_id.to_string()),
    ]);
    let dir =
        crate::paths::resolve_path(&dossier::render(&root_template(global, project), &values));
    let states = states_yaml(global, project)?;
    WorkRoot::ensure(&dir, &states)?;
    Ok(dir)
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
