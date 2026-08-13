//! `ephor rebase` — the deterministic move, run on its own
//! (§FS-004-quick-actions.6).
//!
//! The reader presses a key for it and a state machine runs it as a program;
//! both arrive here, so there is one answer to what a clean rebase is
//! (§FS-005-dispatch.12). What it cannot finish — a conflict — it hands over
//! as work rather than deciding.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::branches::Placement;
use crate::cli::RebaseArgs;
use crate::error::{EphorError, Result};
use crate::feed::cache;
use crate::feed::config::load_config;
use crate::feed::model::Item;
use crate::git;
use crate::work::Dispatcher;

/// A conflict is not a failure: it is where the work starts.
const CONFLICT: u8 = 3;

/// An argument, or what a program state put in the environment for it. A
/// state machine hands its program everything through `env:` (the manual's
/// §8.5), so the flags a reader types and the names a state sets are the same
/// four inputs spelled two ways.
fn or_env(flag: &Option<String>, name: &str) -> Option<String> {
    flag.clone()
        .or_else(|| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        // A `{meta.branch}` the runtime could not fill arrives as itself; an
        // unresolved placeholder is not a value.
        .filter(|value| !value.is_empty() && !value.contains('{'))
}

pub fn rebase(args: &RebaseArgs) -> Result<ExitCode> {
    let checkout = match or_env(&args.checkout, "CHECKOUT") {
        Some(path) => crate::paths::resolve_path(&path),
        None => std::env::current_dir().map_err(|err| {
            EphorError::Command(format!("Cannot read the working directory: {err}"))
        })?,
    };
    if !checkout.is_dir() {
        return Err(EphorError::Command(format!(
            "No checkout at {} — nothing to rebase.",
            checkout.display()
        )));
    }

    let item = match or_env(&args.item, "ITEM") {
        Some(id) => Some(find_item(&id)?),
        None => None,
    };
    let project =
        or_env(&args.project, "PROJECT").or_else(|| item.as_ref().map(|item| item.project.clone()));
    let placement = project.as_deref().and_then(|project| {
        Placement::load(&crate::feed::commands::load_registry_doc().ok()?, project)
    });

    let repo_paths = placement
        .as_ref()
        .map(|placement| placement.repo_paths.clone())
        .unwrap_or_default();
    let base = match or_env(&args.onto, "ONTO").or_else(|| {
        placement
            .as_ref()
            .and_then(|placement| placement.main_branch.clone())
    }) {
        Some(base) => base,
        // No registry to ask: what origin itself calls its default is the
        // closest thing to an answer, and saying which one was chosen is what
        // makes a wrong guess visible.
        None => git::repos(&checkout, &repo_paths)
            .first()
            .and_then(|repo| git::default_base(repo))
            .ok_or_else(|| {
                EphorError::Command(format!(
                    "Nothing says what to rebase {} onto — pass --onto, or --project for a \
                     registry entry with a main_branch.",
                    checkout.display()
                ))
            })?,
    };

    let outcome = git::rebase(&checkout, &repo_paths, &base);
    print!("{}", outcome.report());
    if let Some(path) = or_env(&args.report, "REPORT") {
        write_report(&path, &outcome.report())?;
    }

    if outcome.repos.is_empty() {
        return Err(EphorError::Command(format!(
            "No git repository under {}.",
            checkout.display()
        )));
    }

    let conflicted = outcome.conflicted().len();
    if conflicted > 0 {
        // Everything the algorithm could do, it did; the rest is a question
        // about the code (§FS-005-dispatch.12).
        if args.dispatch {
            match &item {
                Some(item) => hand_over(item, &outcome)?,
                None => {
                    eprintln!("note: --dispatch needs --item to know whose work this conflict is.")
                }
            }
        } else if let Some(item) = &item {
            println!(
                "Hand the conflict to the runtime:\n  ephor work dispatch --item {} \
                 --recipe rebase",
                item.id
            );
        }
        return Ok(ExitCode::from(CONFLICT));
    }
    if !outcome.stuck().is_empty() {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// Open the ticket the conflict is about, carrying what the rebase reached.
fn hand_over(item: &Item, outcome: &git::Rebase) -> Result<()> {
    let config = load_config()?;
    let mut dispatcher = Dispatcher::load(&config)?;
    let recipe = dispatcher
        .recipes(&item.project)
        .into_iter()
        .find(|recipe| recipe.id == "rebase")
        .ok_or_else(|| {
            EphorError::Command("No recipe named 'rebase' is configured.".to_string())
        })?;
    // The brief carries the situation, not the request to reproduce it: the
    // repository is standing in the conflict this text describes.
    let recipe = crate::work::recipe::Recipe {
        brief: format!("{}\n\n{}", recipe.brief, outcome.report()),
        ..recipe
    };
    let opened = dispatcher.dispatch(item, &recipe, false)?;
    dispatcher.save()?;
    println!("\nhanded over: {}", opened.describe());
    println!("  ephor work run --item {}", item.id);
    Ok(())
}

fn write_report(path: &str, contents: &str) -> Result<()> {
    let path = PathBuf::from(crate::paths::resolve_path(path));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            EphorError::Command(format!("Cannot create {}: {err}", parent.display()))
        })?;
    }
    std::fs::write(&path, contents)
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", path.display())))
}

/// The item by its feed id, out of whatever the last refresh cached.
fn find_item(id: &str) -> Result<Item> {
    let config = load_config()?;
    for project in config.projects.keys() {
        let Some(feed) = cache::load_feed(project)? else {
            continue;
        };
        let found = feed.items().find(|item| item.id == id);
        if let Some(item) = found {
            return Ok(item);
        }
    }
    Err(EphorError::Command(format!(
        "{id} is not in any cached feed — run `ephor refresh` first."
    )))
}
