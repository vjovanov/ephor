//! `ephor status | feed | refresh | mark-read` command implementations.

use std::collections::BTreeSet;
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
use crate::scope::Projects;

/// What the registry says, read once per run and remembered for as long as
/// the file has not moved under us.
///
/// Several things want it in one invocation — the session's placements
/// (§AR-004-forest.3) and the dispatcher's own copy among them — and each read
/// is a file, a parse and a whole-document schema validation. Reading it four
/// times to answer one question is four chances to answer from four different
/// registries, which is the drift §AR-009-surfaces.2 puts one session below
/// both surfaces to stop.
///
/// Keyed on where the file is and on *what it says*, never on the path alone
/// and never on when the filesystem thinks it last said it: a command that
/// rewrites the registry and reads it back in the same process gets what it
/// wrote.
///
/// The stamp used to be `(path, mtime, len)`, which cannot keep that promise.
/// A modification time is the filesystem's opinion at whatever resolution it
/// keeps, and two writes of the same length inside one tick share it: measured
/// here, 152 of 200 immediate same-length rewrites came back with an identical
/// `st_mtime_ns`. Nothing in the tree rewrites the registry in-process today,
/// so nothing was wrong — but a guard that states an invariant it does not hold
/// is worse than one that states a weaker one, because the next caller reads
/// the comment rather than the key. The bytes are hashed instead, and parsed
/// from the same read, so the key and the document can never come from two
/// different registries.
pub(crate) fn load_registry_doc() -> Result<Value> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::Mutex;

    type Stamp = (std::path::PathBuf, u64);
    static SEEN: Mutex<Option<(Stamp, Value)>> = Mutex::new(None);

    let registry_path = crate::paths::default_registry_path();
    // Unreadable is not a cache key: the error below says why, in its own
    // words, every time it is asked.
    let text = std::fs::read_to_string(&registry_path)
        .map_err(|err| registry_error(format!("Cannot read {}: {err}", registry_path.display())))?;
    let stamp: Stamp = {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        (registry_path.clone(), hasher.finish())
    };
    // A poisoned lock is a panic in another thread while it held nothing but
    // a cache; the answer is still to read the registry.
    if let Ok(seen) = SEEN.lock() {
        if let Some((was, document)) = seen.as_ref() {
            if was == &stamp {
                return Ok(document.clone());
            }
        }
    }
    let schema: Value = serde_json::from_str(registry::EMBEDDED_SCHEMA)
        .map_err(|err| registry_error(format!("Invalid embedded schema: {err}")))?;
    let document = registry::parse_registry(&text, &registry_path, &schema)?;
    if let Ok(mut seen) = SEEN.lock() {
        *seen = Some((stamp, document.clone()));
    }
    Ok(document)
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
    let mut per_project: Vec<serde_json::Value> = Vec::new();
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
        per_project.push(serde_json::json!({
            "project": project,
            "items": outcome.item_count,
            "lost": outcome.total_failure,
            "failures": outcome
                .failures
                .iter()
                .map(|failure| failure.describe())
                .collect::<Vec<_>>(),
        }));
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
    if !config.sources.is_empty() {
        per_project.push(serde_json::json!({
            "project": "sources",
            "items": shared.item_count,
            "lost": shared.total_failure,
            "failures": shared
                .failures
                .iter()
                .map(|failure| failure.describe())
                .collect::<Vec<_>>(),
        }));
    }
    if !quiet && shared.item_count > 0 {
        println!("sources: {} items placed", shared.item_count);
    }

    Ok(RefreshTally {
        refreshed,
        total_failures,
        degraded,
        projects: per_project,
    })
}

/// How a whole refresh went, across every selected project.
struct RefreshTally {
    refreshed: usize,
    /// Projects where every provider failed.
    total_failures: usize,
    /// Projects that lost some providers but not all.
    degraded: usize,
    /// Per project: what came back, and what did not. A provider that did not
    /// deliver is reported rather than counted away — its section of the feed
    /// is last-good data or nothing at all, and either reads exactly like "you
    /// have no work here" (§FS-001-forge-interface.6).
    projects: Vec<serde_json::Value>,
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

/// `ephor status` — one project's feed, or a row per watched project.
///
/// `scope` is what `--workspace`, `--tag` and `--org` selected: the summary
/// walks the projects it holds rather than every configured one, and a project
/// named beside a selector that excludes it is refused by name
/// (§FS-011-command-line.9).
pub fn status(args: &StatusArgs, scope: &Projects) -> Result<ExitCode> {
    let config = load_config()?;
    let seen = cache::load_seen()?;
    let style = Style::detect();

    let feeds: Vec<ProjectFeed> = match &args.project {
        Some(project) => {
            scope.admit(project)?;
            known_project(&config, project)?;
            vec![ensure_fresh(&config, project, args)?]
        }
        None => {
            let mut feeds = Vec::new();
            for project in scope.over(config.projects.keys())? {
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

/// `ephor feed` — the flat stream across the watched projects
/// (§FS-011-command-line.9 for what `scope` narrows it to).
pub fn feed(args: &FeedArgs, scope: &Projects) -> Result<ExitCode> {
    let config = load_config()?;
    let seen = cache::load_seen()?;
    let style = Style::detect();
    let kind_filter = match &args.kind {
        Some(kind) => Some(ItemKind::parse(kind).ok_or_else(|| {
            registry_error(format!(
                "Unknown kind '{kind}' (pr|ci|issue|task|message|status)."
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
            scope.over(config.projects.keys())?
        } else {
            for project in &args.project {
                scope.admit(project)?;
            }
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
pub fn refresh(args: &RefreshArgs, scope: &Projects) -> Result<ExitCode> {
    let config = load_config()?;
    // The compatible positional spelling and the repeatable named spelling
    // are one direct-project set. Keep positional inputs first and preserve
    // each spelling's order, so existing callers keep their output order while
    // a repeated name never runs its providers twice (§FS-011-command-line.9).
    let mut seen = BTreeSet::new();
    let mut projects = Vec::with_capacity(args.projects.len() + args.project.len());
    for project in args.projects.iter().chain(&args.project) {
        if seen.insert(project.as_str()) {
            projects.push(project.clone());
        }
    }
    // The selectors narrow which projects are fetched and nothing else: the
    // shared sources are fetched once for the whole site and placed by the
    // engine (§AR-008-pipeline.1), so the configuration they are placed
    // against stays the whole one — a scoped fetch must not file another
    // project's conversation under what nothing claimed
    // (§FS-011-command-line.9).
    let selected = scope.narrow(&projects, config.projects.keys())?;
    // Under `--json` the per-project lines would land on standard output
    // beside the reading (§FS-011-command-line.7), so they are withheld and
    // the whole tally is printed at the end instead.
    let tally = refresh_projects(&config, &selected, args.quiet || args.json)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "refreshed": tally.refreshed,
                "degraded": tally.degraded,
                "lost": tally.total_failures,
                "projects": tally.projects,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
    }
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
/// The provider that reported one pull request, the matter itself, and the
/// context to ask in. Both questions a reader asks of a gate — what failed
/// (§FS-004-quick-actions.4) and run it again (§FS-004-quick-actions.9) — take
/// the same four coordinates and resolve them the same way, so they resolve
/// them here: the source that produced the item is the one asked, the matter
/// comes out of ephor's own cache rather than a fresh fetch, and the source's
/// own timeout is the ceiling, because both of these are its slow calls.
fn about_the_gate(
    config: &StatusConfig,
    project: &str,
    source: &str,
    repo: &str,
    number: &str,
) -> Result<(
    Box<dyn crate::feed::provider::Provider>,
    crate::feed::model::Item,
    crate::feed::provider::ProviderContext,
)> {
    let project_config = known_project(config, project)?;
    let block = project_config
        .providers
        .iter()
        .find(|block| block.get("provider").and_then(Value::as_str) == Some(source))
        .ok_or_else(|| {
            registry_error(format!(
                "Project '{project}' has no source named '{source}'."
            ))
        })?;

    let feed = cache::load_feed(project)?.ok_or_else(|| {
        registry_error(format!(
            "No cached feed for '{project}' — run `ephor refresh {project}` first."
        ))
    })?;
    let item = feed
        .items()
        .find(|item| {
            item.source == source
                && item.repo().as_deref() == Some(repo)
                && item.number().as_deref() == Some(number)
        })
        .ok_or_else(|| {
            registry_error(format!(
                "{repo}#{number} is not in {project}'s cached feed from '{source}'."
            ))
        })?;

    let provider = crate::feed::providers::build_provider(block)
        .map_err(|err| EphorError::Command(err.to_string()))?;
    let registry_doc = load_registry_doc()?;
    let mut ctx = crate::feed::refresh::build_context(&registry_doc, project, &config.defaults)?;
    // The source's own ceiling, not the refresh default: the block that set a
    // longer timeout for its fetches meant it for these too.
    if let Some(timeout) = crate::feed::refresh::provider_timeout(block) {
        ctx.timeout = timeout;
    }
    Ok((provider, item, ctx))
}

/// The four coordinates a quick action passes, or the feed id a reader has
/// (§FS-011-command-line.6). One resolution for both, so the two spellings
/// cannot answer about different pull requests.
fn named_gate(
    config: &StatusConfig,
    item: &Option<String>,
    project: &Option<String>,
    source: &Option<String>,
    repo: &Option<String>,
    number: &Option<String>,
) -> Result<(
    Box<dyn crate::feed::provider::Provider>,
    crate::feed::model::Item,
    crate::feed::provider::ProviderContext,
)> {
    if let Some(id) = item {
        let (project, found) = config
            .projects
            .keys()
            .filter_map(|project| {
                let feed = cache::load_feed(project).ok()??;
                let item = feed.items().find(|item| &item.id == id)?;
                Some((project.clone(), item))
            })
            .next()
            .ok_or_else(|| {
                registry_error(format!(
                    "'{id}' is not in any cached feed — `ephor feed --kind pr` lists what is."
                ))
            })?;
        let (repo, number) = (found.repo(), found.number());
        let (Some(repo), Some(number)) = (repo, number) else {
            return Err(registry_error(format!(
                "'{id}' does not name a repository and a number, so its gate cannot be asked about."
            )));
        };
        return about_the_gate(config, &project, &found.source, &repo, &number);
    }
    let missing = || {
        registry_error(
            "Name the pull request: --item ID, or --project, --source, --repo and --number."
                .to_string(),
        )
    };
    about_the_gate(
        config,
        project.as_deref().ok_or_else(missing)?,
        source.as_deref().ok_or_else(missing)?,
        repo.as_deref().ok_or_else(missing)?,
        number.as_deref().ok_or_else(missing)?,
    )
}

pub fn failures(args: &FailuresArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let (provider, item, ctx) = named_gate(
        &config,
        &args.item,
        &args.project,
        &args.source,
        &args.repo,
        &args.number,
    )?;
    let gate = crate::feed::gate::Gate::of(&item);
    let found = provider
        .failures(&ctx, &item)
        .map_err(|err| EphorError::Command(err.to_string()))?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "item": item.id,
                "title": item.title,
                "url": item.url,
                "gate": gate,
                "failures": found,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    let style = Style::detect();
    println!("{}\n", style.bold(&item.title));
    if let Some(gate) = &gate {
        render::render_gate_blockers(gate, &style);
    }
    render::render_failures(found, gate.as_ref(), &style);
    Ok(ExitCode::SUCCESS)
}

/// `ephor restart` (§FS-004-quick-actions.9): ask one pull request's gate to
/// run again, at the scope stated.
///
/// The same shape `failures` has, and for the same reason: the coordinates
/// come from ephor's own cached feed, the source that reported the item is the
/// one asked, and the provider decides how its forge re-runs a gate. What it
/// answers is printed rather than summarised away — a gate is minutes from
/// saying anything itself, so "asked 12 runs" and "asked nothing" are the
/// difference between a restart that happened and one that did not.
pub fn restart(args: &crate::cli::RestartArgs) -> Result<ExitCode> {
    let scope = crate::feed::gate::Scope::parse(&args.scope).ok_or_else(|| {
        registry_error(format!(
            "Unknown scope '{}': expected 'failed' or 'all'.",
            args.scope
        ))
    })?;
    let config = load_config()?;
    let (provider, item, ctx) = named_gate(
        &config,
        &args.item,
        &args.project,
        &args.source,
        &args.repo,
        &args.number,
    )?;
    // Neutral about how much that is: on a forge with per-job reruns it is the
    // failed jobs, and on one that starts its gate as a whole it is everything
    // downstream of them (§FS-006-project-interface.6). The row in the
    // interface says which; this says what was asked for.
    let asked = match scope {
        crate::feed::gate::Scope::Failed => "what is not green",
        crate::feed::gate::Scope::All => "the whole gate",
    };
    let style = Style::detect();
    if !args.json {
        println!("{}\n", style.bold(&item.title));
        println!("restarting {asked} on {}", item.id);
    }
    let restarted = provider
        .restart(&ctx, &item, scope)
        .map_err(|err| EphorError::Command(err.to_string()))?;
    // What it answered, printed rather than summarised away — a gate is
    // minutes from saying anything itself, so "asked 12 runs" and "asked
    // nothing" are the difference between a restart that happened and one
    // that did not.
    if args.json {
        let mut reading = serde_json::json!({
            "item": item.id,
            "title": item.title,
            "scope": scope.name(),
            // The fact a loop reads: whether anything at all was asked to run
            // again. A forge that counted zero asked nothing; one that counted
            // any asked; and one that took the whole-gate start without
            // counting *did* ask, which is why `None` is not zero here
            // (§FS-001-forge-interface.1). The prose phrase this used to carry
            // said what `scope` already says and did not match the published
            // shape, so a program validating the reading rejected every
            // restart ephor has ever printed.
            "asked": restarted.asked != Some(0),
            "says": restarted.says(),
            "skipped": restarted.skipped,
        });
        if let Some(reading) = reading.as_object_mut() {
            // Absent rather than null where the forge does not count: reporting
            // it as a number would read as a restart that found nothing to do,
            // which is the one thing this answer may not be confused with.
            if let Some(count) = restarted.asked {
                reading.insert("asked_count".to_string(), serde_json::json!(count));
            }
            if let Some(note) = &restarted.note {
                reading.insert("note".to_string(), serde_json::json!(note));
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&reading).unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!("\n{}", restarted.says());
    for skipped in &restarted.skipped {
        println!("· {skipped}");
    }
    Ok(ExitCode::SUCCESS)
}

/// `ephor mark-read` — what has been read, per version rather than per item
/// (§FS-007-matters.5).
///
/// The sweep across every watched project is `mark-read`'s own `--all`, after
/// the verb, rather than the global one it used to read: that flag says which
/// *branch entries* a managed-workspace verb walks, and one flag meaning two
/// things is exactly what a caller cannot tell from the output
/// (§FS-011-command-line.9). The project selectors narrow the sweep like
/// every other project selection.
pub fn mark_read(args: &MarkReadArgs, scope: &Projects) -> Result<ExitCode> {
    let config = load_config()?;
    let mut seen = cache::load_seen()?;
    let kind_filter = match &args.kind {
        Some(kind) => Some(ItemKind::parse(kind).ok_or_else(|| {
            registry_error(format!(
                "Unknown kind '{kind}' (pr|ci|issue|task|message|status)."
            ))
        })?),
        None => None,
    };

    let selected: Vec<String> = match (&args.project, args.all) {
        (Some(project), _) => {
            scope.admit(project)?;
            vec![project.clone()]
        }
        (None, true) => scope
            .over(config.projects.keys())?
            .into_iter()
            .cloned()
            .collect(),
        (None, false) if args.id.is_some() => scope
            .over(config.projects.keys())?
            .into_iter()
            .cloned()
            .collect(),
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
    // Only where the sweep really covered every watched project: a run
    // narrowed by `--org` read some of the feeds, and pruning from those
    // would forget that another organization's items were ever read
    // (§FS-011-command-line.9).
    if args.id.is_none() && kind_filter.is_none() && config.projects.len() == selected.len() {
        seen.retain(|id, _| live_ids.contains(id));
    }

    cache::store_seen(&seen)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "marked": marked,
                "projects": selected,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!("Marked {marked} items as read.");
    Ok(ExitCode::SUCCESS)
}
