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
mod operations;
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
use crate::feed::config::{load_config, ActionConfig, CheckoutConfig, Handed, StatusConfig};
use crate::feed::model::Item;
use crate::feed::react::{self, ReactTarget};
use crate::feed::reply::{self, ReplyTarget};
use crate::feed::task::Task;
use crate::forest::{Staleness, Standing, Upstream};
use crate::paths;
use crate::registry;
use crate::seams::dossier;
use crate::seams::summons::{self, Outcome, Place, Site};

use actions::{ActionMenu, MenuOutcome};
use gate::GateScreen;
use navigator::NavigatorState;
use operations::OperationsScreen;
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
    /// Where each checked-out branch stands against its own published copy
    /// (§DA-003-upstream-is-the-published-copy) — a different fact from
    /// [`Ctx::behind`]'s distance to main, kept beside it and never summed
    /// with it. Keyed and recomputed the same way.
    pub standing: BTreeMap<(String, String), Standing>,
    /// Which branch each item is on, by `(project, item id)` → index into that
    /// project's branches, and how many items each branch holds. Worked out
    /// when the feeds are read, never while drawing: placing an item means
    /// matching its whole recorded conversation against every branch, and a
    /// frame that does that once per branch row costs a third of a second to
    /// move the cursor one line.
    pub on_branch: BTreeMap<(String, String), usize>,
    pub linked: BTreeMap<(String, String), usize>,
    /// Per project: visible items, unread, and unread awaiting a response.
    /// Counted per rebuild, for the same reason.
    pub stats: BTreeMap<String, (usize, usize, usize)>,
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
    /// repeated later replacing the entry where it already sits — and then the
    /// work that can be handed over about it, because the recipes and the
    /// actions are one menu (§FS-005-dispatch.1).
    ///
    /// `recipes` is the project's resolved list, shipped and configured. It is
    /// handed in rather than read here so that all four sources are selected
    /// against the same measurement of the same checkout: two folds a moment
    /// apart would eventually offer a rebase entry beside a recipe that says
    /// the branch is current.
    pub fn actions_for(
        &self,
        item: &Item,
        recipes: &[crate::work::recipe::Recipe],
    ) -> Vec<ActionConfig> {
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
        // One fold of the item's checkout answers both rebase offers and every
        // selector that asks about it (§FS-004-quick-actions.8).
        let trailing = self.item_trailing(item);
        let facts = trailing
            .as_ref()
            .map(actions::Trailing::facts)
            .unwrap_or_default();
        let mut recognized = crate::feed::providers::quick_actions(blocks, item);
        // ephor's own quick actions, offered because of what is on disk rather
        // than because a source said something (§FS-004-quick-actions.6).
        if let Some(trailing) = &trailing {
            recognized.extend(self.rebase_offers(&item.project, trailing));
        }
        let mut menu = actions::merge(vec![
            recognized,
            self.offers(item, &facts),
            actions::applicable(&self.actions, project, item, &facts),
        ]);
        actions::add_unclaimed(
            &mut menu,
            recipes
                .iter()
                .filter(|recipe| recipe.matches(item, &facts))
                .map(actions::agent_entry)
                .collect(),
        );
        // What work is offered on, for every entry that asks for it whoever
        // wrote it: never about an item that is finished
        // (§FS-005-dispatch.6), and — where the work edits the change — only
        // where the change is on this machine, which is the checkout's
        // question rather than the work's (§FS-004-quick-actions.7). An offer
        // that would be refused on the keystroke is worse than no offer
        // (§FS-004-quick-actions.2).
        let here = matches!(
            self.checkout(item).map(|checkout| checkout.state),
            Some(WorkspaceState::Ready)
        );
        menu.retain(|entry| match &entry.agent {
            Some(recipe) => !item.is_finished() && (here || !recipe.needs_checkout),
            None => true,
        });
        menu
    }

    /// What ephor offers on one checkout that trails something: the replay
    /// onto the project's main branch, and the replay onto the branch's own
    /// published copy (§FS-004-quick-actions.6, §FS-004-quick-actions.8).
    ///
    /// The two are gated apart, because they need different things. The first
    /// has to name the branch it replays onto, so it is offered only where the
    /// project declares a main branch; the second resolves its ref inside each
    /// repository and needs no base named anywhere, so a project that declares
    /// none is still offered it. One implementation, called from an item's
    /// menu and from a branch row's, so the two cannot come to disagree about
    /// what is on offer.
    fn rebase_offers(&self, project: &str, trailing: &actions::Trailing) -> Vec<ActionConfig> {
        let mut offers = Vec::new();
        if let (Some(main_branch), Some(behind)) = (
            self.main_branch(project),
            trailing.behind.filter(|behind| *behind > 0),
        ) {
            offers.push(actions::rebase_action(main_branch, behind));
        }
        // The count already leaves out every repository whose copy is simply
        // its base — that distance is the first entry's — so a checkout of
        // nothing but such repositories measures nothing here and the entry
        // never carries the first one's number under another name
        // (§FS-004-quick-actions.8).
        if let Some(behind) = trailing.behind_upstream.filter(|behind| *behind > 0) {
            offers.push(actions::upstream_rebase_action(
                trailing.published.as_deref(),
                behind,
            ));
        }
        offers
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

    /// Where the item's own checkout stands. What decides this is whether the
    /// item resolves to a branch workspace on disk, never what kind of row it
    /// is (§FS-004-quick-actions.6): a change is stale or it is not, and a
    /// forge having filed a pull request about it is not the fact being acted
    /// on. None where nothing resolves — no branch, or a workspace that was
    /// never checked out — because an offer that would fail on the keystroke
    /// is worse than no offer (§FS-004-quick-actions.2).
    fn item_trailing(&self, item: &Item) -> Option<actions::Trailing> {
        let (name, _) = self.effective_branch(item);
        self.branch_trailing(&item.project, &name?)
    }

    /// How far one branch's checkout trails: the project's main branch and its
    /// own published copy, summed across its forest (§AR-004-forest.1), what
    /// that copy is called, and whether it is the base again
    /// (§FS-004-quick-actions.8). One fold, read fresh — a menu is opened
    /// rarely enough to measure rather than remember. None where the workspace
    /// is not on disk, which is the checkout's question rather than the
    /// rebase's (§FS-004-quick-actions.7).
    fn branch_trailing(&self, project: &str, branch: &str) -> Option<actions::Trailing> {
        let placement = self.placements.get(project)?;
        let workspace = placement
            .workspace_for(branch)
            // A project without branch workspaces works in its root.
            .unwrap_or_else(|| placement.root.clone());
        if !workspace.is_dir() {
            return None;
        }
        let mut trailing = actions::Trailing::of(&placement.forest(&workspace));
        // A distance to a base nobody named is not a fact anything here acts
        // on: the row does not show it and the entry has nothing to put in
        // "rebase onto …" (§FS-004-quick-actions.6), so a selector asking
        // whether this branch is behind must not be answered `true` from it
        // either — an entry and the work it hands over cannot be gated on
        // different measurements of the same checkout (§FS-005-dispatch.1).
        if placement.main_branch.is_none() {
            trailing.behind = None;
        }
        Some(trailing)
    }

    /// The menu a branch row opens: ephor's own offers about the branch, with
    /// no matter behind them (§FS-004-quick-actions.6). Only what ephor
    /// recognizes on disk — an item's own menu is where a source's, a
    /// project's and a person's entries belong, since those are selected
    /// against an item and a branch row has none to select against
    /// (§FS-004-quick-actions.2).
    pub fn branch_actions(&self, project: &str, branch: &str) -> Vec<ActionConfig> {
        match self.branch_trailing(project, branch) {
            Some(trailing) => self.rebase_offers(project, &trailing),
            None => Vec::new(),
        }
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
        self.stats.get(project).copied().unwrap_or((0, 0, 0))
    }

    /// Count each project's visible, unread, and awaiting-response items.
    /// One walk of the feed for all three — `items()` rebuilds every matter
    /// into a row, so asking three times costs three times — and one walk per
    /// rebuild rather than per frame, since a draw that counts is a draw whose
    /// cost is paid again every time the cursor moves.
    pub fn recompute_stats(&mut self) {
        let now = Utc::now();
        let mut stats = BTreeMap::new();
        for project in self.projects.clone() {
            let Some(feed) = self.feed(&project) else {
                continue;
            };
            let (mut total, mut unread, mut respond) = (0, 0, 0);
            for item in feed.items() {
                if !item.is_visible(now, self.recent_days) {
                    continue;
                }
                total += 1;
                if cache::is_unread(&self.seen, &item) {
                    unread += 1;
                    if item.needs_response {
                        respond += 1;
                    }
                }
            }
            stats.insert(project, (total, unread, respond));
        }
        self.stats = stats;
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

    /// Whether an item's branch workspace is on disk. None when the state is
    /// unknowable: no branch workspaces, or no branch name.
    ///
    /// Any kind, for the reason the rebase offer is any kind
    /// (§FS-004-quick-actions.6): what the marker reports is a change on this
    /// machine, and a forge having filed a pull request about it is not that
    /// fact. Restricted to pull requests, an issue whose workspace is on disk
    /// was offered the rebase from its own row and shown nothing saying the
    /// branch was there.
    pub fn item_checked_out(&self, item: &Item) -> Option<bool> {
        let placement = self.placements.get(&item.project)?;
        let workspace = placement.workspace_for(&placement.branch_name(item)?)?;
        Some(workspace.is_dir())
    }

    /// Re-measure where each checked-out branch stands: how far it trails its
    /// project's main branch, and how far its own published copy
    /// (§DA-003-upstream-is-the-published-copy) — one fold over the branch
    /// workspace's forest, per repository and then summed (§AR-004-forest.1),
    /// with the behind-main half derived from the standing so the two counts
    /// on a row cannot come from different measurements. Local refs only (no
    /// fetch), so counts are relative to what was last fetched.
    pub fn recompute_behind(&mut self) {
        let mut behind = BTreeMap::new();
        let mut standing = BTreeMap::new();
        for project in &self.projects {
            let Some(placement) = self.placements.get(project) else {
                continue;
            };
            for branch in &placement.branches {
                if !self.branch_checked_out(project, branch) {
                    continue;
                }
                let workspace = placement
                    .workspace_for(&branch.branch)
                    .unwrap_or_else(|| placement.root.clone());
                let stand = placement.forest(&workspace).standing();
                let staleness = stand.staleness();
                // The two facts are gated apart, the same way the two offers
                // are: the distance to main is only a distance to something a
                // project named, while the distance to the branch's own copy
                // is answered inside each repository and needs no such name
                // (§FS-004-quick-actions.6). A project that declares no main
                // branch shows, and is offered, the second alone.
                if placement.main_branch.is_some() && staleness.total().is_some() {
                    behind.insert((project.clone(), branch.branch.clone()), staleness);
                }
                if stand
                    .repos
                    .iter()
                    .any(|repo| repo.upstream != Upstream::Unknown)
                {
                    standing.insert((project.clone(), branch.branch.clone()), stand);
                }
            }
        }
        self.behind = behind;
        self.standing = standing;
    }

    /// How far one branch's checkout trails, per repository. None where it was
    /// never measured — no checkout, or nothing measurable in it.
    pub fn branch_behind(&self, project: &str, branch: &str) -> Option<&Staleness> {
        self.behind.get(&(project.to_string(), branch.to_string()))
    }

    /// Where one branch's checkout stands against its published copies, per
    /// repository. None where nothing was read — no checkout, or nothing on a
    /// branch in it (§DA-003-upstream-is-the-published-copy).
    pub fn branch_standing(&self, project: &str, branch: &str) -> Option<&Standing> {
        self.standing
            .get(&(project.to_string(), branch.to_string()))
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

    /// Place every item on a branch, once, for every project. Each item is
    /// matched against the project's whole branch list — so a row's group and
    /// the count on the branch above it are the same answer — and the result
    /// is kept, because matching is the expensive thing here and a draw must
    /// not do it (§FS-008-attribution.2).
    pub fn recompute_placements(&mut self) {
        self.place_scope(None);
    }

    /// The same pass over one project. A refresh lands one project at a time
    /// (§FS-001-forge-interface.7), and a landing changes that project's feed
    /// and no other, so re-placing the whole site pays for every project on
    /// every arrival — the same matching N times over a run of N projects,
    /// where one project's worth is the whole of what moved.
    pub fn recompute_placements_for(&mut self, project: &str) {
        self.place_scope(Some(project));
    }

    /// One pass, scoped to every project or to one. Both go through this,
    /// rather than being written twice: a row filed one way while the reader
    /// is mid-scan and another way when the run finishes is the disagreement
    /// keeping one answer exists to prevent (§AR-004-forest.3).
    fn place_scope(&mut self, only: Option<&str>) {
        let scope: Vec<String> = match only {
            Some(project) => vec![project.to_string()],
            None => self.projects.clone(),
        };
        let mut on_branch = BTreeMap::new();
        let mut linked = BTreeMap::new();
        for project in &scope {
            let (Some(placement), Some(feed)) = (self.placements.get(project), self.feed(project))
            else {
                continue;
            };
            for item in feed.items() {
                let Some(index) = crate::branches::place(&item, &placement.branches) else {
                    continue;
                };
                on_branch.insert((project.clone(), item.id.clone()), index);
                let key = (project.clone(), placement.branches[index].branch.clone());
                *linked.entry(key).or_insert(0) += 1;
            }
        }
        match only {
            // Everything was re-placed, so everything the maps held is
            // replaced: a project the registry has stopped describing takes
            // its rows out with it.
            None => {
                self.on_branch = on_branch;
                self.linked = linked;
            }
            // Only this project was answered for, so only its entries go —
            // dropped first, because an item that left its feed must not keep
            // the branch it had — and the rest of the site stands untouched.
            Some(project) => {
                self.on_branch
                    .retain(|(owner, _), _| owner.as_str() != project);
                self.linked
                    .retain(|(owner, _), _| owner.as_str() != project);
                self.on_branch.extend(on_branch);
                self.linked.extend(linked);
            }
        }
    }

    /// Which of the project's branches this item is on, as an index into
    /// [`Ctx::branches`]. None for an item on no branch of this project.
    pub fn item_branch(&self, project: &str, item: &Item) -> Option<usize> {
        self.on_branch
            .get(&(project.to_string(), item.id.clone()))
            .copied()
    }

    /// How many of the project's items are on this branch — the size of the
    /// group the branch row heads.
    pub fn branch_linked(&self, project: &str, branch: &BranchInfo) -> usize {
        self.linked
            .get(&(project.to_string(), branch.branch.clone()))
            .copied()
            .unwrap_or(0)
    }

    /// The project's items that belong to this branch, in feed order.
    pub fn items_on_branch(&self, project: &str, branch: &BranchInfo) -> Vec<Item> {
        let Some(position) = self
            .branches(project)
            .iter()
            .position(|other| other.branch == branch.branch)
        else {
            return Vec::new();
        };
        match self.feed(project) {
            Some(feed) => feed
                .items()
                .filter(|item| self.item_branch(project, item) == Some(position))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Best link for a branch row: its most urgent matching feed item.
    pub fn branch_url(&self, project: &str, branch: &BranchInfo) -> Option<String> {
        self.items_on_branch(project, branch)
            .into_iter()
            .filter(|item| item.url.is_some())
            .max_by_key(|item| (item.needs_response, item.updated_at))
            .and_then(|item| item.url)
    }
}

pub(crate) use crate::branches::WorkspaceState;

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
    /// The same menu on a branch row, which has no item behind it: what ephor
    /// offers about the branch itself (§FS-004-quick-actions.6).
    OpenBranchActions {
        project: String,
        branch: BranchInfo,
    },
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
        /// Whose plan this is: the ledger entry behind it answers which hand
        /// rides the run (§FS-005-dispatch.14).
        item: String,
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
    /// The operations board, watch-only (§FS-005-dispatch.15).
    Operations(OperationsScreen),
}

/// How often the interface glances at the work artifacts between key reads
/// (§FS-005-dispatch.15.1): a clock gates stat calls, a changed timestamp
/// gates the re-read, and nothing is ever read while drawing.
const WORK_TICK: Duration = Duration::from_secs(2);

struct App {
    ctx: Ctx,
    navigator: NavigatorState,
    screen: Screen,
    /// The screen the reader was on when the operations board opened over
    /// it — one modal layer, restored by Esc. What the board's Enter opens
    /// replaces the pair rather than nesting (§FS-005-dispatch.15): only a
    /// Back from the board itself restores this.
    saved: Option<Screen>,
    /// Open action menu, drawn over the active screen.
    menu: Option<ActionMenu>,
    /// A line the reader is typing, drawn over everything else.
    prompt: Option<Prompt>,
    /// The half of ephor that hands work over (§FS-005-dispatch). None when
    /// the registry could not be read for it — the inbox still works.
    dispatcher: Option<crate::work::Dispatcher>,
    /// A refresh running underneath this screen, where one is
    /// (§FS-001-forge-interface.7).
    refresh: Option<crate::feed::refresh::BackgroundRefresh>,
    /// The work configuration, kept for the board and the tick: both read
    /// the runtime's artifacts through the binding (§AR-007-runtime.1).
    work: crate::work::recipe::WorkConfig,
    /// When the work artifacts were last glanced at, and the newest write
    /// seen then (§FS-005-dispatch.15.1).
    ticked_at: std::time::Instant,
    work_seen: Option<std::time::SystemTime>,
    message: String,
}

/// What a menu entry's summons is told it is about: a matter where there is
/// one, and otherwise the project and the branch, because a branch row has no
/// matter and a stand-in one would put a kind, a source and an id into the
/// contract that nothing filed (§AR-002-summons.1).
///
/// The branch subject says the item id too, and says it empty. A summons
/// inherits the environment it was started in (§AR-002-summons), so a variable
/// left unset is whatever the shell that launched ephor happened to hold — and
/// an entry reading `$EPHOR_ITEM_ID` would bind this branch's rebase to
/// somebody else's matter.
fn menu_dossier(
    menu: &ActionMenu,
    workspace: &Path,
    forest: Option<&crate::forest::Forest>,
) -> Vec<(String, String)> {
    match menu.subject.item() {
        Some(item) => dossier::of_item(item, &menu.root, workspace, menu.branch.as_ref(), forest),
        None => dossier::of_branch(
            menu.subject.project(),
            &menu.root,
            workspace,
            menu.branch.as_ref(),
            forest,
        ),
    }
}

/// What an entry says about who gets its work, out of the answer the seven steps
/// gave (§FS-005-dispatch.14). A hand that cannot be asked right now is named
/// with the reason rather than hidden; a choice that cannot stand is the whole
/// reason and refuses the entry; and where nobody named anybody the sentence
/// is the runtime's own — with no runner bound, the *workable* rung's, because
/// there is nobody to ask and the ticket is written all the same.
fn who_gets_it(choice: &crate::work::runtime::roster::Choice, unbound: Option<&str>) -> Handed {
    use crate::work::runtime::roster::Choice;
    let says = match choice {
        Choice::Chosen { hand, effort, .. } => {
            let at = match effort {
                Some(effort) => format!(" at {effort}"),
                None => String::new(),
            };
            match &hand.available {
                Some(why) => format!("{}{at} (unavailable: {why})", hand.id),
                None => format!("{}{at}", hand.id),
            }
        }
        Choice::Refused(why) => why.clone(),
        Choice::Unasked { note: Some(note) } => note.clone(),
        Choice::Unasked { note: None } => unbound
            .map(str::to_string)
            .unwrap_or_else(|| "whoever the runtime picks".to_string()),
    };
    Handed {
        says,
        refusal: match choice {
            Choice::Refused(why) => Some(why.clone()),
            _ => None,
        },
    }
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
                standing: BTreeMap::new(),
                on_branch: BTreeMap::new(),
                linked: BTreeMap::new(),
                stats: BTreeMap::new(),
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
            saved: None,
            menu: None,
            prompt: None,
            dispatcher: crate::work::Dispatcher::load(config).ok(),
            refresh: None,
            work: config.work.clone(),
            ticked_at: std::time::Instant::now(),
            work_seen: None,
            message: String::new(),
        };
        app.reload_feeds()?;
        // What is on disk now is the baseline the tick moves from: the load
        // just read it, and re-reading it two seconds in would be a glance
        // at nothing (§FS-005-dispatch.15.1).
        app.work_seen = app.work_wrote();
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
        self.ctx.recompute_placements();
        self.ctx.recompute_capabilities();
        self.ctx.recompute_resurfacing();
        self.reload_work();
        self.rebuild_view();
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

            // Between key reads, never instead of them: whatever the refresh
            // beneath this screen has finished lands here, and the screen goes
            // back to being the reader's (§FS-001-forge-interface.7).
            if self.collect_refresh()? {
                self.reload_operations();
                continue;
            }

            // Also between key reads: the clock-gated glance at the work
            // artifacts, so what the runtime moved on disk surfaces without
            // waiting for a refresh (§FS-005-dispatch.15.1).
            if self.tick() {
                continue;
            }

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
                        // An entry that carries a brief is handed over rather
                        // than run: the terminal stays where it is, because
                        // nothing runs in front of the reader
                        // (§FS-005-dispatch.1).
                        } else if entry.action.agent.is_some() {
                            self.dispatch_entry(&menu, &entry);
                        } else {
                            self.run_menu_entry(terminal, &menu, &entry)?;
                        }
                    }
                }
                continue;
            }
            // The operations board opens from anywhere over whatever is on
            // screen, and closes back to it (§FS-005-dispatch.15). Below the
            // prompt and the menu: a `;` typed into either is theirs — and
            // below a screen's own modal for the same reason, since a board
            // opened over an armed reaction picker leaves it armed beneath.
            let inside = match &self.screen {
                Screen::Thread(thread) => thread.is_picking(),
                _ => false,
            };
            if key.code == KeyCode::Char(';') && !inside {
                self.toggle_operations();
                continue;
            }
            let action = match &mut self.screen {
                Screen::Navigator => self.navigator.handle_key(&self.ctx, key.code),
                Screen::Thread(thread) => thread.handle_key(key.code),
                Screen::Gate(gate) => gate.handle_key(key.code),
                Screen::Work(work) => work.handle_key(key.code),
                Screen::Operations(board) => board.handle_key(key.code),
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
            Action::Back => {
                // The board is one modal layer: leaving it restores the
                // screen it opened over, and leaving anything else drops any
                // stale slot on the way to the navigator
                // (§FS-005-dispatch.15).
                self.screen = match self.saved.take() {
                    Some(previous) if matches!(self.screen, Screen::Operations(_)) => previous,
                    _ => Screen::Navigator,
                }
            }
            Action::ToggleUnread => {
                self.ctx.unread_only = !self.ctx.unread_only;
                self.rebuild_view();
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
                // The recipes this project offers, so the menu carries the
                // work that can be handed over about the item beside the
                // commands that can be run on it (§FS-005-dispatch.1).
                let recipes = self
                    .dispatcher
                    .as_ref()
                    .map(|dispatcher| dispatcher.recipes(&item.project))
                    .unwrap_or_default();
                let mut applicable = self.ctx.actions_for(&item, &recipes);
                self.name_the_hands(&item, &mut applicable, config);
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
                            actions::Subject::Item(Box::new(item)),
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
            // The same menu, opened from the row the fact is shown on
            // (§FS-004-quick-actions.6). It carries ephor's own offers only:
            // there is no item here for a source's, a project's or a person's
            // entries to be selected against.
            // A branch row carries no recipes for the same reason it carries
            // no configured entries: work is asked for about a matter, and
            // there is none here (§FS-005-dispatch.2).
            Action::OpenBranchActions { project, branch } => {
                let applicable = self.ctx.branch_actions(&project, &branch.branch);
                let refusal = self.ctx.can(&project).refusal(&[Rung::Placed]);
                match self.ctx.placement(&project).cloned() {
                    Some(placement) if refusal.is_none() => {
                        let root = placement.root.clone();
                        // A branch whose workspace the project puts somewhere
                        // and has not got there yet is the checkout's question
                        // first (§FS-004-quick-actions.7); a project that keeps
                        // one checkout at its root is always ready. Where the
                        // target is not there the commands run in the root —
                        // the same fallback [`Placement::checkout`] makes for
                        // an item (§AR-004-forest.3), because pointing
                        // `EPHOR_WORKSPACE` at a directory that does not exist
                        // is an offer that fails on the keystroke
                        // (§FS-004-quick-actions.2).
                        let (workspace, state) = match placement.workspace_for(&branch.branch) {
                            None => (root.clone(), WorkspaceState::Ready),
                            Some(target) if target.is_dir() => (target, WorkspaceState::Ready),
                            Some(target) => (root.clone(), WorkspaceState::Missing(target)),
                        };
                        let checkout = self.ctx.checkouts.get(&project).cloned();
                        let can = self.ctx.can(&project);
                        self.menu = Some(ActionMenu::new(
                            actions::Subject::Branch {
                                project: project.clone(),
                                branch: branch.branch.clone(),
                            },
                            root,
                            workspace,
                            Some(branch),
                            state,
                            checkout,
                            &can,
                            applicable,
                        ));
                    }
                    _ => {
                        self.message = refusal
                            .unwrap_or_else(|| format!("{project} has no root in the registry"));
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
                item,
                root,
                checkout,
                plan_id,
                label,
            } => {
                // The runtime is a rung: refused here in the same words the
                // command line uses, instead of handing the terminal over to a
                // command that cannot start (§AR-005-capabilities.2). Before
                // the hand is resolved and before anything is ceded, because a
                // refusal answered after the terminal is gone is answered too
                // late (§FS-005-dispatch.14).
                //
                // False, not true: this arm refuses the run, it does not end the
                // session. Returning quit here shut the inbox down on a machine
                // with no runtime bound — the one machine where the refusal is
                // the whole point of the message. The screen ahead of it does
                // not offer the key at all when nothing can run, so this is the
                // second line of defence: a screen built before the runtime
                // left `PATH` can still send the action.
                if let Some(refusal) = crate::work::runtime::refusal(&config.work) {
                    self.message = refusal;
                    return Ok(false);
                }
                // Who gets this run, resolved the way `work run` resolves it —
                // a hand the plan language could not spell rides here as the
                // runtime's own agent flags (§FS-005-dispatch.14). This run
                // names one plan and advances no other, so that plan's tickets
                // settle its flags alone and there is nothing to group.
                let (hand, notes) = match &mut self.dispatcher {
                    Some(dispatcher) => dispatcher.run_hand_for(&item),
                    None => (None, Vec::new()),
                };
                // The checkout, not the plan directory: it is where the work
                // is, and where the runtime falls back to when a workspace has
                // no one repository to be found by looking.
                self.handover(
                    terminal,
                    "▶",
                    &format!(
                        "{} — {label}{}",
                        crate::work::runtime::label(&config.work),
                        match &hand {
                            Some(hand) => format!(" · {}", hand.describe()),
                            None => String::new(),
                        }
                    ),
                    &Site::root(&checkout),
                    &crate::work::runtime::summons_with(
                        &config.work,
                        &root,
                        std::slice::from_ref(&plan_id),
                        hand.as_ref(),
                        &[],
                    ),
                )?;
                // The runtime just advanced the plans this reads.
                self.reload_work();
                self.rebuild_view();
                if let Screen::Work(screen) = &self.screen {
                    let item = screen.item.clone();
                    self.open_work(item);
                }
                // What the resolution had to say, kept for after the run: the
                // terminal was the runtime's while it worked, and a note
                // printed into it would have scrolled away with the run
                // (§FS-005-dispatch.14).
                if !notes.is_empty() {
                    self.message = format!("{} · {}", self.message, notes.join(" · "));
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
                // The reader may have edited what the screens read.
                self.reload_work();
                self.reload_operations();
            }
            Action::Refresh => self.start_refresh(config),
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
                self.rebuild_view();
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
        // Whether anything here can run a plan, answered before the screen
        // advertises the key rather than when it is pressed
        // (§FS-004-quick-actions.2).
        let refusal = crate::work::runtime::refusal(&self.work);
        self.screen = Screen::Work(WorkScreen::new(item, status, offers, refusal));
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
        self.hand_over(item, &recipe);
    }

    /// An agent entry of the action menu, handed over
    /// (§FS-005-dispatch.1). The recipe rides on the entry, so this dispatches
    /// what the row was built from rather than looking something up by name
    /// and hoping it is the same thing — and it goes through
    /// [`App::hand_over`], the one implementation, so the ledger sees a menu
    /// dispatch and a work-screen dispatch alike (§FS-005-dispatch.4).
    fn dispatch_entry(&mut self, menu: &ActionMenu, entry: &actions::MenuEntry) {
        if let actions::Gate::Blocked(reason) = &entry.gate {
            self.message = reason.clone();
            return;
        }
        let (Some(item), Some(recipe)) = (menu.subject.item().cloned(), entry.action.agent.clone())
        else {
            self.message = "There is no matter here to open work about".to_string();
            return;
        };
        self.hand_over(&item, &recipe);
        // Where the reader pressed is not a fact about the work: they land on
        // the same screen the work key would have shown them.
        self.open_work(item);
    }

    /// Handing one recipe over about one item, and saying what landed. Both
    /// keys that dispatch come through here (§FS-005-dispatch.4).
    fn hand_over(&mut self, item: &Item, recipe: &crate::work::recipe::Recipe) {
        let Some(dispatcher) = &mut self.dispatcher else {
            self.message = "Work needs the registry, which could not be read".to_string();
            return;
        };
        // The screen below already shows the plan and its tickets, so the
        // header says what was asked for rather than repeating a long path.
        self.message = match dispatcher.dispatch(item, recipe, false) {
            Ok(crate::work::Outcome::Opened { ticket, .. })
            | Ok(crate::work::Outcome::Reopened { ticket, .. }) => match dispatcher.save() {
                Ok(()) => format!("{} {} — {ticket}", recipe.icon, recipe.description),
                Err(err) => err.to_string(),
            },
            Ok(outcome) => outcome.describe(),
            Err(err) => err.to_string(),
        };
        self.reload_work();
        self.rebuild_view();
    }

    /// Who would get each piece of work in this menu, before the key is
    /// pressed (§FS-005-dispatch.14). Resolved through the one implementation
    /// that answers it at dispatch, and against the work root the dispatch
    /// will use, so what the row says and what the ticket gets cannot come
    /// apart. A choice that cannot stand rides along as its whole reason, and
    /// the entry is shown unable to run (§FS-006-project-interface.9).
    fn name_the_hands(&mut self, item: &Item, menu: &mut [ActionConfig], config: &StatusConfig) {
        if !menu.iter().any(|entry| entry.agent.is_some()) {
            return;
        }
        let Some(root) = self.work_root(item, config) else {
            return;
        };
        // With no runner bound there is nobody to ask, and the entry says so
        // in the workable rung's own words rather than naming a hand it does
        // not have (§FS-005-dispatch.14). The ticket is written all the same.
        let unbound = crate::work::runtime::refusal(&self.work);
        let work = config
            .projects
            .get(&item.project)
            .map(|project| project.work.clone());
        let Some(dispatcher) = &mut self.dispatcher else {
            return;
        };
        for entry in menu.iter_mut() {
            let Some(recipe) = &entry.agent else {
                continue;
            };
            // A recipe spelling the runtime's own execution identity has
            // pinned itself and no table displaces it, narrowing included
            // (§FS-006-project-interface.9). The row says that rather than
            // naming a hand the dispatch will not use.
            if recipe.hand.is_none() && (recipe.target.is_some() || recipe.model.is_some()) {
                let what = format!(
                    "recipe '{}' pins the runtime's own execution identity",
                    recipe.id
                );
                let refusal = crate::work::runtime::roster::refuse_unnamed(work.as_ref(), &what);
                entry.hand = Some(Handed {
                    says: refusal.clone().unwrap_or(what),
                    refusal,
                });
                continue;
            }
            let pinned = recipe.hand.clone();
            let choice = dispatcher.hand(&item.project, &recipe.id, None, pinned.as_ref(), &root);
            entry.hand = Some(who_gets_it(&choice, unbound.as_deref()));
        }
    }

    /// Where this item's work root would be: the template the dispatcher and
    /// `ephor checkout` both resolve (§FS-006-project-interface.7), rendered
    /// from the item's own checkout. Read rather than created — a menu that
    /// opened would otherwise make a work root on every item it was opened on.
    fn work_root(&self, item: &Item, config: &StatusConfig) -> Option<PathBuf> {
        let checkout = self.ctx.checkout(item)?;
        let root = self.ctx.root(&item.project)?.to_path_buf();
        let template = crate::work::root_template(
            &self.work,
            config
                .projects
                .get(&item.project)
                .map(|project| &project.work),
        );
        let subject = crate::work::dossier::Subject {
            item,
            checkout: &checkout,
            root: &root,
        };
        Some(PathBuf::from(crate::paths::resolve_path(
            &crate::work::dossier::render(&template, &subject.placeholders()),
        )))
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
        self.rebuild_view();
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
            let project = menu.subject.project();
            let forest = self
                .ctx
                .placement(project)
                .map(|placement| placement.forest(workspace));
            let carrying = menu_dossier(menu, workspace, forest.as_ref());
            let summons = summons::Summons::new(description, command).carrying(carrying);
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
        self.rebuild_view();
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

    /// Rebuild the view, having first settled everything a row shows that
    /// would otherwise be worked out again on every frame. A draw reads what
    /// is already decided and does no matching and no counting: the cursor
    /// moves without rebuilding, so anything left in the draw path is paid
    /// once per keystroke.
    fn rebuild_view(&mut self) {
        self.ctx.recompute_stats();
        self.navigator.rebuild(&self.ctx);
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
        self.rebuild_view();
        Ok(())
    }

    /// Start a refresh and give the screen straight back
    /// (§FS-001-forge-interface.7). What it asks for is what the view shows:
    /// one project in Detail, every configured project otherwise.
    fn start_refresh(&mut self, config: &StatusConfig) {
        if self.refresh.is_some() {
            self.message = "Already refreshing".to_string();
            return;
        }
        let only = self.navigator.refresh_filter(&self.ctx);
        match crate::feed::refresh::BackgroundRefresh::start(config, only.as_deref()) {
            Ok(refresh) => {
                // The header carries the run from here; what it says about the
                // last thing that happened is finished with.
                self.message.clear();
                self.refresh = Some(refresh);
            }
            Err(err) => self.message = err.to_string(),
        }
    }

    /// Take in whatever the running refresh has finished. Returns true when
    /// something changed on screen, so the caller redraws rather than waiting
    /// out its poll.
    fn collect_refresh(&mut self) -> Result<bool> {
        let Some(refresh) = &mut self.refresh else {
            return Ok(false);
        };
        let arrived = refresh.collect();
        let done = refresh.done();
        if arrived.is_empty() && !done {
            return Ok(false);
        }
        // Each project takes its place as it answers, rather than the whole
        // run landing at the pace of its slowest forge
        // (§FS-001-forge-interface.7).
        for landed in &arrived {
            self.absorb(&landed.project)?;
        }
        if done {
            let mut refresh = self.refresh.take().expect("a refresh is in flight");
            refresh.finish();
            // Named, not counted. "6 provider warnings" is the same sentence
            // whether a forge has been uninstalled for months or a laptop is
            // off the VPN for a minute, and in both cases the sections those
            // providers fill just look empty.
            self.message = refresh.summary();
            // Once, at the end: what a checkout trails and what a project can
            // do are questions for git and the disk, not answers a forge just
            // sent, and asking them per arrival would put the cost the run
            // avoided back on the screen a project at a time.
            self.reload_feeds()?;
        }
        Ok(true)
    }

    /// Take one project's newly written feed into the interface, without the
    /// passes that ask the world. The reader is mid-scan: this is the cheap
    /// half of [`App::reload_feeds`], and the rest waits for the end of the
    /// run (§FS-001-forge-interface.7).
    fn absorb(&mut self, project: &str) -> Result<()> {
        let landed = cache::load_feed(project)?.unwrap_or_else(|| ProjectFeed {
            project: project.to_string(),
            ..ProjectFeed::default()
        });
        let Some(slot) = self
            .ctx
            .feeds
            .iter_mut()
            .find(|feed| feed.project == project)
        else {
            // Configured, but not among the feeds this screen was built over.
            // There is no row to put it next to.
            return Ok(());
        };
        *slot = landed;
        self.ctx.recompute_resurfacing();
        // Where each new item sits, in the same pass. This is the cheap half —
        // an in-memory fold over what just landed, asking the world nothing —
        // and the tree reads the answer rather than placing rows itself, so
        // skipping it files every item that arrives mid-run under *not linked
        // to a branch* and undercounts the branch above it until the whole run
        // finishes (§FS-001-forge-interface.7, §FS-008-attribution.2). Over
        // this project alone: it is the only one whose feed moved, and the
        // whole-site pass would re-match every other project's items again on
        // every arrival.
        self.ctx.recompute_placements_for(project);
        self.reload_work();
        self.rebuild_view();
        Ok(())
    }

    /// Open the operations board over whatever is on screen, or close it
    /// back to where the reader was (§FS-005-dispatch.15).
    fn toggle_operations(&mut self) {
        if matches!(self.screen, Screen::Operations(_)) {
            self.screen = self.saved.take().unwrap_or(Screen::Navigator);
            return;
        }
        let (rows, refusal) = self.board_rows();
        let board = Screen::Operations(OperationsScreen::new(rows, refusal));
        self.saved = Some(std::mem::replace(&mut self.screen, board));
    }

    /// The board's rows, built off the draw path (§FS-005-dispatch.15):
    /// every execution root the ledger knows, grouped — rhei locks per root
    /// and ephor's work root is per branch workspace, so two items in one
    /// workspace are one operation — with the runtime's artifacts answering
    /// what is live there. From the ledger for now, which is every operation
    /// about an item ephor dispatched; enumerating the work roots themselves
    /// is a later task.
    fn board_rows(&self) -> (Vec<operations::OpRow>, Option<String>) {
        use crate::work::runtime::watch;
        let Some(dispatcher) = &self.dispatcher else {
            return (
                Vec::new(),
                Some("Work needs the registry, which could not be read at startup".to_string()),
            );
        };
        let mut groups: BTreeMap<PathBuf, watch::RootPlans> = BTreeMap::new();
        for (item_id, entry) in &dispatcher.ledger.entries {
            let group = groups
                .entry(entry.root.clone())
                .or_insert_with(|| watch::RootPlans {
                    root: entry.root.clone(),
                    plans: Vec::new(),
                });
            group.plans.push(watch::PlanRef {
                project: entry.project.clone(),
                plan_id: entry.plan_id.clone(),
                path: entry.plan.clone(),
                item: Some(item_id.clone()),
                title: entry.title.clone(),
            });
        }
        let groups: Vec<watch::RootPlans> = groups.into_values().collect();
        let watch::Board {
            operations,
            refusal,
        } = watch::board(&self.work, &groups);
        // Every row's matter in one walk of the feeds, rather than a walk per
        // row: `items()` rebuilds each matter into a row, so asking it once
        // per operation pays for the whole feed once per operation.
        let wanted: std::collections::BTreeSet<String> = operations
            .iter()
            .filter_map(|op| op.item().map(str::to_string))
            .collect();
        let mut matters: BTreeMap<String, Item> = BTreeMap::new();
        if !wanted.is_empty() {
            for feed in &self.ctx.feeds {
                for item in feed.items() {
                    if wanted.contains(&item.id) {
                        matters.insert(item.id.clone(), item);
                    }
                }
            }
        }
        let rows = operations
            .into_iter()
            .map(|op| operations::OpRow {
                item: op.item().and_then(|id| matters.get(id).cloned()),
                // The operation's own plan where a ticket of it names one, and
                // the ledger's for this root otherwise: a live run whose
                // tickets were all filtered out still has a plan behind it,
                // and `e` on that row would otherwise answer that there is
                // none (§FS-005-dispatch.15).
                plan: op.plan().map(Path::to_path_buf).or_else(|| {
                    groups
                        .iter()
                        .find(|group| group.root == op.root)
                        .and_then(|group| group.plans.first())
                        .map(|plan| plan.path.clone())
                }),
                op,
            })
            .collect();
        (rows, refusal)
    }

    /// Rebuild the open board's rows; a closed board costs nothing.
    fn reload_operations(&mut self) {
        if !matches!(self.screen, Screen::Operations(_)) {
            return;
        }
        let (rows, refusal) = self.board_rows();
        if let Screen::Operations(board) = &mut self.screen {
            board.replace(rows, refusal);
        }
    }

    /// The tick (§FS-005-dispatch.15.1): every couple of seconds between key
    /// reads — never in the draw path — glance at what the ledger points at.
    /// A moved timestamp re-reads the plans, so a ticket the runtime parked
    /// resurfaces when it parks (§FS-005-dispatch.9) instead of at the next
    /// refresh; an unmoved one costs stat calls and nothing else. The open
    /// board also probes liveness: neither a run dying nor a run starting
    /// moves a file — the OS takes and releases the lock — so the lock is
    /// probed rather than watched, on the roots the board shows and on the
    /// ones it does not.
    ///
    /// Returns true when something on screen changed, and only then: a board
    /// asking for a frame every couple of seconds regardless is paying to
    /// show the reader what they are already looking at.
    fn tick(&mut self) -> bool {
        if self.ticked_at.elapsed() < WORK_TICK {
            return false;
        }
        self.ticked_at = std::time::Instant::now();
        let newest = self.work_wrote();
        let moved = match (newest, self.work_seen) {
            (Some(now), Some(seen)) => now > seen,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if moved {
            self.work_seen = newest;
            self.reload_work();
            self.rebuild_view();
            self.reload_operations();
            return true;
        }
        let shown = match &self.screen {
            Screen::Operations(board) => board.roots(),
            _ => return false,
        };
        // A run *starting* on a root the board has no row for writes nothing
        // the timestamp above watches — the OS takes its lock, and that is the
        // whole event. So the roots the ledger knows and the board is not
        // showing are probed for exactly that, and one that came alive asks
        // for the rebuild that would give it a row (§FS-005-dispatch.15.1).
        let appeared = self.dispatcher.as_ref().is_some_and(|dispatcher| {
            let mut probed: std::collections::BTreeSet<&Path> = std::collections::BTreeSet::new();
            dispatcher.ledger.entries.values().any(|entry| {
                probed.insert(entry.root.as_path())
                    && !shown.contains(&entry.root)
                    && crate::work::runtime::watch::live(&self.work, &entry.root)
            })
        });
        let work = &self.work;
        let found = match &mut self.screen {
            Screen::Operations(board) => {
                board.repulse(|root| crate::work::runtime::watch::pulse(work, root))
            }
            _ => return false,
        };
        if found.flipped || appeared {
            self.reload_operations();
            return true;
        }
        // Only where something actually moved: a board that redrew itself
        // every couple of seconds regardless would be paying for a frame to
        // show the reader what they are already looking at.
        found.changed
    }

    /// The newest write across everything the ledger points at: each plan
    /// file, and each execution root's own run artifacts. Stat calls only —
    /// the gate in front of every re-read (§FS-005-dispatch.15.1).
    fn work_wrote(&self) -> Option<std::time::SystemTime> {
        let dispatcher = self.dispatcher.as_ref()?;
        let mut newest: Option<std::time::SystemTime> = None;
        let mut fold = |at: Option<std::time::SystemTime>| {
            if let Some(at) = at {
                if newest.map(|seen| at > seen).unwrap_or(true) {
                    newest = Some(at);
                }
            }
        };
        let mut roots: std::collections::BTreeSet<&Path> = std::collections::BTreeSet::new();
        for entry in dispatcher.ledger.entries.values() {
            fold(
                std::fs::metadata(&entry.plan)
                    .and_then(|meta| meta.modified())
                    .ok(),
            );
            roots.insert(&entry.root);
        }
        for root in roots {
            fold(crate::work::runtime::watch::wrote_at(&self.work, root));
        }
        newest
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
            Screen::Operations(board) => board.title(),
        };
        // A screen that stays live during a fetch is also a screen that looks
        // finished, so a run in flight says so and says where it has got to —
        // a half-filled feed read as the whole answer is the same failure as
        // an empty section that only means "not asked yet"
        // (§FS-001-forge-interface.7).
        let progress = match &self.refresh {
            Some(refresh) => format!("{}   ", refresh.progress()),
            None => String::new(),
        };
        frame.render_widget(
            Paragraph::new(format!("{title}   {progress}{}", self.message))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            header_area,
        );

        match &mut self.screen {
            Screen::Navigator => self.navigator.draw(&self.ctx, frame, body_area),
            Screen::Thread(thread) => thread.draw(frame, body_area),
            Screen::Gate(gate) => gate.draw(frame, body_area),
            Screen::Work(work) => work.draw(frame, body_area),
            // The refresh reports on the board additionally — the header
            // above keeps its line (§FS-001-forge-interface.7).
            Screen::Operations(board) => {
                let line = self
                    .refresh
                    .as_ref()
                    .map(crate::feed::refresh::BackgroundRefresh::progress);
                board.draw(frame, body_area, line)
            }
        }
        if let Some(menu) = &self.menu {
            menu.draw(frame, body_area);
        }
        if let Some(prompt) = &self.prompt {
            prompt.draw(frame, body_area);
        }

        let footer = if self.prompt.is_some() {
            " type  ·  enter sends  ·  esc cancels  ·  ^w word back  ·  ^u clear".to_string()
        // Built from what is selected, not fixed for the menu: an entry that
        // hands work over and an entry that runs a command are not the same
        // key (§FS-004-quick-actions.2).
        } else if let Some(menu) = &self.menu {
            menu.footer()
        } else {
            match &self.screen {
                Screen::Navigator => self.navigator.footer().to_string(),
                // Built from what is selected, not fixed per screen
                // (§FS-004-quick-actions.2).
                Screen::Thread(thread) => thread.footer(),
                Screen::Gate(gate) => gate.footer().to_string(),
                Screen::Work(work) => work.footer().to_string(),
                Screen::Operations(board) => board.footer().to_string(),
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

    pub(super) fn ctx_with_branch(root: &Path, template: Option<&str>) -> Ctx {
        let branch = BranchInfo {
            branch: "you/ABC-42-retry-window".to_string(),
            ticket: Some("ABC-42".to_string()),
            active: true,
            is_release: false,
            declared: true,
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
            standing: BTreeMap::new(),
            on_branch: BTreeMap::new(),
            linked: BTreeMap::new(),
            stats: BTreeMap::new(),
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

    /// One project's cached feed, holding one matter the forge put on `branch`.
    fn feed_on(project: &str, key: &str, branch: &str) -> ProjectFeed {
        let matter = crate::matter::Matter {
            key: crate::matter::SubjectKey::stated(key),
            kind: ItemKind::Pr,
            placement: crate::matter::Placement::on(project),
            source: "github-prs".to_string(),
            title: "Retry window".to_string(),
            role: None,
            url: None,
            state: None,
            needs_response: false,
            updated_at: Utc::now(),
            links: Vec::new(),
            discussions: Vec::new(),
            events: Vec::new(),
            fingerprint: Default::default(),
            raw: json!({ "branch": branch }),
        };
        ProjectFeed {
            project: project.to_string(),
            providers: BTreeMap::from([(
                "github-prs".to_string(),
                crate::feed::cache::ProviderSlot {
                    ok: true,
                    matters: vec![matter],
                    ..Default::default()
                },
            )]),
            ..ProjectFeed::default()
        }
    }

    /// A second project beside the fixture's, with a branch and a feed of its
    /// own, so a pass scoped to one has something to leave alone.
    fn with_second_project(ctx: &mut Ctx) {
        let placement = Placement {
            project: "gadget".to_string(),
            branches: vec![BranchInfo {
                branch: "you/XYZ-7-widen".to_string(),
                ticket: Some("XYZ-7".to_string()),
                active: true,
                is_release: false,
                declared: true,
            }],
            ..ctx.placements["widget"].clone()
        };
        ctx.projects.push("gadget".to_string());
        ctx.placements.insert("gadget".to_string(), placement);
        ctx.feeds = vec![
            feed_on(
                "widget",
                "github-prs:acme/widget#42",
                "you/ABC-42-retry-window",
            ),
            feed_on("gadget", "github-prs:acme/gadget#7", "you/XYZ-7-widen"),
        ];
    }

    /// A refresh lands one project at a time, and the placement pass it runs
    /// per landing answers for that project alone: the rest of the site keeps
    /// the rows it had, and what the scoped pass leaves behind is what the
    /// whole-site pass would have left there — one implementation, so the
    /// mid-scan answer and the end-of-run answer cannot disagree
    /// (§FS-001-forge-interface.7, §FS-008-attribution.2).
    #[test]
    fn a_landing_places_its_own_project_and_leaves_the_rest_standing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = ctx_with_branch(tmp.path(), None);
        with_second_project(&mut ctx);
        ctx.recompute_placements();

        let widget = ctx.branches("widget")[0].clone();
        let gadget = ctx.branches("gadget")[0].clone();
        assert_eq!(ctx.branch_linked("widget", &widget), 1);
        assert_eq!(ctx.branch_linked("gadget", &gadget), 1);

        // Widget's feed lands again, this time with the item on no branch the
        // project knows. Only widget is re-placed.
        ctx.feeds[0] = feed_on("widget", "github-prs:acme/widget#42", "you/ABC-99-other");
        ctx.recompute_placements_for("widget");
        assert_eq!(ctx.branch_linked("widget", &widget), 0);
        assert_eq!(ctx.branch_linked("gadget", &gadget), 1);
        // The row that left the branch left the map with it — a stale entry
        // would keep filing it under a branch it is no longer on.
        assert!(ctx
            .on_branch
            .keys()
            .all(|(project, _)| project.as_str() != "widget"));

        // And the two scopes agree about the whole site.
        let scoped = (ctx.on_branch.clone(), ctx.linked.clone());
        ctx.recompute_placements();
        assert_eq!(scoped, (ctx.on_branch.clone(), ctx.linked.clone()));
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
        let menu = ctx.actions_for(&ci, &[]);
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

        let menu = ctx.actions_for(&ticket_item(), &[]);
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
        let menu = ctx.actions_for(&ticket_item(), &[]);
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

    /// How far the item's checkout trails the project's main branch, out of
    /// the one fold the offers read.
    fn behind(ctx: &Ctx, item: &Item) -> Option<u64> {
        ctx.item_trailing(item).and_then(|trailing| trailing.behind)
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

    /// The standing rides beside the behind count, from the same fold: two
    /// distances, two facts — one against the project's main branch, one
    /// against the branch's own published copy, and the branch is read off
    /// each repository's `HEAD`, never the workspace directory's name
    /// (§DA-003-upstream-is-the-published-copy).
    #[test]
    fn the_standing_is_measured_beside_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        let repo = workspace.join("ce");
        repo_behind(&repo, 3);
        // The branch was pushed, then its copy grew two commits this
        // checkout has not pulled — no tracking config, the worktree shape.
        for step in 0..2 {
            git(
                &repo,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("pushed {step}"),
                ],
            );
        }
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/feature", "HEAD"],
        );
        git(&repo, &["reset", "-q", "--hard", "HEAD~2"]);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        ctx.recompute_behind();
        assert_eq!(
            ctx.branch_behind("widget", "you/ABC-42-retry-window")
                .and_then(Staleness::total),
            Some(3)
        );
        let standing = ctx
            .branch_standing("widget", "you/ABC-42-retry-window")
            .expect("the copy was read");
        assert_eq!(standing.behind_upstream(), Some(2));
        assert_eq!(standing.repos[0].branch.as_deref(), Some("feature"));
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Published {
                remote: "origin".to_string(),
                branch: "feature".to_string(),
            }
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
        assert_eq!(behind(&ctx, &pr), Some(5));
        let menu = ctx.actions_for(&pr, &[]);
        assert_eq!(menu[0].description, "rebase onto master (5 behind)");
        assert!(menu[0].command.contains("rebase --project"));
        assert!(menu[0].requires_checkout);

        // Level with master: nothing to replay, so nothing offered.
        for repo in ["ce", "ee"] {
            git(&workspace.join(repo), &["checkout", "-q", "master"]);
        }
        assert_eq!(behind(&ctx, &pr), Some(0));
        assert!(ctx.actions_for(&pr, &[]).is_empty());
    }

    #[test]
    fn the_rebase_is_not_offered_where_there_is_nothing_to_measure() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));

        // The branch workspace was never checked out.
        assert_eq!(behind(&ctx, &ticket_item()), None);
        assert!(ctx.actions_for(&ticket_item(), &[]).is_empty());

        // An item that resolves to no branch at all has nowhere to rebase,
        // whatever kind it is (§FS-004-quick-actions.2).
        let mut nowhere = ticket_item();
        nowhere.title = "Nothing about any branch".to_string();
        assert_eq!(behind(&ctx, &nowhere), None);
        assert!(ctx.actions_for(&nowhere, &[]).is_empty());
    }

    /// The offer follows the branch on disk, not the kind of the row that
    /// mentions it: an issue and a message about the same change are offered
    /// exactly what the pull request is (§FS-004-quick-actions.6).
    #[test]
    fn any_item_that_resolves_to_a_workspace_is_offered_the_rebase() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 4);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let offered = "rebase onto master (4 behind)";
        for kind in [
            ItemKind::Pr,
            ItemKind::Issue,
            ItemKind::Message,
            ItemKind::Ci,
            ItemKind::Status,
        ] {
            let mut item = ticket_item();
            item.kind = kind;
            let menu = ctx.actions_for(&item, &[]);
            assert_eq!(menu.len(), 1, "{kind:?}: {menu:?}");
            assert_eq!(menu[0].description, offered, "{kind:?}");
            // And the entry says nothing about kinds any more, so nothing
            // downstream can narrow it back to pull requests.
            assert!(menu[0].kinds.is_empty(), "{kind:?}");
        }
    }

    /// The two offers are gated apart: replaying onto the published copy
    /// resolves its ref inside each repository, so a project that declares no
    /// main branch is still offered it — and is offered nothing to replay onto
    /// a base nothing names (§FS-004-quick-actions.6).
    #[test]
    fn a_project_with_no_main_branch_is_still_offered_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = root.join("you/ABC-42-retry-window/ce");
        repo_behind(&repo, 3);
        published_ahead(&repo, "feature", 2);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        ctx.placements
            .get_mut("widget")
            .expect("the fixture project")
            .main_branch = None;

        let menu = ctx.actions_for(&ticket_item(), &[]);
        assert_eq!(menu.len(), 1, "{menu:?}");
        assert_eq!(menu[0].id, "rebase-upstream");
        assert_eq!(menu[0].description, "rebase onto origin/feature (2 behind)");

        // The row is gated the same way, so what it shows and what the menu
        // offers cannot disagree: the copy's distance, and no distance to a
        // main branch the project never named.
        ctx.recompute_behind();
        assert!(ctx
            .branch_behind("widget", "you/ABC-42-retry-window")
            .is_none());
        assert_eq!(
            ctx.branch_standing("widget", "you/ABC-42-retry-window")
                .and_then(Standing::behind_upstream),
            Some(2)
        );
    }

    /// The branch row carries the same offers, built by the same code: this is
    /// where a reader looking at a stale branch is standing
    /// (§FS-004-quick-actions.6).
    #[test]
    fn a_branch_row_carries_the_same_offers_as_the_items_on_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);
        published_ahead(&workspace.join("ce"), "feature", 1);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let offered = ctx.branch_actions("widget", "you/ABC-42-retry-window");
        assert_eq!(offered.len(), 2, "{offered:?}");
        assert_eq!(offered[0].description, "rebase onto master (2 behind)");
        assert_eq!(
            offered[1].description,
            "rebase onto origin/feature (1 behind)"
        );

        // The same entries the item's menu carries — one implementation, so a
        // reader cannot be told two different things about one checkout.
        let menu = ctx.actions_for(&ticket_item(), &[]);
        let described = |actions: &[ActionConfig]| -> Vec<(String, String)> {
            actions
                .iter()
                .map(|action| (action.id.clone(), action.command.clone()))
                .collect()
        };
        assert_eq!(described(&offered), described(&menu));

        // A branch whose workspace is not on disk is a checkout question
        // (§FS-004-quick-actions.7), so the rebase is withheld rather than
        // offered and left to fail.
        assert!(ctx
            .branch_actions("widget", "you/never-checked-out")
            .is_empty());
    }

    /// Publish the branch this repository is on and move that copy `commits`
    /// ahead of the checkout — somebody else pushed to it, and no tracking
    /// config was ever written (§DA-003-upstream-is-the-published-copy).
    fn published_ahead(dir: &Path, branch: &str, commits: usize) {
        for index in 0..commits {
            git(
                dir,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("pushed {index}"),
                ],
            );
        }
        git(
            dir,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                "HEAD",
            ],
        );
        if commits > 0 {
            git(dir, &["reset", "-q", "--hard", &format!("HEAD~{commits}")]);
        }
    }

    /// A repository parked on the base itself and tracking it, whose copy is
    /// `commits` ahead: the workspace repository a change does not touch. Its
    /// published copy *is* its base, so both distances are the same distance.
    fn repo_on_the_base(dir: &Path, commits: usize) {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q", "-b", "master"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
        published_ahead(dir, "master", commits);
        git(dir, &["remote", "add", "origin", "."]);
        git(
            dir,
            &["branch", "--set-upstream-to=origin/master", "master"],
        );
    }

    /// The second offer: onto the branch's own published copy, naming the ref
    /// so the two entries differ in the word that matters
    /// (§FS-004-quick-actions.8).
    #[test]
    fn the_rebase_onto_the_published_copy_is_offered_and_names_the_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = root.join("you/ABC-42-retry-window/ce");
        // Level with main, so only the published copy has anything to replay.
        repo_behind(&repo, 0);
        published_ahead(&repo, "feature", 2);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let pr = ticket_item();
        let menu = ctx.actions_for(&pr, &[]);
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].id, "rebase-upstream");
        assert_eq!(menu[0].description, "rebase onto origin/feature (2 behind)");
        assert!(menu[0].command.contains("rebase --upstream --project"));
        assert!(menu[0].requires_checkout);

        // Level with the copy: nothing to replay, so nothing offered — and a
        // branch published nowhere is the same silence for the same reason.
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/feature", "HEAD"],
        );
        assert!(ctx.actions_for(&pr, &[]).is_empty());
        git(&repo, &["update-ref", "-d", "refs/remotes/origin/feature"]);
        assert!(ctx.actions_for(&pr, &[]).is_empty());
    }

    /// A forest where the repositories disagree — one on the change's branch,
    /// one parked on the base — is offered both, because a forest is not one
    /// branch (§FS-004-quick-actions.8). The copy entry counts, and names,
    /// only the repository that trails a copy of its own: the parked one's
    /// distance is the first entry's, not this one's twice.
    #[test]
    fn both_rebases_are_offered_where_the_forest_disagrees() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 0);
        published_ahead(&workspace.join("ce"), "feature", 2);
        repo_on_the_base(&workspace.join("ee"), 1);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        let menu = ctx.actions_for(&ticket_item(), &[]);
        assert_eq!(menu.len(), 2);
        assert_eq!(menu[0].description, "rebase onto master (1 behind)");
        // `ee`'s copy is its base, so it neither counts here nor keeps the
        // entry from naming the one ref the counted repositories share.
        assert_eq!(menu[1].id, "rebase-upstream");
        assert_eq!(menu[1].description, "rebase onto origin/feature (2 behind)");
    }

    /// And where every repository's published copy *is* its base, the copy
    /// entry has nothing of its own to count: only the first is offered
    /// (§FS-004-quick-actions.8).
    #[test]
    fn the_rebase_onto_the_copy_is_not_offered_where_the_copy_is_the_base() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_on_the_base(&workspace.join("ce"), 1);
        repo_on_the_base(&workspace.join("ee"), 1);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        // The distance is real, and the base count carries it; the copy sum
        // leaves it out entirely, so no gate anywhere reads one distance
        // under two names.
        let trailing = ctx
            .item_trailing(&ticket_item())
            .expect("the checkout was measured");
        assert_eq!(trailing.behind, Some(2));
        assert_eq!(trailing.behind_upstream, None);
        let menu = ctx.actions_for(&ticket_item(), &[]);
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].id, "rebase");
        assert_eq!(menu[0].description, "rebase onto master (2 behind)");
    }

    /// A red gate on my own change, on a checkout that trails: the commands
    /// and the work stand in one menu (§FS-005-dispatch.1), each carrying its
    /// own icon, and the replay appears once — the recipe named `rebase` is
    /// what that entry hands its conflict to, not a second row saying the same
    /// thing.
    #[test]
    fn the_menu_carries_the_work_that_can_be_handed_over() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let mut mine = ticket_item();
        mine.role = Some(crate::feed::model::ItemRole::Author);
        mine.raw = json!({ "gate": { "repos": [
            { "repo": "ce", "passed": 1, "failed": 2, "running": 0 }
        ] } });

        let recipes = crate::work::recipe::shipped();
        let menu = ctx.actions_for(&mine, &recipes);
        let described: Vec<(&str, &str, bool)> = menu
            .iter()
            .map(|entry| {
                (
                    entry.icon.as_str(),
                    entry.description.as_str(),
                    entry.agent.is_some(),
                )
            })
            .collect();
        assert_eq!(
            described,
            [
                ("⤴", "rebase onto master (2 behind)", false),
                ("🛠", "fix the red gate", true),
            ],
            "{described:?}"
        );
        // The work rides on the entry whole, so what is dispatched from the
        // menu is the recipe itself (§FS-005-dispatch.4).
        let work = menu[1].agent.as_ref().expect("the recipe rides along");
        assert_eq!(work.id, "fix-gate");
        assert!(work.brief.starts_with("The gate on {title} is red."));
        // And the replay is one entry, the deterministic one.
        assert_eq!(menu.iter().filter(|entry| entry.id == "rebase").count(), 1);
    }

    /// Offered only where it would work (§FS-004-quick-actions.2): work that
    /// edits the change waits on the change being here, work that reads one
    /// does not, and nothing is asked about an item that is finished
    /// (§FS-005-dispatch.6).
    #[test]
    fn work_is_offered_where_it_would_work_and_nowhere_else() {
        let tmp = tempfile::tempdir().unwrap();
        // Nothing checked out: the branch workspace the template names is not
        // on disk.
        let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
        let recipes = crate::work::recipe::shipped();
        let ids = |ctx: &Ctx, item: &Item| -> Vec<String> {
            ctx.actions_for(item, &recipes)
                .into_iter()
                .filter(|entry| entry.agent.is_some())
                .map(|entry| entry.id)
                .collect()
        };

        // Fixing a gate edits the change, so it is the checkout's question
        // first (§FS-004-quick-actions.7).
        let mut mine = ticket_item();
        mine.role = Some(crate::feed::model::ItemRole::Author);
        mine.raw = json!({ "gate": { "repos": [
            { "repo": "ce", "passed": 1, "failed": 2, "running": 0 }
        ] } });
        assert!(ids(&ctx, &mine).is_empty());

        // Reviewing one reads it, and fetches what it needs: offered with
        // nothing on disk at all.
        let mut theirs = ticket_item();
        theirs.role = Some(crate::feed::model::ItemRole::Reviewer);
        assert_eq!(ids(&ctx, &theirs), ["review"]);

        // Merged: there is nothing to ask for about it any more.
        let mut done = theirs.clone();
        done.state = Some("merged".to_string());
        assert!(ids(&ctx, &done).is_empty());
    }

    /// With no runner bound the work is still offered — a ticket is written
    /// whether or not anything can run it — and where the entry would say who
    /// gets it, it says instead that nobody can be asked, in the *workable*
    /// rung's own words (§FS-005-dispatch.14).
    #[test]
    fn with_no_runner_bound_the_work_is_still_offered_and_says_nobody_can_be_asked() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
        let mut theirs = ticket_item();
        theirs.role = Some(crate::feed::model::ItemRole::Reviewer);
        let offered = ctx.actions_for(&theirs, &crate::work::recipe::shipped());
        assert_eq!(
            offered.iter().filter(|entry| entry.agent.is_some()).count(),
            1
        );

        // The rung's own sentence about a runner that is not there.
        let unbound = crate::work::runtime::refusal(&crate::work::recipe::WorkConfig {
            runner: Some("no-such-runner-here".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        })
        .expect("a runner that is not on PATH is refused");
        assert!(unbound.contains("no-such-runner-here"), "{unbound}");

        use crate::work::runtime::roster::{Choice, Hand};
        let nobody = who_gets_it(&Choice::Unasked { note: None }, Some(&unbound));
        assert_eq!(nobody.says, unbound);
        // Said, not refused: the ticket is written all the same.
        assert!(nobody.refusal.is_none());

        // With a runner there and nobody named, the runtime picks unasked.
        let unasked = who_gets_it(&Choice::Unasked { note: None }, None);
        assert_eq!(unasked.says, "whoever the runtime picks");

        // A chosen hand names itself, and carries why it cannot be asked right
        // now rather than vanishing (§FS-005-dispatch.14).
        let chosen = who_gets_it(
            &Choice::Chosen {
                hand: Hand {
                    id: "luna".to_string(),
                    agent: Some("claude-code".to_string()),
                    model: None,
                    provider: None,
                    efforts: vec!["high".to_string()],
                    available: Some("'claude-code' is not on PATH".to_string()),
                },
                effort: Some("high".to_string()),
                whence: "the site's default hand".to_string(),
                note: None,
            },
            None,
        );
        assert_eq!(
            chosen.says,
            "luna at high (unavailable: 'claude-code' is not on PATH)"
        );
        assert!(chosen.refusal.is_none());

        // And a choice that cannot stand is the whole reason, and refuses.
        let refused = who_gets_it(&Choice::Refused("permits only sonnet".to_string()), None);
        assert_eq!(refused.refusal.as_deref(), Some("permits only sonnet"));
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
        assert!(!can.holds(Rung::LocalIssues));
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
        assert!(can.holds(Rung::LocalIssues));

        // A project the registry says nothing about holds nothing, and the
        // table answers rather than being absent.
        assert!(ctx.can("ghost").held().is_empty());
    }

    /// The checkout offered on a branch row can actually run. The entry runs
    /// `ephor checkout`, which needs to be told a branch or a matter it can
    /// read one off (§FS-004-quick-actions.7); a branch row has no matter
    /// (§FS-004-quick-actions.6), so the dossier says the branch and says the
    /// item id empty rather than leaving a stale inherited one to bind the
    /// command to somebody else's change. An offer refused on the keystroke is
    /// worse than no offer (§FS-004-quick-actions.2).
    #[test]
    fn a_branch_rows_checkout_is_told_the_branch_and_no_matter() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
        let branch = ctx.branches("widget")[0].clone();
        let target = tmp.path().join(&branch.branch);
        let entry = actions::checkout_action(&target);
        // Both are named, so the one command serves an item row and a branch
        // row alike.
        assert!(entry.command.contains("--item \"$EPHOR_ITEM_ID\""));
        assert!(entry.command.contains("--branch \"$EPHOR_BRANCH\""));

        let menu = ActionMenu::new(
            actions::Subject::Branch {
                project: "widget".to_string(),
                branch: branch.branch.clone(),
            },
            tmp.path().to_path_buf(),
            tmp.path().to_path_buf(),
            Some(branch.clone()),
            WorkspaceState::Missing(target),
            None,
            &ctx.can("widget"),
            Vec::new(),
        );
        let carried = menu_dossier(&menu, tmp.path(), None);
        let value = |key: &str| {
            carried
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value("EPHOR_BRANCH"), Some(branch.branch.as_str()));
        assert_eq!(value("EPHOR_TICKET"), Some("ABC-42"));
        assert_eq!(value("EPHOR_PROJECT"), Some("widget"));
        // Said, and said empty: an unset variable is whatever the shell that
        // launched ephor held.
        assert_eq!(value("EPHOR_ITEM_ID"), Some(""));

        // An item row is unchanged: its own id, and its own branch.
        let item_menu = ActionMenu::new(
            actions::Subject::Item(Box::new(ticket_item())),
            tmp.path().to_path_buf(),
            tmp.path().to_path_buf(),
            Some(branch.clone()),
            WorkspaceState::Ready,
            None,
            &ctx.can("widget"),
            Vec::new(),
        );
        let carried = menu_dossier(&item_menu, tmp.path(), None);
        let value = |key: &str| {
            carried
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value("EPHOR_ITEM_ID"), Some(ticket_item().id.as_str()));
        assert_eq!(value("EPHOR_BRANCH"), Some(branch.branch.as_str()));
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
