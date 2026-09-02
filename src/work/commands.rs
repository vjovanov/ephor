//! `ephor work` — hand items to the runtime, and see what came of it
//! (§FS-005-dispatch).
//!
//! The interactive path dispatches one item the reader is looking at. This is
//! the other one: every item in a project at once, and the sweep that reopens
//! work whose item has moved (§FS-005-dispatch.5) — which is what a timer
//! runs.

use std::collections::BTreeMap;
use std::process::ExitCode;

use chrono::Utc;

use crate::cli::{WorkArgs, WorkCommand};
use crate::error::{registry_error, EphorError, Result};
use crate::feed::cache;
use crate::feed::config::{load_config, StatusConfig};
use crate::feed::model::{Item, ItemKind};
use crate::feed::render::Style;
use crate::seams::summons::{self, Outcome as SummonsOutcome};
use crate::work::ledger::Entry;
use crate::work::runtime;

use crate::work::{Dispatcher, Outcome};

pub fn work(args: &WorkArgs) -> Result<ExitCode> {
    let config = load_config()?;
    match args
        .command
        .as_ref()
        .unwrap_or(&WorkCommand::List(crate::cli::WorkListArgs::default()))
    {
        WorkCommand::List(list) => list_work(&config, list),
        WorkCommand::Offers(offers) => work_offers(&config, offers),
        WorkCommand::Dispatch(dispatch) => dispatch_work(&config, dispatch),
        WorkCommand::Ask(ask) => ask_work(&config, ask),
        WorkCommand::Sync(sync) => sync_work(&config, sync),
        WorkCommand::Cancel(cancel) => cancel_work(&config, cancel),
        WorkCommand::Run(run) => run_work(&config, run),
        WorkCommand::Workflows(workflows) => list_workflows(&config, workflows),
        WorkCommand::Lay(lay) => lay_workflow(&config, lay),
        WorkCommand::Forget(forget) => forget_work(&config, forget),
        WorkCommand::States(states) => work_states(&config, states),
    }
}

/// `ephor work states` — the machine ephor's tickets actually run under
/// (§FS-005-dispatch.11).
///
/// Under `--json` it says *which* machine as well as what is in it, which is a
/// fact the prose form never carried: a program reading a YAML document on
/// standard output could not tell the one ephor ships from the one this site
/// configured, and those are different answers to "what states can a ticket be
/// in". The document rides in a field rather than being the whole output
/// because it is not JSON and never will be — the machine is the runtime's
/// language, not ephor's (§REQ-001-boundary.1).
fn work_states(config: &StatusConfig, args: &crate::cli::WorkStatesArgs) -> Result<ExitCode> {
    // The one configured for these projects when there is one, so what is
    // printed is what tickets would actually run under.
    let configured = config
        .work
        .states
        .as_ref()
        .or_else(|| {
            config
                .projects
                .values()
                .find_map(|project| project.work.states.as_ref())
        })
        .map(|path| crate::paths::resolve_path(path));
    let (source, path, states) = match configured {
        Some(path) => {
            let text = std::fs::read_to_string(&path).map_err(|err| {
                EphorError::Command(format!(
                    "Cannot read the configured state machine {}: {err}",
                    path.display()
                ))
            })?;
            ("configured", Some(path.display().to_string()), text)
        }
        None => (
            "shipped",
            None,
            crate::work::runtime::plan::SHIPPED_STATES.to_string(),
        ),
    };
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "source": source,
                "path": path,
                "states": states,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    print!("{states}");
    Ok(ExitCode::SUCCESS)
}

/// One row with every absent fact left out rather than spelled `null`.
///
/// The published shapes say what these fields *are* — a state is a string, a
/// verdict is a sentence — and a fact that is not there is absent, which is
/// what the API's own views say with `skip_serializing_if` (§REQ-002-parity.4).
/// `serde_json`'s macro turns a `None` into a `null` typed as neither, so a row
/// built by hand has to say it by hand.
fn stated(mut row: serde_json::Value) -> serde_json::Value {
    if let Some(object) = row.as_object_mut() {
        object.retain(|_, value| !value.is_null());
    }
    row
}

/// Every item in the cached feed of the selected projects, with the project's
/// name. Dispatch reads the cache rather than fetching: what it hands over is
/// what the reader was last shown, and a refresh in the middle of a sweep
/// would dispatch work about items nobody has seen.
fn selected_items(config: &StatusConfig, projects: &[String]) -> Result<Vec<Item>> {
    let selected: Vec<&String> = if projects.is_empty() {
        config.projects.keys().collect()
    } else {
        for project in projects {
            if !config.projects.contains_key(project) {
                return Err(registry_error(format!(
                    "Project '{project}' has no feed configuration."
                )));
            }
        }
        projects.iter().collect()
    };
    let now = Utc::now();
    let mut items = Vec::new();
    for project in selected {
        let Some(feed) = cache::load_feed(project)? else {
            continue;
        };
        for item in feed.items() {
            if item.is_visible(now, config.defaults.recent_days) {
                items.push(item.clone());
            }
        }
    }
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(items)
}

fn list_work(config: &StatusConfig, args: &crate::cli::WorkListArgs) -> Result<ExitCode> {
    let dispatcher = Dispatcher::load(config)?;
    let style = Style::detect();
    // The items are only needed to say whether an entry has gone stale; work
    // whose item has left the feed is still work, and still listed.
    let items: BTreeMap<String, Item> = selected_items(config, &args.project)?
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect();

    let entries: Vec<(&String, &Entry)> = dispatcher
        .ledger
        .entries
        .iter()
        .filter(|(_, entry)| args.project.is_empty() || args.project.contains(&entry.project))
        .collect();

    if args.json {
        let rows: Vec<serde_json::Value> = entries
            .iter()
            .map(|(id, entry)| {
                let status = dispatcher.status_of(entry, items.get(*id));
                stated(serde_json::json!({
                    "item": id,
                    "project": entry.project,
                    "title": entry.title,
                    "url": entry.url,
                    "plan": entry.plan,
                    "missing": status.missing,
                    "stale": status.stale(),
                    "changes": status.changes,
                    "tickets": status.tickets.iter().map(|ticket| stated(serde_json::json!({
                        "id": ticket.id,
                        "recipe": ticket.recipe,
                        "state": ticket.state,
                        "finished": ticket.finished,
                        "cancelled": ticket.cancelled,
                        // Open and being worked on right now are different
                        // facts, here as on the row (§FS-005-dispatch.23).
                        "running": ticket.running,
                        "queued": ticket.queued,
                        "verdict": ticket.verdict,
                    }))).collect::<Vec<_>>(),
                    // The plans workflows laid down beside this matter's own
                    // (§FS-005-dispatch.19). They carry no ticket inside the
                    // matter's plan, so without this the ledger's record of
                    // them would be readable nowhere.
                    "workflows": entry.dispatches.iter().filter_map(|dispatch| {
                        dispatch.plan.as_ref().map(|plan| serde_json::json!({
                            "plan": plan,
                            "entry": dispatch.recipe,
                            "at": dispatch.at,
                        }))
                    }).collect::<Vec<_>>(),
                }))
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
        return Ok(ExitCode::SUCCESS);
    }

    if entries.is_empty() {
        println!("No work dispatched. `ephor work dispatch --dry-run` shows what would be.");
        return Ok(ExitCode::SUCCESS);
    }

    let width = entries
        .iter()
        .map(|(_, entry)| entry.project.len())
        .max()
        .unwrap_or(0);
    for (id, entry) in entries {
        let status = dispatcher.status_of(entry, items.get(id));
        if args.open && status.open_tickets() == 0 && !status.stale() {
            continue;
        }
        println!(
            "{:<width$}  {}  {}",
            entry.project,
            status.badge(64),
            style.dim(&title(&entry.title)),
        );
        // The matter's own plan, where there is one to name. An entry that
        // is nothing but workflows never had one (§FS-005-dispatch.19), and
        // printing a path to a file ephor never wrote reads as a loss.
        if !status.tickets.is_empty() || status.missing || entry.plan.is_file() {
            println!(
                "{:<width$}  {}",
                "",
                style.dim(&entry.plan.display().to_string())
            );
        }
        // What a workflow laid down beside it, each its own plan
        // (§FS-005-dispatch.19).
        for dispatch in entry.dispatches.iter().filter(|d| d.is_workflow()) {
            let plan = dispatch.plan.as_deref().unwrap_or_default();
            println!(
                "{:<width$}  {}",
                "",
                style.dim(&format!("{plan}  ({})", dispatch.recipe))
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `ephor work offers` — one matter's work screen, without the screen
/// (§FS-011-command-line.5).
fn work_offers(config: &StatusConfig, args: &crate::cli::WorkOffersArgs) -> Result<ExitCode> {
    let mut session = crate::api::Session::open(config)?;
    let item = session.item(&args.item).ok_or_else(|| {
        registry_error(format!(
            "'{}' is not in any cached feed — `ephor feed` lists what is.",
            args.item
        ))
    })?;
    let mut view = session.work_of(&item);
    // Finished tickets are folded away by default, exactly as the work screen
    // folds them: they are history, and the plan holds the whole of it
    // (§FS-005-dispatch.18).
    if !args.all {
        if let Some(status) = &mut view.status {
            status
                .tickets
                .retain(|ticket| !ticket.finished && !ticket.cancelled);
        }
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view).unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    let style = Style::detect();
    println!("{}\n", style.bold(&view.title));
    if let Some(status) = &view.status {
        println!("  {}", style.dim(&status.plan.display().to_string()));
        if status.stale {
            println!("  it has moved since: {}", status.changes.join("; "));
        }
        for ticket in &status.tickets {
            println!(
                "  {} [{}]  {}{}",
                ticket.id,
                ticket.state.as_deref().unwrap_or("?"),
                ticket.recipe,
                match &ticket.verdict {
                    Some(verdict) => format!(" — {verdict}"),
                    None => String::new(),
                }
            );
        }
        println!();
    }
    println!("{}", style.bold("what could be asked for"));
    // An empty list has two causes and they are different answers: nothing
    // selected this matter, or nothing could be asked at all. Saying the second
    // as the first is the absence §REQ-001-boundary.1 forbids.
    match (&view.unavailable, view.offers.is_empty()) {
        (Some(unavailable), _) => println!("  nothing could be offered: {unavailable}"),
        (None, true) => {
            println!("  nothing matches this matter — `ephor work ask` asks in your own words")
        }
        (None, false) => {}
    }
    for offer in &view.offers {
        println!("  {} {} {}", offer.id, offer.icon, offer.description);
        if let Some(hand) = &offer.hand {
            println!("      → {hand}");
        }
        if let Some(refusal) = &offer.refusal {
            println!("      {refusal}");
        }
    }
    // Recipes considered and refused, in the same words the JSON's `excluded`
    // carries (§FS-005-dispatch.27, §REQ-002-parity.3): a reading that says
    // only "nothing matches" cannot tell a matter nothing is configured for
    // from one a recipe was refused about.
    for exclusion in &view.excluded {
        println!("  {} — refused: {}", exclusion.recipe, exclusion.reason);
    }
    for job in &view.jobs {
        println!("\n{} {}", style.bold("ephor ran"), job.says);
    }
    if let Some(refusal) = &view.refusal {
        println!("\nnothing here runs: {refusal}");
    }
    Ok(ExitCode::SUCCESS)
}

fn title(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= 72 {
        return one_line;
    }
    one_line.chars().take(71).collect::<String>() + "…"
}

fn kind_filter(kind: &Option<String>) -> Result<Option<ItemKind>> {
    match kind {
        Some(kind) => Ok(Some(ItemKind::parse(kind).ok_or_else(|| {
            registry_error(format!(
                "Unknown kind '{kind}' (pr|ci|issue|task|message|status)."
            ))
        })?)),
        None => Ok(None),
    }
}

/// Reorders `items` by the ranking file this dispatch is bound to — the one
/// `--ranking` names, or else the one configured, and unchanged where neither
/// names one (§FS-005-dispatch.26). What the file said, or why it could not
/// be used, and every id it named that matched nothing, are said once each
/// the way every other fact this sweep learns is (§FS-006-project-interface.9)
/// — in prose and in `--json` alike, since both read `dispatcher.notes()`.
fn order_by_ranking(
    dispatcher: &mut Dispatcher,
    config: &StatusConfig,
    args: &crate::cli::WorkDispatchArgs,
    items: Vec<Item>,
) -> Vec<Item> {
    let Some(path) = args.ranking.clone().or_else(|| config.work.ranking.clone()) else {
        return items;
    };
    let reading = crate::work::ranking::read(&crate::paths::resolve_path(&path));
    dispatcher.note_once(&reading.says());
    let (ordered, unmatched) =
        crate::work::ranking::order(items, &reading.order, |item| item.id.as_str());
    for id in unmatched {
        dispatcher.note_once(&format!(
            "ranking names '{id}', which matches no item in the feed — skipped"
        ));
    }
    ordered
}

fn dispatch_work(config: &StatusConfig, args: &crate::cli::WorkDispatchArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let items = selected_items(config, &args.project)?;
    let items = order_by_ranking(&mut dispatcher, config, args, items);
    let kind = kind_filter(&args.kind)?;
    // Who does it, for this dispatch alone — the first of the seven steps
    // (§FS-005-dispatch.14), in the same grammar the tables write. Parsed
    // before the sweep, so a pick that is not one refuses before anything is
    // written.
    let picked = args
        .hand
        .as_deref()
        .map(crate::work::recipe::HandPin::parse)
        .transpose()
        .map_err(EphorError::Command)?;
    let style = Style::detect();

    let now = Utc::now();
    let mut opened = 0usize;
    let mut refused = 0usize;
    // Plans a workflow entry that asked to run itself laid down, where no
    // recipe covered the matter (§FS-005-dispatch.28). Counted apart from
    // the tickets because a plan of its own is not a ticket
    // (§FS-005-dispatch.3) — and counted against `--limit` beside them,
    // because both are the sweep handing work over (§FS-005-dispatch.26).
    let mut laid = 0usize;
    // What each item came to, kept as it goes so the machine form is the same
    // sweep the prose describes (§FS-011-command-line.7).
    let mut landed: Vec<serde_json::Value> = Vec::new();
    // Items whose deterministic opening move finished, so there was nothing to
    // dispatch (§FS-005-dispatch.12).
    let mut settled = 0usize;
    let mut asked_for_one = false;
    for item in &items {
        // The bound is on what actually gets dispatched — opened, or
        // would-open under `--dry-run` — never on what a filter or an
        // already-open ticket steps over (§FS-005-dispatch.26). `--item`
        // names one matter, not the sweep the bound bounds, so it is exempt
        // exactly as `--updated-within` already is below.
        if args.limit.is_some_and(|limit| opened + laid >= limit) && args.item.is_none() {
            break;
        }
        if let Some(id) = &args.item {
            if &item.id != id {
                continue;
            }
            asked_for_one = true;
        }
        if let Some(kind) = kind {
            if item.kind != kind {
                continue;
            }
        }
        // A sweep over everything reaches back years; work about a change
        // nobody has touched since is work nobody asked for.
        if let Some(days) = args.updated_within {
            if (now - item.updated_at).num_days() >= days && args.item.is_none() {
                continue;
            }
        }
        let offers = dispatcher.offers(item);
        let recipe = match &args.recipe {
            Some(wanted) => offers.iter().find(|recipe| &recipe.id == wanted).cloned(),
            // The first that matches: recipes are offered in priority order,
            // and one item wants one piece of work.
            None => offers.first().cloned(),
        };
        let has_work = dispatcher.ledger.entries.contains_key(&item.id);
        let Some(recipe) = recipe else {
            // No recipe covers this matter. A workflow entry that asked to
            // run itself lays its plan down instead — and only where nothing
            // is under way about the matter already, because a second plan
            // about one matter is `ephor work lay`'s to be asked for
            // (§FS-005-dispatch.28). Naming a recipe asked for that recipe
            // and not for this.
            if args.recipe.is_none() && !has_work {
                lay_autorun(
                    &mut dispatcher,
                    item,
                    picked.as_ref(),
                    args,
                    &style,
                    &mut laid,
                    &mut refused,
                    &mut landed,
                );
            }
            continue;
        };
        // A sweep leaves work already under way alone — `sync` is what reopens
        // it. Naming a recipe is a different request: it asks for that work,
        // and only the same recipe twice over is the accident worth stopping.
        let already = dispatcher
            .ledger
            .entries
            .get(&item.id)
            .map(|entry| {
                entry
                    .dispatches
                    .iter()
                    .any(|dispatch| dispatch.recipe == recipe.id)
            })
            .unwrap_or(false);
        if !args.again && (already || (has_work && args.recipe.is_none())) {
            if asked_for_one {
                // The same fact in both forms, never one of them
                // (§REQ-002-parity.3): the prose says it and the reading
                // carries a row for it, so a script that asked about one
                // matter learns *why* nothing was opened rather than only
                // that nothing was.
                let says = format!(
                    "{} already has work — `--again` adds another ticket, `sync` reopens it \
                     when the item has moved",
                    item.id
                );
                match args.json {
                    // Under `--json` standard output is the reading's alone
                    // (§FS-011-command-line.7), as it is for every other line
                    // this sweep prints.
                    true => landed.push(serde_json::json!({
                        "item": item.id,
                        "title": item.title,
                        "recipe": recipe.id,
                        "outcome": "has-work",
                        "says": says,
                    })),
                    false => println!("{says}"),
                }
            }
            continue;
        }
        match dispatcher.dispatch(item, &recipe, picked.as_ref(), args.dry_run) {
            // A deterministic opening move that finished is not a ticket
            // (§FS-005-dispatch.12) — it is reported as what it was, and
            // nothing was handed over.
            Ok(outcome @ Outcome::Settled { .. }) => {
                settled += 1;
                landed.push(serde_json::json!({
                    "item": item.id,
                    "title": item.title,
                    "recipe": recipe.id,
                    "outcome": "settled",
                    "says": outcome.describe(),
                }));
                if !args.json {
                    println!(
                        "{}\n  {}",
                        title(&item.title),
                        style.dim(&outcome.describe())
                    );
                }
            }
            Ok(outcome) => {
                opened += 1;
                let ticket = match &outcome {
                    Outcome::Opened { ticket, .. } | Outcome::Reopened { ticket, .. } => {
                        Some(ticket.clone())
                    }
                    _ => None,
                };
                landed.push(serde_json::json!({
                    "item": item.id,
                    "title": item.title,
                    "recipe": recipe.id,
                    "outcome": if args.dry_run { "would-open" } else { "opened" },
                    "ticket": ticket,
                    "says": outcome.describe(),
                }));
                if !args.json {
                    println!(
                        "{} {}\n  {}",
                        if args.dry_run { "would open" } else { "opened" },
                        title(&item.title),
                        style.dim(&outcome.describe())
                    );
                }
            }
            Err(err) => {
                refused += 1;
                landed.push(serde_json::json!({
                    "item": item.id,
                    "title": item.title,
                    "recipe": recipe.id,
                    "outcome": "refused",
                    "says": err.to_string(),
                }));
                eprintln!("note: {}: {err}", item.id);
            }
        }
    }

    if !args.dry_run {
        dispatcher.save()?;
        // Work that needs nobody to start it gets its run in the same breath
        // as the ticket (§FS-005-dispatch.24). The sweep decides what that
        // is, so this starts nothing where nothing asked for it and nothing
        // on a root a run already holds.
        started(&mut dispatcher, &args.project, args.json)?;
    }
    // What the reader should know about who got the work: a hand nobody could
    // be named to, a pair ephor cannot check, an agent with no model of its
    // own (§FS-006-project-interface.9). Said once, after the sweep, because a
    // sweep resolves the same table over and over.
    if !args.json {
        for note in dispatcher.notes() {
            println!("note: {}", style.dim(note));
        }
    }
    if args.item.is_some() && !asked_for_one {
        return Err(EphorError::Command(format!(
            "{} is not in any cached feed — run `ephor refresh` first.",
            args.item.as_deref().unwrap_or("")
        )));
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "opened": opened,
                "laid": laid,
                "settled": settled,
                "refused": refused,
                "dry_run": args.dry_run,
                "items": landed,
                "notes": dispatcher.notes(),
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        if asked_for_one && opened == 0 && settled == 0 && laid == 0 {
            return Ok(ExitCode::from(1));
        }
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "\n{opened} ticket(s){}{}{}{}",
        if args.dry_run {
            " would be opened"
        } else {
            " opened"
        },
        if laid > 0 {
            format!(
                ", {laid} plan(s) {}",
                match args.dry_run {
                    true => "would be laid down",
                    false => "laid down",
                }
            )
        } else {
            String::new()
        },
        if settled > 0 {
            format!(", {settled} item(s) finished without one")
        } else {
            String::new()
        },
        if refused > 0 {
            format!(", {refused} item(s) could not be (see above)")
        } else {
            String::new()
        }
    );
    // Asking for exactly one item and being refused is a failure; a sweep that
    // steps over what it cannot reach is not. An item the deterministic move
    // finished is not a refusal — it is the work being over
    // (§FS-005-dispatch.12) — and a workflow laid down about it is the work
    // being handed over (§FS-005-dispatch.28).
    if asked_for_one && opened == 0 && settled == 0 && laid == 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// Lay the first workflow entry that both applies to this matter and asked to
/// run itself, where one does (§FS-005-dispatch.28).
///
/// Reported exactly as a dispatch is: what landed in prose and in the
/// reading's rows alike, and a refusal — an input nobody answered, a hand a
/// narrowing will not permit, a branch that is not checked out — named with
/// nothing written and the sweep going on to the next matter
/// (§FS-005-dispatch.19, §REQ-002-parity.3).
#[allow(clippy::too_many_arguments)]
fn lay_autorun(
    dispatcher: &mut Dispatcher,
    item: &Item,
    picked: Option<&crate::work::recipe::HandPin>,
    args: &crate::cli::WorkDispatchArgs,
    style: &Style,
    laid: &mut usize,
    refused: &mut usize,
    landed: &mut Vec<serde_json::Value>,
) {
    let Some(entry) = dispatcher
        .workflow_offers(item)
        .into_iter()
        .find(|entry| entry.workflow.as_ref().is_some_and(|ask| ask.autorun))
    else {
        return;
    };
    match laying_for(dispatcher, item, &entry, picked, args.dry_run) {
        Ok(landing) => {
            *laid += 1;
            landed.push(serde_json::json!({
                "item": item.id,
                "title": item.title,
                "entry": entry.id,
                "workflow": landing.workflow,
                "outcome": if args.dry_run { "would-lay" } else { "laid" },
                "plan": landing.plan,
                "says": landing.says,
            }));
            if !args.json {
                println!(
                    "{} {}\n  {}",
                    if args.dry_run { "would lay" } else { "laid" },
                    title(&item.title),
                    style.dim(&landing.says)
                );
            }
        }
        Err(err) => {
            *refused += 1;
            landed.push(serde_json::json!({
                "item": item.id,
                "title": item.title,
                "entry": entry.id,
                "outcome": "refused",
                "says": err.to_string(),
            }));
            eprintln!("note: {}: {err}", item.id);
        }
    }
}

/// What one laying came to, in the words both forms of the reading use.
struct Landing {
    workflow: String,
    plan: std::path::PathBuf,
    says: String,
}

/// Resolve one entry about one matter, and write it unless this is a dry run
/// (§FS-005-dispatch.28). A dry run resolves everything — every input, the
/// hand, the workspace — and writes nothing at all, which is what
/// [§FS-005-dispatch.26](crate) promises of a sweep asked what it would do:
/// the files a real laying puts beside the plan are a real laying's.
fn laying_for(
    dispatcher: &mut Dispatcher,
    item: &Item,
    entry: &crate::feed::config::ActionConfig,
    picked: Option<&crate::work::recipe::HandPin>,
    dry_run: bool,
) -> Result<Landing> {
    let laying = dispatcher.laying(item, entry, &BTreeMap::new(), picked)?;
    let workflow = laying.workflow.id.clone();
    let plan = laying.root().join(&laying.plan_id);
    if dry_run {
        if let Some(why) = laying.refusal() {
            return Err(EphorError::Command(why));
        }
        return Ok(Landing {
            says: format!(
                "would lay {workflow} down as {} in {}",
                laying.plan_id,
                laying.root().display()
            ),
            workflow,
            plan,
        });
    }
    let done = dispatcher.lay(item, &laying, false)?;
    Ok(Landing {
        says: done.outcome.describe(),
        workflow,
        plan,
    })
}

/// `ephor work ask` — one item, whatever the reader wants done to it
/// (§FS-005-dispatch.10).
fn ask_work(config: &StatusConfig, args: &crate::cli::WorkAskArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let item = selected_items(config, &[])?
        .into_iter()
        .find(|item| item.id == args.item)
        .ok_or_else(|| {
            EphorError::Command(format!(
                "{} is not in any cached feed — run `ephor refresh` first.",
                args.item
            ))
        })?;

    // Typed as arguments, or piped in — which is how an ask composed in an
    // editor arrives, and those are the ones worth writing at length.
    let words = if args.words.is_empty() {
        let mut piped = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut piped)
            .map_err(|err| EphorError::Command(format!("Cannot read the ask: {err}")))?;
        piped
    } else {
        args.words.join(" ")
    };

    let outcome = dispatcher.ask(&item, &words, args.state.as_deref(), args.dry_run)?;
    if !args.dry_run {
        dispatcher.save()?;
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "item": item.id,
                "title": item.title,
                "dry_run": args.dry_run,
                "says": outcome.describe(),
                "notes": dispatcher.notes(),
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    let style = Style::detect();
    println!(
        "{} {}\n  {}",
        if args.dry_run { "would ask" } else { "asked" },
        title(&item.title),
        style.dim(&outcome.describe())
    );
    // Who got it, where that is worth saying (§FS-006-project-interface.9).
    for note in dispatcher.notes() {
        println!("note: {}", style.dim(note));
    }
    Ok(ExitCode::SUCCESS)
}

/// `ephor work workflows` — what the runtime offers, and what each one takes
/// (§FS-005-dispatch.19). A reading: nothing is written, and where no runtime
/// is bound the answer is that rung's own sentence rather than an empty list
/// pretending there are none.
fn list_workflows(config: &StatusConfig, args: &crate::cli::WorkWorkflowsArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let project = match &args.project {
        Some(project) => project.clone(),
        None => config
            .projects
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| EphorError::Command("No project is configured.".to_string()))?,
    };
    let offered = dispatcher.workflows(&project);
    // Nothing offered is an answer, and under `--json` it is an empty array
    // rather than a line of prose: the three ways there can be no workflows —
    // a refusal from the binding, a name that matches none, none at all — used
    // to leave standard output carrying a sentence, so a program asking what
    // the runtime offers got something it could not parse (§REQ-002-parity.3).
    // The reason still reaches whoever is watching, on the error stream
    // (§FS-011-command-line.7).
    if let Some(refusal) = &offered.refusal {
        match args.json {
            true => println!("[]"),
            false => println!("{refusal}"),
        }
        if args.json {
            eprintln!("note: {refusal}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let style = Style::detect();
    let named: Vec<&crate::work::runtime::workflow::Workflow> = match &args.workflow {
        Some(wanted) => offered
            .workflows
            .iter()
            .filter(|workflow| &workflow.id == wanted)
            .collect(),
        None => offered.workflows.iter().collect(),
    };
    if named.is_empty() {
        let (says, code) = match &args.workflow {
            Some(wanted) => (
                format!("The runtime offers no workflow called '{wanted}'."),
                ExitCode::FAILURE,
            ),
            None => (
                "The runtime offers no workflows here.".to_string(),
                ExitCode::SUCCESS,
            ),
        };
        match args.json {
            true => {
                println!("[]");
                eprintln!("note: {says}");
            }
            false => println!("{says}"),
        }
        return Ok(code);
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&as_json(&named)).unwrap_or_else(|_| "[]".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    // An entry beside a workflow is what makes it an action, so the listing
    // says which ones already have one (§FS-005-dispatch.19).
    let entries: Vec<(String, String)> = dispatcher
        .workflow_entries(&project)
        .into_iter()
        .filter_map(|(_, entry)| {
            entry
                .workflow
                .as_ref()
                .map(|ask| (ask.name.clone(), entry.id.clone()))
        })
        .collect();
    for workflow in &named {
        let entry = match entries.iter().find(|(name, _)| name == &workflow.id) {
            Some((_, id)) => format!(" · '{id}' beside it"),
            None => String::new(),
        };
        println!(
            "{}  {}",
            workflow.id,
            style.dim(&format!(
                "{} · {}{entry}",
                workflow.version,
                workflow.source.label()
            ))
        );
        if !workflow.description.is_empty() {
            println!("  {}", style.dim(&workflow.description));
        }
        // The inputs in full only where one workflow was named: the whole
        // listing with every input is a screen nobody reads.
        if args.workflow.is_some() {
            for input in &workflow.inputs {
                let mut says = vec![input.kind.label().to_string()];
                if input.required {
                    says.push("required".to_string());
                }
                if input.hand {
                    says.push("names who does the work".to_string());
                }
                if let Some(default) = &input.default {
                    says.push(format!("default {default}"));
                }
                println!("    {}  {}", input.name, style.dim(&says.join(" · ")));
                if !input.description.is_empty() {
                    println!("      {}", style.dim(&input.description));
                }
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn as_json(workflows: &[&crate::work::runtime::workflow::Workflow]) -> serde_json::Value {
    serde_json::Value::Array(
        workflows
            .iter()
            .map(|workflow| {
                serde_json::json!({
                    "id": workflow.id,
                    "description": workflow.description,
                    "version": workflow.version,
                    "source": workflow.source.label(),
                    "inputs": workflow.inputs.iter().map(|input| serde_json::json!({
                        "name": input.name,
                        "description": input.description,
                        "type": input.kind.label(),
                        "required": input.required,
                        "hand": input.hand,
                        "default": input.default,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// `ephor work lay` — lay one workflow down about one item
/// (§FS-005-dispatch.19). Writes files and nothing else: what runs the plan
/// is the reader, from the board.
fn lay_workflow(config: &StatusConfig, args: &crate::cli::WorkLayArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let item = selected_items(config, &[])?
        .into_iter()
        .find(|item| item.id == args.item)
        .ok_or_else(|| {
            EphorError::Command(format!(
                "{} is not in any cached feed — run `ephor refresh` first.",
                args.item
            ))
        })?;
    let typed = typed_inputs(&args.set)?;
    let picked = args
        .hand
        .as_deref()
        .map(crate::work::recipe::HandPin::parse)
        .transpose()
        .map_err(EphorError::Command)?;
    let entry = workflow_entry(&mut dispatcher, config, &item, &args.entry)?;
    let file_values = workflow_values(&args.values)?;
    let laying = dispatcher.laying_with_values(
        &item,
        &entry,
        &typed,
        &file_values,
        !args.values.is_empty(),
        picked.as_ref(),
    )?;
    let style = Style::detect();
    if !args.json {
        println!(
            "{} {} {}",
            match args.dry_run {
                true => "would lay",
                false => "laying",
            },
            laying.workflow.id,
            style.dim(&format!("about {}", title(&item.title)))
        );
        // What answered every input, before anything is written: the account a
        // reader is owed of a plan they are about to get (§FS-005-dispatch.19).
        for answer in &laying.answered.answers {
            println!(
                "  {}  {}",
                answer.input,
                style.dim(&match answer.shown.is_empty() {
                    true => format!("({})", answer.from.label()),
                    false => format!("{}  ({})", one_line(&answer.shown), answer.from.label()),
                })
            );
        }
    }
    // What answered every input, in the machine form too: a reader is owed the
    // account of a plan they are about to get, and so is a script
    // (§FS-005-dispatch.19).
    let answers: Vec<serde_json::Value> = laying
        .answered
        .answers
        .iter()
        .map(|answer| {
            serde_json::json!({
                "input": answer.input,
                "shown": answer.shown,
                "from": answer.from.label(),
            })
        })
        .collect();
    let workflow = laying.workflow.id.clone();
    let plan = laying.root().join(&laying.plan_id);
    let laid = dispatcher.lay(&item, &laying, args.dry_run)?;
    if !args.dry_run {
        dispatcher.save()?;
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "item": item.id,
                "workflow": workflow,
                "plan": plan,
                "dry_run": args.dry_run,
                "says": laid.outcome.describe(),
                "report": laid.report,
                "answers": answers,
                "notes": dispatcher.notes(),
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!("{}", style.dim(&laid.outcome.describe()));
    if !laid.report.trim().is_empty() {
        for line in laid.report.lines() {
            println!("  {}", style.dim(line));
        }
    }
    for note in dispatcher.notes() {
        println!("note: {}", style.dim(note));
    }
    Ok(ExitCode::SUCCESS)
}

/// `--set <input>=<value>`, as the reader wrote them.
fn typed_inputs(set: &[String]) -> Result<std::collections::BTreeMap<String, String>> {
    set.iter()
        .map(|pair| {
            pair.split_once('=')
                .map(|(name, value)| (name.trim().to_string(), value.to_string()))
                .ok_or_else(|| EphorError::Command(format!("'{pair}' is not <input>=<value>.")))
        })
        .collect()
}

/// Load the reader's workflow values in command-line order. The runtime's
/// values-file format is a mapping, and keeping the JSON values intact here
/// lets the answer account and the runtime see the same lists, records,
/// numbers, and flags (§FS-005-dispatch.19).
fn workflow_values(files: &[String]) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut merged = serde_json::Map::new();
    for name in files {
        let path = workflow_values_path(name)?;
        let text = std::fs::read_to_string(&path).map_err(|err| {
            EphorError::Command(format!(
                "Cannot read workflow values file '{}': {err}",
                path.display()
            ))
        })?;
        let yaml: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|err| {
            EphorError::Command(format!(
                "Cannot parse workflow values file '{}': {err}",
                path.display()
            ))
        })?;
        let value = serde_json::to_value(yaml).map_err(|err| {
            EphorError::Command(format!(
                "Cannot read workflow values file '{}': {err}",
                path.display()
            ))
        })?;
        let serde_json::Value::Object(fields) = value else {
            return Err(EphorError::Command(format!(
                "Workflow values file '{}' must contain a mapping at its root.",
                path.display()
            )));
        };
        merged.extend(fields);
    }
    Ok(merged)
}

/// Resolve a values file relative to the directory where ephor was invoked,
/// not the project's checkout or the process's later runtime directory.
fn workflow_values_path(name: &str) -> Result<std::path::PathBuf> {
    let path = std::path::PathBuf::from(crate::paths::expand_user_vars(name));
    if path.is_absolute() {
        return Ok(path);
    }
    std::env::current_dir()
        .map(|dir| dir.join(path))
        .map_err(|err| {
            EphorError::Command(format!(
                "Cannot resolve workflow values file '{name}': {err}"
            ))
        })
}

/// The entry named on the command line: one written beside a workflow, one
/// the project or the person configured, or — where nothing names it — the
/// workflow itself, asked for by name, which is what
/// [§FS-005-dispatch.10](crate) keeps possible.
fn workflow_entry(
    dispatcher: &mut Dispatcher,
    config: &StatusConfig,
    item: &Item,
    named: &str,
) -> Result<crate::feed::config::ActionConfig> {
    let configured = config
        .projects
        .get(&item.project)
        .map(|project| project.actions.as_slice())
        .unwrap_or_default()
        .iter()
        .chain(config.actions.iter())
        .find(|action| action.id == named && action.workflow.is_some());
    if let Some(action) = configured {
        return Ok(action.clone());
    }
    if let Some((_, entry)) = dispatcher
        .workflow_entries(&item.project)
        .into_iter()
        .find(|(_, entry)| entry.id == named)
    {
        return Ok(entry);
    }
    let offered = dispatcher.workflows(&item.project);
    let workflow = offered.find(named).ok_or_else(|| {
        EphorError::Command(match &offered.refusal {
            Some(why) => format!("'{named}' cannot be laid down: {why}"),
            None => format!(
                "Nothing here is called '{named}' — no entry names it, and the runtime offers \
                 no such workflow. `ephor work workflows` lists what there is."
            ),
        })
    })?;
    Ok(crate::work::workflow::Beside::default().action(workflow))
}

fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match joined.chars().count() > 72 {
        true => format!("{}…", joined.chars().take(71).collect::<String>()),
        false => joined,
    }
}

/// `ephor work cancel` — take tickets back, through the runtime's own move
/// (§FS-005-dispatch.16). One ticket at a time, each reported: a refusal on
/// one — a live run holds it, it is over already, the runtime would not — is
/// said and does not stop the next, and the exit code says whether every one
/// asked for was cancelled.
fn cancel_work(config: &StatusConfig, args: &crate::cli::WorkCancelArgs) -> Result<ExitCode> {
    let dispatcher = Dispatcher::load(config)?;
    let style = Style::detect();
    let why = args.why.as_deref().unwrap_or("");
    let mut refused = 0usize;
    let mut landed: Vec<serde_json::Value> = Vec::new();
    for ticket in &args.tickets {
        match dispatcher.cancel(&args.item, ticket, why, args.dry_run) {
            Ok(cancelled) => {
                landed.push(serde_json::json!({
                    "ticket": ticket,
                    "cancelled": true,
                    "from": cancelled.from,
                    "plan": cancelled.plan,
                    "says": cancelled.describe(),
                    "left_waiting": cancelled.left_waiting,
                }));
                if args.json {
                    continue;
                }
                println!(
                    "{}{}\n  {}",
                    if args.dry_run {
                        "would cancel — "
                    } else {
                        ""
                    },
                    cancelled.describe(),
                    style.dim(&format!(
                        "from '{}' in {}",
                        cancelled.from,
                        cancelled.plan.display()
                    ))
                );
            }
            Err(err) => {
                refused += 1;
                landed.push(serde_json::json!({
                    "ticket": ticket,
                    "cancelled": false,
                    "says": err.to_string(),
                }));
                eprintln!("note: {ticket}: {err}");
            }
        }
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "item": args.item,
                "dry_run": args.dry_run,
                "refused": refused,
                "tickets": landed,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
    }
    // The ledger records what was asked, and it still was: nothing to save.
    if refused > 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn sync_work(config: &StatusConfig, args: &crate::cli::WorkSyncArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let items = selected_items(config, &args.project)?;
    let mut reopened = 0usize;
    let mut landed: Vec<serde_json::Value> = Vec::new();
    for item in &items {
        if !dispatcher.ledger.entries.contains_key(&item.id) {
            continue;
        }
        match dispatcher.sync(item, args.dry_run) {
            Ok(Outcome::Current) => {}
            // Reported, not counted: nothing was written, and the reader still
            // wants to know their work is about something that is over.
            Ok(Outcome::Dormant { changes }) => {
                landed.push(serde_json::json!({
                    "item": item.id,
                    "title": item.title,
                    "outcome": "dormant",
                    "changes": changes,
                }));
                if !args.json {
                    println!(
                        "{}\n  {}",
                        title(&item.title),
                        Style::detect().dim(&format!(
                            "{} — no recipe applies to it now; \
                             `ephor work forget --done` clears it",
                            changes.join("; ")
                        ))
                    );
                }
            }
            Ok(outcome) => {
                reopened += 1;
                landed.push(serde_json::json!({
                    "item": item.id,
                    "title": item.title,
                    "outcome": if args.dry_run { "would-reopen" } else { "reopened" },
                    "says": outcome.describe(),
                }));
                if !args.json {
                    println!(
                        "{} {}\n  {}",
                        if args.dry_run {
                            "would reopen"
                        } else {
                            "reopened"
                        },
                        title(&item.title),
                        Style::detect().dim(&outcome.describe())
                    );
                }
            }
            Err(err) => eprintln!("note: {}: {err}", item.id),
        }
    }
    if !args.dry_run {
        dispatcher.save()?;
        // The same continuation dispatch makes: work reopened because its
        // item moved is work again, and where it needs nobody to start it,
        // nobody has to (§FS-005-dispatch.24, §FS-005-dispatch.5).
        started(&mut dispatcher, &args.project, args.json)?;
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "reopened": reopened,
                "dry_run": args.dry_run,
                "items": landed,
                "notes": dispatcher.notes(),
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "\n{reopened} ticket(s) {}",
        if args.dry_run {
            "would be reopened"
        } else {
            "reopened"
        }
    );
    // The continuation above read the ceilings, so it can have something to
    // say about them (§FS-005-dispatch.24).
    for note in dispatcher.notes() {
        println!("note: {}", Style::detect().dim(note));
    }
    Ok(ExitCode::SUCCESS)
}

/// Run the runtime over every work root that still has something to do. One
/// root at a time: the tickets in a root are about one checkout, and two
/// agents in one working tree are two agents editing the same files.
fn run_work(config: &StatusConfig, args: &crate::cli::WorkRunArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    // The runtime is a rung like every other capacity ephor leans on, and the
    // refusal is the table's sentence rather than this command's own
    // (§AR-005-capabilities.2).
    if let Some(refusal) = runtime::refusal(&config.work) {
        return Err(EphorError::Command(refusal));
    }
    let entries: Vec<Entry> = dispatcher
        .ledger
        .entries
        .iter()
        .filter(|(id, entry)| {
            (args.project.is_empty() || args.project.contains(&entry.project))
                && args.item.as_ref().is_none_or(|item| item == *id)
        })
        .map(|(_, entry)| entry.clone())
        .collect();
    // Grouped by root, because the tickets in one root are about one checkout
    // and two agents in one working tree edit the same files — but named plan
    // by plan, so a runtime project the reader keeps there for their own work
    // is not swept up by ephor's. A hand the plan language could not spell
    // rides the run as agent flags (§FS-005-dispatch.14), so plans wanting
    // different flags run separately: flags are per-run, and one flag over
    // two hands would re-aim one of them.
    // (work root, checkout to run from, the hand riding the run, the plans)
    type Group = (
        std::path::PathBuf,
        std::path::PathBuf,
        Option<runtime::roster::HandFlags>,
        Vec<String>,
    );
    // The sweep behind autorun (§FS-005-dispatch.24): which roots are due is
    // read from the world — the plans on disk, the machine's own words about
    // their states, and the runtime's lock — rather than from the ledger's
    // memory of what was dispatched. The starting itself is the engine's, so
    // this command, the timer, and a dispatch that just wrote a ticket cannot
    // drift into three ways of doing one thing (§AR-009-surfaces.1).
    if args.due {
        return swept(config, &mut dispatcher, args);
    }
    let mut roots: Vec<Group> = Vec::new();
    for entry in &entries {
        let status = dispatcher.status_of(entry, None);
        if status.missing || status.open_tickets() == 0 {
            continue;
        }
        let hand = dispatcher.run_hand(entry, &status);
        match roots
            .iter_mut()
            .find(|(root, _, flags, _)| root == &entry.root && flags == &hand)
        {
            Some((_, _, _, plans)) => plans.push(entry.plan_id.clone()),
            None => roots.push((
                entry.root.clone(),
                entry.checkout(),
                hand,
                vec![entry.plan_id.clone()],
            )),
        }
    }
    // What the reader should know about who gets this run — a hand that went
    // unbound, a name nothing resolves — said once, before the terminal is
    // handed over (§FS-006-project-interface.9).
    for note in dispatcher.notes() {
        match args.json {
            // The reading is alone on standard output (§FS-011-command-line.7);
            // a note still reaches whoever is watching.
            true => eprintln!("note: {note}"),
            false => println!("note: {}", Style::detect().dim(note)),
        }
    }
    if roots.is_empty() {
        if args.json {
            println!("{}", serde_json::json!({ "runs": [], "failed": 0 }));
            return Ok(ExitCode::SUCCESS);
        }
        println!("Nothing to run: no dispatched ticket is still open.");
        return Ok(ExitCode::SUCCESS);
    }

    let mut failed = 0usize;
    let mut runs: Vec<serde_json::Value> = Vec::new();
    // A run starts beneath the screen unless the reader asked to watch it
    // (§FS-005-dispatch.20) — and unless the binding has no detached shape, in
    // which case it runs attached as it always did and this says so rather
    // than pretending (§AR-007-runtime.3). Decided once, before any root: the
    // answer is the binding's and does not change between two of them.
    let detaching = !args.watch && runtime::can_detach(&config.work);
    if !args.watch && !detaching {
        let note = format!(
            "{} cannot start a run detached here — watching it in this terminal",
            runtime::runner(&config.work)
        );
        match args.json {
            true => eprintln!("note: {note}"),
            false => println!("note: {}", Style::detect().dim(&note)),
        }
    }
    for (root, checkout, hand, plans) in &roots {
        if !args.json {
            println!(
                "\n▶ {} {} ({} plan(s){})",
                runtime::label(&config.work),
                root.display(),
                plans.len(),
                match hand {
                    // The same phrase the key in the interface shows: one run,
                    // one sentence about who is getting it (§FS-005-dispatch.14).
                    Some(hand) => format!(", {}", hand.describe()),
                    None => String::new(),
                }
            );
        }
        let landed = |outcome: &str, says: Option<String>, id: Option<&str>| {
            let mut row = serde_json::json!({
                "root": root,
                "checkout": checkout,
                "plans": plans,
                "outcome": outcome,
                "says": says,
            });
            // Absent where the tickets agreed on nobody, which is what the
            // published shape declares — a `null` typed as a string is a
            // reading that does not hold to its own schema
            // (§REQ-002-parity.4).
            if let (Some(row), Some(hand)) = (row.as_object_mut(), hand.as_ref()) {
                row.insert("hand".to_string(), serde_json::json!(hand.describe()));
            }
            // What the run is called, where it named itself — the id the
            // reader and the runtime agree on (§FS-005-dispatch.20).
            if let (Some(row), Some(id)) = (row.as_object_mut(), id) {
                row.insert("id".to_string(), serde_json::json!(id));
            }
            row
        };
        if detaching {
            // Started and left: the launcher waits for the child to publish
            // its descriptor and returns, and what comes back is the one line
            // saying the run began and what it is called
            // (§FS-005-dispatch.20). The root turns live on the board from the
            // lock, as every run does.
            match runtime::start_detached(
                &config.work,
                root,
                checkout,
                plans,
                hand.as_ref(),
                &args.runner_args,
            ) {
                Ok(started) => {
                    // What the launcher's own descriptor said, both halves of
                    // it: a run that had nothing to do and exited inside the
                    // handshake is reported as over rather than as started, so
                    // that nobody is sent to a board with nothing on it
                    // (§FS-005-dispatch.20).
                    let (outcome, says) = match (&started.id, started.finished) {
                        (Some(id), false) => ("started", format!("▶ run {id} started")),
                        (Some(id), true) => ("done", format!("✓ run {id} finished already")),
                        // A run that named itself nothing is still a run: the
                        // row is live from the lock alone (§AR-007-runtime.3).
                        (None, false) => ("started", "▶ run started".to_string()),
                        (None, true) => ("done", "✓ the run finished already".to_string()),
                    };
                    if !args.json {
                        println!("{says}");
                    }
                    runs.push(landed(outcome, Some(says), started.id.as_deref()));
                }
                Err(err) => {
                    failed += 1;
                    runs.push(landed("failed", Some(err.to_string()), None));
                    eprintln!("error: {err} — {}", root.display());
                }
            }
            continue;
        }
        match runtime::run(
            &config.work,
            root,
            checkout,
            plans,
            hand.as_ref(),
            &args.runner_args,
            // Under `--json` this command's standard output is the reading's,
            // so the runtime's own output goes beside it rather than into it
            // — a run that narrated itself onto the reading would hand a
            // script ephor's prose to parse (§REQ-002-parity.3,
            // §FS-011-command-line.7). It still has a terminal to ask on.
            match args.json {
                true => summons::Mode::Aside,
                false => summons::Mode::Interactive,
            },
        ) {
            Ok(answer) => match answer.outcome {
                SummonsOutcome::Done => runs.push(landed("done", None, None)),
                // The runtime declining for now is not a failed run
                // (§FS-006-project-interface.3).
                SummonsOutcome::Parked => {
                    runs.push(landed("parked", None, None));
                    if !args.json {
                        println!("  parked: {}", root.display());
                    }
                }
                SummonsOutcome::Failed => {
                    failed += 1;
                    let says = answer.refusal(&runtime::label(&config.work));
                    runs.push(landed("failed", Some(says.clone()), None));
                    eprintln!("error: {says} — {}", root.display());
                }
            },
            Err(err) => {
                failed += 1;
                runs.push(landed("failed", Some(err.to_string()), None));
                eprintln!(
                    "error: {} {}: {err}",
                    runtime::label(&config.work),
                    root.display()
                );
            }
        }
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "runs": runs,
                "failed": failed,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
    }
    if failed > 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// Start whatever the sweep says is due, after a command that just wrote
/// tickets (§FS-005-dispatch.24).
///
/// The continuation, and it is the same act `--due` and the timer make: one
/// sweep, idempotent, safe on a root that already has a run. Said in one line
/// per run so the reader who asked for a ticket learns that it also began —
/// and, where it could not begin, why.
fn started(dispatcher: &mut Dispatcher, projects: &[String], json: bool) -> Result<()> {
    let launched = dispatcher.start_due(Utc::now(), projects, &[], None)?;
    if launched.is_empty() {
        return Ok(());
    }
    for run in &launched {
        match (json, run.failed.is_some()) {
            // Under `--json` the reading is alone on standard output
            // (§FS-011-command-line.7); a line about a run still reaches
            // whoever is watching.
            (true, _) | (false, true) => eprintln!("note: {}", run.says()),
            (false, false) => println!("{}", run.says()),
        }
    }
    Ok(())
}

/// `ephor work run --due` — start a run on every root that wants one and has
/// none (§FS-005-dispatch.24).
///
/// The starting is the engine's; this says what came of it. A sweep that
/// starts nothing is the ordinary case and is reported as success, because
/// "every root that wanted a run has one" is the answer, not a failure — a
/// timer that went red on a quiet machine would be a watch reporting on
/// itself.
fn swept(
    config: &StatusConfig,
    dispatcher: &mut Dispatcher,
    args: &crate::cli::WorkRunArgs,
) -> Result<ExitCode> {
    let style = Style::detect();
    let launched = dispatcher.start_due(
        Utc::now(),
        &args.project,
        &args.runner_args,
        args.max_concurrent,
    )?;
    let failed = launched.iter().filter(|run| run.failed.is_some()).count();
    if args.json {
        let rows: Vec<serde_json::Value> = launched
            .iter()
            .map(|run| {
                stated(serde_json::json!({
                    "root": run.root,
                    "project": run.project,
                    "item": run.item,
                    "tickets": run.tickets,
                    "outcome": run.outcome(),
                    "says": run.says(),
                    "id": run.id,
                    "reason": run.reason(),
                }))
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "runs": rows,
                "failed": failed,
                // What the sweep learned about the ceilings it read — a pair
                // written the wrong way round, said where it bites rather
                // than only at a check (§FS-005-dispatch.24).
                "notes": dispatcher.notes(),
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return exit_code(failed);
    }
    if launched.is_empty() {
        println!("Nothing is due: no work root is waiting for a run.");
    } else {
        for run in &launched {
            println!(
                "\n▶ {} {}",
                runtime::label(&config.work),
                run.root.display()
            );
            // What made the root due, so a run nobody asked for still says
            // what it is about (§FS-005-dispatch.24).
            println!("  {}", style.dim(&run.tickets.join(", ")));
            match &run.failed {
                Some(_) => eprintln!("error: {}", run.says()),
                None => println!("{}", run.says()),
            }
        }
    }
    // The ceilings this sweep read, where one of them contradicts another. It
    // is said whether or not anything started, because the reading is what is
    // wrong (§FS-005-dispatch.24).
    for note in dispatcher.notes() {
        println!("note: {}", style.dim(note));
    }
    exit_code(failed)
}

/// A sweep is a success unless a launch actually failed: a root passed over
/// for capacity is an answer, not a failure (§FS-005-dispatch.24).
fn exit_code(failed: usize) -> Result<ExitCode> {
    if failed > 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// Drop ledger entries. The plans stay on disk: they are the record of what
/// was done, and ephor deleting a reader's work would be the one irreversible
/// thing in here.
fn forget_work(config: &StatusConfig, args: &crate::cli::WorkForgetArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let ids: Vec<String> = dispatcher
        .ledger
        .entries
        .iter()
        .filter(|(id, entry)| {
            if let Some(wanted) = &args.item {
                return *id == wanted;
            }
            let status = dispatcher.status_of(entry, None);
            (args.done && status.open_tickets() == 0) || (args.missing && status.missing)
        })
        .map(|(id, _)| id.clone())
        .collect();
    if ids.is_empty() {
        if args.json {
            println!("{}", serde_json::json!({ "forgot": [] }));
            return Ok(ExitCode::SUCCESS);
        }
        println!("Nothing to forget (pass --item, --done, or --missing).");
        return Ok(ExitCode::SUCCESS);
    }
    let mut forgot: Vec<serde_json::Value> = Vec::new();
    for id in &ids {
        if let Some(entry) = dispatcher.ledger.entries.remove(id) {
            forgot.push(serde_json::json!({ "item": id, "plan": entry.plan }));
            if !args.json {
                println!("forgot {id} — its plan stays at {}", entry.plan.display());
            }
        }
    }
    dispatcher.save()?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "forgot": forgot }))
                .unwrap_or_else(|_| "null".to_string())
        );
    }
    Ok(ExitCode::SUCCESS)
}
