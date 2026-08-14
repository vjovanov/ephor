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
use crate::seams::summons::Outcome as SummonsOutcome;
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
        WorkCommand::Dispatch(dispatch) => dispatch_work(&config, dispatch),
        WorkCommand::Ask(ask) => ask_work(&config, ask),
        WorkCommand::Sync(sync) => sync_work(&config, sync),
        WorkCommand::Run(run) => run_work(&config, run),
        WorkCommand::Forget(forget) => forget_work(&config, forget),
        WorkCommand::States => {
            // The one configured for these projects when there is one, so what
            // is printed is what tickets would actually run under.
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
            match configured {
                Some(path) => print!(
                    "{}",
                    std::fs::read_to_string(&path).map_err(|err| EphorError::Command(format!(
                        "Cannot read the configured state machine {}: {err}",
                        path.display()
                    )))?
                ),
                None => print!("{}", crate::work::runtime::plan::SHIPPED_STATES),
            }
            Ok(ExitCode::SUCCESS)
        }
    }
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
                serde_json::json!({
                    "item": id,
                    "project": entry.project,
                    "title": entry.title,
                    "url": entry.url,
                    "plan": entry.plan,
                    "missing": status.missing,
                    "stale": status.stale(),
                    "changes": status.changes,
                    "tickets": status.tickets.iter().map(|ticket| serde_json::json!({
                        "id": ticket.id,
                        "recipe": ticket.recipe,
                        "state": ticket.state,
                        "finished": ticket.finished,
                        "verdict": ticket.verdict,
                    })).collect::<Vec<_>>(),
                })
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
        println!(
            "{:<width$}  {}",
            "",
            style.dim(&entry.plan.display().to_string())
        );
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
                "Unknown kind '{kind}' (pr|ci|issue|message|status)."
            ))
        })?)),
        None => Ok(None),
    }
}

fn dispatch_work(config: &StatusConfig, args: &crate::cli::WorkDispatchArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let items = selected_items(config, &args.project)?;
    let kind = kind_filter(&args.kind)?;
    let style = Style::detect();

    let now = Utc::now();
    let mut opened = 0usize;
    let mut refused = 0usize;
    let mut asked_for_one = false;
    for item in &items {
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
        let Some(recipe) = recipe else {
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
        let has_work = dispatcher.ledger.entries.contains_key(&item.id);
        if !args.again && (already || (has_work && args.recipe.is_none())) {
            if asked_for_one {
                println!(
                    "{} already has work — `--again` adds another ticket, `sync` reopens it \
                     when the item has moved",
                    item.id
                );
            }
            continue;
        }
        match dispatcher.dispatch(item, &recipe, args.dry_run) {
            Ok(outcome) => {
                opened += 1;
                println!(
                    "{} {}\n  {}",
                    if args.dry_run { "would open" } else { "opened" },
                    title(&item.title),
                    style.dim(&outcome.describe())
                );
            }
            Err(err) => {
                refused += 1;
                eprintln!("note: {}: {err}", item.id);
            }
        }
    }

    if !args.dry_run {
        dispatcher.save()?;
    }
    if args.item.is_some() && !asked_for_one {
        return Err(EphorError::Command(format!(
            "{} is not in any cached feed — run `ephor refresh` first.",
            args.item.as_deref().unwrap_or("")
        )));
    }
    println!(
        "\n{opened} ticket(s){}{}",
        if args.dry_run {
            " would be opened"
        } else {
            " opened"
        },
        if refused > 0 {
            format!(", {refused} item(s) could not be (see above)")
        } else {
            String::new()
        }
    );
    // Asking for exactly one item and being refused is a failure; a sweep that
    // steps over what it cannot reach is not.
    if asked_for_one && opened == 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
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
    println!(
        "{} {}\n  {}",
        if args.dry_run { "would ask" } else { "asked" },
        title(&item.title),
        Style::detect().dim(&outcome.describe())
    );
    Ok(ExitCode::SUCCESS)
}

fn sync_work(config: &StatusConfig, args: &crate::cli::WorkSyncArgs) -> Result<ExitCode> {
    let mut dispatcher = Dispatcher::load(config)?;
    let items = selected_items(config, &args.project)?;
    let mut reopened = 0usize;
    for item in &items {
        if !dispatcher.ledger.entries.contains_key(&item.id) {
            continue;
        }
        match dispatcher.sync(item, args.dry_run) {
            Ok(Outcome::Current) => {}
            // Reported, not counted: nothing was written, and the reader still
            // wants to know their work is about something that is over.
            Ok(Outcome::Dormant { changes }) => println!(
                "{}\n  {}",
                title(&item.title),
                Style::detect().dim(&format!(
                    "{} — no recipe applies to it now; `ephor work forget --done` clears it",
                    changes.join("; ")
                ))
            ),
            Ok(outcome) => {
                reopened += 1;
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
            Err(err) => eprintln!("note: {}: {err}", item.id),
        }
    }
    if !args.dry_run {
        dispatcher.save()?;
    }
    println!(
        "\n{reopened} ticket(s) {}",
        if args.dry_run {
            "would be reopened"
        } else {
            "reopened"
        }
    );
    Ok(ExitCode::SUCCESS)
}

/// Run the runtime over every work root that still has something to do. One
/// root at a time: the tickets in a root are about one checkout, and two
/// agents in one working tree are two agents editing the same files.
fn run_work(config: &StatusConfig, args: &crate::cli::WorkRunArgs) -> Result<ExitCode> {
    let dispatcher = Dispatcher::load(config)?;
    // The runtime is a rung like every other capacity ephor leans on, and the
    // refusal is the table's sentence rather than this command's own
    // (§AR-005-capabilities.2).
    if let Some(refusal) = runtime::refusal(&config.work) {
        return Err(EphorError::Command(refusal));
    }
    // Grouped by root, because the tickets in one root are about one checkout
    // and two agents in one working tree edit the same files — but named plan
    // by plan, so a runtime project the reader keeps there for their own work
    // is not swept up by ephor's.
    // (work root, checkout to run from, the plans ephor opened there)
    let mut roots: Vec<(std::path::PathBuf, std::path::PathBuf, Vec<String>)> = Vec::new();
    for (id, entry) in &dispatcher.ledger.entries {
        if !args.project.is_empty() && !args.project.contains(&entry.project) {
            continue;
        }
        if let Some(item) = &args.item {
            if id != item {
                continue;
            }
        }
        let status = dispatcher.status_of(entry, None);
        if status.missing || status.open_tickets() == 0 {
            continue;
        }
        match roots.iter_mut().find(|(root, _, _)| root == &entry.root) {
            Some((_, _, plans)) => plans.push(entry.plan_id.clone()),
            None => roots.push((
                entry.root.clone(),
                entry.checkout(),
                vec![entry.plan_id.clone()],
            )),
        }
    }
    if roots.is_empty() {
        println!("Nothing to run: no dispatched ticket is still open.");
        return Ok(ExitCode::SUCCESS);
    }

    let mut failed = 0usize;
    for (root, checkout, plans) in &roots {
        println!(
            "\n▶ {} {} ({} plan(s))",
            runtime::label(&config.work),
            root.display(),
            plans.len()
        );
        match runtime::run(&config.work, root, checkout, plans, &args.runner_args) {
            Ok(answer) => match answer.outcome {
                SummonsOutcome::Done => {}
                // The runtime declining for now is not a failed run
                // (§FS-006-project-interface.3).
                SummonsOutcome::Parked => {
                    println!("  parked: {}", root.display());
                }
                SummonsOutcome::Failed => {
                    failed += 1;
                    eprintln!(
                        "error: {} — {}",
                        answer.refusal(&runtime::label(&config.work)),
                        root.display()
                    );
                }
            },
            Err(err) => {
                failed += 1;
                eprintln!(
                    "error: {} {}: {err}",
                    runtime::label(&config.work),
                    root.display()
                );
            }
        }
    }
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
        println!("Nothing to forget (pass --item, --done, or --missing).");
        return Ok(ExitCode::SUCCESS);
    }
    for id in &ids {
        if let Some(entry) = dispatcher.ledger.entries.remove(id) {
            println!("forgot {id} — its plan stays at {}", entry.plan.display());
        }
    }
    dispatcher.save()?;
    Ok(ExitCode::SUCCESS)
}
