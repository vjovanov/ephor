//! Interactive two-screen browser over the status feed.
//!
//! - **Navigator** (`navigator.rs`): everything organized per organization,
//!   then per project, then per type (Status, Pull Requests, CI, Messages),
//!   then per branch. Three modes toggled with Tab / Enter: Stream (full
//!   tree), Projects (org-grouped summary), Detail (one project plus its
//!   registry branches).
//! - **Thread** (`thread.rs`): full-screen visualization of one item's
//!   conversation — per-message selection, reactions, and a reaction picker.
//!
//! Screens never mutate shared state directly: key handlers return an
//! [`Action`] and the shell here executes it. "Done" is mark-read: an item
//! resurfaces when it changes again.

mod actions;
mod gate;
mod navigator;
mod prompt;
mod thread;
mod work;

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use chrono::{DateTime, Utc};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::DefaultTerminal;
use serde_json::Value;

use crate::branches::{Checkout, Placement};
use crate::capabilities::{Bindings, CapabilitySet, Rung};
use crate::error::{EphorError, Result};
use crate::feed::cache::{self, ProjectFeed, Seen};
use crate::feed::config::{load_config, ActionConfig, CheckoutConfig, StatusConfig};
use crate::feed::model::{Item, ItemKind};
use crate::feed::react::{self, ReactTarget};
use crate::feed::reply::{self, ReplyTarget};
use crate::feed::task::Task;
use crate::forest::Staleness;
use crate::paths;
use crate::registry;
use crate::seams::dossier;
use crate::seams::summons::{self, Outcome, Place, Site};

use actions::{ActionMenu, MenuOutcome};
use gate::GateScreen;
use navigator::NavigatorState;
use prompt::{Asking, Prompt, PromptOutcome};
use thread::ThreadScreen;
use work::WorkScreen;

#[derive(Clone)]
pub(crate) struct OrgInfo {
    pub id: String,
    pub name: String,
    pub root: Option<String>,
}

pub(crate) use crate::branches::BranchInfo;

/// What an item's work is doing, condensed to what fits on its row
/// (§FS-005-dispatch.4). Recomputed from the plans whenever anything could
/// have changed them — never remembered across a change.
#[derive(Clone)]
pub(crate) struct WorkBadge {
    pub text: String,
    pub open: bool,
    pub stale: bool,
}

/// Shared data both screens read. Mutations go through the shell so screens
/// stay pure key-to-[`Action`] translators.
pub(crate) struct Ctx {
    pub feeds: Vec<ProjectFeed>,
    pub seen: Seen,
    /// Feed-configured projects, ordered by organization (registry order).
    pub projects: Vec<String>,
    pub orgs: Vec<OrgInfo>,
    pub project_org: BTreeMap<String, String>,
    /// Where each project is, how a branch becomes a workspace, which branches
    /// the registry knows, and what its forest is declared to hold — the one
    /// answer the whole program shares (§AR-004-forest.3).
    pub placements: BTreeMap<String, Placement>,
    /// How far each checked-out branch trails main, per repository and summed
    /// (§AR-004-forest.1); computed at load and refresh time.
    pub behind: BTreeMap<(String, String), Staleness>,
    /// What each project can do (§AR-005-capabilities). Resolved at load and
    /// whenever the world may have moved, and consulted by everything that
    /// offers, gates, or refuses — nothing here runs its own check.
    pub capabilities: BTreeMap<String, CapabilitySet>,
    /// Why a matter is back, keyed by matter key — shown on the row so a
    /// reappearance never sends the reader to re-read everything
    /// (§FS-007-matters.5).
    pub resurfacing: BTreeMap<String, String>,
    /// Conversations attribution could not place, and ones two projects
    /// claimed equally. Shown rather than dropped: a guess that lands wrong
    /// amends someone's matter silently (§FS-008-attribution.4).
    pub unattributed: Vec<Item>,
    /// Item actions: global, plus per-project extras.
    pub actions: Vec<ActionConfig>,
    pub project_actions: BTreeMap<String, Vec<ActionConfig>>,
    /// Each project's provider blocks, kept so the source that produced an
    /// item can be asked what it offers on it (§FS-004-quick-actions.1).
    pub provider_blocks: BTreeMap<String, Vec<Value>>,
    /// Per-project branch checkout commands.
    pub checkouts: BTreeMap<String, CheckoutConfig>,
    /// How long finished work stays under Recent (§FS-003-feed-categories.3).
    pub recent_days: u64,
    pub unread_only: bool,
    /// Per item id, what has been handed to the runtime about it.
    pub work: BTreeMap<String, WorkBadge>,
}

impl Ctx {
    pub fn feed(&self, project: &str) -> Option<&ProjectFeed> {
        self.feeds.iter().find(|feed| feed.project == project)
    }

    /// The item's menu, in provenance order (§FS-006-project-interface.9):
    /// what its source offers on it unasked (§FS-004-quick-actions.3), then
    /// what the project offers of itself, then the person's own — an id
    /// repeated later replacing the entry where it already sits.
    pub fn actions_for(&self, item: &Item) -> Vec<ActionConfig> {
        let project = self
            .project_actions
            .get(&item.project)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let blocks = self
            .provider_blocks
            .get(&item.project)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let facts = crate::work::recipe::Facts {
            behind: self.item_behind(item),
        };
        let mut recognized = crate::feed::providers::quick_actions(blocks, item);
        // ephor's own quick action, offered because of what is on disk rather
        // than because a source said something (§FS-004-quick-actions.6).
        if let (Some(main_branch), Some(behind)) = (
            self.main_branch(&item.project),
            facts.behind.filter(|behind| *behind > 0),
        ) {
            recognized.push(actions::rebase_action(main_branch, behind));
        }
        actions::merge(vec![
            recognized,
            self.offers(item, &facts),
            actions::applicable(&self.actions, project, item, &facts),
        ])
    }

    /// What the project says it can do on this item, where it speaks and the
    /// row lets it be read (§FS-006-project-interface.2,
    /// §FS-006-project-interface.9). A manifest trusted for descriptions only
    /// carries no offers to begin with, so trust needs no second check here.
    fn offers(&self, item: &Item, facts: &crate::work::recipe::Facts) -> Vec<ActionConfig> {
        self.placements
            .get(&item.project)
            .and_then(crate::branches::Placement::manifest)
            .map(|manifest| {
                manifest
                    .offers
                    .iter()
                    .map(crate::manifest::Offer::action)
                    .filter(|offer| offer.matches(item, facts))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Commits the item's own checkout trails the project's main branch,
    /// summed across its forest (§AR-004-forest.1). None where there is
    /// nothing on disk to measure: not a pull request, no branch, or a
    /// workspace that was never checked out.
    pub fn item_behind(&self, item: &Item) -> Option<u64> {
        if item.kind != ItemKind::Pr {
            return None;
        }
        let placement = self.placements.get(&item.project)?;
        placement.main_branch.as_ref()?;
        let (name, _) = self.effective_branch(item);
        let workspace = placement
            .workspace_for(&name?)
            // A project without branch workspaces works in its root.
            .unwrap_or_else(|| placement.root.clone());
        if !workspace.is_dir() {
            return None;
        }
        placement.forest(&workspace).staleness().total()
    }

    /// One matter by its key, across every project's feed — for the moments
    /// that need the model rather than the row rendered from it.
    pub fn matter(&self, key: &str) -> Option<crate::matter::Matter> {
        self.feeds.iter().find_map(|feed| {
            feed.matters()
                .into_iter()
                .find(|matter| matter.key.as_str() == key)
        })
    }

    /// Why each matter is back in front of the reader, keyed by matter key
    /// (§FS-007-matters.5). Recomputed with the feed.
    pub fn recompute_resurfacing(&mut self) {
        let mut reasons = BTreeMap::new();
        for feed in &self.feeds {
            for matter in feed.matters() {
                if let Some(reason) = cache::resurfacing(&self.seen, &matter) {
                    reasons.insert(matter.key.as_str().to_string(), reason);
                }
            }
        }
        self.resurfacing = reasons;
    }

    /// What a project can do. A project the registry does not describe holds
    /// nothing, and says so rather than being absent.
    pub fn can(&self, project: &str) -> CapabilitySet {
        self.capabilities
            .get(project)
            .cloned()
            .unwrap_or_else(|| CapabilitySet::unknown(project))
    }

    /// Re-resolve every project's ladder. Cheap by construction — stat calls,
    /// config lookups, one walk of PATH (§AR-005-capabilities.1) — so it runs
    /// again whenever a refresh or a checkout may have moved the world.
    pub fn recompute_capabilities(&mut self) {
        let mut table = BTreeMap::new();
        for project in &self.projects {
            let gate_reported = self
                .feed(project)
                .is_some_and(crate::feed::cache::ProjectFeed::reports_a_gate);
            // What the project says about itself, read once per project here
            // rather than by each rung (§FS-006-project-interface.2).
            let manifest = self
                .placements
                .get(project)
                .and_then(crate::branches::Placement::manifest);
            let bindings = Bindings {
                sources: self.provider_blocks.get(project).map(Vec::len).unwrap_or(0),
                // What answered at the last refresh, out of the cache the
                // refresh wrote (§FS-006-project-interface.10). No cache yet
                // is nothing answered, which is the honest reading before the
                // first refresh has run.
                answering: self
                    .feed(project)
                    .and_then(crate::feed::cache::ProjectFeed::answering),
                checkout: self
                    .checkouts
                    .get(project)
                    .map(|checkout| checkout.command.as_str()),
                runner: Some(crate::work::runtime::RUNNER),
                gate_reported,
                manifest: manifest.as_ref(),
            };
            table.insert(
                project.clone(),
                CapabilitySet::resolve(project, self.placements.get(project), &bindings),
            );
        }
        self.capabilities = table;
    }

    /// A project's provider blocks, for a write that has to go back through
    /// the source that reported what it acts on.
    pub fn blocks_for(&self, project: &str) -> Vec<Value> {
        self.provider_blocks
            .get(project)
            .cloned()
            .unwrap_or_default()
    }

    pub fn org_projects(&self, org_id: &str) -> Vec<String> {
        self.projects
            .iter()
            .filter(|project| {
                self.project_org
                    .get(*project)
                    .map(String::as_str)
                    .unwrap_or("")
                    == org_id
            })
            .cloned()
            .collect()
    }

    pub fn org_label(&self, org: &OrgInfo) -> String {
        match &org.root {
            Some(root) => format!("{} — {}", org.name, display_root(root)),
            None => org.name.clone(),
        }
    }

    pub fn unread_stats(&self, project: &str) -> (usize, usize, usize) {
        let Some(feed) = self.feed(project) else {
            return (0, 0, 0);
        };
        let now = Utc::now();
        let visible = || {
            feed.items()
                .filter(|item| item.is_visible(now, self.recent_days))
        };
        let total = visible().count();
        let unread = visible()
            .filter(|item| cache::is_unread(&self.seen, item))
            .count();
        let respond = visible()
            .filter(|item| item.needs_response && cache::is_unread(&self.seen, item))
            .count();
        (total, unread, respond)
    }

    /// One project's placement, or nothing where the registry does not
    /// describe it.
    pub fn placement(&self, project: &str) -> Option<&Placement> {
        self.placements.get(project)
    }

    /// Where a project is checked out.
    pub fn root(&self, project: &str) -> Option<&Path> {
        self.placements
            .get(project)
            .map(|placement| placement.root.as_path())
    }

    /// The registry branches of a project, in registry order.
    pub fn branches(&self, project: &str) -> &[BranchInfo] {
        self.placements
            .get(project)
            .map(|placement| placement.branches.as_slice())
            .unwrap_or_default()
    }

    /// What this project's branches are measured against.
    pub fn main_branch(&self, project: &str) -> Option<&str> {
        self.placements
            .get(project)
            .and_then(|placement| placement.main_branch.as_deref())
    }

    /// The item's branch name — the provider-recorded one (ground truth),
    /// or the matched registry branch's — plus the registry match itself.
    /// One rule, shared with dispatch and the CLI (§AR-004-forest.3).
    pub fn effective_branch(&self, item: &Item) -> (Option<String>, Option<BranchInfo>) {
        let Some(placement) = self.placements.get(&item.project) else {
            return (None, None);
        };
        (
            placement.branch_name(item),
            placement.matched(item).cloned(),
        )
    }

    /// Whether a PR item's branch workspace is on disk. None when the state
    /// is unknowable: not a PR, no branch workspaces, or no branch name.
    pub fn item_checked_out(&self, item: &Item) -> Option<bool> {
        if item.kind != ItemKind::Pr {
            return None;
        }
        let placement = self.placements.get(&item.project)?;
        let workspace = placement.workspace_for(&placement.branch_name(item)?)?;
        Some(workspace.is_dir())
    }

    /// Re-measure how far each checked-out branch trails its project's main
    /// branch — a fold over the branch workspace's forest, per repository and
    /// then summed (§AR-004-forest.1). Local refs only (no fetch), so counts
    /// are relative to what was last fetched.
    pub fn recompute_behind(&mut self) {
        let mut behind = BTreeMap::new();
        for project in &self.projects {
            let Some(placement) = self.placements.get(project) else {
                continue;
            };
            if placement.main_branch.is_none() {
                continue;
            }
            for branch in &placement.branches {
                if !self.branch_checked_out(project, branch) {
                    continue;
                }
                let workspace = placement
                    .workspace_for(&branch.branch)
                    .unwrap_or_else(|| placement.root.clone());
                let staleness = placement.forest(&workspace).staleness();
                if staleness.total().is_some() {
                    behind.insert((project.clone(), branch.branch.clone()), staleness);
                }
            }
        }
        self.behind = behind;
    }

    /// How far one branch's checkout trails, per repository. None where it was
    /// never measured — no checkout, or nothing measurable in it.
    pub fn branch_behind(&self, project: &str, branch: &str) -> Option<&Staleness> {
        self.behind.get(&(project.to_string(), branch.to_string()))
    }

    /// Whether a registry branch has its checkout on disk.
    pub fn branch_checked_out(&self, project: &str, branch: &BranchInfo) -> bool {
        let Some(placement) = self.placements.get(project) else {
            return false;
        };
        match placement.workspace_for(&branch.branch) {
            Some(workspace) => workspace.is_dir(),
            None => placement.root.is_dir(),
        }
    }

    /// Where an item's work belongs: the branch it is on, the directory to run
    /// commands in, and whether its workspace is on disk
    /// (§AR-004-forest.3). The same answer dispatch and the CLI get.
    pub fn checkout(&self, item: &Item) -> Option<Checkout> {
        Some(self.placements.get(&item.project)?.checkout(item))
    }

    /// Best link for a branch row: its most urgent matching feed item.
    pub fn branch_url(&self, project: &str, branch: &BranchInfo) -> Option<String> {
        self.feed(project)?
            .items()
            .filter(|item| matches_branch(item, branch))
            .filter(|item| item.url.is_some())
            .max_by_key(|item| (item.needs_response, item.updated_at))
            .and_then(|item| item.url.clone())
    }
}

pub(crate) use crate::branches::WorkspaceState;

pub(crate) use crate::branches::matches as matches_branch;

/// What a screen asks the shell to do in response to a key.
pub(crate) enum Action {
    None,
    Quit,
    /// Show the item's thread screen. With `or_url`, an item without any
    /// recorded messages falls back to opening its URL.
    OpenThread {
        item: Item,
        or_url: bool,
    },
    /// Show the item's gate screen: the counts spelled out and the
    /// forge's own reasons for refusing the merge.
    OpenGate(Item),
    /// Leave the current screen for the navigator.
    Back,
    OpenUrl(Option<String>),
    /// Mark items read: `(id, updated_at, title)`. `pop` returns to the
    /// navigator afterwards (used by the thread screen).
    MarkDone {
        marks: Vec<(String, DateTime<Utc>, String)>,
        pop: bool,
    },
    /// Post `content` (palette name) on a message; `message` is the flat
    /// message index for the optimistic local update. `project` is carried
    /// because a reaction on a forge item is a write back through that
    /// project's provider block, not a call ephor makes on its own.
    React {
        target: ReactTarget,
        content: &'static str,
        emoji: &'static str,
        project: String,
        message: usize,
    },
    /// Tick a task on a message (§FS-004-quick-actions.5).
    ResolveTask {
        task: Task,
        project: String,
        message: usize,
    },
    /// Send the reply a run drafted, as it now stands (§FS-005-dispatch.13).
    /// The item comes along because posting is also what retires the draft.
    PostReply {
        target: ReplyTarget,
        text: String,
        project: String,
        item: Item,
    },
    /// Open a drafted reply in the reader's editor before it goes anywhere.
    EditReply {
        path: PathBuf,
        item: Item,
    },
    /// Summon the configured action menu for an item.
    OpenActionMenu(Item),
    /// Show what is being done about an item, and what could be
    /// (§FS-005-dispatch).
    OpenWork(Item),
    /// Hand the item to the runtime under one recipe.
    DispatchWork {
        item: Item,
        recipe: String,
    },
    /// Reopen work whose item has moved under it (§FS-005-dispatch.5).
    SyncWork(Item),
    /// Leave the interface and let the runtime work one item's plan.
    RunWork {
        root: PathBuf,
        /// Where to run it from — the checkout the work is about.
        checkout: PathBuf,
        plan_id: String,
        label: String,
    },
    /// Open a plan in the reader's editor.
    ReadPlan(PathBuf),
    /// Ask this item for something no recipe covers (§FS-005-dispatch.10).
    AskWork(Item),
    ToggleUnread,
    Refresh,
    SetMessage(String),
}

enum Screen {
    Navigator,
    Thread(ThreadScreen),
    Gate(GateScreen),
    Work(WorkScreen),
}

struct App {
    ctx: Ctx,
    navigator: NavigatorState,
    screen: Screen,
    /// Open action menu, drawn over the active screen.
    menu: Option<ActionMenu>,
    /// A line the reader is typing, drawn over everything else.
    prompt: Option<Prompt>,
    /// The half of ephor that hands work over (§FS-005-dispatch). None when
    /// the registry could not be read for it — the inbox still works.
    dispatcher: Option<crate::work::Dispatcher>,
    message: String,
}

pub fn run() -> Result<ExitCode> {
    let config = load_config()?;
    let mut app = App::load(&config)?;
    let mut terminal = ratatui::init();
    let result = app.event_loop(&mut terminal, &config);
    ratatui::restore();
    result
}

struct RegistryInfo {
    orgs: Vec<OrgInfo>,
    project_org: BTreeMap<String, String>,
    placements: BTreeMap<String, Placement>,
}

fn load_registry_info(projects: &[String]) -> Result<RegistryInfo> {
    let registry_doc = crate::feed::commands::load_registry_doc()?;

    let mut orgs: Vec<OrgInfo> = registry::array_field(&registry_doc, "organizations")
        .iter()
        .map(|org| OrgInfo {
            id: registry::id_of(org).to_string(),
            name: registry::str_field(org, "name").unwrap_or("").to_string(),
            root: registry::str_field(org, "root").map(String::from),
        })
        .collect();
    orgs.push(OrgInfo {
        id: String::new(),
        name: "Other".to_string(),
        root: None,
    });

    let mut project_org = BTreeMap::new();
    let mut placements = BTreeMap::new();
    for project in registry::array_field(&registry_doc, "projects") {
        let project_id = registry::id_of(project).to_string();
        if !projects.contains(&project_id) {
            continue;
        }
        project_org.insert(
            project_id.clone(),
            registry::str_field(project, "organization")
                .unwrap_or("")
                .to_string(),
        );
        // The same reading dispatch and the CLI do — one description of where
        // a project is, not a second copy of it (§AR-004-forest.3).
        if let Some(placement) = Placement::load(&registry_doc, &project_id) {
            placements.insert(project_id, placement);
        }
    }
    Ok(RegistryInfo {
        orgs,
        project_org,
        placements,
    })
}

fn display_root(root: &str) -> String {
    let resolved = paths::resolve_path(root).to_string_lossy().into_owned();
    let home = paths::home_dir().to_string_lossy().into_owned();
    match resolved.strip_prefix(&home) {
        Some(rest) if rest.starts_with('/') || rest.is_empty() => format!("~{rest}"),
        _ => resolved,
    }
}

pub(crate) fn highlight_style() -> Style {
    Style::default()
        .bg(Color::Rgb(60, 60, 80))
        .add_modifier(Modifier::BOLD)
}

impl App {
    fn load(config: &StatusConfig) -> Result<Self> {
        let configured: Vec<String> = config.projects.keys().cloned().collect();
        let info = load_registry_info(&configured)?;

        // Order projects by organization (registry order), then by name.
        let org_index = |project: &String| {
            let org_id = info.project_org.get(project).cloned().unwrap_or_default();
            info.orgs
                .iter()
                .position(|org| org.id == org_id)
                .unwrap_or(info.orgs.len() - 1)
        };
        let mut projects = configured;
        projects.sort_by(|a, b| org_index(a).cmp(&org_index(b)).then(a.cmp(b)));

        let mut app = App {
            ctx: Ctx {
                feeds: Vec::new(),
                seen: cache::load_seen()?,
                projects,
                orgs: info.orgs,
                project_org: info.project_org,
                placements: info.placements,
                behind: BTreeMap::new(),
                capabilities: BTreeMap::new(),
                resurfacing: BTreeMap::new(),
                unattributed: Vec::new(),
                actions: config.actions.clone(),
                project_actions: config
                    .projects
                    .iter()
                    .map(|(id, project)| (id.clone(), project.actions.clone()))
                    .collect(),
                provider_blocks: config
                    .projects
                    .iter()
                    .map(|(id, project)| (id.clone(), project.providers.clone()))
                    .collect(),
                checkouts: config
                    .projects
                    .iter()
                    .filter_map(|(id, project)| {
                        project
                            .checkout
                            .clone()
                            .map(|checkout| (id.clone(), checkout))
                    })
                    .collect(),
                recent_days: config.defaults.recent_days,
                unread_only: true,
                work: BTreeMap::new(),
            },
            navigator: NavigatorState::new(),
            screen: Screen::Navigator,
            menu: None,
            prompt: None,
            dispatcher: crate::work::Dispatcher::load(config).ok(),
            message: String::new(),
        };
        app.reload_feeds()?;
        if !app.navigator.has_stream_entries()
            && app.ctx.feeds.iter().all(|feed| feed.fetched_at.is_none())
        {
            app.message = "No cached data — press r to refresh".to_string();
        }
        Ok(app)
    }

    fn reload_feeds(&mut self) -> Result<()> {
        self.ctx.feeds.clear();
        // What nothing claimed is read like any other feed, so it can be shown
        // rather than only counted (§FS-008-attribution.4).
        self.ctx.unattributed = cache::load_feed(crate::feed::refresh::UNATTRIBUTED)?
            .map(|feed| feed.items().collect())
            .unwrap_or_default();
        for project in self.ctx.projects.clone() {
            match cache::load_feed(&project)? {
                Some(feed) => self.ctx.feeds.push(feed),
                None => self.ctx.feeds.push(ProjectFeed {
                    project,
                    ..ProjectFeed::default()
                }),
            }
        }
        self.ctx.recompute_behind();
        self.ctx.recompute_capabilities();
        self.ctx.recompute_resurfacing();
        self.reload_work();
        self.navigator.rebuild(&self.ctx);
        Ok(())
    }

    /// Re-read every dispatched item's plan. The state of the work belongs to
    /// the runtime, so it is read rather than remembered
    /// (§FS-005-dispatch.4) — including after this interface itself has just
    /// changed it.
    fn reload_work(&mut self) {
        let Some(dispatcher) = &self.dispatcher else {
            return;
        };
        let mut work = BTreeMap::new();
        for feed in &self.ctx.feeds {
            for item in feed.items() {
                if let Some(status) = dispatcher.status(&item) {
                    work.insert(
                        item.id.clone(),
                        WorkBadge {
                            // A row has already spent its width on the item;
                            // what is left is a phrase, not a paragraph.
                            text: status.badge(40),
                            open: status.open_tickets() > 0,
                            stale: status.stale(),
                        },
                    );
                }
            }
        }
        self.ctx.work = work;
    }

    fn event_loop(
        &mut self,
        terminal: &mut DefaultTerminal,
        config: &StatusConfig,
    ) -> Result<ExitCode> {
        loop {
            terminal
                .draw(|frame| self.draw(frame))
                .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;

            if !event::poll(Duration::from_millis(250))
                .map_err(|err| EphorError::Command(format!("event poll failed: {err}")))?
            {
                continue;
            }
            let Event::Key(key) = event::read()
                .map_err(|err| EphorError::Command(format!("event read failed: {err}")))?
            else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(ExitCode::SUCCESS);
            }

            // The prompt is over everything, including the menu that opened
            // it: what is typed there is meant for it and nothing else.
            if let Some(prompt) = &mut self.prompt {
                match prompt.handle_key(key.code, key.modifiers) {
                    PromptOutcome::Stay => {}
                    PromptOutcome::Cancel => {
                        self.prompt = None;
                        self.message = "Nothing asked for".to_string();
                    }
                    PromptOutcome::Submit(line) => {
                        let prompt = self.prompt.take().expect("prompt is open");
                        self.submit(terminal, prompt.asking, &line)?;
                    }
                }
                continue;
            }
            if let Some(menu) = &mut self.menu {
                match menu.handle_key(key.code) {
                    MenuOutcome::Stay => {}
                    MenuOutcome::Close => self.menu = None,
                    MenuOutcome::Run(entry) => {
                        let menu = self.menu.take().expect("menu is open");
                        // The one entry that has no command yet: the reader
                        // types it (§FS-005-dispatch.10).
                        if entry.is_freehand {
                            self.prompt = Some(Prompt::new(
                                Asking::Command(Box::new(menu)),
                                "run a command here",
                                "runs in the item's checkout, with its EPHOR_* environment  ·  enter runs  ·  esc cancels",
                            ));
                        } else {
                            self.run_menu_entry(terminal, &menu, &entry)?;
                        }
                    }
                }
                continue;
            }
            let action = match &mut self.screen {
                Screen::Navigator => self.navigator.handle_key(&self.ctx, key.code),
                Screen::Thread(thread) => thread.handle_key(key.code),
                Screen::Gate(gate) => gate.handle_key(key.code),
                Screen::Work(work) => work.handle_key(key.code),
            };
            if self.apply(action, terminal, config)? {
                return Ok(ExitCode::SUCCESS);
            }
        }
    }

    /// Execute a screen's action. Returns true to quit.
    fn apply(
        &mut self,
        action: Action,
        terminal: &mut DefaultTerminal,
        config: &StatusConfig,
    ) -> Result<bool> {
        match action {
            Action::None => {}
            Action::Quit => return Ok(true),
            Action::SetMessage(message) => self.message = message,
            Action::OpenUrl(url) => self.open_url(url),
            Action::OpenThread { item, or_url } => {
                // What a run drafted about this matter, read from the work
                // root every time it is shown (§FS-005-dispatch.13).
                let proposal = self.proposal(&item);
                match ThreadScreen::open(item.clone(), proposal) {
                    Some(screen) => self.screen = Screen::Thread(screen),
                    None if or_url => self.open_url(item.url),
                    None => self.message = "No messages recorded for this item".to_string(),
                }
            }
            Action::OpenGate(item) => match GateScreen::open(item) {
                Some(screen) => self.screen = Screen::Gate(screen),
                None => self.message = "No gate recorded for this item".to_string(),
            },
            Action::Back => self.screen = Screen::Navigator,
            Action::ToggleUnread => {
                self.ctx.unread_only = !self.ctx.unread_only;
                self.navigator.rebuild(&self.ctx);
            }
            Action::MarkDone { marks, pop } => {
                self.mark_done(marks)?;
                if pop {
                    self.screen = Screen::Navigator;
                }
            }
            Action::React {
                target,
                content,
                emoji,
                project,
                message,
            } => {
                self.message = format!("Reacting {emoji}…");
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                let blocks = self.ctx.blocks_for(&project);
                match react::post(&target, content, emoji, &blocks, &project, &config.defaults) {
                    Ok(()) => {
                        self.message = format!("Reacted {emoji}");
                        if let Screen::Thread(thread) = &mut self.screen {
                            thread.add_local_reaction(message, emoji);
                        }
                    }
                    Err(err) => self.message = err.to_string(),
                }
            }
            Action::ResolveTask {
                task,
                project,
                message,
            } => {
                self.message = "Ticking…".to_string();
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                let blocks = self.ctx.blocks_for(&project);
                match crate::feed::task::resolve(&task, &blocks, &project, &config.defaults) {
                    Ok(()) => {
                        self.message = "Ticked".to_string();
                        if let Screen::Thread(thread) = &mut self.screen {
                            thread.tick_local(message);
                        }
                    }
                    Err(err) => self.message = err.to_string(),
                }
            }
            Action::PostReply {
                target,
                text,
                project,
                item,
            } => {
                self.message = "Posting the reply…".to_string();
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                let blocks = self.ctx.blocks_for(&project);
                match reply::post(&target, &text, &blocks, &project, &config.defaults) {
                    Ok(()) => {
                        // Sent, so the draft is retired where it lives: a
                        // proposal still offered after it was posted invites
                        // posting it twice (§FS-005-dispatch.13).
                        self.message = match self
                            .dispatcher
                            .as_ref()
                            .map(|dispatcher| dispatcher.proposal_posted(&item))
                        {
                            Some(Err(err)) => format!("Posted — but {err}"),
                            _ => "Posted".to_string(),
                        };
                        if let Screen::Thread(thread) = &mut self.screen {
                            thread.reply_posted();
                        }
                    }
                    Err(err) => self.message = err.to_string(),
                }
            }
            Action::EditReply { path, item } => {
                self.edit_file(terminal, &path)?;
                // The reader may have rewritten it, emptied it, or left it
                // alone: what is on disk now is what would be posted.
                let proposal = self.proposal(&item);
                if let Screen::Thread(thread) = &mut self.screen {
                    thread.reread(proposal);
                }
            }
            Action::OpenActionMenu(item) => {
                let applicable = self.ctx.actions_for(&item);
                // An empty menu is no longer empty: the last entry is always
                // "run a command here…" (§FS-005-dispatch.10), and refusing
                // to open would hide it exactly where nothing is configured.
                // The menu opens where the project is placed; where it is not,
                // the ladder's own sentence says why (§AR-005-capabilities.2).
                let refusal = self.ctx.can(&item.project).refusal(&[Rung::Placed]);
                match self.ctx.root(&item.project).map(Path::to_path_buf) {
                    Some(root) if refusal.is_none() => {
                        // One resolver answers where the work is, which branch
                        // it is on, and whether its workspace is there
                        // (§AR-004-forest.3).
                        let placed = self.ctx.checkout(&item).expect("the project is placed");
                        let branch = self
                            .ctx
                            .placement(&item.project)
                            .and_then(|placement| placement.matched(&item).cloned());
                        let checkout = self.ctx.checkouts.get(&item.project).cloned();
                        // The ladder answers what each entry said it needs, so
                        // an offer and a configured action are refused in the
                        // same sentence (§AR-005-capabilities.2).
                        let can = self.ctx.can(&item.project);
                        self.menu = Some(ActionMenu::new(
                            item,
                            root.clone(),
                            placed.workspace,
                            branch,
                            placed.state,
                            checkout,
                            &can,
                            applicable,
                        ));
                    }
                    _ => {
                        self.message = refusal.unwrap_or_else(|| {
                            format!("{} has no root in the registry", item.project)
                        });
                    }
                }
            }
            Action::OpenWork(item) => self.open_work(item),
            Action::DispatchWork { item, recipe } => {
                self.dispatch_work(&item, &recipe);
                self.open_work(item);
            }
            Action::SyncWork(item) => {
                self.sync_work(&item);
                self.open_work(item);
            }
            Action::RunWork {
                root,
                checkout,
                plan_id,
                label,
            } => {
                // The runtime is a rung: refused here in the same words the
                // command line uses, instead of handing the terminal over to a
                // command that cannot start (§AR-005-capabilities.2).
                if let Some(refusal) = crate::work::runtime::refusal(&config.work) {
                    self.message = refusal;
                    return Ok(true);
                }
                // The checkout, not the plan directory: it is where the work
                // is, and where the runtime falls back to when a workspace has
                // no one repository to be found by looking.
                self.handover(
                    terminal,
                    "▶",
                    &format!("{} — {label}", crate::work::runtime::label(&config.work)),
                    &Site::root(&checkout),
                    &crate::work::runtime::summons(
                        &config.work,
                        &root,
                        std::slice::from_ref(&plan_id),
                        &[],
                    ),
                )?;
                // The runtime just advanced the plans this reads.
                self.reload_work();
                self.navigator.rebuild(&self.ctx);
                if let Screen::Work(screen) = &self.screen {
                    let item = screen.item.clone();
                    self.open_work(item);
                }
            }
            Action::AskWork(item) => {
                self.prompt = Some(Prompt::new(
                    Asking::Work(item),
                    "ask for something",
                    "becomes a ticket with the dossier  ·  enter opens it  ·  esc cancels",
                ))
            }
            Action::ReadPlan(path) => {
                self.edit_file(terminal, &path)?;
                self.reload_work();
            }
            Action::Refresh => {
                self.message = "Refreshing…".to_string();
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                self.refresh(config)?;
            }
        }
        Ok(false)
    }

    /// What the reader typed, done: a ticket in their own words, or a command
    /// run exactly as a configured one is (§FS-005-dispatch.10).
    fn submit(&mut self, terminal: &mut DefaultTerminal, asking: Asking, line: &str) -> Result<()> {
        match asking {
            Asking::Work(item) => {
                let Some(dispatcher) = &mut self.dispatcher else {
                    self.message = "Work needs the registry, which could not be read".to_string();
                    return Ok(());
                };
                // The screen below shows the plan; the header says what landed
                // in it.
                self.message = match dispatcher.ask(&item, line, None, false) {
                    Ok(crate::work::Outcome::Opened { ticket, .. })
                    | Ok(crate::work::Outcome::Reopened { ticket, .. }) => {
                        match dispatcher.save() {
                            Ok(()) => format!("✎ asked — {ticket}"),
                            Err(err) => err.to_string(),
                        }
                    }
                    Ok(outcome) => outcome.describe(),
                    Err(err) => err.to_string(),
                };
                self.reload_work();
                self.navigator.rebuild(&self.ctx);
                self.open_work(item);
            }
            Asking::Command(menu) => {
                let entry = actions::MenuEntry {
                    action: ActionConfig {
                        icon: "⌨".to_string(),
                        description: line.to_string(),
                        command: line.to_string(),
                        ..ActionConfig::default()
                    },
                    is_checkout: false,
                    is_freehand: false,
                    gate: actions::Gate::Ready,
                };
                self.run_menu_entry(terminal, &menu, &entry)?;
            }
        }
        Ok(())
    }

    /// The work screen for an item, rebuilt from the plan each time it opens.
    /// Hand a file to the reader's editor, the terminal theirs while they have
    /// it — the same handover the runtime gets (§AR-002-summons).
    fn edit_file(&mut self, terminal: &mut DefaultTerminal, path: &Path) -> Result<()> {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "less".to_string());
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let command = format!(
            "{editor} {}",
            crate::feed::providers::shell_quote(&path.to_string_lossy())
        );
        self.handover(
            terminal,
            "📖",
            &editor,
            &Site::root(&dir),
            &summons::Summons::new("edit", command),
        )?;
        Ok(())
    }

    /// The reply a run drafted about a matter, where there is a dispatcher to
    /// ask and a run that drafted one (§FS-005-dispatch.13).
    fn proposal(&self, item: &Item) -> Option<crate::work::runtime::results::Proposal> {
        self.dispatcher
            .as_ref()
            .and_then(|dispatcher| dispatcher.proposal(item))
    }

    fn open_work(&mut self, item: Item) {
        let Some(dispatcher) = &mut self.dispatcher else {
            self.message =
                "Work needs the registry, which could not be read at startup".to_string();
            return;
        };
        let status = dispatcher.status(&item);
        let offers = dispatcher
            .offers(&item)
            .into_iter()
            .map(|recipe| work::Offer {
                brief: dispatcher.brief(&item, &recipe),
                recipe,
            })
            .collect();
        self.screen = Screen::Work(WorkScreen::new(item, status, offers));
    }

    fn dispatch_work(&mut self, item: &Item, recipe_id: &str) {
        let Some(dispatcher) = &mut self.dispatcher else {
            return;
        };
        let Some(recipe) = dispatcher
            .offers(item)
            .into_iter()
            .find(|recipe| recipe.id == recipe_id)
        else {
            self.message = format!("'{recipe_id}' does not apply to this item any more");
            return;
        };
        // The screen below already shows the plan and its tickets, so the
        // header says what was asked for rather than repeating a long path.
        self.message = match dispatcher.dispatch(item, &recipe, false) {
            Ok(crate::work::Outcome::Opened { ticket, .. })
            | Ok(crate::work::Outcome::Reopened { ticket, .. }) => match dispatcher.save() {
                Ok(()) => format!("{} {} — {ticket}", recipe.icon, recipe.description),
                Err(err) => err.to_string(),
            },
            Ok(outcome) => outcome.describe(),
            Err(err) => err.to_string(),
        };
        self.reload_work();
        self.navigator.rebuild(&self.ctx);
    }

    fn sync_work(&mut self, item: &Item) {
        let Some(dispatcher) = &mut self.dispatcher else {
            return;
        };
        self.message = match dispatcher.sync(item, false) {
            Ok(crate::work::Outcome::Reopened {
                ticket, changes, ..
            }) => match dispatcher.save() {
                Ok(()) => format!("reopened as {ticket} — {}", changes.join("; ")),
                Err(err) => err.to_string(),
            },
            Ok(outcome) => outcome.describe(),
            Err(err) => err.to_string(),
        };
        self.reload_work();
        self.navigator.rebuild(&self.ctx);
    }

    /// Leave the interface, run something the reader watches, and come back.
    /// The runtime writes for minutes and asks questions; putting it behind a
    /// spinner would hide the only thing worth seeing.
    /// Leave the TUI, run one summons attached to the real terminal, and come
    /// back. Handing the terminal over is this call site's property, not the
    /// binding's (§AR-002-summons.2).
    fn handover(
        &mut self,
        terminal: &mut DefaultTerminal,
        icon: &str,
        description: &str,
        site: &Site,
        summons: &summons::Summons,
    ) -> Result<()> {
        ratatui::restore();
        match site.resolve(&Place::Workspace) {
            Ok(place) => println!("\n{icon} {description}   ({})\n", place.display()),
            Err(err) => println!("\n{icon} {description}   ({err})\n"),
        }
        self.message = match summons::run(summons, site, summons::Mode::Interactive) {
            Ok(answer) if answer.is_done() => format!("{description}: ok"),
            Ok(answer) => answer.refusal(description),
            Err(err) => format!("{description}: {err}"),
        };
        println!("\n{}", self.message);
        print!("Press Enter to return to ephor… ");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().lock().read_line(&mut String::new());
        *terminal = ratatui::init();
        terminal
            .clear()
            .map_err(|err| EphorError::Command(format!("terminal clear failed: {err}")))
    }

    /// Run a menu entry attached to the real terminal: leave the TUI, run
    /// the checkout dependency first when the entry needs one, then the
    /// action itself in the item's checkout, wait for a keypress, and come
    /// back. Blocked entries only set a status message.
    fn run_menu_entry(
        &mut self,
        terminal: &mut DefaultTerminal,
        menu: &ActionMenu,
        entry: &actions::MenuEntry,
    ) -> Result<()> {
        if let actions::Gate::Blocked(reason) = &entry.gate {
            self.message = reason.clone();
            return Ok(());
        }
        ratatui::restore();

        // A menu entry is a summons like every other command ephor runs
        // (§AR-002-summons): one spawn path, one environment contract, one
        // reading of the exit code. The terminal is handed over because that is
        // this call site's property, not the binding's (§AR-002-summons.2).
        let step = |command: &str,
                    icon: &str,
                    description: &str,
                    where_: &Place,
                    site: &Site,
                    workspace: &Path| {
            let place = site
                .resolve(where_)
                .map_err(|err| format!("{description}: {err}"))?;
            println!("\n▶ {icon} {description}   ({})", place.display());
            println!("  $ {command}\n");
            // The forest of the place it runs in, so a command that folds
            // over repositories folds over the same ones ephor does
            // (§AR-004-forest.1).
            let forest = self
                .ctx
                .placement(&menu.item.project)
                .map(|placement| placement.forest(workspace));
            let summons = summons::Summons::new(description, command).carrying(dossier::of_item(
                &menu.item,
                &menu.root,
                workspace,
                menu.branch.as_ref(),
                forest.as_ref(),
            ));
            let answer = summons::run(&summons, site, summons::Mode::Interactive)
                .map_err(|err| err.to_string())?;
            match answer.outcome {
                Outcome::Done => Ok(()),
                _ => Err(answer.refusal(description)),
            }
        };

        let needs_checkout =
            entry.is_checkout || matches!(entry.gate, actions::Gate::NeedsCheckout);
        let outcome = (|| {
            let mut workspace = menu.workspace.clone();
            if needs_checkout {
                let (checkout, target) =
                    menu.checkout_step().expect("gated on a missing workspace");
                // The checkout runs in the root — its job is to create the
                // target workspace, which ephor verifies rather than trusts.
                step(
                    &checkout.command,
                    &checkout.icon,
                    &checkout.description,
                    &Place::Root,
                    &Site::root(&menu.root),
                    &target,
                )?;
                if !target.is_dir() {
                    return Err(format!(
                        "{}: did not create {}",
                        checkout.description,
                        target.display()
                    ));
                }
                workspace = target;
            }
            if !entry.is_checkout {
                let action = &entry.action;
                // Where the entry said it runs — a project's offer may name
                // one repository of the forest (§AR-002-summons.1); a person's
                // action that says nothing runs where it always has.
                let where_ = match &action.cwd {
                    Some(spec) => Place::parse(spec).map_err(|err| err.to_string())?,
                    None => Place::Workspace,
                };
                step(
                    &action.command,
                    &action.icon,
                    &action.description,
                    &where_,
                    &Site::workspace(&menu.root, &workspace),
                    &workspace,
                )?;
                return Ok(format!("{} {}: ok", action.icon, action.description));
            }
            Ok(format!("✓ checked out {}", workspace.display()))
        })();
        self.message = match outcome {
            Ok(message) => message,
            Err(message) => message,
        };

        println!("\n{}", self.message);
        print!("Press Enter to return to ephor… ");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().lock().read_line(&mut String::new());
        *terminal = ratatui::init();
        terminal
            .clear()
            .map_err(|err| EphorError::Command(format!("terminal clear failed: {err}")))?;
        // A checkout changes what the branch rows show, and buys the rungs
        // that were waiting on it (§AR-005-capabilities.1).
        if needs_checkout {
            self.ctx.recompute_behind();
            self.ctx.recompute_capabilities();
        }
        self.navigator.rebuild(&self.ctx);
        Ok(())
    }

    fn open_url(&mut self, url: Option<String>) {
        match url {
            Some(url) => {
                let result = std::process::Command::new("xdg-open")
                    .arg(&url)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                self.message = match result {
                    Ok(_) => format!("Opened {url}"),
                    Err(err) => format!("xdg-open failed: {err}"),
                };
            }
            None => self.message = "Nothing to open here".to_string(),
        }
    }

    fn mark_done(&mut self, marks: Vec<(String, DateTime<Utc>, String)>) -> Result<()> {
        self.message = match marks.as_slice() {
            [(_, _, title)] => format!("Done: {title}"),
            _ => format!("Done: {} items", marks.len()),
        };
        for (id, updated_at, _) in marks {
            // Remember what it looked like, so the row can say what moved when
            // it comes back (§FS-007-matters.5).
            let mark = self
                .ctx
                .matter(&id)
                .map(|matter| cache::Mark::of(&matter))
                .unwrap_or_else(|| cache::Mark::at(updated_at));
            self.ctx.resurfacing.remove(&id);
            self.ctx.seen.insert(id, mark);
        }
        cache::store_seen(&self.ctx.seen)?;
        self.navigator.rebuild(&self.ctx);
        Ok(())
    }

    fn refresh(&mut self, config: &StatusConfig) -> Result<()> {
        let registry_doc = crate::feed::commands::load_registry_doc()?;
        let filter_project = self.navigator.refresh_filter(&self.ctx);
        // Named, not counted. "6 provider warnings" is the same sentence
        // whether a forge has been uninstalled for months or a laptop is off
        // the VPN for a minute, and in both cases the sections those providers
        // fill just look empty.
        let mut lost: Vec<String> = Vec::new();
        let mut unreachable = 0usize;
        for (project_id, project_config) in &config.projects {
            if let Some(filter) = &filter_project {
                if project_id != filter {
                    continue;
                }
            }
            match crate::feed::refresh::refresh_project(
                &registry_doc,
                project_id,
                project_config,
                &config.defaults,
            ) {
                Ok(outcome) => {
                    unreachable += outcome.unreachable_count();
                    lost.extend(
                        outcome
                            .failures
                            .iter()
                            .map(|failure| format!("{project_id}/{}", failure.provider)),
                    );
                }
                Err(err) => {
                    self.message = format!("Refresh failed for {project_id}: {err}");
                    lost.push(project_id.clone());
                }
            }
        }
        self.reload_feeds()?;
        self.message = if lost.is_empty() {
            "Refreshed".to_string()
        } else {
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
            format!("Refreshed — NO DATA from {}: {shown}{network}", lost.len())
        };
        Ok(())
    }

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let [header_area, body_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        let title = match &self.screen {
            Screen::Navigator => self.navigator.title(&self.ctx),
            Screen::Thread(thread) => thread.title(),
            Screen::Gate(gate) => gate.title(),
            Screen::Work(work) => work.title(),
        };
        frame.render_widget(
            Paragraph::new(format!("{title}   {}", self.message))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            header_area,
        );

        match &mut self.screen {
            Screen::Navigator => self.navigator.draw(&self.ctx, frame, body_area),
            Screen::Thread(thread) => thread.draw(frame, body_area),
            Screen::Gate(gate) => gate.draw(frame, body_area),
            Screen::Work(work) => work.draw(frame, body_area),
        }
        if let Some(menu) = &self.menu {
            menu.draw(frame, body_area);
        }
        if let Some(prompt) = &self.prompt {
            prompt.draw(frame, body_area);
        }

        let footer = if self.prompt.is_some() {
            " type  ·  enter sends  ·  esc cancels  ·  ^w word back  ·  ^u clear".to_string()
        } else if self.menu.is_some() {
            " j/k move  1-9 run  enter run  esc cancel".to_string()
        } else {
            match &self.screen {
                Screen::Navigator => self.navigator.footer().to_string(),
                // Built from what is selected, not fixed per screen
                // (§FS-004-quick-actions.2).
                Screen::Thread(thread) => thread.footer(),
                Screen::Gate(gate) => gate.footer().to_string(),
                Screen::Work(work) => work.footer().to_string(),
            }
        };
        frame.render_widget(
            Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
            footer_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::ItemKind;
    use serde_json::json;

    fn ctx_with_branch(root: &Path, template: Option<&str>) -> Ctx {
        let branch = BranchInfo {
            branch: "you/ABC-42-retry-window".to_string(),
            ticket: Some("ABC-42".to_string()),
            active: true,
            is_release: false,
        };
        let placement = Placement {
            project: "widget".to_string(),
            root: root.to_path_buf(),
            template: template.map(String::from),
            branches: vec![branch],
            main_branch: Some("master".to_string()),
            repos: Vec::new(),
            aliases: Vec::new(),
            territory: Vec::new(),
            trust: crate::manifest::Trust::Full,
        };
        Ctx {
            feeds: Vec::new(),
            seen: Seen::new(),
            projects: vec!["widget".to_string()],
            orgs: Vec::new(),
            project_org: BTreeMap::new(),
            placements: BTreeMap::from([("widget".to_string(), placement)]),
            behind: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            resurfacing: BTreeMap::new(),
            unattributed: Vec::new(),
            actions: Vec::new(),
            project_actions: BTreeMap::new(),
            provider_blocks: BTreeMap::new(),
            checkouts: BTreeMap::new(),
            recent_days: 7,
            unread_only: true,
            work: BTreeMap::new(),
        }
    }

    /// Give the fixture project a declared forest.
    fn declare(ctx: &mut Ctx, repos: &[&str]) {
        let placement = ctx
            .placements
            .get_mut("widget")
            .expect("the fixture project");
        placement.repos = repos
            .iter()
            .map(|name| crate::forest::Declaration::at(*name))
            .collect();
    }

    fn ticket_item() -> Item {
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "[ABC-42] Fix condition errors".to_string(),
            url: None,
            state: None,
            needs_response: false,
            updated_at: Utc::now(),
            raw: json!({}),
        }
    }

    #[test]
    fn a_sources_own_action_leads_the_menu() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = ctx_with_branch(tmp.path(), None);
        ctx.actions = vec![serde_json::from_value(json!({
            "icon": "🧪", "description": "run the gate", "command": "just gate"
        }))
        .unwrap()];
        ctx.provider_blocks = BTreeMap::from([(
            "widget".to_string(),
            vec![json!({ "provider": "github-ci", "repos": ["acme/widget"] })],
        )]);

        let mut ci = ticket_item();
        ci.id = "github-ci:acme/widget#42".to_string();
        ci.source = "github-ci".to_string();
        ci.kind = ItemKind::Pr;
        ci.state = None;
        // The gate rides on the pull request now, and the source's own action
        // is offered off the gate rather than off a state word.
        ci.raw = json!({
            "repo": "acme/widget",
            "gate": { "repos": [{
                "repo": "acme/widget", "passed": 1, "failed": 2, "running": 0
            }] }
        });

        // The configured action keeps its place and the source's own goes
        // ahead of it (§FS-004-quick-actions.3) — where `gh` is installed for
        // it to be offered at all.
        let menu = ctx.actions_for(&ci);
        let quick = usize::from(crate::feed::provider::command_exists("gh"));
        assert_eq!(menu.len(), quick + 1);
        assert_eq!(menu.last().unwrap().description, "run the gate");
        if quick == 1 {
            assert_eq!(menu[0].description, "see the CI failures");
        }
    }

    /// Provenance orders the menu and a repeated id wins in place: what ephor
    /// recognized, then what the project offers of itself, then the person's
    /// own (§FS-006-project-interface.9). The project's offers arrive under
    /// the trust the row extends to them (§FS-006-project-interface.2).
    #[test]
    fn the_menu_is_shipped_then_the_projects_then_the_persons() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(crate::manifest::FILE),
            r#"{"actions": [
                 {"id": "bench", "description": "the project's benchmark",
                  "command": "./bench.sh", "when": {"kinds": ["pr"]}},
                 {"id": "nightly", "description": "only on a green gate",
                  "command": "./nightly.sh", "when": {"gate": "green"}},
                 {"id": "rebase", "description": "the project's own rebase",
                  "command": "./rebase.sh"}
               ]}"#,
        )
        .unwrap();
        let mut ctx = ctx_with_branch(tmp.path(), None);
        ctx.actions = vec![serde_json::from_value(json!({
            "id": "bench", "icon": "🧪", "description": "my benchmark", "command": "just bench"
        }))
        .unwrap()];

        let menu = ctx.actions_for(&ticket_item());
        let described: Vec<&str> = menu
            .iter()
            .map(|action| action.description.as_str())
            .collect();
        // The item has no gate, so the offer asking for a green one is not
        // there at all; the person's `bench` replaced the project's, in the
        // place the project's held.
        assert_eq!(
            described,
            ["my benchmark", "the project's own rebase"],
            "{described:?}"
        );
        assert_eq!(menu[0].command, "just bench");

        // A row that trusts the checkout for descriptions only runs none of
        // what it offers.
        ctx.placements
            .get_mut("widget")
            .expect("the fixture project")
            .trust = crate::manifest::Trust::Descriptions;
        let menu = ctx.actions_for(&ticket_item());
        assert_eq!(menu.len(), 1, "only the person's own is left");
        assert_eq!(menu[0].description, "my benchmark");
    }

    #[test]
    fn checkout_resolves_existing_branch_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace_dir = root.join("you/ABC-42-retry-window");
        std::fs::create_dir_all(&workspace_dir).unwrap();

        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        let placed = ctx.checkout(&ticket_item()).unwrap();
        assert_eq!(placed.workspace, workspace_dir);
        assert_eq!(placed.ticket.as_deref(), Some("ABC-42"));
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    /// A repo whose `feature` branch is `commits` commits behind `master`.
    fn repo_behind(dir: &Path, commits: usize) {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q", "-b", "master"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
        git(dir, &["branch", "feature"]);
        for index in 0..commits {
            git(
                dir,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("ahead {index}"),
                ],
            );
        }
        git(dir, &["checkout", "-q", "feature"]);
    }

    #[test]
    fn item_checkout_state_uses_recorded_branch_without_registry_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));

        // A PR whose branch has no registry entry, resolved via raw.branch.
        let mut pr = ticket_item();
        pr.title = "Unrelated title".to_string();
        pr.raw = json!({ "branch": "someone/feature" });
        assert_eq!(ctx.item_checked_out(&pr), Some(false));
        std::fs::create_dir_all(root.join("someone/feature")).unwrap();
        assert_eq!(ctx.item_checked_out(&pr), Some(true));
        assert_eq!(
            ctx.checkout(&pr).unwrap().workspace,
            root.join("someone/feature")
        );

        // No branch information at all: state is unknown.
        pr.raw = json!({});
        assert_eq!(ctx.item_checked_out(&pr), None);
        assert!(matches!(
            ctx.checkout(&pr).unwrap().state,
            WorkspaceState::Unmatched
        ));
    }

    #[test]
    fn behind_sums_across_workspace_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);
        repo_behind(&workspace.join("ee"), 3);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        ctx.recompute_behind();
        let staleness = ctx
            .branch_behind("widget", "you/ABC-42-retry-window")
            .expect("both repositories were measured");
        assert_eq!(staleness.total(), Some(5));
        // The sum is reported, and which repository it came from survives it
        // (§AR-004-forest.1).
        assert_eq!(
            staleness.summary().as_deref(),
            Some("5 behind (ce 2, ee 3)")
        );
    }

    /// The rebase is in the menu because of what is on disk, and only then
    /// (§FS-004-quick-actions.6).
    #[test]
    fn the_rebase_is_offered_on_a_checkout_that_trails_main() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);
        repo_behind(&workspace.join("ee"), 3);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        let pr = ticket_item();
        assert_eq!(ctx.item_behind(&pr), Some(5));
        let menu = ctx.actions_for(&pr);
        assert_eq!(menu[0].description, "rebase onto master (5 behind)");
        assert!(menu[0].command.contains("rebase --project"));
        assert!(menu[0].requires_checkout);

        // Level with master: nothing to replay, so nothing offered.
        for repo in ["ce", "ee"] {
            git(&workspace.join(repo), &["checkout", "-q", "master"]);
        }
        assert_eq!(ctx.item_behind(&pr), Some(0));
        assert!(ctx.actions_for(&pr).is_empty());
    }

    #[test]
    fn the_rebase_is_not_offered_where_there_is_nothing_to_measure() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));

        // The branch workspace was never checked out.
        assert_eq!(ctx.item_behind(&ticket_item()), None);
        assert!(ctx.actions_for(&ticket_item()).is_empty());

        // An item that is not a pull request has no branch to replay.
        let mut message = ticket_item();
        message.kind = ItemKind::Message;
        assert_eq!(ctx.item_behind(&message), None);
    }

    #[test]
    fn behind_skips_unchecked_branches_and_non_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Workspace missing entirely: no entry.
        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        ctx.recompute_behind();
        assert!(ctx.behind.is_empty());

        // Workspace exists but is not a git repo: no entry either.
        std::fs::create_dir_all(root.join("you/ABC-42-retry-window")).unwrap();
        ctx.recompute_behind();
        assert!(ctx.behind.is_empty());
    }

    /// The table is what the surfaces read, and it is honest about time: a
    /// checkout that appears buys the rungs that were waiting on it
    /// (§AR-005-capabilities.1, §AR-005-capabilities.3).
    #[test]
    fn the_capability_table_is_resolved_per_project_and_again_when_the_world_moves() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("widget");
        let mut ctx = ctx_with_branch(&root, Some("{project_root}/{branch}"));
        ctx.recompute_capabilities();

        // Nothing on disk: placed fails, and what cannot be looked in says so.
        let can = ctx.can("widget");
        assert!(!can.holds(Rung::Placed));
        assert!(!can.holds(Rung::Checkable));
        assert!(!can.holds(Rung::Ticketed));
        assert!(can.holds(Rung::BranchAddressable));
        assert!(can
            .refusal(&[Rung::Placed])
            .unwrap()
            .contains("is not on disk"));

        // The project arrives, with a check verb and a ticket store in it.
        std::fs::create_dir_all(root.join("panta")).unwrap();
        std::fs::write(root.join("check.sh"), "#!/bin/sh\n").unwrap();
        ctx.recompute_capabilities();
        let can = ctx.can("widget");
        assert!(can.holds(Rung::Placed));
        assert!(can.holds(Rung::Checkable));
        assert!(can.holds(Rung::Ticketed));

        // A project the registry says nothing about holds nothing, and the
        // table answers rather than being absent.
        assert!(ctx.can("ghost").held().is_empty());
    }

    #[test]
    fn checkout_falls_back_to_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Branch matched but its workspace directory does not exist.
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        let placed = ctx.checkout(&ticket_item()).unwrap();
        assert_eq!(placed.workspace, root);
        assert!(placed.branch.is_some());

        // No branch template at all (plain single-checkout project).
        let ctx = ctx_with_branch(root, None);
        assert_eq!(ctx.checkout(&ticket_item()).unwrap().workspace, root);
    }
}
