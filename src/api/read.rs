//! The readings: what a surface asks the session for (§AR-009-surfaces.1).
//!
//! Each one returns a view and changes nothing. The interface draws what comes
//! back and a command prints it, as prose or as JSON — one answer rendered
//! twice, never two answers (§REQ-002-parity.3).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use crate::branches::{BranchInfo, WorkspaceState};
use crate::capabilities::Rung;
use crate::feed::model::Item;

use super::offers;
use super::views;
use super::Session;

/// What a menu is about: a matter, or a branch row that has none behind it
/// (§FS-004-quick-actions.6).
pub enum Subject<'a> {
    Item(&'a Item),
    Branch { project: &'a str, branch: &'a str },
}

impl Subject<'_> {
    pub fn project(&self) -> &str {
        match self {
            Subject::Item(item) => &item.project,
            Subject::Branch { project, .. } => project,
        }
    }
}

/// Where an entry would run, and what the branch workspace situation is.
pub struct Placed {
    pub root: std::path::PathBuf,
    pub workspace: std::path::PathBuf,
    pub state: WorkspaceState,
    pub branch: Option<BranchInfo>,
}

impl Session {
    /// Where a subject's entries would run (§AR-004-forest.3). The refusal is
    /// the ladder's own sentence for a project that is not placed at all —
    /// returned rather than printed, because it has to reach a greyed row and
    /// a JSON field alike (§AR-005-capabilities.2).
    pub fn place(&self, subject: &Subject) -> Result<Placed, String> {
        let project = subject.project();
        if let Some(refusal) = self.can(project).refusal(&[Rung::Placed]) {
            return Err(refusal);
        }
        let placement = self
            .placement(project)
            .ok_or_else(|| format!("{project} has no root in the registry"))?
            .clone();
        match subject {
            Subject::Item(item) => {
                let placed = self
                    .checkout(item)
                    .ok_or_else(|| format!("{project} has no root in the registry"))?;
                Ok(Placed {
                    root: placement.root.clone(),
                    workspace: placed.workspace,
                    state: placed.state,
                    branch: placement.matched(item).cloned(),
                })
            }
            // A branch whose workspace the project puts somewhere and has not
            // got there yet is the checkout's question first
            // (§FS-004-quick-actions.7); a project keeping one checkout at its
            // root is always ready. Where the target is not there the commands
            // run in the root, so that pointing `EPHOR_WORKSPACE` at a
            // directory that does not exist is never an offer that fails on
            // the keystroke (§FS-004-quick-actions.2).
            Subject::Branch { branch, .. } => {
                let (workspace, state) = match placement.workspace_for(branch) {
                    None => (placement.root.clone(), WorkspaceState::Ready),
                    Some(target) if target.is_dir() => (target, WorkspaceState::Ready),
                    Some(target) => (placement.root.clone(), WorkspaceState::Missing(target)),
                };
                Ok(Placed {
                    root: placement.root.clone(),
                    workspace,
                    state,
                    branch: self
                        .branches(project)
                        .iter()
                        .find(|info| info.branch == *branch)
                        .cloned(),
                })
            }
        }
    }

    /// The entries a subject carries, assembled and gated
    /// (§FS-011-command-line.1). The one list both surfaces show.
    pub fn menu(&mut self, subject: &Subject) -> Result<Vec<offers::MenuEntry>, String> {
        let placed = self.place(subject)?;
        let project = subject.project().to_string();
        let (applicable, has_workflows) = match subject {
            Subject::Item(item) => {
                let item = (*item).clone();
                // The recipes this project offers, so the menu carries the
                // work that can be handed over about the matter beside the
                // commands that can be run on it (§FS-005-dispatch.1).
                let recipes = self
                    .dispatcher
                    .as_ref()
                    .map(|dispatcher| dispatcher.recipes(&item.project))
                    .unwrap_or_default();
                // The third home an entry may live in: beside the workflow
                // itself (§FS-005-dispatch.19).
                let beside = self
                    .dispatcher
                    .as_mut()
                    .map(|dispatcher| dispatcher.workflow_entries(&item.project))
                    .unwrap_or_default();
                let has_workflows = self.dispatcher.as_mut().is_some_and(|dispatcher| {
                    !dispatcher.workflows(&item.project).workflows.is_empty()
                });
                let mut applicable = self.actions_with(&item, &recipes, &beside);
                self.name_the_hands(&item, &mut applicable);
                (applicable, has_workflows)
            }
            // A branch row carries ephor's own offers only: there is no matter
            // here for a source's, a project's or a person's entries to be
            // selected against, and none for a recipe either
            // (§FS-005-dispatch.2).
            Subject::Branch { branch, .. } => (self.branch_actions(&project, branch), false),
        };
        let checkout = self.checkouts.get(&project).cloned();
        let can = self.can(&project);
        let mut entries =
            offers::entries(&placed.state, &checkout, &can, applicable, has_workflows);
        self.mark_running(subject, &mut entries);
        Ok(entries)
    }

    /// Mark every entry that has work going about its subject, and say the way
    /// in (§FS-005-dispatch.21).
    ///
    /// One assembly for both surfaces (§AR-009-surfaces.1): the menu sets these
    /// rows apart and `ephor actions` prints the same mark with the same facts,
    /// so a program reading the menu cannot start what a person reading it
    /// would have opened (§REQ-002-parity.2).
    ///
    /// Everything here is found by looking. A job is a held lock and a record
    /// naming the entry it came from (§FS-005-dispatch.17); a run is a held
    /// lock and the descriptor beside it (§FS-005-dispatch.20). Nothing is
    /// remembered from the keypress, so a second ephor opening the same menu
    /// sees the same rows and a job that died is not running.
    fn mark_running(&self, subject: &Subject, entries: &mut [offers::MenuEntry]) {
        let project = subject.project();
        let (item, branch) = match subject {
            Subject::Item(item) => (Some(*item), None),
            Subject::Branch { branch, .. } => (None, Some(*branch)),
        };
        // Live only: a record that a job started is a different claim from a
        // job that is running (§AR-002-summons.5).
        let jobs: Vec<crate::seams::jobs::Job> = crate::seams::jobs::all()
            .into_iter()
            .filter(|job| job.live)
            .collect();
        let now = chrono::Utc::now();
        for entry in entries.iter_mut() {
            let key = entry.key();
            // A job started from *this* entry, about *this* subject: the
            // record says which entry it came from and, on a branch row, which
            // branch, because nothing could otherwise match it back
            // (§FS-005-dispatch.21).
            let started_here = jobs.iter().find(|job| {
                job.record.project == project
                    && job.record.action.as_deref() == Some(key.as_str())
                    && job.record.item.as_deref() == item.map(|item| item.id.as_str())
                    && job.record.branch.as_deref() == branch
            });
            if let Some(job) = started_here {
                entry.running = Some(match job.record.window.clone() {
                    // A windowed program's inspection is its window: what it
                    // wrote is on that screen and nowhere else
                    // (§FS-005-dispatch.22).
                    Some(handle) => offers::Running::Window {
                        job: job.id.clone(),
                        handle,
                        since: job.took(now),
                        says: job.says(),
                    },
                    None => offers::Running::Job {
                        id: job.id.clone(),
                        since: job.took(now),
                        says: job.says(),
                        log: job.log_path(),
                    },
                });
                continue;
            }
            // An entry that hands work over is running where the ticket it
            // would open, or the plan it would lay, is open and its root is
            // live or will reach it (§FS-005-dispatch.21).
            if entry.action.agent.is_none() && entry.action.workflow.is_none() {
                continue;
            }
            let (Some(item), Some(dispatcher)) = (item, self.dispatcher.as_ref()) else {
                continue;
            };
            let Some(going) = dispatcher.work_going(item, &key) else {
                continue;
            };
            // The run's own identity, read from the descriptor beside the lock
            // — the way in is the runner's own attach command
            // (§FS-005-dispatch.20, §FS-011-command-line.8).
            let run = crate::work::runtime::watch::identity(&self.work_config, going.root());
            let id = run.as_ref().and_then(|run| run.id.clone());
            let attach = id
                .as_deref()
                .map(|id| crate::work::runtime::attach_command(&self.work_config, id));
            let since = run
                .as_ref()
                .and_then(|run| run.started_at.as_deref())
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| (now - at.with_timezone(&chrono::Utc)).num_seconds());
            entry.running = Some(match going {
                crate::work::WorkGoing::Running { root, doing } => offers::Running::Run {
                    root,
                    id,
                    control_url: run.and_then(|run| run.control_url),
                    attach,
                    since,
                    doing,
                },
                crate::work::WorkGoing::Queued { root } => offers::Running::Queued {
                    root,
                    id,
                    attach,
                    since,
                },
            });
        }
    }

    /// The whole `ephor actions` reading for one subject
    /// (§FS-011-command-line.1).
    pub fn actions(&mut self, subject: &Subject) -> Result<views::Actions, String> {
        let placed = self.place(subject)?;
        let entries = self.menu(subject)?;
        let project = subject.project().to_string();
        // The hands `--hand` may name, read against the work root the dispatch
        // would use (§FS-005-dispatch.14) — empty where there is no agent
        // entry to pick for or nobody to pick, which is what withholds the
        // choice entirely.
        let roster = match (
            entries.iter().any(|entry| entry.action.agent.is_some()),
            subject,
        ) {
            (true, Subject::Item(item)) => {
                let root = self.work_root(item);
                match (root, &mut self.dispatcher) {
                    (Some(root), Some(dispatcher)) => dispatcher.pickable(&project, &root),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
        let (kind, id, title) = match subject {
            Subject::Item(item) => ("item", item.id.clone(), item.title.clone()),
            Subject::Branch { branch, .. } => ("branch", branch.to_string(), branch.to_string()),
        };
        Ok(views::Actions {
            project,
            subject: kind,
            id,
            title,
            root: placed.root,
            workspace: placed.workspace,
            workspace_state: workspace_state_name(&placed.state),
            branch: placed.branch.map(|info| info.branch),
            offers: entries.iter().map(offer_of).collect(),
            roster: roster
                .iter()
                .map(|hand| views::HandOffer {
                    id: hand.id.clone(),
                    efforts: hand.efforts.clone(),
                    unavailable: hand.available.clone(),
                })
                .collect(),
        })
    }

    /// A project's branches, and where each one stands
    /// (§FS-011-command-line.2).
    pub fn branch_rows(&self, project: &str) -> Vec<views::Branch> {
        let main = self.main_branch(project).map(str::to_string);
        self.branches(project)
            .iter()
            .map(|info| {
                let standing = self.branch_standing(project, &info.branch);
                views::Branch {
                    project: project.to_string(),
                    branch: info.branch.clone(),
                    declared: info.declared,
                    active: info.active,
                    is_release: info.is_release,
                    ticket: info.ticket.clone(),
                    workspace: self
                        .placement(project)
                        .and_then(|placement| placement.workspace_for(&info.branch)),
                    checked_out: self.branch_checked_out(project, info),
                    behind: self
                        .branch_behind(project, &info.branch)
                        .and_then(crate::forest::Staleness::trail)
                        .map(views::Distance::from),
                    behind_upstream: standing
                        .and_then(crate::forest::Standing::upstream_trail)
                        .map(views::Distance::from),
                    published: standing.and_then(published_ref),
                    main_branch: main.clone(),
                    items: self.branch_linked(project, info),
                    url: self.branch_url(project, info),
                }
            })
            .collect()
    }

    /// Every execution root, enumerated afresh (§FS-005-dispatch.15) — found
    /// by looking, never remembered: a plan written by hand, or a run started
    /// in another terminal on a root ephor never dispatched into, is a row
    /// like any other.
    pub fn work_roots(&mut self) -> Vec<crate::work::runtime::watch::RootPlans> {
        match &mut self.dispatcher {
            Some(dispatcher) => dispatcher.work_roots(),
            None => Vec::new(),
        }
    }

    /// The runtime's half of the operations board (§FS-005-dispatch.15): what
    /// its artifacts say is going on under each root, with the matter behind
    /// each row where the feed still carries one and the plan that row reads.
    ///
    /// `groups` is the enumeration to read it from, and is the one thing the
    /// two surfaces differ in: a command enumerates on the spot, while the
    /// interface keeps the last enumeration between ticks and stats it rather
    /// than walking it again (§FS-005-dispatch.15.1). Everything downstream of
    /// that — which roots are live, whose matter each row is about, which plan
    /// `e` opens on it — is worked out here, once, so the board a key opens
    /// and the board `ephor operations` prints cannot come apart
    /// (§AR-009-surfaces.1).
    pub fn running(
        &self,
        groups: &[crate::work::runtime::watch::RootPlans],
    ) -> (Vec<Running>, Option<String>) {
        use crate::work::runtime::watch;

        if self.dispatcher.is_none() {
            return (
                Vec::new(),
                Some("Work needs the registry, which could not be read at startup".to_string()),
            );
        }
        let watch::Board {
            operations,
            refusal,
        } = watch::board(&self.work_config, groups);
        // Every row's matter in one walk of the feeds rather than a walk per
        // row: rebuilding each matter into a row costs the whole feed once.
        let wanted: BTreeSet<String> = operations
            .iter()
            .filter_map(|op| op.item().map(str::to_string))
            .collect();
        let mut matters: BTreeMap<String, Item> = BTreeMap::new();
        if !wanted.is_empty() {
            for feed in &self.feeds {
                for item in feed.items() {
                    if wanted.contains(&item.id) {
                        matters.insert(item.id.clone(), item);
                    }
                }
            }
        }
        let rows = operations
            .into_iter()
            .map(|op| Running {
                item: op.item().and_then(|id| matters.get(id).cloned()),
                // The operation's own plan where a ticket of it names one, and
                // the ledger's for this root otherwise: a live run whose
                // tickets were all filtered out still has a plan behind it,
                // and a row saying it has none would be a row leading nowhere
                // (§FS-005-dispatch.15).
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

    /// What ephor is running itself, right now (§FS-005-dispatch.17). It needs
    /// no runtime and no registry — it is ephor running a command — so these
    /// rows stand even where the runtime's half of the board says why it is
    /// empty.
    pub fn live_jobs(&self) -> Vec<crate::seams::jobs::Job> {
        crate::seams::jobs::all()
            .into_iter()
            .filter(|job| job.live)
            .collect()
    }

    /// The operations board (§FS-011-command-line.3): what the runtime is at,
    /// and what ephor is running itself, in the order the board puts them.
    pub fn operations(&mut self) -> views::Operations {
        // The reader's own move first, exactly as the interface orders it: a
        // job is something they pressed a key for moments ago.
        let mut rows: Vec<views::Operation> = self.live_jobs().into_iter().map(job_row).collect();
        let groups = self.work_roots();
        let (running, refusal) = self.running(&groups);
        for row in running {
            rows.push(views::Operation {
                kind: "root",
                id: row.op.root.display().to_string(),
                project: row.op.project.clone(),
                says: row.op.says(),
                state: row.op.state().to_string(),
                live: row.op.live,
                item: row.op.item().map(str::to_string),
                title: row.item.as_ref().map(|item| item.title.clone()),
                root: Some(row.op.root.clone()),
                plan: row.plan,
                log: None,
                dashboard: row.op.dashboard.clone(),
                tickets: row.op.tickets.iter().map(ticket_row).collect(),
            });
        }
        views::Operations {
            refusal,
            operations: rows,
        }
    }
}

/// One operation of the runtime's, with what a surface needs to put it on a
/// row: the artifacts' own answer, the matter behind it where the feed still
/// carries one, and the plan the row reads and edits.
///
/// Resolved by [`Session::running`] rather than by whoever is drawing — the
/// interface builds these off the draw path and a command prints them, and
/// neither works out its own (§AR-009-surfaces.1).
pub struct Running {
    pub op: crate::work::runtime::watch::Operation,
    pub item: Option<Item>,
    pub plan: Option<std::path::PathBuf>,
}

/// One entry as a reading names it (§FS-011-command-line.1).
pub fn offer_of(entry: &offers::MenuEntry) -> views::Offer {
    views::Offer {
        id: entry.key(),
        icon: entry.action.icon.clone(),
        description: entry.action.description.clone(),
        kind: entry.kind(),
        gate: entry.gate.name(),
        refusal: entry.gate.refusal().map(str::to_string),
        hand: entry.action.hand.as_ref().map(|hand| hand.says.clone()),
        command: (!entry.action.command.is_empty()).then(|| entry.action.command.clone()),
        // Filled in by the reading that has a dispatcher to resolve it
        // against, which is the one that answers about work
        // ([`Session::work_of`]).
        brief: None,
        cwd: entry.action.cwd.clone(),
        background: entry.action.background,
        confirm: entry.action.confirm,
        requires: entry.action.requires.clone(),
        running: entry.running.as_ref().map(running_of),
    }
}

/// What is going about an entry, as a reading names it (§FS-011-command-line.8).
/// One rendering of the one mark, so the row a screen sets apart and the line a
/// command prints carry the same facts (§AR-009-surfaces.1).
pub fn running_of(running: &offers::Running) -> views::Running {
    let mut view = views::Running {
        kind: running.name(),
        says: running.says(),
        since_seconds: running.since(),
        job: None,
        log: None,
        root: None,
        run: None,
        attach: None,
        control_url: None,
        window: None,
    };
    match running {
        offers::Running::Job { id, log, .. } => {
            view.job = Some(id.clone());
            view.log = Some(log.clone());
        }
        offers::Running::Run {
            root,
            id,
            control_url,
            attach,
            ..
        } => {
            view.root = Some(root.clone());
            view.run = id.clone();
            view.attach = attach.clone();
            view.control_url = control_url.clone();
        }
        offers::Running::Queued {
            root, id, attach, ..
        } => {
            view.root = Some(root.clone());
            view.run = id.clone();
            view.attach = attach.clone();
        }
        offers::Running::Window { job, handle, .. } => {
            view.job = Some(job.clone());
            view.window = Some(handle.clone());
        }
    }
    view
}

fn workspace_state_name(state: &WorkspaceState) -> &'static str {
    match state {
        WorkspaceState::Ready => "ready",
        WorkspaceState::Missing(_) => "missing",
        WorkspaceState::Unmatched => "unmatched",
    }
}

/// The ref every counted repository names, where they all name one. Two
/// different refs have no one name, and reporting one of them would be telling
/// the reader about half their checkout (§FS-004-quick-actions.8).
fn published_ref(standing: &crate::forest::Standing) -> Option<String> {
    let mut named: Vec<String> = Vec::new();
    for repo in &standing.repos {
        if repo.copies_the_base() {
            continue;
        }
        let crate::forest::Upstream::Published { remote, branch } = &repo.upstream else {
            continue;
        };
        let reference = format!("{remote}/{branch}");
        if !named.contains(&reference) {
            named.push(reference);
        }
    }
    (named.len() == 1).then(|| named[0].clone())
}

/// A job of ephor's own as a board row (§FS-005-dispatch.17).
pub fn job_row(job: crate::seams::jobs::Job) -> views::Operation {
    // The state is the lock's answer, never the record's: a job whose
    // supervisor died says so rather than claiming to be running
    // (§AR-002-summons.5).
    let state = match (&job.ended, job.live) {
        (_, true) => "running",
        (Some(ended), false) => ended.outcome.as_str(),
        (None, false) => "died",
    };
    views::Operation {
        kind: "job",
        id: job.id.clone(),
        project: job.record.project.clone(),
        says: job.says(),
        state: state.to_string(),
        live: job.live,
        item: job.record.item.clone(),
        title: Some(job.record.description.clone()),
        root: Some(job.record.root.clone()),
        plan: None,
        log: Some(job.log_path()),
        dashboard: None,
        tickets: Vec::new(),
    }
}

fn ticket_row(ticket: &crate::work::runtime::watch::BoardTicket) -> views::OperationTicket {
    views::OperationTicket {
        id: format!("{}.{}", ticket.plan_id, ticket.ticket),
        says: ticket.doing.says(),
        state: ticket.state.clone().unwrap_or_else(|| "?".to_string()),
        doing: ticket.doing.name().to_string(),
        assignee: match &ticket.doing {
            crate::work::runtime::watch::Doing::Claimed { assignee, .. } => Some(assignee.clone()),
            _ => None,
        },
    }
}

/// Kept so a surface can hand the blocks for a project to a provider without
/// reaching past the API (§AR-009-surfaces.5).
pub fn blocks(session: &Session, project: &str) -> Vec<Value> {
    session.blocks_for(project)
}

impl Session {
    /// What could be handed over about one matter: the entries of its menu
    /// that open a ticket or lay a plan down, rather than run a command here
    /// (§FS-005-dispatch.1). A command that runs `lazygit` is on the action
    /// menu, not on the question "what work is there".
    ///
    /// The one derivation of that set, for the work screen and for `ephor work
    /// offers` alike (§AR-009-surfaces.1). It goes through [`Session::menu`],
    /// which is where the gating lives: work is not offered about a matter
    /// that is finished (§FS-005-dispatch.6), and work that edits the change
    /// is not offered where the change is not on this machine
    /// (§FS-004-quick-actions.7) — because the dispatch refuses both, and an
    /// offer that would be refused on the keystroke is worse than no offer
    /// (§FS-004-quick-actions.2). A second derivation without those filters
    /// put a row on the work screen that `ephor work offers` said was not on
    /// the table, which is exactly the drift §REQ-002-parity.2 forbids.
    ///
    /// The refusal comes back rather than becoming an empty list. Assembling
    /// the menu needs the project *placed* (§AR-004-forest.3), and a registry
    /// root that is not on disk fails that for reasons that have nothing to do
    /// with whether work can be handed over — `ephor work dispatch` goes
    /// through regardless. Swallowing it left the work screen and `ephor work
    /// offers` both saying "nothing matches this matter" about a matter plenty
    /// matches, which is the absence §REQ-001-boundary.1 forbids and the drift
    /// §REQ-002-parity.2 forbids at once.
    pub fn work_entries(&mut self, item: &Item) -> Result<Vec<offers::MenuEntry>, String> {
        Ok(self
            .menu(&Subject::Item(item))?
            .into_iter()
            .filter(|entry| entry.action.agent.is_some() || entry.action.workflow.is_some())
            .collect())
    }

    /// What could be handed over about one matter, as the rows both surfaces
    /// draw (§FS-011-command-line.5) — the entries of [`Session::work_entries`]
    /// with the words each ticket would actually carry rendered against the
    /// matter (§FS-005-dispatch.7).
    ///
    /// One derivation, three renderings: the work screen's rows, the prose
    /// `ephor work offers` prints, and the `offers` field of the reading. A
    /// screen that mapped entries to rows itself is how a row one surface
    /// offers stops being a row the other has (§REQ-002-parity.2).
    pub fn work_offers(&mut self, item: &Item) -> Result<Vec<views::Offer>, String> {
        let entries = self.work_entries(item)?;
        Ok(entries
            .iter()
            .map(|entry| views::Offer {
                // The words the ticket would actually carry about this matter
                // (§FS-005-dispatch.7). The work screen puts them under the
                // row the cursor is on, so the reading carries them too: the
                // prose form may summarise, but it may never *know* something
                // the machine form does not (§REQ-002-parity.3).
                brief: match (&entry.action.agent, &mut self.dispatcher) {
                    (Some(recipe), Some(dispatcher)) => Some(dispatcher.brief(item, recipe)),
                    _ => None,
                },
                ..offer_of(entry)
            })
            .collect())
    }

    /// One matter's work: what could be handed over about it, and what
    /// already has been (§FS-011-command-line.5). The reading behind the work
    /// screen, and behind `ephor work offers`.
    pub fn work_of(&mut self, item: &Item) -> views::Work {
        // Everything here goes on working with nothing bound — the plan is
        // written, read and reopened either way. The refusal is what the run
        // key answers with, carried so a surface can say why rather than
        // offering a move nothing can make (§FS-005-dispatch).
        let refusal = crate::work::runtime::refusal(&self.work_config);
        // Two different questions, and so two fields: whether a plan can be
        // *run* here, and whether the menu the offers come from could be
        // assembled at all. A reader told neither, and shown an empty list,
        // would read an oversight (§REQ-001-boundary.1).
        let (offers, unavailable) = match self.work_offers(item) {
            Ok(offers) => (offers, None),
            Err(refusal) => (Vec::new(), Some(refusal)),
        };
        let status = self
            .dispatcher
            .as_ref()
            .and_then(|dispatcher| dispatcher.status(item))
            .map(|status| views::WorkStatus {
                plan: status.plan.clone(),
                plan_id: status.plan_id.clone(),
                root: status.root.clone(),
                checkout: status.checkout.clone(),
                stale: status.stale(),
                missing: status.missing,
                changes: status.changes.clone(),
                tickets: status
                    .tickets
                    .iter()
                    .map(|ticket| views::Ticket {
                        id: ticket.id.clone(),
                        recipe: ticket.recipe.clone(),
                        state: ticket.state.clone(),
                        finished: ticket.finished,
                        cancelled: ticket.cancelled,
                        verdict: ticket.verdict.clone(),
                    })
                    .collect(),
            });
        // What ephor has run about this matter itself (§FS-005-dispatch.17),
        // newest first — the same rows the board shows, narrowed to one matter.
        let jobs = crate::seams::jobs::all()
            .into_iter()
            .filter(|job| job.record.item.as_deref() == Some(item.id.as_str()))
            .map(job_row)
            .collect();
        views::Work {
            item: item.id.clone(),
            project: item.project.clone(),
            title: item.title.clone(),
            url: item.url.clone(),
            refusal,
            unavailable,
            offers,
            status,
            jobs,
        }
    }
}
