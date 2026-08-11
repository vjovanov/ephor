//! `ephor status | feed | refresh | mark-read` command implementations.

use std::process::ExitCode;

use chrono::Utc;
use serde_json::Value;

use crate::cli::{FeedArgs, MarkReadArgs, RefreshArgs, StatusArgs};
use crate::error::{registry_error, EphorError, Result};
use crate::feed::cache::{self, ProjectFeed};
use crate::feed::config::{load_config, StatusConfig};
use crate::feed::model::ItemKind;
use crate::feed::refresh::refresh_project;
use crate::feed::render::{self, Style};
use crate::registry;

pub(crate) fn load_registry_doc() -> Result<Value> {
    let registry_path = crate::paths::default_registry_path();
    let schema: Value = serde_json::from_str(registry::EMBEDDED_SCHEMA)
        .map_err(|err| registry_error(format!("Invalid embedded schema: {err}")))?;
    registry::load_registry(&registry_path, &schema)
}

fn known_project<'a>(
    config: &'a StatusConfig,
    project: &str,
) -> Result<&'a crate::feed::config::ProjectFeedConfig> {
    config.projects.get(project).ok_or_else(|| {
        registry_error(format!(
            "Project '{project}' has no feed configuration in {} (configured: {}).",
            crate::feed::config::config_path().display(),
            config
                .projects
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

fn refresh_projects(
    config: &StatusConfig,
    projects: &[String],
    quiet: bool,
) -> Result<(usize, usize)> {
    let registry_doc = load_registry_doc()?;
    let selected: Vec<&String> = if projects.is_empty() {
        config.projects.keys().collect()
    } else {
        projects.iter().collect()
    };

    let mut failures = 0usize;
    let mut refreshed = 0usize;
    for project in selected {
        let project_config = known_project(config, project)?;
        let outcome = refresh_project(&registry_doc, project, project_config, &config.defaults)?;
        refreshed += 1;
        for error in &outcome.errors {
            eprintln!("warning: {project}: {error}");
        }
        if outcome.total_failure {
            failures += 1;
        }
        if !quiet {
            println!("{project}: {} items", outcome.item_count);
        }
    }
    Ok((refreshed, failures))
}

/// Refresh when the cache is missing or older than the TTL (unless --cached).
fn ensure_fresh(config: &StatusConfig, project: &str, args: &StatusArgs) -> Result<ProjectFeed> {
    let ttl = args.max_age.unwrap_or(config.defaults.ttl_seconds);
    let cached = cache::load_feed(project)?;
    let fresh_enough = cached
        .as_ref()
        .and_then(|feed| feed.fetched_at)
        .map(|fetched| (Utc::now() - fetched).num_seconds() < ttl as i64)
        .unwrap_or(false);

    if args.cached || (fresh_enough && !args.refresh) {
        return cached.ok_or_else(|| {
            EphorError::Command(format!(
                "No cached feed for '{project}'; run `ephor refresh {project}`."
            ))
        });
    }

    let registry_doc = load_registry_doc()?;
    let project_config = known_project(config, project)?;
    let outcome = refresh_project(&registry_doc, project, project_config, &config.defaults)?;
    for error in &outcome.errors {
        eprintln!("warning: {project}: {error}");
    }
    cache::load_feed(project)?
        .ok_or_else(|| EphorError::Command(format!("Refresh produced no cache for '{project}'.")))
}

fn check_exit(feeds: &[ProjectFeed], seen: &cache::Seen, check: bool) -> ExitCode {
    if !check {
        return ExitCode::SUCCESS;
    }
    let pending = feeds.iter().any(|feed| {
        feed.items()
            .any(|item| item.needs_response && cache::is_unread(seen, item))
    });
    if pending {
        ExitCode::from(4)
    } else {
        ExitCode::SUCCESS
    }
}

pub fn status(args: &StatusArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let seen = cache::load_seen()?;
    let style = Style::detect();

    let feeds: Vec<ProjectFeed> = match &args.project {
        Some(project) => {
            known_project(&config, project)?;
            vec![ensure_fresh(&config, project, args)?]
        }
        None => {
            let mut feeds = Vec::new();
            for project in config.projects.keys() {
                feeds.push(ensure_fresh(&config, project, args)?);
            }
            feeds
        }
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&feeds).unwrap());
        return Ok(check_exit(&feeds, &seen, args.check));
    }

    if args.project.is_some() {
        render::render_project(&feeds[0], &seen, &style);
    } else {
        // Summary table across all configured projects.
        println!(
            "{:<30}  {:>6}  {:>6}  {:>7}  {:>7}",
            "Project", "Items", "Unread", "Respond", "Failing"
        );
        for feed in &feeds {
            let (project, unread, needs, failing) = render::render_summary_row(feed, &seen);
            let items = feed.items().count();
            println!("{project:<30}  {items:>6}  {unread:>6}  {needs:>7}  {failing:>7}");
        }
    }
    Ok(check_exit(&feeds, &seen, args.check))
}

pub fn feed(args: &FeedArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let seen = cache::load_seen()?;
    let style = Style::detect();
    let kind_filter = match &args.kind {
        Some(kind) => Some(ItemKind::parse(kind).ok_or_else(|| {
            registry_error(format!("Unknown kind '{kind}' (pr|ci|message|status)."))
        })?),
        None => None,
    };

    let selected: Vec<&String> = if args.project.is_empty() {
        config.projects.keys().collect()
    } else {
        args.project.iter().collect()
    };

    let mut feeds = Vec::new();
    for project in selected {
        known_project(&config, project)?;
        if let Some(feed) = cache::load_feed(project)? {
            feeds.push(feed);
        }
    }

    let now = Utc::now();
    let mut lines: Vec<(chrono::DateTime<Utc>, String, Value)> = Vec::new();
    for feed in &feeds {
        for item in feed.items() {
            if let Some(kind) = kind_filter {
                if item.kind != kind {
                    continue;
                }
            }
            if args.unread && !cache::is_unread(&seen, item) {
                continue;
            }
            lines.push((
                item.updated_at,
                render::render_item_line(item, feed, &seen, &style, true, now),
                serde_json::to_value(item).unwrap(),
            ));
        }
    }
    lines.sort_by(|a, b| b.0.cmp(&a.0));

    if args.json {
        let items: Vec<&Value> = lines.iter().map(|(_, _, value)| value).collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else if lines.is_empty() {
        println!("No feed items. Run `ephor refresh` or adjust filters.");
    } else {
        for (_, line, _) in &lines {
            println!("{line}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn refresh(args: &RefreshArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let (refreshed, failures) = refresh_projects(&config, &args.projects, args.quiet)?;
    if refreshed > 0 && failures == refreshed {
        return Ok(ExitCode::from(3));
    }
    Ok(ExitCode::SUCCESS)
}

pub fn mark_read(args: &MarkReadArgs, all: bool) -> Result<ExitCode> {
    let config = load_config()?;
    let mut seen = cache::load_seen()?;
    let kind_filter = match &args.kind {
        Some(kind) => Some(ItemKind::parse(kind).ok_or_else(|| {
            registry_error(format!("Unknown kind '{kind}' (pr|ci|message|status)."))
        })?),
        None => None,
    };

    let selected: Vec<String> = match (&args.project, all) {
        (Some(project), _) => vec![project.clone()],
        (None, true) => config.projects.keys().cloned().collect(),
        (None, false) if args.id.is_some() => config.projects.keys().cloned().collect(),
        (None, false) => {
            return Err(registry_error(
                "mark-read needs a project, --all, or --id.".to_string(),
            ))
        }
    };

    let mut marked = 0usize;
    let mut live_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for project in &selected {
        known_project(&config, project)?;
        let Some(feed) = cache::load_feed(project)? else {
            continue;
        };
        for item in feed.items() {
            live_ids.insert(item.id.clone());
            if let Some(id) = &args.id {
                if &item.id != id {
                    continue;
                }
            }
            if let Some(kind) = kind_filter {
                if item.kind != kind {
                    continue;
                }
            }
            seen.insert(item.id.clone(), item.updated_at);
            marked += 1;
        }
    }

    // Prune seen entries for items that no longer exist in any cached feed.
    if args.id.is_none()
        && kind_filter.is_none()
        && (all || config.projects.len() == selected.len())
    {
        seen.retain(|id, _| live_ids.contains(id));
    }

    cache::store_seen(&seen)?;
    println!("Marked {marked} items as read.");
    Ok(ExitCode::SUCCESS)
}
