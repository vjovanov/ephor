//! Refresh orchestration: fetch every configured provider for the selected
//! projects in parallel, isolate failures, merge into the cache.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;

use crate::error::{registry_error, Result};
use crate::feed::cache::{load_feed, store_feed, ProjectFeed, ProviderSlot};
use crate::feed::config::{Defaults, ProjectFeedConfig};
use crate::feed::provider::ProviderContext;
use crate::feed::providers::build_provider;
use crate::paths;
use crate::registry::{self, Workspace};

pub struct RefreshOutcome {
    pub item_count: usize,
    pub errors: Vec<String>,
    /// True when every configured provider failed.
    pub total_failure: bool,
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
    let extracted = registry::extract_ticket(branch);
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
                let (name, outcome) = match build_provider(provider_config) {
                    Ok(provider) => {
                        let name = provider.name().to_string();
                        if !provider.available(ctx) {
                            (
                                name.clone(),
                                (
                                    false,
                                    Some("unavailable (missing tool or secret)".to_string()),
                                    Vec::new(),
                                ),
                            )
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

    let results = results.into_inner().unwrap();
    let now = Utc::now();
    let mut feed = ProjectFeed {
        project: project_id.to_string(),
        fetched_at: Some(now),
        providers: BTreeMap::new(),
    };
    let mut errors = Vec::new();
    let mut ok_count = 0usize;

    for (name, (ok, error, items)) in results {
        let slot = if ok {
            ok_count += 1;
            ProviderSlot {
                fetched_at: Some(now),
                ok: true,
                error: None,
                stale: false,
                items,
                cursor: previous
                    .providers
                    .get(&name)
                    .and_then(|slot| slot.cursor.clone()),
            }
        } else {
            let message = error.unwrap_or_else(|| "unknown error".to_string());
            errors.push(format!("{name}: {message}"));
            let previous_slot = previous.providers.get(&name);
            ProviderSlot {
                fetched_at: previous_slot.and_then(|slot| slot.fetched_at),
                ok: false,
                error: Some(message),
                stale: previous_slot
                    .map(|slot| !slot.items.is_empty())
                    .unwrap_or(false),
                items: previous_slot
                    .map(|slot| slot.items.clone())
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
        errors,
        total_failure,
    })
}
