//! `ephor checkout` — making the workspace that is not there
//! (§FS-004-quick-actions.7).
//!
//! The reader presses a key for it and a state machine runs it as a program;
//! both arrive here, so there is one answer to where a project's branch
//! workspace goes and what it holds (§FS-005-dispatch.12). Everything it needs
//! is already in the registry — the directory template, the repositories, the
//! main branch — which is why nobody has to configure a command for it.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::branches::Placement;
use crate::cli::CheckoutArgs;
use crate::error::{EphorError, Result};
use crate::feed::cache;
use crate::feed::config::load_config;
use crate::feed::model::Item;
use crate::git;

/// An argument, or what a program state put in the environment for it — the
/// same two spellings `ephor rebase` takes.
fn or_env(flag: &Option<String>, name: &str) -> Option<String> {
    flag.clone()
        .or_else(|| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains('{'))
}

pub fn checkout(args: &CheckoutArgs) -> Result<ExitCode> {
    let item = match or_env(&args.item, "ITEM") {
        Some(id) => Some(find_item(&id)?),
        None => None,
    };
    let project = or_env(&args.project, "PROJECT")
        .or_else(|| item.as_ref().map(|item| item.project.clone()))
        .ok_or_else(|| {
            EphorError::Command(
                "Nothing says which project this branch belongs to — pass --project or --item."
                    .to_string(),
            )
        })?;

    let registry = crate::feed::commands::load_registry_doc()?;
    let placement = Placement::load(&registry, &project).ok_or_else(|| {
        EphorError::Command(format!(
            "{project}: no root in the registry, so there is nowhere to put a checkout."
        ))
    })?;

    let branch = or_env(&args.branch, "BRANCH")
        .or_else(|| item.as_ref().and_then(|item| placement.branch_name(item)))
        .ok_or_else(|| {
            EphorError::Command(
                "Nothing says which branch to check out — pass --branch, or --item for one \
                 the feed knows the branch of."
                    .to_string(),
            )
        })?;

    let target = placement.workspace_for(&branch).ok_or_else(|| {
        EphorError::Command(format!(
            "{project} does not use a checkout per branch (no branch_root_template), so \
             there is no workspace to make for {branch} — its root is the checkout."
        ))
    })?;
    if target.is_dir() {
        println!("{} is already checked out.", target.display());
        return Ok(ExitCode::SUCCESS);
    }

    // A working tree is added from a repository, so one has to be on disk.
    let source = placement.source_checkout().ok_or_else(|| {
        EphorError::Command(format!(
            "{project} has no checkout on disk to make {} from — clone the project first.",
            target.display()
        ))
    })?;

    let base = match or_env(&args.from, "FROM").or_else(|| placement.main_branch.clone()) {
        Some(base) => base,
        None => placement
            .repos(&source)
            .first()
            .and_then(|repo| git::default_base(repo))
            .ok_or_else(|| {
                EphorError::Command(format!(
                    "Nothing says what to grow {branch} from — pass --from, or give \
                     {project} a main_branch in the registry."
                ))
            })?,
    };

    let outcome = git::create(&source, &target, &placement.repo_paths, &branch, &base);
    print!("{}", outcome.report());
    if let Some(path) = or_env(&args.report, "REPORT") {
        write_report(&path, &outcome.report())?;
    }

    if outcome.repos.is_empty() {
        return Err(EphorError::Command(format!(
            "No repository under {} to make a workspace from.",
            source.display()
        )));
    }
    if !outcome.refused().is_empty() {
        // Half a workspace is not one: whatever is missing, the next thing to
        // run in here would fail on it.
        return Ok(ExitCode::from(1));
    }
    println!("{}", outcome.summary());
    Ok(ExitCode::SUCCESS)
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
