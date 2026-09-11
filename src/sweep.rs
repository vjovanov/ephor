//! `ephor rebase` given a selector: the same per-checkout replay, swept over
//! every branch checkout nobody is holding (§FS-004-quick-actions.6.1).
//!
//! Nothing here is a second rebase. Per checkout it is exactly the move
//! [`crate::rebase`] makes for one, through the one replay in [`crate::git`],
//! and the only thing this module adds is which checkouts it is asked about
//! and what it refuses to ask about — the four questions, the review reading
//! behind one of them, and the restoring disposition, because nobody is
//! waiting on a replay a timer ran at three in the morning
//! (§FS-005-dispatch.12).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{json, Value};

use crate::branches::Placement;
use crate::cli::RebaseArgs;
use crate::error::{registry_error, EphorError, Result};
use crate::feed::config::StatusConfig;
use crate::feed::model::{Item, ItemKind};
use crate::git;
use crate::scope::{Act, Projects};
use crate::work;

/// The verb, as a refusal and a held gate name it.
const VERB: &str = "rebase";

/// A conflict is not a failure: it is where the work starts. The same code one
/// checkout's replay exits with, because forty of them become one answer and
/// the precedence is already settled (§FS-004-quick-actions.6.1).
const CONFLICT: u8 = 3;

/// The recipe a project names to have a conflict this sweep stopped on written
/// up as work, and the plan that work lives in — named after the sweep, since
/// no matter is its subject (§FS-005-dispatch.3).
const RECIPE: &str = "rebase-sweep";

/// What became of one checkout.
enum Outcome {
    /// Replayed onto the base; the per-checkout reading says by how much.
    Replayed(git::Rebase),
    /// Measured level and moved nowhere — reported, never silently skipped.
    Level(git::Rebase),
    /// Stopped in a conflict, and put back on the commit it started from.
    Conflicted(git::Rebase),
    /// Something a person has to clear: uncommitted work, no repository, or
    /// git refused.
    Refused(git::Rebase),
    /// One of the four questions answered, and this is its reason.
    PassedOver(String),
    /// The gate held, so nothing ran here. Not measured either: measuring the
    /// distance means fetching into the repository, and a run that reports
    /// writes nothing at all (§FS-011-command-line.10).
    Would,
}

impl Outcome {
    /// The one word a reading carries. `passed-over` is an outcome and not a
    /// silence: a sweep that says nothing about a checkout it decided not to
    /// touch cannot be told from one that never saw it.
    fn name(&self) -> &'static str {
        match self {
            Outcome::Replayed(_) => "replayed",
            Outcome::Level(_) => "level",
            Outcome::Conflicted(_) => "conflicted",
            Outcome::Refused(_) => "refused",
            Outcome::PassedOver(_) => "passed-over",
            Outcome::Would => "would-replay",
        }
    }

    /// The per-checkout replay this came out of, where any ran — the same
    /// `rebase --json` shape one checkout prints, nested unchanged.
    fn replay(&self) -> Option<&git::Rebase> {
        match self {
            Outcome::Replayed(rebase)
            | Outcome::Level(rebase)
            | Outcome::Conflicted(rebase)
            | Outcome::Refused(rebase) => Some(rebase),
            Outcome::PassedOver(_) | Outcome::Would => None,
        }
    }

    /// What the replay came to, in the words the one-checkout report uses.
    fn of(rebase: git::Rebase) -> Outcome {
        // The precedence one rebase already has: a conflict outranks a tree
        // nobody could touch, which outranks a replay, which outranks level.
        if !rebase.conflicted().is_empty() {
            return Outcome::Conflicted(rebase);
        }
        if rebase.repos.is_empty() || !rebase.stuck().is_empty() {
            return Outcome::Refused(rebase);
        }
        if rebase.rebased() > 0 {
            return Outcome::Replayed(rebase);
        }
        Outcome::Level(rebase)
    }
}

/// One row of the sweep: which checkout, and what became of it.
struct Swept {
    project: String,
    branch: String,
    checkout: PathBuf,
    outcome: Outcome,
    /// The ticket the conflict was written up as, where one was
    /// (§FS-004-quick-actions.6.1).
    ticket: Option<String>,
}

/// Whether a project's checkouts were reached at all, and why not where they
/// were not (§FS-004-quick-actions.6.1).
struct Reached {
    project: String,
    /// Why none of this project's checkouts was replayed. `None` is reached.
    refusal: Option<String>,
}

/// What the watch already knows about which of a project's branches somebody
/// is reviewing (§FS-004-quick-actions.6.1).
enum Reading {
    /// A current answer, from what the last refresh cached: every matter this
    /// project has, to be asked per branch.
    Read(Vec<Item>),
    /// No current answer could be had, and why. *Could not tell* is not *no*,
    /// so this stops the project and replays none of it.
    Unread(String),
}

/// The sweep (§FS-004-quick-actions.6.1). `projects` is what the selector
/// resolved to, and its being narrowed at all is what brought the command
/// here.
pub fn sweep(args: &RebaseArgs, projects: &Projects, act: Act) -> Result<ExitCode> {
    // What names one checkout or one matter has no meaning across a sweep, so
    // it is refused by name rather than quietly applied to one of forty
    // (§FS-011-command-line.9). Before anything is read, let alone written.
    refuse_what_names_one_checkout(args)?;

    let config = crate::feed::config::load_config()?;
    let registry = crate::feed::commands::load_registry_doc()?;
    let in_scope: Vec<String> = projects
        .over(config.projects.keys())?
        .into_iter()
        .cloned()
        .collect();
    // Sweeping writes into trees, so it reports and acts only under `--act`,
    // at every width it sweeps at: a verb whose pre-rule unit was one checkout
    // is above the gate the moment it sweeps (§FS-011-command-line.10).
    let gate = act.over_a_sweep(VERB);

    // Which trees a live run holds, taken over the whole site before any of
    // this is judged — the same reading the `work run --due` sweep makes, and
    // resolved the same way, so a symbolic link or a relative spelling does
    // not defeat the guard (§FS-005-dispatch.24). A guard that cannot be read
    // is not a guard that passes: `Dispatcher::load` failing here stops the
    // sweep rather than leaving it to write into a tree it cannot ask about.
    let mut dispatcher = work::Dispatcher::load(&config)?;
    let roots = dispatcher.work_roots();
    let busy = work::live_checkouts(&config.work, &roots, &dispatcher.ledger);

    let mut rows: Vec<Swept> = Vec::new();
    let mut reached: Vec<Reached> = Vec::new();
    for project in &in_scope {
        let Some(placement) = Placement::load(&registry, project) else {
            reached.push(Reached {
                project: project.clone(),
                refusal: Some("no root in the registry, so it has no checkouts".to_string()),
            });
            continue;
        };
        let Some(base) = placement.main_branch.clone() else {
            reached.push(Reached {
                project: project.clone(),
                refusal: Some(
                    "it names no main branch, so there is nothing to replay its branches onto"
                        .to_string(),
                ),
            });
            continue;
        };
        // One reading per project, freshened once where the cache has aged —
        // the way a status reading is, and never once per branch, which is
        // what the ticket's author ruled out (§FS-004-quick-actions.6.1).
        let matters = match reading(&config, project, config.defaults.ttl_seconds) {
            Reading::Read(items) => items,
            Reading::Unread(why) => {
                reached.push(Reached {
                    project: project.clone(),
                    refusal: Some(why),
                });
                continue;
            }
        };
        reached.push(Reached {
            project: project.clone(),
            refusal: None,
        });

        for branch in &placement.branches {
            // The project's main-branch checkout is not one of these: that
            // directory belongs to `ephor update`, and where both could claim
            // it `update` wins, because branch drift is the whole subject here
            // (§FS-004-quick-actions.6.1).
            if placement.is_main_branch(&branch.branch) {
                continue;
            }
            let Some(checkout) = placement.workspace_for(&branch.branch) else {
                continue;
            };
            if !checkout.is_dir() {
                continue;
            }
            let outcome = match passed_over(
                &config, &placement, project, branch, &checkout, &busy, &matters,
            ) {
                Some(why) => Outcome::PassedOver(why),
                None if gate.holds() => Outcome::Would,
                // The same replay the reader's key runs, asked for the
                // disposition nobody being here calls for: a tree put back on
                // the commit it was on, and the conflict reported instead
                // (§FS-005-dispatch.12).
                None => Outcome::of(git::rebase(
                    &placement.forest(&checkout),
                    &git::Onto::Base(base.clone()),
                    git::Stopped::Restore,
                )),
            };
            let mut row = Swept {
                project: project.clone(),
                branch: branch.branch.clone(),
                checkout,
                outcome,
                ticket: None,
            };
            // The conflict is in the report either way; the ticket is the
            // extra, and a ticket that could not be opened does not swallow
            // the report (§REQ-001-boundary.1).
            if let Outcome::Conflicted(rebase) = &row.outcome {
                match write_up(&mut dispatcher, &config, &placement, &row, rebase) {
                    Ok(ticket) => row.ticket = ticket,
                    Err(err) => eprintln!("note: {}: {err}", row.branch),
                }
            }
            rows.push(row);
        }
    }

    let report = report(projects.said(), &rows, &reached, &gate);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view(projects.said(), &rows, &reached, &gate, &report))
                .unwrap_or_else(|_| "null".to_string())
        );
    } else {
        print!("{report}");
    }
    // The same contract `--report` has for one checkout: one file, holding
    // what this run came to.
    if let Some(path) = crate::given::value(&args.report, "REPORT")? {
        crate::rebase::write_report(&path, &report)?;
    }
    Ok(exit_code(&rows, &reached))
}

/// Everything that names one checkout or one matter, refused beside a selector
/// (§FS-011-command-line.9).
///
/// Every one of these command lines exits 2 today for the selector alone, so
/// no invocation that works now gains a refusal. `--report` is the exception
/// and needs none: it writes this run's own report, one file, which is the
/// same contract it has for one checkout.
fn refuse_what_names_one_checkout(args: &RebaseArgs) -> Result<()> {
    let mut refused: Vec<String> = Vec::new();
    for (flag, name) in [
        (&args.project, "PROJECT"),
        (&args.checkout, "CHECKOUT"),
        (&args.item, "ITEM"),
        (&args.hand, "HAND"),
        (&args.onto, "ONTO"),
    ] {
        if let Some(input) = crate::given::input(flag, name) {
            refused.push(input);
        }
    }
    // The two that are not values. `--upstream` has an environment spelling
    // like the rest; `--dispatch` is a flag a reader types and nothing else.
    if args.upstream || std::env::var("UPSTREAM").is_ok_and(|said| !said.trim().is_empty()) {
        refused.push("--upstream".to_string());
    }
    if args.dispatch {
        refused.push("--dispatch".to_string());
    }
    if refused.is_empty() {
        return Ok(());
    }
    Err(registry_error(format!(
        "{VERB} does not take {} beside a scope selector. Each of them names one checkout \
         or one matter, and a sweep has neither: it replays every branch checkout of every \
         project the selector names. `--project` in particular keeps the meaning it has — \
         which project the one checkout belongs to — and does not select. Drop the selector \
         to rebase one checkout, or drop {} to sweep.",
        refused.join(", "),
        refused.join(", ")
    )))
}

/// Why this checkout is not touched, where something says so
/// (§FS-004-quick-actions.6.1). Asked before any git runs, and each answer is
/// an outcome with a reason rather than a silence.
fn passed_over(
    config: &StatusConfig,
    placement: &Placement,
    project: &str,
    branch: &crate::branches::BranchInfo,
    checkout: &std::path::Path,
    busy: &BTreeMap<PathBuf, PathBuf>,
    matters: &[Item],
) -> Option<String> {
    // A live run holds the tree. This is the one that matters: an agent
    // mid-edit in a working tree rebased under it loses work nothing recovers,
    // and the invariant is read over the tree rather than over the kind of
    // caller — a writer that is not a run is held by it all the same, and is
    // never forced (§FS-005-dispatch.24).
    if let Some(root) = work::holding(busy, checkout) {
        return Some(format!(
            "a live run holds this checkout, from {}",
            root.display()
        ));
    }
    // A branch under review is somebody's to move.
    if let Some(said) = under_review(matters, &branch.branch) {
        return Some(said);
    }
    // And a conflict an earlier sweep already wrote up: an hourly timer that
    // retried the same conflict would be a loop nobody asked for.
    if let Some(ticket) = open_conflict_ticket(config, placement, project, &branch.branch) {
        return Some(format!(
            "a conflict ticket from an earlier sweep is still open about this checkout \
             ({ticket})"
        ));
    }
    None
}

/// Whether an open pull request that is not a draft is on this branch
/// (§FS-004-quick-actions.6.1).
///
/// A pull request whose draft state the forge did not report **protects** the
/// branch: only one *known* to be out of draft passes it over, for the same
/// reason an unreadable project stops the whole project — *could not tell* is
/// not *no*. A draft does not protect, because a draft is not yet anybody's to
/// review.
fn under_review(matters: &[Item], branch: &str) -> Option<String> {
    matters
        .iter()
        .filter(|item| item.kind == ItemKind::Pr && !item.is_finished())
        .filter(|item| item.raw.get("branch").and_then(Value::as_str) == Some(branch))
        .find(|item| item.raw.get("draft").and_then(Value::as_bool) != Some(true))
        .map(|item| {
            format!(
                "an open pull request that is not a draft is on this branch — {} ({})",
                item.title, item.id
            )
        })
}

/// What every ticket about a conflict on this branch is named after, in the
/// plan the sweep's work lives in.
///
/// Named after the checkout rather than only counted, so the sweep an hour
/// later asks whether its own earlier ticket is still open by looking an id up
/// rather than by matching prose (§FS-004-quick-actions.6.1).
fn ticket_stem(branch: &str) -> String {
    let mut slug = String::with_capacity(branch.len());
    for ch in branch.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    format!("{RECIPE}-{}", slug.trim_matches('-'))
}

/// The three tiers a work root resolves through — the site's, the project's
/// organization's, and its own (§FS-005-dispatch.6.1). Read in one place, so
/// the directory the sweep looks in and the one it writes to are one answer.
fn tiers<'a>(
    config: &'a StatusConfig,
    placement: &Placement,
    project: &str,
) -> (
    Option<&'a crate::work::recipe::OrganizationWorkConfig>,
    Option<&'a crate::work::recipe::ProjectWorkConfig>,
) {
    (
        placement
            .organization
            .as_ref()
            .and_then(|placed_in| config.organizations.get(&placed_in.id))
            .map(|organization| &organization.work),
        config.projects.get(project).map(|project| &project.work),
    )
}

/// Where this project's own work goes — resolved and nothing created, because
/// this is asked by a run that may be doing nothing but reporting.
fn work_root(config: &StatusConfig, placement: &Placement, project: &str) -> Option<PathBuf> {
    let (organization, own) = tiers(config, placement, project);
    work::work_root_in(
        &config.work,
        organization,
        own,
        project,
        placement.organization.as_ref(),
        &placement.root,
        &placement.root,
    )
    .ok()
}

/// A conflict ticket from an earlier sweep that is still open about this
/// checkout (§FS-004-quick-actions.6.1).
///
/// Any ticket named after this checkout that the machine does not call over:
/// the same conflict written up twice is the loop an hourly timer would
/// otherwise run forever.
fn open_conflict_ticket(
    config: &StatusConfig,
    placement: &Placement,
    project: &str,
    branch: &str,
) -> Option<String> {
    let root = work_root(config, placement, project)?;
    let path = work::runtime::plan::plan_path_in(&root, RECIPE);
    let plan = work::runtime::plan::Plan::read(&path).ok().flatten()?;
    // Finality is the machine's word. A root that declares none cannot say
    // whether anything is over, and that is not a licence to retry the
    // conflict, so every ticket there counts as standing
    // (§FS-005-dispatch.15).
    let machine = work::runtime::plan::WorkRoot::open(&root).ok().flatten();
    let stem = format!("{}-", ticket_stem(branch));
    plan.tickets()
        .into_iter()
        .filter(|ticket| ticket.id.starts_with(&stem))
        .find(|ticket| {
            !ticket.cancelled()
                && !machine.as_ref().is_some_and(|machine| {
                    ticket
                        .state
                        .as_deref()
                        .is_some_and(|state| machine.is_final(state))
                })
        })
        .map(|ticket| ticket.id)
}

/// The conflict as a ticket, where the project's work configuration names a
/// recipe for it (§FS-004-quick-actions.6.1).
///
/// One plan per sweep, in the project's own work root, with one ticket per
/// conflicted checkout appended to it by every later sweep rather than a rival
/// copy of the same work somewhere else (§FS-005-dispatch.3,
/// §FS-014-work-root-scopes.2). The brief carries what the replay reached —
/// the checkout, the branch, the ref, the repositories and their unmerged
/// paths, both sides by ref — plus the one sentence that is the whole
/// difference from §FS-005-dispatch.12's handover: the tree was restored, and
/// the conflict is not standing in it.
fn write_up(
    dispatcher: &mut work::Dispatcher,
    config: &StatusConfig,
    placement: &Placement,
    row: &Swept,
    rebase: &git::Rebase,
) -> Result<Option<String>> {
    let Some(recipe) = dispatcher
        .recipes(&row.project)
        .into_iter()
        .find(|recipe| recipe.id == RECIPE)
    else {
        return Ok(None);
    };
    let (organization, own) = tiers(config, placement, &row.project);
    let store = work::ensure_store(
        &config.work,
        organization,
        own,
        &row.project,
        placement.organization.as_ref(),
        &placement.root,
        &placement.root,
    )?;
    let root = work::runtime::plan::WorkRoot::in_force(&store.dir)?;
    if !root.declares(&recipe.state) {
        return Err(EphorError::Command(format!(
            "recipe '{RECIPE}' starts in state '{}', which the machine '{}' in {} does not \
             declare (it has: {}).",
            recipe.state,
            root.machine,
            root.dir.display(),
            root.state_names().join(", ")
        )));
    }
    let path = root.plan_path(RECIPE);
    let existing = work::runtime::plan::Plan::read(&path)?;
    // Named after the checkout and then counted, so a checkout that conflicts
    // again after its earlier ticket was finished gets a ticket of its own
    // rather than a second one wearing the first's id.
    let stem = ticket_stem(&row.branch);
    let id = existing
        .as_ref()
        .map(|plan| plan.next_ticket_id(&stem))
        .unwrap_or_else(|| format!("{stem}-1"));
    let body = format!(
        "{}\n\n{}\n**The tree was restored.** The conflict is not standing in \
         `{}` — the sweep put the repository back on the commit it was on, because nobody \
         was waiting on that replay (§FS-005-dispatch.12). To see it again, replay it: \
         `ephor rebase --project {} --checkout {}`.\n",
        recipe.brief,
        rebase.report(),
        row.checkout.display(),
        row.project,
        row.checkout.display()
    );
    let ticket = work::runtime::plan::Ticket {
        id: id.clone(),
        title: format!("{} — {}", recipe.description, row.branch),
        state: recipe.state.clone(),
        // Ordered after nothing. Two checkouts that conflicted are two
        // independent questions about two trees, and a chain would stop the
        // second until somebody finished the first (§FS-005-dispatch.5).
        prior: None,
        target: recipe.target.clone(),
        model: recipe.model.clone(),
        body,
    };
    match existing {
        Some(mut plan) => {
            plan.append(&ticket);
            plan.save()?;
        }
        None => {
            let plan = work::runtime::plan::Plan::create(
                &path,
                &root.machine,
                "the rebase sweep's conflicts",
                &format!(
                    "The checkouts an unattended `ephor rebase` sweep stopped on in \
                     {}, one ticket each (§FS-004-quick-actions.6.1).",
                    row.project
                ),
                &ticket,
            );
            plan.save()?;
        }
    }
    Ok(Some(format!("{}#{id}", path.display())))
}

/// What the watch already knows about this project, freshened once
/// (§FS-004-quick-actions.6.1).
///
/// Where no current reading can be had — no cached feed, or a slot flagged
/// stale or failed — none of the project's checkouts is replayed at all.
/// *Could not tell* is not *no*, and the branch this question protects is
/// exactly the one nobody is present to protect by hand. One project that
/// cannot be read stops that project and no other, because the drift this
/// exists to correct goes on everywhere else.
fn reading(config: &StatusConfig, project: &str, ttl: u64) -> Reading {
    let feed = match crate::feed::commands::freshened(config, project, ttl) {
        Ok(feed) => feed,
        Err(err) => {
            return Reading::Unread(format!(
                "no reading of what is under review here could be had — {err}"
            ))
        }
    };
    if feed.fetched_at.is_none() {
        return Reading::Unread(
            "no refresh has ever produced a reading of what is under review here".to_string(),
        );
    }
    let lost: Vec<&str> = feed
        .providers
        .iter()
        .filter(|(_, slot)| slot.stale || !slot.ok)
        .map(|(name, _)| name.as_str())
        .collect();
    if !lost.is_empty() {
        return Reading::Unread(format!(
            "the reading from {} is stale or failed, so what is under review here cannot be \
             told from what is not",
            lost.join(", ")
        ));
    }
    Reading::Read(feed.items().collect())
}

/// Forty checkouts become one exit code, and conflict wins — the precedence
/// one rebase already has (§FS-004-quick-actions.6.1). *Replayed*, *level* and
/// *passed over* are all good ends.
fn exit_code(rows: &[Swept], reached: &[Reached]) -> ExitCode {
    if rows
        .iter()
        .any(|row| matches!(row.outcome, Outcome::Conflicted(_)))
    {
        return ExitCode::from(CONFLICT);
    }
    let refused = rows
        .iter()
        .any(|row| matches!(row.outcome, Outcome::Refused(_)));
    // A project that could not be read is the timer's only way to say it is
    // not doing its job, which is why it is not a quiet pass-over.
    let unreached = reached.iter().any(|project| project.refusal.is_some());
    if refused || unreached {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// Whether any conflict this stopped on was put back where it was. `None`
/// where nothing conflicted (§FS-005-dispatch.12).
fn restored(rows: &[Swept]) -> Option<bool> {
    let stopped: Vec<&git::Rebase> = rows
        .iter()
        .filter_map(|row| match &row.outcome {
            Outcome::Conflicted(rebase) => Some(rebase),
            _ => None,
        })
        .collect();
    if stopped.is_empty() {
        return None;
    }
    Some(stopped.iter().all(|rebase| rebase.restored() == Some(true)))
}

/// How many checkouts came to each end.
fn counted(rows: &[Swept]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        *counts.entry(row.outcome.name()).or_insert(0) += 1;
    }
    counts
}

/// One line for the checkouts, counted — the summary a log keeps.
fn summary(rows: &[Swept], reached: &[Reached]) -> String {
    let counts = counted(rows);
    let mut parts: Vec<String> = Vec::new();
    for (name, count) in &counts {
        parts.push(format!("{count} {name}"));
    }
    let unreached = reached
        .iter()
        .filter(|project| project.refusal.is_some())
        .count();
    if unreached > 0 {
        parts.push(format!("{unreached} project(s) not reached"));
    }
    if parts.is_empty() {
        return "no branch checkout to sweep".to_string();
    }
    parts.join(", ")
}

/// The whole sweep as markdown: one line per checkout, whichever end it came
/// to, and the conflicts in full beneath.
fn report(said: &str, rows: &[Swept], reached: &[Reached], gate: &crate::scope::Gate) -> String {
    let mut out = format!("# rebase sweep over {said}\n\n");
    for project in reached {
        out.push_str(&format!("## {}\n\n", project.project));
        if let Some(why) = &project.refusal {
            out.push_str(&format!(
                "**Not reached.** No checkout of this project was replayed: {why}.\n\n"
            ));
            continue;
        }
        out.push_str("The main branch's checkout is `ephor update`'s, and is not swept.\n\n");
        let mine: Vec<&Swept> = rows
            .iter()
            .filter(|row| row.project == project.project)
            .collect();
        if mine.is_empty() {
            out.push_str("No branch checkout is on disk here.\n\n");
            continue;
        }
        for row in mine {
            out.push_str(&format!("- {}\n", says(row)));
        }
        out.push('\n');
    }
    let stopped: Vec<&Swept> = rows
        .iter()
        .filter(|row| matches!(row.outcome, Outcome::Conflicted(_)))
        .collect();
    if !stopped.is_empty() {
        out.push_str("## The checkouts it stopped on\n\n");
        for row in stopped {
            if let Some(rebase) = row.outcome.replay() {
                out.push_str(&rebase.report());
            }
            if let Some(ticket) = &row.ticket {
                out.push_str(&format!("Written up as {ticket}\n\n"));
            }
        }
    }
    out.push_str(&format!("{}\n", summary(rows, reached)));
    if let Some(held) = gate.says() {
        out.push_str(&format!("{held}\n"));
    }
    out
}

/// One checkout's line: the branch, what became of it, and where.
fn says(row: &Swept) -> String {
    let what = match &row.outcome {
        Outcome::Replayed(rebase) => format!("replayed — {}", rebase.summary()),
        Outcome::Level(rebase) => format!("level — {}", rebase.summary()),
        Outcome::Conflicted(rebase) => {
            format!("conflicted and put back — {}", rebase.summary())
        }
        Outcome::Refused(rebase) => format!("not replayed — {}", rebase.summary()),
        Outcome::PassedOver(why) => format!("passed over: {why}"),
        Outcome::Would => "would be replayed".to_string(),
    };
    format!("{} — {what} ({})", row.branch, row.checkout.display())
}

/// The same answer for a program: forty checkouts as one reading, and every
/// fact the prose gave as a field (§REQ-002-parity.3).
fn view(
    said: &str,
    rows: &[Swept],
    reached: &[Reached],
    gate: &crate::scope::Gate,
    report: &str,
) -> Value {
    let counts = counted(rows);
    let mut view = json!({
        "scope": said,
        "summary": summary(rows, reached),
        "report": report,
        "replayed": counts.get("replayed").copied().unwrap_or(0),
        "level": counts.get("level").copied().unwrap_or(0),
        "passed_over": counts.get("passed-over").copied().unwrap_or(0),
        "conflicted": counts.get("conflicted").copied().unwrap_or(0),
        "refused": counts.get("refused").copied().unwrap_or(0),
        "projects": reached.iter().map(|project| {
            let mut row = json!({
                "project": project.project,
                "reached": project.refusal.is_none(),
            });
            if let (Some(row), Some(why)) = (row.as_object_mut(), &project.refusal) {
                row.insert("says".to_string(), json!(why));
            }
            row
        }).collect::<Vec<_>>(),
        "checkouts": rows.iter().map(|row| {
            let mut checkout = json!({
                "project": row.project,
                "branch": row.branch,
                "checkout": row.checkout,
                "outcome": row.outcome.name(),
                "says": says(row),
            });
            let object = checkout.as_object_mut().expect("a row is an object");
            // The per-checkout reading, nested exactly as one checkout's
            // `rebase --json` prints it (§REQ-002-parity.3).
            if let Some(rebase) = row.outcome.replay() {
                object.insert("rebase".to_string(), rebase.view());
            }
            if let Some(ticket) = &row.ticket {
                object.insert("ticket".to_string(), json!(ticket));
            }
            serde_json::Value::Object(object.clone())
        }).collect::<Vec<_>>(),
    });
    let object = view.as_object_mut().expect("the reading is an object");
    // Absent where nothing stopped: "no conflict" and "a conflict still
    // standing" are different answers (§FS-005-dispatch.12).
    if let Some(restored) = restored(rows) {
        object.insert("restored".to_string(), json!(restored));
    }
    if let Some(held) = gate.says() {
        object.insert("gated".to_string(), json!(true));
        object.insert("says".to_string(), json!(held));
    }
    view
}
