//! The session: everything both surfaces read, opened once and shared
//! (§AR-009-surfaces.2).
//!
//! The feeds, what is seen, where each project is placed (§AR-004-forest.3),
//! what each project can do (§AR-005-capabilities), the branch standings, the
//! configured entries and the provider blocks. This was the interface's own
//! state; it lives below the screen so that a command answers from exactly the
//! data a key would have answered from (§REQ-002-parity.3).

use std::collections::BTreeMap;
use std::path::Path;

use chrono::Utc;
use serde_json::Value;

use crate::branches::{BranchInfo, Checkout, Placement, WorkspaceState};
use crate::capabilities::{Bindings, CapabilitySet};
use crate::error::Result;
use crate::feed::cache::{self, ProjectFeed, Seen};
use crate::feed::config::{ActionConfig, CheckoutConfig, Handed, Minted, StatusConfig};
use crate::feed::model::Item;
use crate::forest::{Staleness, Standing, Upstream};
use crate::registry;

use super::offers;

#[derive(Clone)]
pub struct OrgInfo {
    pub id: String,
    pub name: String,
    pub root: Option<String>,
}

/// What an item's work is doing, a row per open ticket
/// (§FS-005-dispatch.4, §FS-005-dispatch.23). Recomputed from the plans
/// whenever anything could have changed them — never remembered across a
/// change.
pub type WorkLines = Vec<crate::work::WorkLine>;

/// What a finished job's one line is filed under, so it lands on the row the
/// job ran on rather than at the top of the screen (§FS-005-dispatch.17). A
/// record names a matter or a branch and never both (§FS-004-quick-actions.6),
/// and one that names neither is still about its project.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum JobSubject {
    /// The matter it was started about: its project, and its item id.
    Matter(String, String),
    /// The branch it ran on: its project, and the branch.
    Branch(String, String),
    /// The project, where the record names neither.
    Project(String),
}

impl JobSubject {
    /// What a job was about, read from the record it wrote before it started.
    pub fn of(record: &crate::seams::jobs::Record) -> JobSubject {
        match (&record.item, &record.branch) {
            (Some(item), _) => JobSubject::Matter(record.project.clone(), item.clone()),
            (None, Some(branch)) => JobSubject::Branch(record.project.clone(), branch.clone()),
            (None, None) => JobSubject::Project(record.project.clone()),
        }
    }

    /// Whose part of the tree this row sits in. Every subject carries it, so
    /// that a line whose own row is nowhere on the screen still has a project
    /// row to land on rather than being lost.
    pub fn project(&self) -> &str {
        match self {
            JobSubject::Matter(project, _)
            | JobSubject::Branch(project, _)
            | JobSubject::Project(project) => project,
        }
    }
}

/// Shared data both screens read. Mutations go through the shell so screens
/// stay pure key-to-[`Action`] translators.
#[derive(Default)]
pub struct Session {
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
    /// [`Session::behind`]'s distance to main, kept beside it and never summed
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
    pub work: BTreeMap<String, WorkLines>,
    /// The one line each finished job left, filed under the subject it ran on
    /// (§FS-005-dispatch.17). Written when a job is seen to end and read off
    /// again when the reader opens that row: news, not a state, which is why
    /// it is remembered here rather than recomputed from the records.
    pub job_news: BTreeMap<JobSubject, String>,
    /// The half of ephor that hands work over (§FS-005-dispatch). None when
    /// the registry could not be read for it — the watch still works.
    pub dispatcher: Option<crate::work::Dispatcher>,
    /// The work configuration, kept for the board: it reads the runtime's
    /// artifacts through the binding (§AR-007-runtime.1).
    pub work_config: crate::work::recipe::WorkConfig,
    /// The site configuration this session was opened from, kept because a
    /// reading that resolves a work root needs the project's own work block
    /// and re-loading it would be a second answer to one question.
    pub config: StatusConfig,
}

impl Session {
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
    /// The entries written beside the runtime's own workflows
    /// (§FS-005-dispatch.19). They are handed in rather than read here for the
    /// reason the recipes are: reading them asks the binding, and every source
    /// in one menu has to be selected against one measurement of one checkout.
    /// Each arrives with where its workflow was found, which is where the
    /// entry ranks — what the binding ships with what ephor ships, the
    /// project's with the project's offers, the person's with the person's own.
    pub fn actions_with(
        &self,
        item: &Item,
        recipes: &[crate::work::recipe::Recipe],
        beside: &[(crate::work::runtime::workflow::Source, ActionConfig)],
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
            .map(offers::Trailing::facts)
            .unwrap_or_default();
        let mut recognized = crate::feed::providers::quick_actions(blocks, item);
        // ephor's own quick actions, offered because of what is on disk rather
        // than because a source said something (§FS-004-quick-actions.6).
        if let Some(trailing) = &trailing {
            recognized.extend(self.rebase_offers(&item.project, trailing));
        }
        let from = |want: crate::work::runtime::workflow::Source| -> Vec<ActionConfig> {
            beside
                .iter()
                .filter(|(source, entry)| *source == want && entry.matches(item, &facts))
                .map(|(_, entry)| entry.clone())
                .collect()
        };
        recognized.extend(from(crate::work::runtime::workflow::Source::Runtime));
        let mut offered = self.offers(item, &facts);
        offered.extend(from(crate::work::runtime::workflow::Source::Project));
        let mut configured = offers::applicable(&self.actions, project, item, &facts);
        configured.extend(from(crate::work::runtime::workflow::Source::Person));
        let mut menu = offers::merge(vec![recognized, offered, configured]);
        offers::add_unclaimed(
            &mut menu,
            recipes
                .iter()
                .filter(|recipe| recipe.matches(item, &facts))
                .map(offers::agent_entry)
                .collect(),
        );
        // What work is offered on, for every entry that asks for it whoever
        // wrote it: never about an item that is finished
        // (§FS-005-dispatch.6), and — where the work edits the change — only
        // where the change is on this machine, which is the checkout's
        // question rather than the work's (§FS-004-quick-actions.7). An offer
        // that would be refused on the keystroke is worse than no offer
        // (§FS-004-quick-actions.2).
        let placed = self.checkout(item).map(|checkout| checkout.state);
        let here = matches!(placed, Some(WorkspaceState::Ready));
        // The one matter a `branch` template applies to: one on no branch at
        // all, where there is nothing for a template to displace and no
        // workspace for work that edits the change to run in. Such an entry
        // says which branch it belongs on and dispatch makes it, so it is
        // offered where its template can render (§FS-005-dispatch.25) — and
        // what a valid template comes to here is the gate's answer, not this
        // one's. A real field this matter has empty means the entry does not
        // serve it, so it is withheld before naming (§FS-005-dispatch.27).
        let branchless = matches!(placed, Some(WorkspaceState::Unmatched));
        let offered = |needs_checkout: bool, branch: &Option<String>| {
            here || !needs_checkout || (branchless && branch.is_some())
        };
        let placement = self.placement(&item.project);
        let serves = |branch: &Option<String>| {
            branch.as_deref().is_none_or(|template| {
                placement.is_none_or(|placement| {
                    crate::branches::why_not_served(placement, item, template).is_none()
                })
            })
        };
        menu.retain(|entry| match (&entry.agent, &entry.workflow) {
            (Some(recipe), _) => {
                !item.is_finished()
                    && offered(recipe.needs_checkout, &recipe.branch)
                    && serves(&recipe.branch)
            }
            // A workflow hands work over too, so it is gated the same way
            // (§FS-005-dispatch.19).
            (None, Some(_)) => {
                !item.is_finished()
                    && offered(entry.requires_checkout, &entry.branch)
                    && serves(&entry.branch)
            }
            _ => true,
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
    fn rebase_offers(&self, project: &str, trailing: &offers::Trailing) -> Vec<ActionConfig> {
        let mut offers = Vec::new();
        // Measurable, not behind: the reading that says *level* is the reading
        // the replay would refresh, and it is only ever as fresh as the last
        // fetch, so withholding the entry on it hides the one move that would
        // correct it (§FS-004-quick-actions.6). What is required is a base to
        // name — the entry has to say what it replays onto.
        if let (Some(main_branch), Some(trail)) = (self.main_branch(project), trailing.behind) {
            offers.push(offers::rebase_action(main_branch, trail));
        }
        // The same, and the fold already leaves out every repository whose
        // copy is simply its base — that distance is the first entry's — so a
        // checkout of nothing but such repositories measures nothing here and
        // the entry never carries the first one's number under another name
        // (§FS-004-quick-actions.8).
        if let Some(trail) = trailing.behind_upstream {
            offers.push(offers::upstream_rebase_action(
                trailing.published.as_deref(),
                trail,
            ));
        }
        offers
    }

    /// What the project says it can do on this item, where it speaks and the
    /// row lets it be read (§FS-006-project-interface.2,
    /// §FS-006-project-interface.9). A manifest trusted for descriptions only
    /// carries no offers to begin with, so trust needs no second check here.
    pub fn offers(&self, item: &Item, facts: &crate::work::recipe::Facts) -> Vec<ActionConfig> {
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

    /// Recipes considered for this matter and refused, and which selector or
    /// branch-template field refused each one (§FS-005-dispatch.27).
    ///
    /// "Considered" narrows [`crate::work::recipe::Selector::explain`]'s full
    /// generality to what a reader can act on. A recipe whose `kinds` refused
    /// was never in the running for a matter of this shape at all — a `pr`
    /// recipe has nothing to say about a task — so it is dropped rather than
    /// burying the one refusal worth reading under every recipe that was
    /// never close. `behind` and `behind_upstream` are dropped the same way
    /// when they are all that refused: whether a branch trails is a fact
    /// about a checkout on this machine, already its own concept on the menu
    /// (the rebase entries, and the `needs_checkout` gate), and reporting
    /// "could not be measured" on every item with no local checkout would be
    /// noise on nearly every reading rather than the one thing worth saying.
    /// What is left — `roles`, `gate`, `needs_response`, `sources` — are
    /// facts about the matter itself, true wherever it is read from.
    ///
    /// The same recipes and the same facts [`Session::actions_with`] matches
    /// them against, so the offers reading never says less than what decided
    /// the menu (§REQ-002-parity.3). Empty on a finished item — the same
    /// population [`crate::work::recipe::Recipe::matches`] itself never
    /// offers to.
    pub fn excluded_recipes(
        &self,
        item: &Item,
        recipes: &[crate::work::recipe::Recipe],
    ) -> Vec<super::views::Exclusion> {
        if item.is_finished() {
            return Vec::new();
        }
        let trailing = self.item_trailing(item);
        let facts = trailing
            .as_ref()
            .map(offers::Trailing::facts)
            .unwrap_or_default();
        let placement = self.placement(&item.project);
        recipes
            .iter()
            .filter_map(|recipe| {
                let refused = recipe.when.explain(item, &facts);
                if refused.iter().any(|refusal| refusal.field == "kinds") {
                    return None;
                }
                let worth_reading: Vec<_> = refused
                    .into_iter()
                    .filter(|refusal| !matches!(refusal.field, "behind" | "behind_upstream"))
                    .collect();
                let branch = recipe.branch.as_deref().and_then(|template| {
                    placement.and_then(|placement| {
                        crate::branches::why_not_served(placement, item, template)
                    })
                });
                if worth_reading.is_empty() && branch.is_none() {
                    return None;
                }
                Some(super::views::Exclusion {
                    recipe: recipe.id.clone(),
                    reason: worth_reading
                        .into_iter()
                        .map(|refusal| refusal.reason)
                        .chain(branch)
                        .collect::<Vec<_>>()
                        .join("; "),
                })
            })
            .collect()
    }

    /// Where the item's own checkout stands. What decides this is whether the
    /// item resolves to a branch workspace on disk, never what kind of row it
    /// is (§FS-004-quick-actions.6): a change is stale or it is not, and a
    /// forge having filed a pull request about it is not the fact being acted
    /// on. None where nothing resolves — no branch, or a workspace that was
    /// never checked out — because an offer that would fail on the keystroke
    /// is worse than no offer (§FS-004-quick-actions.2).
    pub fn item_trailing(&self, item: &Item) -> Option<offers::Trailing> {
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
    pub fn branch_trailing(&self, project: &str, branch: &str) -> Option<offers::Trailing> {
        let placement = self.placements.get(project)?;
        let workspace = placement
            .workspace_for(branch)
            // A project without branch workspaces works in its root.
            .unwrap_or_else(|| placement.root.clone());
        if !workspace.is_dir() {
            return None;
        }
        let mut trailing = offers::Trailing::of(&placement.forest(&workspace));
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
                if !self.shows(&item, now) {
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
    /// [`Session::branches`]. None for an item on no branch of this project.
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

/// A root as a person reads it: their home written `~`, everything else as it
/// stands. Shown on an organization row, and on any command that names where
/// a project lives.
pub fn display_root(root: &str) -> String {
    let resolved = crate::paths::resolve_path(root)
        .to_string_lossy()
        .into_owned();
    let home = crate::paths::home_dir().to_string_lossy().into_owned();
    match resolved.strip_prefix(&home) {
        Some(rest) if rest.starts_with('/') || rest.is_empty() => format!("~{rest}"),
        _ => resolved,
    }
}

/// The organizations, which project belongs to which, and where each one
/// is placed — read from the registry once for whichever surface asked
/// (§AR-009-surfaces.2).
pub struct RegistryInfo {
    pub orgs: Vec<OrgInfo>,
    pub project_org: BTreeMap<String, String>,
    pub placements: BTreeMap<String, Placement>,
}

pub fn load_registry_info(projects: &[String]) -> Result<RegistryInfo> {
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

impl Session {
    /// Open the session both surfaces read from (§AR-009-surfaces.2): the
    /// registry, the configured entries, the cached feeds, and everything
    /// computed from where the projects sit on disk.
    ///
    /// A command that wants one fact still opens this. It is a read of cache
    /// and configuration rather than a fetch, and a cheaper path that skipped
    /// the placement would answer a different question than the screen does —
    /// two answers to "can this project be checked out" is exactly what
    /// §AR-005-capabilities exists to prevent.
    pub fn open(config: &StatusConfig) -> Result<Session> {
        Session::open_over(config, &crate::scope::Projects::every())
    }

    /// The same session, over the projects a scope selector left
    /// (§FS-011-command-line.9).
    ///
    /// This is where the two files meet: `--workspace`, `--tag` and `--org`
    /// name rows of the registry, and the session picks its projects from the
    /// site's own watch list. Narrowing here rather than at each reader is
    /// what makes the branch rows, the feeds, the stats and the screen agree
    /// about what is in scope — they all read `Session::projects`.
    pub fn open_over(config: &StatusConfig, scope: &crate::scope::Projects) -> Result<Session> {
        let configured: Vec<String> = scope
            .over(config.projects.keys())?
            .into_iter()
            .cloned()
            .collect();
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

        let mut session = Session {
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
            job_news: BTreeMap::new(),
            dispatcher: crate::work::Dispatcher::load(config).ok(),
            work_config: config.work.clone(),
            config: config.clone(),
        };
        session.reload_feeds()?;
        session.reload_work();
        Ok(session)
    }

    /// Re-read every cached feed and everything derived from it. What nothing
    /// claimed is read like any other feed, so it can be shown rather than
    /// only counted (§FS-008-attribution.4).
    pub fn reload_feeds(&mut self) -> Result<()> {
        self.feeds.clear();
        self.unattributed = cache::load_feed(crate::feed::refresh::UNATTRIBUTED)?
            .map(|feed| feed.items().collect())
            .unwrap_or_default();
        for project in self.projects.clone() {
            match cache::load_feed(&project)? {
                Some(feed) => self.feeds.push(feed),
                None => self.feeds.push(ProjectFeed {
                    project,
                    ..ProjectFeed::default()
                }),
            }
        }
        self.recompute_behind();
        self.recompute_placements();
        self.recompute_capabilities();
        self.recompute_resurfacing();
        Ok(())
    }

    /// Re-read every dispatched item's plan. The state of the work belongs to
    /// the runtime, so it is read rather than remembered (§FS-005-dispatch.4)
    /// — including right after a surface has just changed it.
    pub fn reload_work(&mut self) {
        let Some(dispatcher) = &self.dispatcher else {
            return;
        };
        let mut work = BTreeMap::new();
        // One probe per work root across every matter on the feed, not one
        // per matter: two matters often share a checkout, and the lock and
        // the run's own record answer the same way for both
        // (§FS-005-dispatch.15.1).
        let mut look = crate::work::RootLook::default();
        for feed in &self.feeds {
            for item in feed.items() {
                if let Some(status) = dispatcher.status_seen(&item, &mut look) {
                    // A row of its own is still a row: the verdict is cut
                    // where one ends, and the rest is in the artifact
                    // (§FS-005-dispatch.23).
                    work.insert(item.id.clone(), status.lines(60));
                }
            }
        }
        self.work = work;
    }

    /// Whether the runtime still holds a ticket open about this matter — a
    /// line the plan is going on with, or one parked for a person
    /// (§FS-005-dispatch.23). What is over is one line and is not one of them.
    ///
    /// It is the third loose end of §FS-003-feed-categories.2, and the ledger's
    /// rather than any report's: a finished matter with a run still on it stays
    /// on the feed, because the run stands on rows beneath it and a run nobody
    /// can see is a run nobody can take back.
    pub fn working(&self, id: &str) -> bool {
        self.work.get(id).is_some_and(|lines| {
            lines.iter().any(|line| {
                matches!(
                    line.tone,
                    crate::work::Tone::Going | crate::work::Tone::Waiting
                )
            })
        })
    }

    /// The whole of §FS-003-feed-categories.2 for one matter: the two loose
    /// ends a report knows, and the one only the ledger does. None on
    /// unfinished work, which is in the feed because of its category.
    ///
    /// Work is asked about first because it is the one the window does not age
    /// out: a run in flight is not history, however long ago the matter it was
    /// asked about last moved.
    pub fn loose_end(
        &self,
        item: &Item,
        now: chrono::DateTime<Utc>,
    ) -> Option<crate::feed::model::LooseEnd> {
        if !item.is_finished() {
            return None;
        }
        if self.working(&item.id) {
            return Some(crate::feed::model::LooseEnd::Working);
        }
        item.within_recent_window(now, self.recent_days)
            .then(|| item.loose_end())
            .flatten()
    }

    /// In the feed at all, asked with the ledger in hand
    /// (§FS-003-feed-categories.2): unfinished work always, finished work while
    /// it still has a loose end.
    pub fn shows(&self, item: &Item, now: chrono::DateTime<Utc>) -> bool {
        !item.is_finished() || self.loose_end(item, now).is_some()
    }

    /// The window opener bound here (§FS-005-dispatch.22, §AR-002-summons.6):
    /// what site configuration names, else the environment ephor was started
    /// in, else none. None is the terminal, which is the floor and is never
    /// removed (§DA-007-window-is-a-bound-opener).
    pub fn opener(&self) -> Option<crate::seams::window::Opener> {
        crate::seams::window::bound(self.config.defaults.window.as_ref())
    }

    /// Where this matter's work lives: the project's work root, resolved at
    /// the checkout the matter is about (§FS-005-dispatch.4).
    ///
    /// `branch` is the template carried by the entry being asked about, where
    /// it carries one — the work root is then inside the workspace that entry
    /// names, which is where its dispatch will write (§FS-005-dispatch.25). A
    /// caller asking about the matter rather than about one entry passes
    /// `None`.
    pub fn work_root(&self, item: &Item, branch: Option<&str>) -> Option<std::path::PathBuf> {
        let placement = self.placements.get(&item.project)?;
        let checkout = crate::branches::placed_through(placement, item, branch);
        let root = self.root(&item.project)?.to_path_buf();
        let organization = placement.organization.as_ref();
        let template = crate::work::root_template(
            &self.work_config,
            organization
                .and_then(|org| self.config.organizations.get(&org.id))
                .map(|organization| &organization.work),
            self.config
                .projects
                .get(&item.project)
                .map(|project| &project.work),
        );
        let subject = crate::work::dossier::Subject {
            item,
            checkout: &checkout,
            root: &root,
            organization,
        };
        // Nothing where the dispatch would refuse: this reading is a preview
        // of that write, and a preview that guessed past a refusal would name
        // a directory nothing will ever be in (§FS-005-dispatch.6.1).
        subject.work_root(&template).ok()
    }

    /// The work root the picker over one matter's menu reads its roster at
    /// (§FS-005-dispatch.14). The picker stands over the whole list rather
    /// than one row, so it is the root every entry that could use it would be
    /// dispatched into — where they agree, which is every menu whose entries
    /// say the same thing about the branch their work belongs on. Where they
    /// do not, the matter's own root answers for all of them rather than one
    /// entry's answer standing for the rest (§FS-005-dispatch.25). None where
    /// nothing on the menu hands work over: there is nobody to pick for.
    pub fn roster_root(
        &self,
        item: &Item,
        entries: &[offers::MenuEntry],
    ) -> Option<std::path::PathBuf> {
        let mut wanted = entries
            .iter()
            .filter_map(|entry| entry.action.agent.as_ref())
            .map(|recipe| self.work_root(item, recipe.branch.as_deref()));
        let first = wanted.next()?;
        match wanted.all(|root| root == first) {
            true => first,
            false => self.work_root(item, None),
        }
    }

    /// Every item of every configured project's cached feed, still visible
    /// (§FS-003-feed-categories.2), newest first — what a command that takes
    /// a feed id looks its argument up in.
    pub fn items(&self) -> Vec<Item> {
        let now = Utc::now();
        let mut items: Vec<Item> = self
            .feeds
            .iter()
            .flat_map(|feed| feed.items())
            .filter(|item| self.shows(item, now))
            .collect();
        items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        items
    }

    /// One matter by its feed id. Named rather than guessed: a command that
    /// silently acted on the nearest match would be a command nobody can
    /// script against (§REQ-002-parity.3).
    pub fn item(&self, id: &str) -> Option<Item> {
        self.feeds
            .iter()
            .flat_map(|feed| feed.items())
            .find(|item| item.id == id)
    }
}

/// What an entry says about who gets its work, out of the answer the seven steps
/// gave (§FS-005-dispatch.14). A hand that cannot be asked right now is named
/// with the reason rather than hidden; a choice that cannot stand is the whole
/// reason and refuses the entry; and where nobody named anybody the sentence
/// is the runtime's own — with no runner bound, the *workable* rung's, because
/// there is nobody to ask and the ticket is written all the same.
pub fn who_gets_it(choice: &crate::work::runtime::roster::Choice, unbound: Option<&str>) -> Handed {
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

/// The session, standing behind one matter's list: what
/// [`Session::naming`] hands the assembly so it can fill in the facts an
/// entry cannot carry (§AR-009-surfaces.1).
pub struct Filling<'a> {
    session: &'a mut Session,
    item: Option<&'a Item>,
}

impl offers::Naming for Filling<'_> {
    fn name(&mut self, actions: &mut [ActionConfig]) {
        let Some(item) = self.item else {
            return;
        };
        self.session.name_the_hands(item, actions);
        // And where the workspace would be, for an entry that says which
        // branch its work belongs on (§FS-005-dispatch.25).
        self.session.name_the_branches(item, actions);
    }
}

impl Session {
    /// The two passes [`offers::entries`](crate::api::offers::entries) runs
    /// over every list about a matter, in the one place both surfaces go
    /// through (§AR-009-surfaces.1). `None` where there is no matter to
    /// resolve against — a branch row carries ephor's own offers only, and
    /// neither pass has anything to say about them (§FS-005-dispatch.2).
    pub fn naming<'a>(&'a mut self, item: Option<&'a Item>) -> Filling<'a> {
        Filling {
            session: self,
            item,
        }
    }

    /// Fill in what each entry's `branch` template comes to on this matter
    /// (§FS-005-dispatch.25), so the gate on the row is the answer the
    /// dispatch would give (§FS-004-quick-actions.2).
    ///
    /// Only where the matter has no branch of its own — the forge's answer is
    /// never displaced by a template — and only on an entry that hands work
    /// over, which is the only kind the key is accepted on.
    fn name_the_branches(&mut self, item: &Item, menu: &mut [ActionConfig]) {
        let template_of = |entry: &ActionConfig| match &entry.agent {
            Some(recipe) => recipe.branch.clone(),
            None => entry.workflow.as_ref().and(entry.branch.clone()),
        };
        if !menu.iter().any(|entry| template_of(entry).is_some()) {
            return;
        }
        let Some(placement) = self.placements.get(&item.project).cloned() else {
            return;
        };
        // A matter the registry or the forge already put on a branch of its
        // own resolves through that branch, and a template has nothing to
        // add — the project's main branch never counts (§FS-005-dispatch.25).
        if placement.own_branch(item).is_some() {
            return;
        }
        for entry in menu.iter_mut() {
            let Some(template) = template_of(entry) else {
                continue;
            };
            entry.minted = Some(match crate::branches::minted(&placement, item, &template) {
                Ok(checkout) => Minted::Named {
                    branch: checkout.branch.unwrap_or_default(),
                    workspace: checkout.workspace,
                    state: checkout.state,
                },
                Err(why) => Minted::Refused(why),
            });
        }
    }

    /// Who each entry's work would go to, filled in when the list is
    /// built (§FS-005-dispatch.14). Never configuration: nobody writes it,
    /// ephor resolves it so the reader sees it before pressing the key —
    /// or, on the command line, before typing the id.
    fn name_the_hands(&mut self, item: &Item, menu: &mut [ActionConfig]) {
        if !menu.iter().any(|entry| entry.agent.is_some()) {
            return;
        }
        // One root per entry, read before the dispatcher is borrowed: an entry
        // that says which branch its work belongs on is dispatched inside the
        // workspace it names, and the hand this row shows is the hand that
        // dispatch would resolve there (§FS-005-dispatch.25).
        let roots: Vec<Option<std::path::PathBuf>> = menu
            .iter()
            .map(|entry| {
                let recipe = entry.agent.as_ref()?;
                self.work_root(item, recipe.branch.as_deref())
            })
            .collect();
        // With no runner bound there is nobody to ask, and the entry says so
        // in the workable rung's own words rather than naming a hand it does
        // not have (§FS-005-dispatch.14). The ticket is written all the same.
        let unbound = crate::work::runtime::refusal(&self.work_config);
        let work = self
            .config
            .projects
            .get(&item.project)
            .map(|project| project.work.clone());
        let Some(dispatcher) = &mut self.dispatcher else {
            return;
        };
        for (entry, root) in menu.iter_mut().zip(roots) {
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
            // Nowhere to resolve it against is nothing to say about it: the
            // row carries no hand rather than one read at the wrong root.
            let Some(root) = root else {
                continue;
            };
            let pinned = recipe.hand.clone();
            let choice = dispatcher.hand(&item.project, &recipe.id, None, pinned.as_ref(), &root);
            entry.hand = Some(who_gets_it(&choice, unbound.as_deref()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seams::jobs::Record;

    fn record() -> Record {
        Record {
            version: crate::seams::jobs::VERSION,
            project: "widget".to_string(),
            item: None,
            icon: "⤴".to_string(),
            description: "rebase onto master".to_string(),
            root: std::path::PathBuf::from("/w"),
            workspace: None,
            action: Some("rebase".to_string()),
            branch: None,
            window: None,
            windowed: false,
            steps: Vec::new(),
            dossier: Vec::new(),
            started: String::new(),
        }
    }

    fn merged(updated_at: chrono::DateTime<Utc>) -> Item {
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: crate::feed::model::ItemKind::Pr,
            role: None,
            title: "Retry window".to_string(),
            url: None,
            state: Some("merged".to_string()),
            needs_response: false,
            updated_at,
            raw: Value::Null,
        }
    }

    fn work_line(tone: crate::work::Tone) -> crate::work::WorkLine {
        crate::work::WorkLine {
            tone,
            marker: "⚙",
            said: "fix-gate · running".to_string(),
            ticket: Some("t1".to_string()),
            asked: None,
        }
    }

    /// The third loose end of §FS-003-feed-categories.2 is the ledger's: a
    /// finished matter the runtime is still working on stays on the feed,
    /// because the work stands on rows beneath it (§FS-005-dispatch.23) and a
    /// run nobody can see is a run nobody can take back. What is over is not
    /// one of those rows, and does not hold the matter here.
    #[test]
    fn a_finished_matter_stays_while_work_is_still_open_on_it() {
        let now = Utc::now();
        let item = merged(now - chrono::Duration::hours(2));
        let mut session = Session {
            recent_days: 7,
            ..Session::default()
        };
        assert!(!session.shows(&item, now), "merged, and nothing left to do");

        session
            .work
            .insert(item.id.clone(), vec![work_line(crate::work::Tone::Going)]);
        assert!(session.working(&item.id));
        assert!(session.shows(&item, now));
        assert_eq!(
            session.loose_end(&item, now),
            Some(crate::feed::model::LooseEnd::Working)
        );

        // The window does not age out a run in flight: the ledger says it is
        // going, whatever the matter's own last activity says.
        let old = merged(now - chrono::Duration::days(90));
        assert!(session.shows(&old, now));

        // A plan holding nothing open is one line for what the last ticket
        // decided — history, not a run.
        session
            .work
            .insert(item.id.clone(), vec![work_line(crate::work::Tone::Over)]);
        assert!(!session.working(&item.id));
        assert!(!session.shows(&item, now));
    }

    /// A job's line is filed under what the job was about, so it lands on that
    /// row rather than at the top of the screen (§FS-005-dispatch.17). The
    /// record names a matter or a branch and never both
    /// (§FS-004-quick-actions.6), and every subject carries its project so
    /// that a line whose own row is nowhere still has one to land on.
    #[test]
    fn a_line_is_filed_under_what_the_job_was_about() {
        let branch = Record {
            branch: Some("you/ABC-42".to_string()),
            ..record()
        };
        assert_eq!(
            JobSubject::of(&branch),
            JobSubject::Branch("widget".to_string(), "you/ABC-42".to_string())
        );

        let matter = Record {
            item: Some("forge-prs:acme/widget#42".to_string()),
            ..record()
        };
        assert_eq!(
            JobSubject::of(&matter),
            JobSubject::Matter("widget".to_string(), "forge-prs:acme/widget#42".to_string())
        );

        // Neither: the project is still what it was about.
        assert_eq!(
            JobSubject::of(&record()),
            JobSubject::Project("widget".to_string())
        );
        for subject in [
            JobSubject::of(&branch),
            JobSubject::of(&matter),
            JobSubject::of(&record()),
        ] {
            assert_eq!(subject.project(), "widget");
        }
    }
}
