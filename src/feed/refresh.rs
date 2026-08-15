//! Refresh orchestration: fetch every configured provider for the selected
//! projects in parallel, isolate failures, merge into the cache.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;

use crate::error::{registry_error, EphorError, Result};
use crate::feed::cache::{load_feed, store_feed, ProjectFeed, ProviderSlot};
use crate::feed::config::{Defaults, ProjectFeedConfig, StatusConfig};
use crate::feed::provider::ProviderContext;
use crate::feed::providers::build_provider;
use crate::feed::reachability;
use crate::paths;
use crate::registry::{self, Workspace};

/// One provider that did not deliver, and whether it is the reader's to fix.
pub struct ProviderFailure {
    pub provider: String,
    pub message: String,
    /// The destination could not be reached at all. Kept apart from every
    /// other failure because it asks nothing of the reader but a working
    /// network, and because the items left on screen are last-good data
    /// rather than this source's current state.
    pub unreachable: bool,
}

impl ProviderFailure {
    /// One warning line: which provider, what happened, and whether this is a
    /// network condition or a configuration to go and fix.
    pub fn describe(&self) -> String {
        if self.unreachable {
            format!("{}: unreachable — {}", self.provider, self.message)
        } else {
            format!("{}: {}", self.provider, self.message)
        }
    }
}

pub struct RefreshOutcome {
    pub item_count: usize,
    pub failures: Vec<ProviderFailure>,
    /// True when every configured provider failed.
    pub total_failure: bool,
}

impl RefreshOutcome {
    pub fn unreachable_count(&self) -> usize {
        self.failures.iter().filter(|f| f.unreachable).count()
    }
}

/// Build the provider context for one project from the registry.
pub fn build_context(
    registry_doc: &Value,
    project_id: &str,
    defaults: &Defaults,
) -> Result<ProviderContext> {
    let project = registry::array_field(registry_doc, "projects")
        .iter()
        .find(|p| registry::id_of(p) == project_id)
        .ok_or_else(|| {
            registry_error(format!(
                "Feed config references unknown project '{project_id}' (not in the registry)."
            ))
        })?;

    let mut tickets: Vec<String> = Vec::new();
    for workspace in registry::derive_project_workspaces(project)? {
        if !workspace.active {
            continue;
        }
        if let Some(ticket) = workspace_ticket(&workspace) {
            tickets.push(ticket);
        }
    }
    tickets.sort();
    tickets.dedup();

    Ok(ProviderContext {
        project_id: project_id.to_string(),
        project_root: paths::resolve_path(registry::str_field(project, "root").unwrap_or("")),
        main_branch: registry::str_field(project, "main_branch")
            .unwrap_or("")
            .to_string(),
        tickets,
        github_user: defaults.github_user.clone(),
        timeout: Duration::from_secs(defaults.provider_timeout_seconds),
        secrets_dir: paths::secrets_dir(),
    })
}

/// A provider block's own timeout ceiling, when it sets one.
///
/// `provider_timeout_seconds` is the shared default, sized for a local `gh`
/// call. A forge reached over a VPN, or one that walks a pull request's
/// comment threads, needs longer — and raising the default for its sake would
/// make every genuinely hung provider take that long to give up.
pub fn provider_timeout(config: &Value) -> Option<Duration> {
    config
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
}

fn workspace_ticket(workspace: &Workspace) -> Option<String> {
    if let Some(ticket) = workspace.vars.get("ticket").and_then(Value::as_str) {
        if !ticket.is_empty() {
            return Some(ticket.to_string());
        }
    }
    let branch = workspace
        .vars
        .get("branch")
        .and_then(Value::as_str)
        .unwrap_or("");
    let extracted = crate::ticket_ids::extract_ticket(branch);
    if extracted.is_empty() {
        None
    } else {
        Some(extracted)
    }
}

/// Fetch all providers of one project concurrently and store the merged feed.
pub fn refresh_project(
    registry_doc: &Value,
    project_id: &str,
    project_config: &ProjectFeedConfig,
    defaults: &Defaults,
) -> Result<RefreshOutcome> {
    let ctx = build_context(registry_doc, project_id, defaults)?;
    let previous = load_feed(project_id)?.unwrap_or_default();

    let results: Mutex<BTreeMap<String, (bool, Option<String>, Vec<crate::feed::model::Item>)>> =
        Mutex::new(BTreeMap::new());

    std::thread::scope(|scope| {
        for provider_config in &project_config.providers {
            let ctx = &ctx;
            let results = &results;
            scope.spawn(move || {
                let raised;
                let ctx = match provider_timeout(provider_config) {
                    Some(timeout) => {
                        raised = ProviderContext {
                            timeout,
                            ..ctx.clone()
                        };
                        &raised
                    }
                    None => ctx,
                };
                let (name, outcome) = match build_provider(provider_config) {
                    Ok(provider) => {
                        let name = provider.name().to_string();
                        if !provider.available(ctx) {
                            let reason = provider.unavailable_reason().unwrap_or_else(|| {
                                "unavailable (missing tool or secret)".to_string()
                            });
                            (name.clone(), (false, Some(reason), Vec::new()))
                        } else {
                            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                provider.fetch(ctx)
                            })) {
                                Ok(Ok(items)) => (name.clone(), (true, None, items)),
                                Ok(Err(err)) => (name.clone(), (false, Some(err.0), Vec::new())),
                                Err(_) => (
                                    name.clone(),
                                    (false, Some("provider panicked".to_string()), Vec::new()),
                                ),
                            }
                        }
                    }
                    Err(err) => {
                        let name = provider_config
                            .get("provider")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string();
                        (name, (false, Some(err.0), Vec::new()))
                    }
                };
                results.lock().unwrap().insert(name, outcome);
            });
        }
    });

    let mut results = results.into_inner().unwrap();

    // Checkout sources read the forest itself, where there is one on disk
    // (§AR-008-pipeline.1). A task store is a project-native thing that
    // exists without ephor, so finding one is a capability rather than an
    // obligation (§FS-006-project-interface.7).
    if let Some(placement) = crate::branches::Placement::load(registry_doc, project_id) {
        let manifest = placement.manifest();
        // The root, and every active branch's workspace: work about a change
        // lives in that change's tree, so a branch-addressable project keeps a
        // store per workspace (§FS-006-project-interface.7).
        let stores: Vec<crate::seams::tasks::Store> = std::iter::once(placement.root.clone())
            .chain(placement.task_stores())
            .flat_map(|root| crate::seams::tasks::find(&root, manifest.as_ref()))
            .collect();
        for store in stores {
            let name = store.kind.name().to_string();
            match crate::seams::tasks::read(&store, project_id) {
                // A store that answered lands its matters, empty answer
                // included: a store with no open tasks has said so, and its
                // slot saying so is what tells last refresh's tasks to go.
                Ok(items) => {
                    let slot = results.entry(name).or_insert((true, None, Vec::new()));
                    slot.0 = true;
                    slot.2.extend(items);
                }
                // A store ephor could not read is reported like any other
                // source that did not answer, rather than read as a store with
                // nothing in it (§FS-001-forge-interface.6).
                Err(why) => {
                    results.insert(name, (false, Some(why), Vec::new()));
                }
            }
        }
    }

    let now = Utc::now();
    let mut feed = ProjectFeed {
        project: project_id.to_string(),
        fetched_at: Some(now),
        model: crate::feed::cache::MODEL,
        providers: BTreeMap::new(),
    };
    let mut failures = Vec::new();
    let mut ok_count = 0usize;

    for (name, (ok, error, items)) in results {
        let slot = if ok {
            ok_count += 1;
            ProviderSlot {
                fetched_at: Some(now),
                ok: true,
                error: None,
                stale: false,
                unreachable: false,
                // Fetch normalization: what a source reported becomes the
                // matter it is about, once, here (§AR-008-pipeline.1).
                matters: items.iter().map(crate::matter::Matter::of_item).collect(),
                cursor: previous
                    .providers
                    .get(&name)
                    .and_then(|slot| slot.cursor.clone()),
            }
        } else {
            let message = error.unwrap_or_else(|| "unknown error".to_string());
            let unreachable = reachability::is_unreachable(&message);
            failures.push(ProviderFailure {
                provider: name.clone(),
                message: message.clone(),
                unreachable,
            });
            let previous_slot = previous.providers.get(&name);
            ProviderSlot {
                fetched_at: previous_slot.and_then(|slot| slot.fetched_at),
                ok: false,
                error: Some(message),
                stale: previous_slot
                    .map(|slot| !slot.matters.is_empty())
                    .unwrap_or(false),
                unreachable,
                matters: previous_slot
                    .map(|slot| slot.matters.clone())
                    .unwrap_or_default(),
                cursor: previous_slot.and_then(|slot| slot.cursor.clone()),
            }
        };
        feed.providers.insert(name, slot);
    }

    let item_count = feed.items().count();
    let total_failure = ok_count == 0 && !project_config.providers.is_empty();
    store_feed(&feed)?;
    Ok(RefreshOutcome {
        item_count,
        failures,
        total_failure,
    })
}

/// One project's refresh, as it lands.
pub struct Landed {
    pub project: String,
    /// Providers of this project that did not deliver, each already qualified
    /// by the project, since the reader's screen holds several
    /// (§FS-001-forge-interface.6).
    pub lost: Vec<String>,
    pub unreachable: usize,
    /// Why the project could not be asked at all — a registry that does not
    /// describe it, a context that would not build. None of its providers ran.
    pub failed: Option<String>,
}

/// A refresh running beneath the interface (§FS-001-forge-interface.7).
///
/// The fetch is the slowest thing ephor does — a source is entitled to a
/// ceiling of its own (§FS-001-forge-interface.2), and minutes is a legitimate
/// answer — while the reader's screen is not the fetch's to hold. So the run
/// lives on a thread of its own and reports one project at a time through a
/// channel the interface drains between key reads.
///
/// Projects are still asked one after another, exactly as a foreground refresh
/// asked them: what moved off the reader's screen is the waiting, not the load
/// on the forge (§GOAL-005-costless).
pub struct BackgroundRefresh {
    landed: Receiver<Landed>,
    /// Held so the thread is reaped when the run ends. Dropping it detaches,
    /// which is what a reader quitting mid-run should do: nothing on their
    /// way out waits for a forge, and every feed is written whole
    /// (§AR-008-pipeline.1) so a half-finished run leaves no torn cache.
    worker: Option<JoinHandle<()>>,
    /// The projects this run asks for, in the order it asks them — so the one
    /// in flight is the one after those that have reported.
    queue: Vec<String>,
    finished: usize,
    lost: Vec<String>,
    unreachable: usize,
    /// The first project that could not be asked at all, and why. Kept apart
    /// from the lost providers: it is a sentence about configuration, and a
    /// name on its own does not say what to go and fix.
    failed: Option<String>,
}

impl BackgroundRefresh {
    /// Start a run and give the screen straight back. `only` narrows it to a
    /// single project, as the detail view does.
    ///
    /// The registry is read here, on the caller's thread: it is a local file,
    /// and a registry that will not parse is an answer the keystroke can have
    /// rather than one that arrives later looking like a refresh that lost
    /// everything.
    pub fn start(config: &StatusConfig, only: Option<&str>) -> Result<Self> {
        let registry_doc = crate::feed::commands::load_registry_doc()?;
        let defaults = config.defaults.clone();
        let jobs: Vec<(String, ProjectFeedConfig)> = config
            .projects
            .iter()
            .filter(|(project, _)| match only {
                Some(one) => project.as_str() == one,
                None => true,
            })
            .map(|(project, project_config)| (project.clone(), project_config.clone()))
            .collect();
        let queue: Vec<String> = jobs.iter().map(|(project, _)| project.clone()).collect();

        let (sender, landed) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("ephor-refresh".to_string())
            .spawn(move || {
                for (project, project_config) in jobs {
                    let report = match refresh_project(
                        &registry_doc,
                        &project,
                        &project_config,
                        &defaults,
                    ) {
                        Ok(outcome) => Landed {
                            lost: outcome
                                .failures
                                .iter()
                                .map(|failure| format!("{project}/{}", failure.provider))
                                .collect(),
                            unreachable: outcome.unreachable_count(),
                            failed: None,
                            project,
                        },
                        Err(err) => Landed {
                            lost: vec![project.clone()],
                            unreachable: 0,
                            failed: Some(err.to_string()),
                            project,
                        },
                    };
                    // A reader who left took the receiver with them. There is
                    // nobody to report to, and the rest of the run is work
                    // nobody asked for any more.
                    if sender.send(report).is_err() {
                        return;
                    }
                }
            })
            .map_err(|err| EphorError::Command(format!("Could not start the refresh: {err}")))?;

        Ok(BackgroundRefresh {
            landed,
            worker: Some(worker),
            queue,
            finished: 0,
            lost: Vec::new(),
            unreachable: 0,
            failed: None,
        })
    }

    /// Everything that has landed since the last call, folded into the run's
    /// tally on the way past. Never blocks: a project still being asked is
    /// simply not in the answer yet.
    pub fn collect(&mut self) -> Vec<Landed> {
        let mut arrived = Vec::new();
        loop {
            match self.landed.try_recv() {
                Ok(report) => {
                    self.finished += 1;
                    self.unreachable += report.unreachable;
                    self.lost.extend(report.lost.iter().cloned());
                    if let (None, Some(why)) = (&self.failed, &report.failed) {
                        self.failed = Some(format!("{}: {why}", report.project));
                    }
                    arrived.push(report);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                // The thread the run was on is gone with projects left to
                // ask. Nothing more is coming, so the run is over — and what
                // never reported is named as lost rather than left as a
                // progress line that never moves again
                // (§FS-001-forge-interface.6).
                Err(mpsc::TryRecvError::Disconnected) => {
                    let unasked: Vec<String> = self.queue[self.finished..].to_vec();
                    if !unasked.is_empty() {
                        self.lost.extend(unasked);
                        if self.failed.is_none() {
                            self.failed = Some("the refresh stopped early".to_string());
                        }
                        self.finished = self.queue.len();
                    }
                    break;
                }
            }
        }
        arrived
    }

    /// Whether every project the run asked for has reported.
    pub fn done(&self) -> bool {
        self.finished >= self.queue.len()
    }

    /// What the header says while the run is in flight: the project being
    /// asked now, and how far through the run it is
    /// (§FS-001-forge-interface.7).
    pub fn progress(&self) -> String {
        match self.queue.get(self.finished) {
            Some(project) => format!(
                "Refreshing {project} ({}/{})…",
                self.finished + 1,
                self.queue.len()
            ),
            None => "Refreshing…".to_string(),
        }
    }

    /// The one line a finished run leaves behind.
    pub fn summary(&self) -> String {
        summarize(&self.lost, self.unreachable, self.failed.as_deref())
    }

    /// Reap the worker once the run is done. It has already sent everything it
    /// will send, so this only collects the thread.
    pub fn finish(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// What a finished run tells the reader it lost (§FS-001-forge-interface.6).
///
/// Named, not counted. "6 provider warnings" is the same sentence whether a
/// forge has been uninstalled for months or a laptop is off the VPN for a
/// minute, and in both cases the sections those providers fill just look
/// empty.
pub fn summarize(lost: &[String], unreachable: usize, failed: Option<&str>) -> String {
    if lost.is_empty() {
        return "Refreshed".to_string();
    }
    // Enough names to act on, then a count for the rest.
    const NAMED: usize = 3;
    let mut shown = lost
        .iter()
        .take(NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if lost.len() > NAMED {
        shown = format!("{shown} +{} more", lost.len() - NAMED);
    }
    let network = if unreachable > 0 {
        format!(" ({unreachable} unreachable — check the network/VPN)")
    } else {
        String::new()
    };
    // A project that could not be asked at all is a configuration to go and
    // fix, and its name on its own does not say what.
    let why = match failed {
        Some(failed) => format!(" — {failed}"),
        None => String::new(),
    };
    format!(
        "Refreshed — NO DATA from {}: {shown}{network}{why}",
        lost.len()
    )
}

/// The reserved feed a matter lands in when nothing claimed it, or when two
/// projects claimed it equally (§FS-008-attribution.4). The bucket is part of
/// the store rather than a log line: a conversation ephor could not place is
/// still a conversation somebody is waiting on.
pub const UNATTRIBUTED: &str = "unattributed";

/// Fetch the sources shared across the site, once, and let the engine place
/// what they report (§AR-008-pipeline.1, §AR-008-pipeline.2).
///
/// This is the half of the pipeline no source owns: a notification stream asks
/// nothing about any one project and answers about all of them, so it is
/// fetched once here and its findings are placed against every project's
/// identity (§AR-003-attribution). What nothing claims is kept, visibly.
pub fn refresh_shared(registry_doc: &Value, config: &StatusConfig) -> Result<RefreshOutcome> {
    if config.sources.is_empty() {
        return Ok(RefreshOutcome {
            item_count: 0,
            failures: Vec::new(),
            total_failure: false,
        });
    }

    // Every configured project's identity, compiled from its row. A project
    // the registry does not describe cannot claim anything, and is skipped
    // rather than guessed at.
    let identities: Vec<crate::attribution::Identity> = config
        .projects
        .keys()
        .filter_map(|project| crate::branches::Placement::load(registry_doc, project))
        .map(|placement: crate::branches::Placement| placement.identity())
        .collect();

    // One context for the whole site: no project, because that is the point.
    let ctx = ProviderContext {
        project_id: String::new(),
        project_root: paths::state_dir(),
        main_branch: String::new(),
        tickets: Vec::new(),
        github_user: config.defaults.github_user.clone(),
        timeout: Duration::from_secs(config.defaults.provider_timeout_seconds),
        secrets_dir: paths::secrets_dir(),
    };

    let results: Mutex<BTreeMap<String, (bool, Option<String>, Vec<crate::feed::model::Item>)>> =
        Mutex::new(BTreeMap::new());
    std::thread::scope(|scope| {
        for source in &config.sources {
            let ctx = &ctx;
            let results = &results;
            scope.spawn(move || {
                let raised;
                let ctx = match provider_timeout(source) {
                    Some(timeout) => {
                        raised = ProviderContext {
                            timeout,
                            ..ctx.clone()
                        };
                        &raised
                    }
                    None => ctx,
                };
                let (name, outcome) = fetch_one(source, ctx);
                results.lock().unwrap().insert(name, outcome);
            });
        }
    });

    let results = results.into_inner().unwrap();
    let now = Utc::now();
    let mut failures = Vec::new();
    let mut ok_count = 0usize;
    let mut item_count = 0usize;
    // Where each source's findings ended up: project id (or the bucket) to the
    // matters placed there.
    let mut placed: BTreeMap<String, BTreeMap<String, Vec<crate::matter::Matter>>> =
        BTreeMap::new();

    for (name, (ok, error, items)) in results {
        if !ok {
            let message = error.unwrap_or_else(|| "unknown error".to_string());
            failures.push(ProviderFailure {
                unreachable: reachability::is_unreachable(&message),
                provider: name,
                message,
            });
            continue;
        }
        ok_count += 1;
        item_count += items.len();
        for item in items {
            let mut matter = crate::matter::Matter::of_item(&item);
            let evidence = crate::matter::evidence_of(&item);
            matter.placement = match crate::attribution::place(&evidence, &identities) {
                // How firmly it was claimed travels with it: what may be done
                // with a placement depends on how it was reached
                // (§FS-008-attribution.3).
                crate::attribution::Placed::On { project, how } => {
                    crate::matter::Placement::claimed(project, how)
                }
                crate::attribution::Placed::Ambiguous { candidates } => {
                    crate::matter::Placement::Unattributed { candidates }
                }
                crate::attribution::Placed::Nothing => crate::matter::Placement::Unattributed {
                    candidates: Vec::new(),
                },
            };
            let home = matter
                .placement
                .project()
                .unwrap_or(UNATTRIBUTED)
                .to_string();
            placed
                .entry(home)
                .or_default()
                .entry(name.clone())
                .or_default()
                .push(matter);
        }
    }

    // Every project a shared source could have spoken about gets its slot
    // rewritten, including to empty: a source that used to place something
    // here and no longer does has to be able to say so.
    let homes: Vec<String> = config
        .projects
        .keys()
        .cloned()
        .chain(std::iter::once(UNATTRIBUTED.to_string()))
        .collect();
    let names: Vec<String> = config
        .sources
        .iter()
        .filter_map(|source: &Value| source.get("provider").and_then(Value::as_str))
        .map(String::from)
        .collect();
    for home in homes {
        let mut feed = load_feed(&home)?.unwrap_or(ProjectFeed {
            project: home.clone(),
            ..ProjectFeed::default()
        });
        let mine = placed.remove(&home).unwrap_or_default();
        let mut touched = false;
        for name in &names {
            let matters = mine.get(name).cloned().unwrap_or_default();
            let failed = failures.iter().any(|failure| &failure.provider == name);
            if failed {
                // Last-good data waits out a failure here exactly as it does
                // for a per-project source.
                if let Some(slot) = feed.providers.get_mut(name) {
                    slot.ok = false;
                    slot.stale = !slot.matters.is_empty();
                    touched = true;
                }
                continue;
            }
            feed.providers.insert(
                name.clone(),
                ProviderSlot {
                    fetched_at: Some(now),
                    ok: true,
                    error: None,
                    stale: false,
                    unreachable: false,
                    matters,
                    cursor: feed
                        .providers
                        .get(name)
                        .and_then(|slot| slot.cursor.clone()),
                },
            );
            touched = true;
        }
        if touched {
            feed.fetched_at = Some(now);
            feed.model = crate::feed::cache::MODEL;
            store_feed(&feed)?;
        }
    }

    Ok(RefreshOutcome {
        item_count,
        failures,
        total_failure: ok_count == 0,
    })
}

/// Build one source and ask it, reporting a failure as data rather than
/// letting it end the refresh (§FS-001-forge-interface.6).
fn fetch_one(
    config: &Value,
    ctx: &ProviderContext,
) -> (
    String,
    (bool, Option<String>, Vec<crate::feed::model::Item>),
) {
    match build_provider(config) {
        Ok(provider) => {
            let name = provider.name().to_string();
            if !provider.available(ctx) {
                let reason = provider
                    .unavailable_reason()
                    .unwrap_or_else(|| "unavailable (missing tool or secret)".to_string());
                return (name, (false, Some(reason), Vec::new()));
            }
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| provider.fetch(ctx))) {
                Ok(Ok(items)) => (name, (true, None, items)),
                Ok(Err(err)) => (name, (false, Some(err.0), Vec::new())),
                Err(_) => (
                    name,
                    (false, Some("provider panicked".to_string()), Vec::new()),
                ),
            }
        }
        Err(err) => (
            config
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            (false, Some(err.0), Vec::new()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_run_says_only_that_it_refreshed() {
        assert_eq!(summarize(&[], 0, None), "Refreshed");
    }

    #[test]
    fn what_was_lost_is_named_and_the_rest_is_counted() {
        let lost: Vec<String> = ["a/one", "b/two", "c/three", "d/four", "e/five"]
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(
            summarize(&lost, 0, None),
            "Refreshed — NO DATA from 5: a/one, b/two, c/three +2 more"
        );
    }

    /// A network condition asks nothing of the reader but a working network,
    /// so it is said in those words (§FS-001-forge-interface.6).
    #[test]
    fn unreachable_is_its_own_sentence() {
        let lost = vec!["graal/gdev".to_string()];
        assert!(summarize(&lost, 1, None).ends_with("(1 unreachable — check the network/VPN)"));
    }

    /// A project that could not be asked at all carries its reason: the name
    /// alone does not say what to go and fix.
    #[test]
    fn a_project_that_could_not_be_asked_says_why() {
        let lost = vec!["widget".to_string()];
        assert_eq!(
            summarize(&lost, 0, Some("widget: not in the registry")),
            "Refreshed — NO DATA from 1: widget — widget: not in the registry"
        );
    }
}
