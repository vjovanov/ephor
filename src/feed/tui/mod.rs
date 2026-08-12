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

use crate::error::{EphorError, Result};
use crate::feed::cache::{self, ProjectFeed, Seen};
use crate::feed::config::{load_config, ActionConfig, CheckoutConfig, StatusConfig};
use crate::feed::model::{Item, ItemKind};
use crate::feed::react::{self, ReactTarget};
use crate::paths;
use crate::registry;

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
    pub branches: BTreeMap<String, Vec<BranchInfo>>,
    /// Resolved checkout roots from the registry.
    pub roots: BTreeMap<String, PathBuf>,
    /// Per-project `branch_root_template` for branch workspaces
    /// (`{project_root}/{branch}`-style).
    pub branch_templates: BTreeMap<String, String>,
    /// Per-project main branch and the project type's repo paths, for
    /// measuring how far branches trail main. Poly-repo workspaces sum
    /// across all their repos.
    pub main_branches: BTreeMap<String, String>,
    pub repo_paths: BTreeMap<String, Vec<String>>,
    /// Commits behind main per (project, branch name); computed for
    /// checked-out branches at load/refresh time.
    pub behind: BTreeMap<(String, String), u64>,
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

    /// The item's menu: what its source offers on it unasked, then the
    /// configured actions (§FS-004-quick-actions.3).
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
        let mut menu = crate::feed::providers::quick_actions(blocks, item);
        menu.extend(actions::applicable(&self.actions, project, item));
        menu
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

    /// A branch name's workspace directory per the project's
    /// `branch_root_template`, whether or not it exists. None when the
    /// project has no branch workspaces — the root is the checkout then.
    pub fn expand_workspace(
        &self,
        project: &str,
        root: &Path,
        branch_name: &str,
    ) -> Option<PathBuf> {
        let template = self.branch_templates.get(project)?;
        Some(PathBuf::from(
            template
                .replace("{project_root}", &root.to_string_lossy())
                .replace("{branch}", branch_name),
        ))
    }

    pub fn branch_workspace(
        &self,
        project: &str,
        root: &Path,
        branch: &BranchInfo,
    ) -> Option<PathBuf> {
        self.expand_workspace(project, root, &branch.branch)
    }

    /// The item's branch name — the provider-recorded one (ground truth),
    /// or the matched registry branch's — plus the registry match itself.
    pub fn effective_branch(&self, item: &Item) -> (Option<String>, Option<BranchInfo>) {
        let matched = self
            .branches
            .get(&item.project)
            .into_iter()
            .flatten()
            .find(|branch| matches_branch(item, branch))
            .cloned();
        let name = item
            .raw
            .get("branch")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(String::from)
            .or_else(|| matched.as_ref().map(|branch| branch.branch.clone()));
        (name, matched)
    }

    /// Whether a PR item's branch workspace is on disk. None when the state
    /// is unknowable: not a PR, no branch workspaces, or no branch name.
    pub fn item_checked_out(&self, item: &Item) -> Option<bool> {
        if item.kind != ItemKind::Pr {
            return None;
        }
        let root = self.roots.get(&item.project)?;
        let (name, _) = self.effective_branch(item);
        let workspace = self.expand_workspace(&item.project, root, &name?)?;
        Some(workspace.is_dir())
    }

    /// Re-measure how many commits each checked-out branch trails its
    /// project's main branch — summed over all the workspace's repos for
    /// poly-repo layouts. Local git only (no fetch), so counts are
    /// relative to the last-fetched origin.
    pub fn recompute_behind(&mut self) {
        let mut behind = BTreeMap::new();
        for project in &self.projects {
            let Some(root) = self.roots.get(project) else {
                continue;
            };
            let Some(main_branch) = self.main_branches.get(project) else {
                continue;
            };
            let default_paths = vec![".".to_string()];
            let repo_paths = self.repo_paths.get(project).unwrap_or(&default_paths);
            for branch in self.branches.get(project).into_iter().flatten() {
                if !self.branch_checked_out(project, branch) {
                    continue;
                }
                let workspace = self
                    .branch_workspace(project, root, branch)
                    .unwrap_or_else(|| root.clone());
                let mut total: Option<u64> = None;
                for path in repo_paths {
                    let repo = if path == "." {
                        workspace.clone()
                    } else {
                        workspace.join(path)
                    };
                    if let Some(count) = commits_behind(&repo, main_branch) {
                        total = Some(total.unwrap_or(0) + count);
                    }
                }
                if let Some(total) = total {
                    behind.insert((project.clone(), branch.branch.clone()), total);
                }
            }
        }
        self.behind = behind;
    }

    /// Whether a registry branch has its checkout on disk.
    pub fn branch_checked_out(&self, project: &str, branch: &BranchInfo) -> bool {
        let Some(root) = self.roots.get(project) else {
            return false;
        };
        match self.branch_workspace(project, root, branch) {
            Some(workspace) => workspace.is_dir(),
            None => root.is_dir(),
        }
    }

    /// The item's checkout context inside `root`: the registry branch it
    /// belongs to (same matching as the tree grouping) and the directory to
    /// run actions in — the branch workspace when it can be resolved (from
    /// the provider-recorded or registry branch) and exists on disk,
    /// otherwise the project root.
    pub fn checkout_for(&self, item: &Item, root: &Path) -> (PathBuf, Option<BranchInfo>) {
        let (name, matched) = self.effective_branch(item);
        if let Some(name) = &name {
            if let Some(workspace) = self.expand_workspace(&item.project, root, name) {
                if workspace.is_dir() {
                    return (workspace, matched);
                }
            }
        }
        (root.to_path_buf(), matched)
    }

    /// Whether the item's branch workspace is on disk, missing (with the
    /// directory a checkout must create), or unresolvable.
    pub fn workspace_state(&self, item: &Item, root: &Path) -> WorkspaceState {
        if self.branch_templates.get(&item.project).is_none() {
            // Single-checkout project: the root is the workspace.
            return WorkspaceState::Ready;
        }
        let (name, _) = self.effective_branch(item);
        match name {
            Some(name) => {
                let target = self
                    .expand_workspace(&item.project, root, &name)
                    .expect("template exists");
                if target.is_dir() {
                    WorkspaceState::Ready
                } else {
                    WorkspaceState::Missing(target)
                }
            }
            None => WorkspaceState::Unmatched,
        }
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

/// Commits `repo`'s HEAD is behind the main branch, preferring the
/// last-fetched `origin/<main>`; None when not a git repo or no usable ref.
fn commits_behind(repo: &Path, main_branch: &str) -> Option<u64> {
    if !crate::update::is_git_work_tree(repo) {
        return None;
    }
    for reference in [format!("origin/{main_branch}"), main_branch.to_string()] {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-list", "--count", &format!("HEAD..{reference}")])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().parse().ok();
        }
    }
    None
}

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
    /// message index for the optimistic local update.
    React {
        target: ReactTarget,
        content: &'static str,
        emoji: &'static str,
        message: usize,
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
        rhei: String,
        label: String,
    },
    /// Open a plan in the reader's editor.
    ReadPlan(PathBuf),
    /// Ask this item for something no recipe covers (§FS-005-dispatch.9).
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
    branches: BTreeMap<String, Vec<BranchInfo>>,
    roots: BTreeMap<String, PathBuf>,
    branch_templates: BTreeMap<String, String>,
    main_branches: BTreeMap<String, String>,
    repo_paths: BTreeMap<String, Vec<String>>,
}

fn load_registry_info(projects: &[String]) -> Result<RegistryInfo> {
    let registry_doc = crate::feed::commands::load_registry_doc()?;

    let mut roots = BTreeMap::new();
    let mut branch_templates = BTreeMap::new();
    let mut main_branches = BTreeMap::new();
    let mut repo_paths = BTreeMap::new();
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
    let mut branches = BTreeMap::new();
    for project in registry::array_field(&registry_doc, "projects") {
        let project_id = registry::id_of(project).to_string();
        if !projects.contains(&project_id) {
            continue;
        }
        let org_id = registry::str_field(project, "organization")
            .unwrap_or("")
            .to_string();
        project_org.insert(project_id.clone(), org_id);
        if let Some(root) = registry::str_field(project, "root") {
            roots.insert(project_id.clone(), paths::resolve_path(root));
        }
        if let Some(template) = registry::str_field(project, "branch_root_template") {
            branch_templates.insert(project_id.clone(), template.to_string());
        }
        if let Some(main_branch) = registry::str_field(project, "main_branch") {
            main_branches.insert(project_id.clone(), main_branch.to_string());
        }
        // Branch-tracked repo paths of the project type: staleness sums
        // across all of them (app + plugins + docs-site for a poly-repo workspace).
        if let Some(type_id) = registry::str_field(project, "type") {
            if let Ok(project_type) = registry::get_project_type(&registry_doc, type_id) {
                let paths: Vec<String> = registry::array_field(project_type, "repos")
                    .iter()
                    .filter(|repo| repo.get("update_mode").and_then(Value::as_str) != Some("skip"))
                    .filter_map(|repo| registry::str_field(repo, "path"))
                    .map(String::from)
                    .collect();
                if !paths.is_empty() {
                    repo_paths.insert(project_id.clone(), paths);
                }
            }
        }

        let mut project_branches = Vec::new();
        for (section, is_release) in [("release_branches", true), ("branches", false)] {
            for entry in registry::branch_entries(project, section) {
                let branch = registry::str_field(entry, "branch")
                    .unwrap_or("")
                    .to_string();
                let ticket = registry::str_field(entry, "ticket")
                    .map(String::from)
                    .or_else(|| {
                        let extracted = registry::extract_ticket(&branch);
                        if extracted.is_empty() {
                            None
                        } else {
                            Some(extracted)
                        }
                    });
                project_branches.push(BranchInfo {
                    branch,
                    ticket,
                    active: entry
                        .get("active")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    is_release,
                });
            }
        }
        branches.insert(project_id, project_branches);
    }
    Ok(RegistryInfo {
        orgs,
        project_org,
        branches,
        roots,
        branch_templates,
        main_branches,
        repo_paths,
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
                branches: info.branches,
                roots: info.roots,
                branch_templates: info.branch_templates,
                main_branches: info.main_branches,
                repo_paths: info.repo_paths,
                behind: BTreeMap::new(),
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
                if let Some(status) = dispatcher.status(item) {
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
                        // types it (§FS-005-dispatch.9).
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
            Action::OpenThread { item, or_url } => match ThreadScreen::open(item.clone()) {
                Some(screen) => self.screen = Screen::Thread(screen),
                None if or_url => self.open_url(item.url),
                None => self.message = "No messages recorded for this item".to_string(),
            },
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
                message,
            } => {
                self.message = format!("Reacting {emoji}…");
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                match react::post(&target, content) {
                    Ok(()) => {
                        self.message = format!("Reacted {emoji}");
                        if let Screen::Thread(thread) = &mut self.screen {
                            thread.add_local_reaction(message, emoji);
                        }
                    }
                    Err(err) => self.message = err.to_string(),
                }
            }
            Action::OpenActionMenu(item) => {
                let applicable = self.ctx.actions_for(&item);
                // An empty menu is no longer empty: the last entry is always
                // "run a command here…" (§FS-005-dispatch.9), and refusing
                // to open would hide it exactly where nothing is configured.
                match self.ctx.roots.get(&item.project) {
                    Some(root) if root.is_dir() => {
                        let (workspace, branch) = self.ctx.checkout_for(&item, root);
                        let state = self.ctx.workspace_state(&item, root);
                        let checkout = self.ctx.checkouts.get(&item.project).cloned();
                        self.menu = Some(ActionMenu::new(
                            item,
                            root.clone(),
                            workspace,
                            branch,
                            state,
                            checkout,
                            applicable,
                        ));
                    }
                    Some(root) => {
                        self.message = format!(
                            "{} is not checked out ({} is missing)",
                            item.project,
                            root.display()
                        );
                    }
                    None => {
                        self.message = format!("{} has no root in the registry", item.project);
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
            Action::RunWork { root, rhei, label } => {
                self.handover(
                    terminal,
                    "▶",
                    &format!("rhei run — {label}"),
                    &root,
                    || {
                        std::process::Command::new("rhei")
                            .arg("run")
                            .arg(&root)
                            .arg("--rhei")
                            .arg(&rhei)
                            .current_dir(&root)
                            .status()
                    },
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
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "less".to_string());
                let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                self.handover(terminal, "📖", &editor, &dir, || {
                    std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("{editor} \"$1\"", editor = editor))
                        .arg("sh")
                        .arg(&path)
                        .status()
                })?;
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
    /// run exactly as a configured one is (§FS-005-dispatch.9).
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
                        kinds: Vec::new(),
                        requires_checkout: false,
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
    fn handover<F>(
        &mut self,
        terminal: &mut DefaultTerminal,
        icon: &str,
        description: &str,
        cwd: &Path,
        run: F,
    ) -> Result<()>
    where
        F: FnOnce() -> std::io::Result<std::process::ExitStatus>,
    {
        ratatui::restore();
        println!("\n{icon} {description}   ({})\n", cwd.display());
        self.message = match run() {
            Ok(status) if status.success() => format!("{description}: ok"),
            Ok(status) => format!("{description}: {status}"),
            Err(err) => format!("{description}: failed to run: {err}"),
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

        let step = |command: &str, icon: &str, description: &str, cwd: &Path, workspace: &Path| {
            println!("\n▶ {icon} {description}   ({})", cwd.display());
            println!("  $ {command}\n");
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(cwd)
                .envs(actions::item_env(
                    &menu.item,
                    &menu.root,
                    workspace,
                    menu.branch.as_ref(),
                ))
                .status();
            match status {
                Ok(status) if status.success() => Ok(()),
                Ok(status) => Err(format!("{description}: {status}")),
                Err(err) => Err(format!("{description}: failed to run: {err}")),
            }
        };

        let needs_checkout =
            entry.is_checkout || matches!(entry.gate, actions::Gate::NeedsCheckout);
        let outcome = (|| {
            let mut workspace = menu.workspace.clone();
            if needs_checkout {
                let checkout = menu.checkout.as_ref().expect("gated on checkout config");
                let target = menu
                    .checkout_target()
                    .expect("gated on a missing workspace");
                // The checkout runs in the root — its job is to create the
                // target workspace, which ephor verifies rather than trusts.
                step(
                    &checkout.command,
                    &checkout.icon,
                    &checkout.description,
                    &menu.root,
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
                step(
                    &action.command,
                    &action.icon,
                    &action.description,
                    &workspace,
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
        // A checkout changes what the branch rows should show.
        if needs_checkout {
            self.ctx.recompute_behind();
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
            self.ctx.seen.insert(id, updated_at);
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
            " type  ·  enter sends  ·  esc cancels  ·  ^w word back  ·  ^u clear"
        } else if self.menu.is_some() {
            " j/k move  1-9 run  enter run  esc cancel"
        } else {
            match &self.screen {
                Screen::Navigator => self.navigator.footer(),
                Screen::Thread(thread) => thread.footer(),
                Screen::Gate(gate) => gate.footer(),
                Screen::Work(work) => work.footer(),
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
        Ctx {
            feeds: Vec::new(),
            seen: Seen::new(),
            projects: vec!["widget".to_string()],
            orgs: Vec::new(),
            project_org: BTreeMap::new(),
            branches: BTreeMap::from([("widget".to_string(), vec![branch])]),
            roots: BTreeMap::from([("widget".to_string(), root.to_path_buf())]),
            branch_templates: template
                .map(|template| BTreeMap::from([("widget".to_string(), template.to_string())]))
                .unwrap_or_default(),
            main_branches: BTreeMap::from([("widget".to_string(), "master".to_string())]),
            repo_paths: BTreeMap::new(),
            behind: BTreeMap::new(),
            actions: Vec::new(),
            project_actions: BTreeMap::new(),
            provider_blocks: BTreeMap::new(),
            checkouts: BTreeMap::new(),
            recent_days: 7,
            unread_only: true,
            work: BTreeMap::new(),
        }
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
        ci.kind = ItemKind::Ci;
        ci.state = Some("failing".to_string());

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

    #[test]
    fn checkout_resolves_existing_branch_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace_dir = root.join("you/ABC-42-retry-window");
        std::fs::create_dir_all(&workspace_dir).unwrap();

        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        let (workspace, branch) = ctx.checkout_for(&ticket_item(), root);
        assert_eq!(workspace, workspace_dir);
        assert_eq!(branch.unwrap().ticket.as_deref(), Some("ABC-42"));
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
        let (workspace, _) = ctx.checkout_for(&pr, root);
        assert_eq!(workspace, root.join("someone/feature"));

        // No branch information at all: state is unknown.
        pr.raw = json!({});
        assert_eq!(ctx.item_checked_out(&pr), None);
        assert!(matches!(
            ctx.workspace_state(&pr, root),
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
        ctx.repo_paths = BTreeMap::from([(
            "widget".to_string(),
            vec!["ce".to_string(), "ee".to_string()],
        )]);
        ctx.recompute_behind();
        assert_eq!(
            ctx.behind
                .get(&("widget".to_string(), "you/ABC-42-retry-window".to_string())),
            Some(&5)
        );
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

    #[test]
    fn checkout_falls_back_to_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Branch matched but its workspace directory does not exist.
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        let (workspace, branch) = ctx.checkout_for(&ticket_item(), root);
        assert_eq!(workspace, root);
        assert!(branch.is_some());

        // No branch template at all (plain single-checkout project).
        let ctx = ctx_with_branch(root, None);
        let (workspace, _) = ctx.checkout_for(&ticket_item(), root);
        assert_eq!(workspace, root);
    }
}
