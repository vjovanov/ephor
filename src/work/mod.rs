//! Dispatch: what ephor watches, it can hand to an agent runtime
//! (§FS-005-dispatch).
//!
//! The feed says what is happening; this says what is being done about it.
//! An item plus a recipe becomes a ticket in a rhei plan, written into the
//! checkout the item's branch resolves to, carrying the dossier of everything
//! ephor already knew. Afterwards ephor keeps the ledger and reads the work's
//! state back out of the plan — never out of its own memory.

pub mod commands;
pub mod dossier;
pub mod ledger;
pub mod plan;
pub mod recipe;

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;
use serde_json::Value;

use crate::branches::{Placement, WorkspaceState};
use crate::error::{EphorError, Result};
use crate::feed::config::StatusConfig;
use crate::feed::model::Item;

use dossier::Subject;
use ledger::{Dispatch, Entry, Ledger, Snapshot};
use plan::{Plan, Ticket, WorkRoot};
use recipe::{ProjectWorkConfig, Recipe, WorkConfig};

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
    pub rhei: String,
    pub plan: PathBuf,
    /// The plan the ledger points at is gone — reported, never repaired.
    pub missing: bool,
    pub tickets: Vec<TicketStatus>,
    /// What has happened to the item since the last dispatch.
    pub changes: Vec<String>,
}

impl WorkStatus {
    pub fn stale(&self) -> bool {
        !self.changes.is_empty()
    }

    pub fn open_tickets(&self) -> usize {
        self.tickets.iter().filter(|t| !t.finished).count()
    }

    /// One line for a row that has room for one: what the work is doing, or
    /// what it decided, and whether the item has moved under it. `verdict` is
    /// how much of the verdict's own sentence fits where this is going.
    pub fn badge(&self, verdict_width: usize) -> String {
        if self.missing {
            return "⚠ plan missing".to_string();
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
            ledger: ledger::load()?,
        })
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
    pub fn offers(&self, item: &Item) -> Vec<Recipe> {
        recipe::applicable(&self.recipes(&item.project), item)
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

    fn placement(&mut self, project: &str) -> Option<&Placement> {
        self.placements
            .entry(project.to_string())
            .or_insert_with(|| Placement::load(&self.registry_doc, project))
            .as_ref()
    }

    /// The states YAML installed into a work root that has none: the project's
    /// own, the global one, or the machine ephor ships.
    fn states_yaml(&self, project: &str) -> Result<String> {
        let configured = self
            .projects
            .get(project)
            .and_then(|work| work.states.clone())
            .or_else(|| self.global.states.clone());
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

    fn root_template(&self, project: &str) -> String {
        self.projects
            .get(project)
            .and_then(|work| work.root.clone())
            .unwrap_or_else(|| self.global.root.clone())
    }

    /// Where an item's work belongs, refusing where it would not run
    /// (§FS-005-dispatch.6).
    fn site(&mut self, item: &Item, recipe: &Recipe) -> Result<Site> {
        let template = self.root_template(&item.project);
        let placement = self.placement(&item.project).ok_or_else(|| {
            EphorError::Command(format!(
                "{}: no root in the registry, so there is nowhere to put the work.",
                item.project
            ))
        })?;
        let checkout = placement.checkout(item);
        if let WorkspaceState::Missing(target) = &checkout.state {
            // Only for work that edits the change. A review or a reply runs in
            // the project's own checkout and fetches what it needs.
            if recipe.needs_checkout {
                return Err(EphorError::Command(format!(
                    "{}: branch {} is not checked out ({} is missing) — check it out first.",
                    item.project,
                    checkout.branch.as_deref().unwrap_or("?"),
                    target.display()
                )));
            }
        }
        let subject = Subject {
            item,
            checkout: &checkout,
            root: &placement.root,
        };
        let values = subject.placeholders();
        Ok(Site {
            dir: crate::paths::resolve_path(&dossier::render(&template, &values)),
            dossier: subject.dossier(),
            values,
            checkout: checkout.clone(),
        })
    }

    /// Hand an item to the runtime under one recipe. Opens the plan when the
    /// item has none, and appends to it when it has.
    pub fn dispatch(&mut self, item: &Item, recipe: &Recipe, dry_run: bool) -> Result<Outcome> {
        let site = self.site(item, recipe)?;
        let states = self.states_yaml(&item.project)?;
        let rhei = plan::rhei_id(&item.id);

        if dry_run {
            // Nothing is created, so the machine cannot be consulted; what a
            // dry run promises is where the ticket would go.
            let path = site.dir.join(format!("{rhei}.rhei.md"));
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
        let path = root.plan_path(&rhei);
        let brief = dossier::render(&recipe.brief, &site.values);
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
                    target: recipe.target.clone(),
                    model: recipe.model.clone(),
                    body: brief,
                };
                Plan::create(&path, &root.machine, &item.title, &site.dossier, &ticket).save()?;
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
                    target: recipe.target.clone(),
                    model: recipe.model.clone(),
                    body,
                });
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
            rhei: rhei.clone(),
            plan: path.clone(),
            dispatches: Vec::new(),
        });
        entry.title = item.title.clone();
        entry.url = item.url.clone();
        entry.root = root.dir.clone();
        entry.plan = path;
        entry.dispatches.push(Dispatch {
            ticket: ticket_id,
            recipe: recipe.id.clone(),
            at: Utc::now(),
            snapshot: Snapshot::of(item),
        });
        Ok(outcome)
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
        self.dispatch(item, &recipe, dry_run)
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
        let tickets = plan
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
                        verdict: ledger::verdict(&entry.root, &entry.rhei, &ticket.id),
                        id: ticket.id,
                        title: ticket.title,
                        state: ticket.state,
                    })
                    .collect()
            })
            .unwrap_or_default();
        WorkStatus {
            project: entry.project.clone(),
            root: entry.root.clone(),
            rhei: entry.rhei.clone(),
            plan: entry.plan.clone(),
            missing: plan.is_none(),
            tickets,
            changes: item
                .map(|item| entry.changes_since(item))
                .unwrap_or_default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        ledger::store(&self.ledger)
    }
}

fn clamp(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit - 1).collect::<String>() + "…"
}

/// Where one item's work goes, and what the ticket will say.
struct Site {
    dir: PathBuf,
    dossier: String,
    values: BTreeMap<&'static str, String>,
    #[allow(dead_code)] // kept for callers that report where work landed
    checkout: crate::branches::Checkout,
}
