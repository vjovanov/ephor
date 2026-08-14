//! `ephor status | feed | refresh | mark-read` command implementations.

use std::process::ExitCode;

use chrono::Utc;
use serde_json::Value;

use crate::cli::{FailuresArgs, FeedArgs, MarkReadArgs, RefreshArgs, StatusArgs};
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
) -> Result<RefreshTally> {
    let registry_doc = load_registry_doc()?;
    let selected: Vec<&String> = if projects.is_empty() {
        config.projects.keys().collect()
    } else {
        projects.iter().collect()
    };

    // Shared sources are fetched once for the whole site and placed by the
    // engine (§AR-008-pipeline.1). Where one is still declared under a
    // project it keeps working, and says where it belongs now.
    for (project, source) in config.misplaced_shared_sources() {
        eprintln!(
            "note: '{source}' is declared under project '{project}'. It asks nothing about one \
             project, so it belongs in the top-level \"sources\" where ephor places what it \
             finds; the per-project form still works for now."
        );
    }
    let mut total_failures = 0usize;
    let mut degraded = 0usize;
    let mut refreshed = 0usize;
    for project in selected {
        let project_config = known_project(config, project)?;
        let outcome = refresh_project(&registry_doc, project, project_config, &config.defaults)?;
        refreshed += 1;
        // A provider that did not deliver is an error, not a warning: its
        // section of the feed is last-good data or nothing at all, and either
        // reads exactly like "you have no work here".
        for failure in &outcome.failures {
            eprintln!("error: {project}: {}", failure.describe());
        }
        if outcome.total_failure {
            total_failures += 1;
        } else if !outcome.failures.is_empty() {
            degraded += 1;
        }
        if !quiet {
            println!("{project}: {} items", outcome.item_count);
        }
    }

    // After the projects, not before: a per-project refresh writes its whole
    // feed, so a shared source's slot has to be folded in once the file it
    // lands in has been rewritten (§AR-008-pipeline.1).
    let shared = crate::feed::refresh::refresh_shared(&registry_doc, config)?;
    for failure in &shared.failures {
        eprintln!("error: sources: {}", failure.describe());
    }
    // The shared sources are a refreshed unit like a project, and their
    // failures count like one: this is the leg that reads the forge's own
    // notice list, so losing it silently is how the completeness capability
    // stays dark while whatever runs the timer sees exit 0
    // (§FS-001-forge-interface.6).
    if !config.sources.is_empty() {
        refreshed += 1;
        if shared.total_failure {
            total_failures += 1;
        } else if !shared.failures.is_empty() {
            degraded += 1;
        }
    }
    if !quiet && shared.item_count > 0 {
        println!("sources: {} items placed", shared.item_count);
    }

    Ok(RefreshTally {
        refreshed,
        total_failures,
        degraded,
    })
}

/// How a whole refresh went, across every selected project.
struct RefreshTally {
    refreshed: usize,
    /// Projects where every provider failed.
    total_failures: usize,
    /// Projects that lost some providers but not all.
    degraded: usize,
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
    for failure in &outcome.failures {
        eprintln!("error: {project}: {}", failure.describe());
    }
    cache::load_feed(project)?
        .ok_or_else(|| EphorError::Command(format!("Refresh produced no cache for '{project}'.")))
}

fn check_exit(
    feeds: &[ProjectFeed],
    seen: &cache::Seen,
    check: bool,
    recent_days: u64,
) -> ExitCode {
    if !check {
        return ExitCode::SUCCESS;
    }
    let now = Utc::now();
    // Work that aged out of Recent no longer awaits anyone.
    let pending = feeds.iter().any(|feed| {
        feed.items().any(|item| {
            item.needs_response
                && cache::is_unread(seen, &item)
                && item.is_visible(now, recent_days)
        })
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

    let recent_days = config.defaults.recent_days;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&feeds).unwrap());
        return Ok(check_exit(&feeds, &seen, args.check, recent_days));
    }

    if args.project.is_some() {
        render::render_project(&feeds[0], &seen, &style, recent_days);
    } else {
        // Summary table across all configured projects.
        let now = Utc::now();
        println!(
            "{:<30}  {:>6}  {:>6}  {:>7}  {:>7}",
            "Project", "Items", "Unread", "Respond", "Failing"
        );
        for feed in &feeds {
            let (project, unread, needs, failing) =
                render::render_summary_row(feed, &seen, recent_days);
            let items = feed
                .items()
                .filter(|item| item.is_visible(now, recent_days))
                .count();
            println!("{project:<30}  {items:>6}  {unread:>6}  {needs:>7}  {failing:>7}");
        }
    }
    Ok(check_exit(&feeds, &seen, args.check, recent_days))
}

pub fn feed(args: &FeedArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let seen = cache::load_seen()?;
    let style = Style::detect();
    let kind_filter = match &args.kind {
        Some(kind) => Some(ItemKind::parse(kind).ok_or_else(|| {
            registry_error(format!(
                "Unknown kind '{kind}' (pr|ci|issue|message|status)."
            ))
        })?),
        None => None,
    };

    let mut feeds = Vec::new();
    if args.unattributed {
        // The bucket is part of the store rather than a log line
        // (§AR-008-pipeline.2): what nothing claimed is still something
        // somebody is waiting on.
        if let Some(feed) = cache::load_feed(crate::feed::refresh::UNATTRIBUTED)? {
            feeds.push(feed);
        }
    } else {
        let selected: Vec<&String> = if args.project.is_empty() {
            config.projects.keys().collect()
        } else {
            args.project.iter().collect()
        };
        for project in selected {
            known_project(&config, project)?;
            if let Some(feed) = cache::load_feed(project)? {
                feeds.push(feed);
            }
        }
    }

    let now = Utc::now();
    let recent_days = config.defaults.recent_days;
    let mut lines: Vec<(chrono::DateTime<Utc>, String, Value)> = Vec::new();
    for feed in &feeds {
        for item in feed.items() {
            if let Some(kind) = kind_filter {
                if item.kind != kind {
                    continue;
                }
            }
            if args.unread && !cache::is_unread(&seen, &item) {
                continue;
            }
            if !item.is_visible(now, recent_days) {
                continue;
            }
            lines.push((
                item.updated_at,
                render::render_item_line(&item, feed, &seen, &style, true, now),
                serde_json::to_value(&item).unwrap(),
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

/// Exit codes: 3 when nothing could be fetched at all, 4 when some providers
/// were lost and others survived. A partial refresh had been reported as
/// success, which is how a forge stayed uninstalled without anyone noticing —
/// the timer that runs this saw exit 0 every time.
pub fn refresh(args: &RefreshArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let tally = refresh_projects(&config, &args.projects, args.quiet)?;
    if tally.refreshed > 0 && tally.total_failures == tally.refreshed {
        return Ok(ExitCode::from(3));
    }
    if tally.total_failures > 0 || tally.degraded > 0 {
        return Ok(ExitCode::from(4));
    }
    Ok(ExitCode::SUCCESS)
}

/// `ephor failures` — what went wrong under one pull request's red gate
/// (§FS-004-quick-actions.4).
///
/// The pull request is looked up in the cached feed rather than rebuilt from
/// the arguments: the cached item is the one carrying the gate the reader is
/// asking about, and answering for a pull request that is not in their feed
/// would be answering a different question than the one the row asked.
pub fn failures(args: &FailuresArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let project_config = known_project(&config, &args.project)?;
    let block = project_config
        .providers
        .iter()
        .find(|block| block.get("provider").and_then(Value::as_str) == Some(args.source.as_str()))
        .ok_or_else(|| {
            registry_error(format!(
                "Project '{}' has no source named '{}'.",
                args.project, args.source
            ))
        })?;

    let feed = cache::load_feed(&args.project)?.ok_or_else(|| {
        registry_error(format!(
            "No cached feed for '{}' — run `ephor refresh {}` first.",
            args.project, args.project
        ))
    })?;
    let item = feed
        .items()
        .find(|item| {
            item.source == args.source
                && item.repo().as_deref() == Some(args.repo.as_str())
                && item.number().as_deref() == Some(args.number.as_str())
        })
        .ok_or_else(|| {
            registry_error(format!(
                "{}#{} is not in {}'s cached feed from '{}'.",
                args.repo, args.number, args.project, args.source
            ))
        })?;

    let provider = crate::feed::providers::build_provider(block)
        .map_err(|err| EphorError::Command(err.to_string()))?;
    let registry_doc = load_registry_doc()?;
    let mut ctx =
        crate::feed::refresh::build_context(&registry_doc, &args.project, &config.defaults)?;
    // The source's own ceiling, not the refresh default: this is the slow
    // question, and the block that set a longer timeout for its fetches meant
    // it for this one too.
    if let Some(timeout) = crate::feed::refresh::provider_timeout(block) {
        ctx.timeout = timeout;
    }

    let style = Style::detect();
    let gate = crate::feed::gate::Gate::of(&item);
    println!("{}\n", style.bold(&item.title));
    if let Some(gate) = &gate {
        render::render_gate_blockers(gate, &style);
    }

    let found = provider
        .failures(&ctx, &item)
        .map_err(|err| EphorError::Command(err.to_string()))?;
    render::render_failures(found, gate.as_ref(), &style);
    Ok(ExitCode::SUCCESS)
}

pub fn mark_read(args: &MarkReadArgs, all: bool) -> Result<ExitCode> {
    let config = load_config()?;
    let mut seen = cache::load_seen()?;
    let kind_filter = match &args.kind {
        Some(kind) => Some(ItemKind::parse(kind).ok_or_else(|| {
            registry_error(format!(
                "Unknown kind '{kind}' (pr|ci|issue|message|status)."
            ))
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
        let prints: std::collections::HashMap<String, cache::Mark> = feed
            .matters()
            .iter()
            .map(|matter| (matter.key.as_str().to_string(), cache::Mark::of(matter)))
            .collect();
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
            // What the matter looked like when it was read, so that when it
            // comes back the row can say what moved (§FS-007-matters.5).
            seen.insert(
                item.id.clone(),
                prints
                    .get(&item.id)
                    .cloned()
                    .unwrap_or_else(|| cache::Mark::at(item.updated_at)),
            );
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
