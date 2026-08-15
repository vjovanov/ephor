//! Watching a run from its artifacts (§AR-007-runtime.1,
//! §FS-005-dispatch.15).
//!
//! A run is live on an execution root iff the runtime's per-root lock is
//! held. The runtime takes that lock with a blocking acquire, so the probe
//! here is a non-blocking try-lock and nothing else — a probe that waited
//! would park ephor behind the very run it is asking about — and the
//! operating system releases the lock when a run dies, however it dies,
//! which is why the lock and not the last write is the liveness signal.
//! Everything else read here — the transition journal, the agent logs, the
//! dashboard address, the runner's own plan listing — is the binding's own
//! artifact grammar, spelled in this module and nowhere else
//! (§REQ-001-boundary.5).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::plan::Plan;
use super::plan::WorkRoot;
use super::runner;
use crate::seams::summons::{self, quote, Mode, Site, Summons};
use crate::work::recipe::WorkConfig;

/// The lock a run holds on its execution root, under the root. The file is
/// created by the first run on a root and never rewritten after: only its
/// birth is a timestamp, everything else about it is the flock itself.
const LOCK: &str = ".rhei/run.lock";

/// The append-only journal a run writes one line to per slot move — and
/// keeps across runs: nothing ever truncates it, so a line is evidence that
/// something happened, never that it is still happening.
const JOURNAL: &str = "runtime/transitions.log";

/// Where a run writes each task invocation's log: `task-<id>-<state>[-…].log`,
/// with a target slug and a visit under fanout — one task, many logs.
const LOGS: &str = "runtime/logs";

/// Where a run serving a dashboard publishes its address, for exactly as long
/// as that run is live.
const DASHBOARD: &str = "runtime/dashboard.json";

/// How long a live run may write nothing before the board notes it. A badge,
/// never a liveness signal (§FS-005-dispatch.15): a long tool call is
/// legitimately quiet, and a run that died released its lock.
const QUIET_AFTER: Duration = Duration::from_secs(10 * 60);

/// The verb the plan listing fills, for messages.
const LIST_VERB: &str = "work.list";

/// How long the runner gets to list its own plans before the floor answers
/// alone. A local parse, not a fetch — seconds are generous.
const LIST_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether a run is live on this execution root: the run lock is held.
///
/// Probed with a non-blocking shared try-lock. `Ok` means nobody holds it —
/// the probe lets go on the spot — and `WouldBlock` means a run does. A lock
/// file that is not there means no run ever locked this root, and nothing is
/// created to find that out: watching writes nothing.
pub fn live(_config: &WorkConfig, root: &Path) -> bool {
    let Ok(file) = fs::File::open(root.join(LOCK)) else {
        return false;
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(fs::TryLockError::WouldBlock) => true,
        // The probe could not say. Not-live is the honest default: a row
        // claiming a run that may not exist is the watch inventing news.
        Err(fs::TryLockError::Error(_)) => false,
    }
}

/// One slot assignment the journal never released: a task some run took up
/// and, as far as the journal alone can say, never let go of.
///
/// Evidence, not truth (§FS-005-dispatch.15): the journal is append-only
/// across runs, and a run that crashed mid-slot leaves its last assignment
/// unreleased forever — under a later run on the same root that stale entry
/// would read as held. So a held entry is believed only where the world
/// still agrees with it: [`Held::still_at`] checks the ticket's own current
/// state against the journaled one, and the caller drops entries whose log
/// predates the root's lock file — no genuine invocation writes its log
/// before the lock that guards every run existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub task: String,
    /// The state(s) the journaled move names: the target, and the plan's own
    /// raw spelling where the two differ — the journal writes the machine's
    /// canonical name for a state the plan may spell as an alias.
    pub states: Vec<String>,
    /// The invocation's log, as the journal spelled it — workspace-relative
    /// where it lives under the root.
    pub log: PathBuf,
}

impl Held {
    /// Whether the ticket's current state is still one this assignment
    /// names. A ticket that moved on since was released by whatever moved
    /// it, whether or not the release line survived the run.
    pub fn still_at(&self, state: &str) -> bool {
        self.states.iter().any(|known| known == state)
    }
}

/// The slot assignments the journal never released, one per invocation.
///
/// Keyed on the (task, log) pair, not the task (§FS-005-dispatch.15): under
/// fanout one task runs as several invocations, each with a log of its own,
/// and an interleaved `assign A₁, assign A₂, release A₁` must leave the task
/// held — last-line-wins per task would mark it free while A₂ still runs.
pub fn holding(_config: &WorkConfig, root: &Path) -> Vec<Held> {
    let Ok(text) = fs::read_to_string(root.join(JOURNAL)) else {
        return Vec::new();
    };
    // Insertion-ordered: the board reads in the order the run took them up.
    let mut slots: Vec<(String, String, Vec<String>, bool)> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split("  ").filter(|field| !field.is_empty()).collect();
        let (Some(task), Some(moved), Some(log)) = (fields.get(1), fields.get(2), fields.get(3))
        else {
            continue;
        };
        // The journal also carries lines that are not slot moves (usage
        // accounting); only assignment and release move the held set.
        let states: Vec<String> = if let Some(state) = moved
            .strip_prefix("start@")
            .or_else(|| moved.strip_prefix("end@"))
        {
            vec![state.to_string()]
        } else if let Some((from, to)) = moved.split_once('→') {
            if from == to {
                vec![to.to_string()]
            } else {
                vec![from.to_string(), to.to_string()]
            }
        } else {
            continue;
        };
        // A release line carries `exit=`/`duration=`/`outcome=` metadata; an
        // assignment carries none. The move itself cannot tell them apart —
        // `a→b` appears on both sides of a slot.
        let released = fields
            .get(4)
            .map(|meta| meta.contains("outcome="))
            .unwrap_or(false);
        match slots
            .iter_mut()
            .find(|(name, path, ..)| name == *task && path == *log)
        {
            Some(slot) => {
                slot.2 = states;
                slot.3 = !released;
            }
            None => slots.push(((*task).to_string(), (*log).to_string(), states, !released)),
        }
    }
    slots
        .into_iter()
        .filter(|(.., holding)| *holding)
        .map(|(task, log, states, _)| Held {
            task,
            states,
            log: PathBuf::from(log),
        })
        .collect()
}

/// Whether a live run holds this ticket right now: the root's lock is held,
/// and the journal names the ticket in the state the plan still has it in,
/// by a log that does not predate the lock. The same reading the board makes
/// for a running row (§FS-005-dispatch.15), asked here before a cancel: a
/// ticket a live run holds is the run's to finish, not the reader's to move
/// (§FS-005-dispatch.16).
pub fn held_by_live_run(
    config: &WorkConfig,
    root: &Path,
    plan_id: &str,
    ticket: &str,
    state: &str,
) -> bool {
    if !live(config, root) {
        return false;
    }
    let lock_born = fs::metadata(root.join(LOCK))
        .and_then(|meta| meta.modified())
        .ok();
    holding(config, root).iter().any(|entry| {
        (entry.task == ticket || entry.task == format!("{plan_id}.{ticket}"))
            && entry.still_at(state)
            && !predates_lock(root, entry, lock_born)
    })
}

/// Whether a held entry's log was last written before the root's lock file
/// existed. The lock is created before the first run on a root does
/// anything, so a genuine invocation's log always postdates it — a log that
/// does not belongs to a workspace that was copied or hand-cleaned, and its
/// entry is stale however much else lines up (§FS-005-dispatch.15). A
/// conservative check, not the main one: the lock file is created once and
/// never touched again, so its mtime says nothing about the current run.
fn predates_lock(root: &Path, held: &Held, lock_born: Option<SystemTime>) -> bool {
    let Some(born) = lock_born else {
        return false;
    };
    fs::metadata(root.join(&held.log))
        .and_then(|meta| meta.modified())
        .map(|wrote| wrote < born)
        .unwrap_or(false)
}

/// The dashboard a run on this root published, per run rather than per
/// ticket: the address file exists only while a run serving one is live, so
/// the caller gates this on [`live`].
pub fn dashboard(_config: &WorkConfig, root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join(DASHBOARD)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
}

/// When a run on this root last moved anything the change gate watches: the
/// newest of the journal — the runtime writes it a line per slot move — and
/// the lock file, which is born when the first run ever locks the root.
///
/// Deliberately not the agent logs (§FS-005-dispatch.15.1): logs accumulate
/// for the life of a project and are appended between slot moves, so statting
/// every one of them on every tick is an unbounded sweep for an answer the
/// journal already gives. How long a live run has been silent is the quiet
/// badge's business — the clock behind [`pulse`] — measured only for a live
/// row.
pub fn wrote_at(_config: &WorkConfig, root: &Path) -> Option<SystemTime> {
    [JOURNAL, LOCK]
        .iter()
        .filter_map(|name| {
            fs::metadata(root.join(name))
                .and_then(|meta| meta.modified())
                .ok()
        })
        .max()
}

/// When a run on this root last wrote anything at all: the change gate's
/// files plus every agent log. The quiet badge's clock — a run is quiet by
/// what it stopped saying, and what it says between slot moves goes to the
/// logs — and only its clock: this walk is paid for a live row, never as a
/// gate on every tick (§FS-005-dispatch.15.1).
fn last_write(config: &WorkConfig, root: &Path) -> Option<SystemTime> {
    let mut newest = wrote_at(config, root);
    if let Ok(entries) = fs::read_dir(root.join(LOGS)) {
        for entry in entries.flatten() {
            if let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) {
                if newest.map(|seen| modified > seen).unwrap_or(true) {
                    newest = Some(modified);
                }
            }
        }
    }
    newest
}

/// How a person frees a ticket somebody claimed and nobody is running, in
/// the bound runner's own words (§FS-005-dispatch.10). Part of the coupling,
/// and so part of this module — a surface shows the line, it does not
/// compose it.
pub fn release_command(config: &WorkConfig, ticket: &str) -> String {
    format!("{} release {ticket}", runner(config))
}

/// The cheap re-probe of one root: liveness, and the badges that ride on it.
/// What a tick can refresh without re-reading any plan
/// (§FS-005-dispatch.15.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulse {
    pub live: bool,
    pub dashboard: Option<String>,
    /// Minutes since a live run last wrote, where that is long enough to
    /// note. None on a not-live root, and on one writing normally.
    pub quiet: Option<u64>,
}

pub fn pulse(config: &WorkConfig, root: &Path) -> Pulse {
    let live = live(config, root);
    Pulse {
        dashboard: live.then(|| dashboard(config, root)).flatten(),
        quiet: if live {
            quiet_minutes(config, root)
        } else {
            None
        },
        live,
    }
}

fn quiet_minutes(config: &WorkConfig, root: &Path) -> Option<u64> {
    let elapsed = last_write(config, root)?.elapsed().ok()?;
    (elapsed >= QUIET_AFTER).then(|| elapsed.as_secs() / 60)
}

/// A ticket as the runner's own listing reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedTicket {
    /// The plan the ticket lives in — its file stem, which is also ephor's
    /// plan id.
    pub plan: String,
    pub ticket: String,
    pub state: Option<String>,
    pub assignee: Option<String>,
}

/// State and assignee as the binary itself reports them, via its own plan
/// listing run as a captured summons. None where the runner is absent, the
/// listing fails, or its answer does not parse — the direct plan read is the
/// floor (§AR-007-runtime.3) and this only sharpens it, never replaces it
/// (§FS-005-dispatch.15).
pub fn listed(config: &WorkConfig, root: &Path) -> Option<Vec<ListedTicket>> {
    if super::refusal(config).is_some() {
        return None;
    }
    listing(config, root)
}

/// The listing itself, with the workable rung already answered for: the
/// board checks it once per build, not once per root
/// (§FS-005-dispatch.15.1).
///
/// The listing's stdout is parsed here: the binding's own JSON, honored by
/// this one binding the way custom-status's stdout is (§AR-002-summons.3) —
/// it is the runner reading back its own plans, not a project's answer
/// envelope.
fn listing(config: &WorkConfig, root: &Path) -> Option<Vec<ListedTicket>> {
    let command = format!(
        "{} list {} --json",
        runner(config),
        quote(&root.to_string_lossy())
    );
    let answer = summons::run(
        &Summons::new(LIST_VERB, command),
        &Site::root(root),
        Mode::Captured(LIST_TIMEOUT),
    )
    .ok()?;
    if !answer.is_done() {
        return None;
    }
    parse_listing(answer.output.as_deref()?)
}

/// The listing's rows, with each id split into the plan it belongs to and
/// the ticket inside it — the listing qualifies ids with the plan's stem.
fn parse_listing(output: &str) -> Option<Vec<ListedTicket>> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(output.trim()).ok()?;
    Some(
        rows.iter()
            .filter_map(|row| {
                let id = row.get("id")?.as_str()?;
                let (plan, ticket) = id.split_once('.')?;
                let word = |key: &str| {
                    row.get(key)
                        .and_then(serde_json::Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(String::from)
                };
                Some(ListedTicket {
                    plan: plan.to_string(),
                    ticket: ticket.to_string(),
                    state: word("state"),
                    assignee: word("assignee"),
                })
            })
            .collect(),
    )
}

/// One plan inside an execution root, as the caller knows it — from the
/// ledger where ephor dispatched it, from enumerating the work roots
/// otherwise (§FS-005-dispatch.15). `item` is the feed item the plan is
/// about, where the caller knows one: it is how the board's Enter finds the
/// matter, and a plan ephor never dispatched has none by construction —
/// Enter opens the plan itself then. `title` may be empty for such a plan;
/// the board fills it from the plan's own heading.
#[derive(Debug, Clone)]
pub struct PlanRef {
    pub project: String,
    pub plan_id: String,
    pub path: PathBuf,
    pub item: Option<String>,
    pub title: String,
}

/// One execution root and the plans the caller knows in it.
#[derive(Debug, Clone)]
pub struct RootPlans {
    pub root: PathBuf,
    pub plans: Vec<PlanRef>,
}

/// What one ticket on the board is doing (§FS-005-dispatch.15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doing {
    /// A live run holds this ticket's slot right now.
    Running,
    /// Its root's run is live and will reach it: rhei locks per root, so a
    /// ticket written into a locked root waits its turn.
    Queued,
    /// Waiting on the reader (§FS-005-dispatch.9), with or without a run:
    /// parked in a state the runtime will not leave on its own — the run
    /// that parked it has usually exited, since nothing else was schedulable.
    /// A question about the work, which is what tells it from [`Doing::Dropped`].
    Waiting,
    /// Dropped by a run that died holding its slot (§FS-005-dispatch.15):
    /// the lock is free and the journal never released the assignment. Not a
    /// question about the work — a run that wants starting again. The
    /// machine's gating word makes a ticket [`Doing::Waiting`]; the journal's
    /// unreleased slot under a lock nobody holds makes it this, so the two
    /// are told apart from artifacts, never guessed.
    Dropped,
    /// Claimed and unschedulable: an assignee, whom the runtime skips. `free`
    /// is the bound runner's own command for releasing the claim — reported,
    /// never run.
    Claimed { assignee: String, free: String },
}

impl Doing {
    /// Where a ticket sorts inside its operation: what asks something of the
    /// reader ahead of anything else its work is doing (§FS-005-dispatch.9)
    /// — a parked question first, then what a dead run dropped — then what
    /// runs, then claims, then the queue (§FS-005-dispatch.15).
    fn rank(&self) -> u8 {
        match self {
            Doing::Waiting => 0,
            Doing::Dropped => 1,
            Doing::Running => 2,
            Doing::Claimed { .. } => 3,
            Doing::Queued => 4,
        }
    }
}

/// One non-finished ticket of an operation, with what it is doing.
#[derive(Debug, Clone)]
pub struct BoardTicket {
    pub plan_id: String,
    pub ticket: String,
    pub state: Option<String>,
    pub doing: Doing,
    pub item: Option<String>,
    pub title: String,
    pub plan: PathBuf,
}

/// One row of the board: an execution root, not a ticket — the runtime
/// schedules one run per root (§FS-005-dispatch.15).
#[derive(Debug, Clone)]
pub struct Operation {
    pub project: String,
    pub root: PathBuf,
    pub live: bool,
    pub dashboard: Option<String>,
    /// Minutes of silence worth noting on a live run. A badge, nothing more.
    pub quiet: Option<u64>,
    pub tickets: Vec<BoardTicket>,
    /// Finished tickets, counted rather than listed: they are history, and
    /// the plan behind Enter holds the whole of it. Tickets taken back are
    /// counted apart, in [`Operation::cancelled`] — a different kind of over
    /// (§FS-005-dispatch.16).
    pub done: usize,
    /// Tickets in the abandonment state, counted beside the finished.
    pub cancelled: usize,
    /// Why this root's own state machine could not be read, where it could
    /// not (§FS-005-dispatch.15): finality and gating are the machine's
    /// words, so nothing on such a row is called queued or waiting and
    /// nothing is counted finished — and the row says so itself, rather than
    /// leaving a zero to be misread as nothing done.
    pub machine_unread: Option<String>,
    /// Every plan the caller knows in this root, one path each — what
    /// [`Operation::plan`] falls back to when every ticket was filtered out
    /// of the row (§FS-005-dispatch.15).
    pub plans: Vec<PathBuf>,
}

impl Operation {
    /// The feed item behind this operation, where any of its plans knows one.
    pub fn item(&self) -> Option<&str> {
        self.tickets
            .iter()
            .find_map(|ticket| ticket.item.as_deref())
    }

    /// The plan Enter falls back to where the operation has no matter: a
    /// ticket's own plan first, else the first plan known in this root — a
    /// live root whose tickets were all filtered out still has one behind it.
    pub fn plan(&self) -> Option<&Path> {
        self.tickets
            .first()
            .map(|ticket| ticket.plan.as_path())
            .or_else(|| self.plans.first().map(PathBuf::as_path))
    }
}

/// Every operation, or why there are none: with no runtime bound an
/// operation cannot exist, and the sentence is the workable rung's own
/// (§FS-005-dispatch.15).
#[derive(Debug, Clone)]
pub struct Board {
    pub operations: Vec<Operation>,
    pub refusal: Option<String>,
}

/// The board, read from the artifacts: liveness from the lock, held tickets
/// from the journal, state and assignee from the plans with the runner's own
/// listing sharpening them where the binary is there (§FS-005-dispatch.15).
/// A root appears iff a run is live on it, a claim sits in it, or a ticket
/// in it waits on the reader — parked, or left mid-slot by a run that died
/// (§FS-005-dispatch.9); everything else is not an operation.
pub fn board(config: &WorkConfig, roots: &[RootPlans]) -> Board {
    if let Some(refusal) = super::refusal(config) {
        return Board {
            operations: Vec::new(),
            refusal: Some(refusal),
        };
    }
    let mut operations = Vec::new();
    for group in roots {
        let is_live = live(config, &group.root);
        let held = holding(config, &group.root);
        let lock_born = fs::metadata(group.root.join(LOCK))
            .and_then(|meta| meta.modified())
            .ok();
        // The floor first: the plans themselves, read off disk. A plan the
        // caller has no title for — enumerated, never dispatched — lends its
        // own heading, so a foreign row still says what the work is about
        // (§FS-005-dispatch.15).
        let floor: Vec<(&PlanRef, String, Vec<super::plan::PlanTicket>)> = group
            .plans
            .iter()
            .filter_map(|plan_ref| {
                let plan = Plan::read(&plan_ref.path).ok().flatten()?;
                let title = match plan_ref.title.is_empty() {
                    true => plan.title().unwrap_or_default(),
                    false => plan_ref.title.clone(),
                };
                Some((plan_ref, title, plan.tickets()))
            })
            .collect();
        // The listing forks the runner's binary, so it is asked only where a
        // row could come of its answer: a live root, or one the floor already
        // shows a claim in. An idle root costs stats and nothing else
        // (§FS-005-dispatch.15.1).
        let floor_claim = floor
            .iter()
            .any(|(.., tickets)| tickets.iter().any(|ticket| ticket.assignee.is_some()));
        let lifted: BTreeMap<(String, String), ListedTicket> = if is_live || floor_claim {
            listing(config, &group.root)
                .unwrap_or_default()
                .into_iter()
                .map(|row| ((row.plan.clone(), row.ticket.clone()), row))
                .collect()
        } else {
            BTreeMap::new()
        };
        let machine = WorkRoot::open(&group.root);
        // Read or not is a fact the row carries (§FS-005-dispatch.15): with
        // the machine unreadable, queued and finished are withheld below, and
        // a row that withheld them silently would leave its zero to be read
        // as nothing done.
        let machine_unread = match &machine {
            Ok(Some(_)) => None,
            Ok(None) => Some("no states.yaml — nothing judged queued or finished".to_string()),
            Err(_) => {
                Some("states.yaml unreadable — nothing judged queued or finished".to_string())
            }
        };
        let machine = machine.ok().flatten();
        let is_final = |state: Option<&str>| {
            state
                .zip(machine.as_ref())
                .map(|(state, machine)| machine.is_final(state))
                .unwrap_or(false)
        };
        let is_gating = |state: Option<&str>| {
            state
                .zip(machine.as_ref())
                .map(|(state, machine)| machine.is_gating(state))
                .unwrap_or(false)
        };
        // A held entry is believed only while the ticket's own state still
        // says so, and only where its log does not predate the lock: the
        // journal survives across runs, and a run that crashed mid-slot left
        // its last assignment unreleased forever (§FS-005-dispatch.15).
        let held_now = |plan_id: &str, ticket: &str, state: Option<&str>| {
            held.iter().any(|entry| {
                (entry.task == ticket || entry.task == format!("{plan_id}.{ticket}"))
                    && state.map(|state| entry.still_at(state)).unwrap_or(false)
                    && !predates_lock(&group.root, entry, lock_born)
            })
        };
        let mut tickets = Vec::new();
        let mut done = 0;
        let mut cancelled = 0;
        let mut known: BTreeSet<(String, String)> = BTreeSet::new();
        for (plan_ref, title, floor_tickets) in &floor {
            for ticket in floor_tickets {
                known.insert((plan_ref.plan_id.clone(), ticket.id.clone()));
                // The floor is the plan file; a listing that names this
                // ticket is fresher and answers for it whole.
                let (state, assignee) =
                    match lifted.get(&(plan_ref.plan_id.clone(), ticket.id.clone())) {
                        Some(row) => (row.state.clone(), row.assignee.clone()),
                        None => (ticket.state.clone(), ticket.assignee.clone()),
                    };
                if is_final(state.as_deref()) {
                    // Over either way; taken back is said apart from finished
                    // (§FS-005-dispatch.16).
                    match state.as_deref() == Some(super::plan::CANCELLED) {
                        true => cancelled += 1,
                        false => done += 1,
                    }
                    continue;
                }
                let holds = held_now(&plan_ref.plan_id, &ticket.id, state.as_deref());
                let doing = if is_gating(state.as_deref()) {
                    // Parked is a flavour of its own, before liveness: the
                    // usual end of the run that parked a ticket is that run
                    // exiting, and the ticket waits on the reader all the
                    // same (§FS-005-dispatch.9). A journal entry held on a
                    // gating state is stale by construction — the runtime
                    // never takes a slot there.
                    Doing::Waiting
                } else if is_live && holds {
                    Doing::Running
                } else if let Some(assignee) = assignee.clone() {
                    // A claim is a claim under a live run too: the runtime
                    // skips an assigned ticket, so "queued" would promise a
                    // turn that never comes (§FS-005-dispatch.15).
                    Doing::Claimed {
                        free: release_command(
                            config,
                            &format!("{}.{}", plan_ref.plan_id, ticket.id),
                        ),
                        assignee,
                    }
                } else if is_live {
                    if machine.is_none() {
                        // No machine, no judgment: with finality and gating
                        // unreadable, "queued" and the done count would be
                        // confident guesses — the row says less instead
                        // (§FS-005-dispatch.15).
                        continue;
                    }
                    Doing::Queued
                } else if holds {
                    // The lock is free and the journal still holds the
                    // ticket where the plan has it: the run died mid-slot,
                    // and nobody but the reader will move this
                    // (§FS-005-dispatch.9). Its own flavour, not Waiting —
                    // a parked ticket asks a question about the work, this
                    // one asks for the run back (§FS-005-dispatch.15).
                    Doing::Dropped
                } else {
                    // Open, unclaimed, and no run: waiting work, not an
                    // operation — the work screen's business.
                    continue;
                };
                tickets.push(BoardTicket {
                    plan_id: plan_ref.plan_id.clone(),
                    ticket: ticket.id.clone(),
                    state,
                    doing,
                    item: plan_ref.item.clone(),
                    title: title.clone(),
                    plan: plan_ref.path.clone(),
                });
            }
        }
        // A held task the floor cannot see — a subtask, which the plan read
        // does not surface — still shows as running where the runner's own
        // listing names it: the listing sharpens the floor, and a run
        // working a ticket the board would otherwise show nothing for is
        // exactly what sharpening is for (§FS-005-dispatch.15).
        if is_live {
            for row in lifted.values() {
                let key = (row.plan.clone(), row.ticket.clone());
                if known.contains(&key) {
                    continue;
                }
                let Some(plan_ref) = group.plans.iter().find(|plan| plan.plan_id == row.plan)
                else {
                    continue;
                };
                if !held_now(&row.plan, &row.ticket, row.state.as_deref()) {
                    continue;
                }
                known.insert(key);
                tickets.push(BoardTicket {
                    plan_id: row.plan.clone(),
                    ticket: row.ticket.clone(),
                    state: row.state.clone(),
                    doing: Doing::Running,
                    item: plan_ref.item.clone(),
                    // The same backfill the floor got: the plan's own heading
                    // where the caller had no title (§FS-005-dispatch.15).
                    title: floor
                        .iter()
                        .find(|(candidate, ..)| candidate.plan_id == row.plan)
                        .map(|(_, title, _)| title.clone())
                        .unwrap_or_else(|| plan_ref.title.clone()),
                    plan: plan_ref.path.clone(),
                });
            }
        }
        // What waits on the reader is shown ahead of anything else its work
        // is doing (§FS-005-dispatch.9); a stable sort keeps plan order
        // inside each flavour.
        tickets.sort_by_key(|ticket| ticket.doing.rank());
        if is_live || !tickets.is_empty() {
            operations.push(Operation {
                project: group
                    .plans
                    .first()
                    .map(|plan| plan.project.clone())
                    .unwrap_or_default(),
                root: group.root.clone(),
                dashboard: is_live.then(|| dashboard(config, &group.root)).flatten(),
                quiet: if is_live {
                    quiet_minutes(config, &group.root)
                } else {
                    None
                },
                live: is_live,
                tickets,
                done,
                cancelled,
                machine_unread,
                plans: group.plans.iter().map(|plan| plan.path.clone()).collect(),
            });
        }
    }
    Board {
        operations,
        refusal: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A runner every machine has, so the workable rung holds and only the
    /// listing itself fails — which is exactly the floor's case.
    fn config() -> WorkConfig {
        WorkConfig {
            runner: Some("sh".to_string()),
            ..WorkConfig::default()
        }
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn stamp(path: &Path, secs: u64) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    }

    /// Released means "reads as not live promptly", not instantaneously: a
    /// process forked by a parallel test between our drop and our probe
    /// briefly duplicates the descriptor and with it the flock, until its
    /// exec closes it. Poll past that window rather than racing it.
    fn assert_released(config: &WorkConfig, root: &Path) {
        for _ in 0..200 {
            if !live(config, root) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the lock should read released");
    }

    #[test]
    fn liveness_is_the_lock_and_only_the_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // No lock file: no run ever locked this root — and the probe wrote
        // nothing to find that out.
        assert!(!live(&config(), root));
        assert!(!root.join(".rhei").exists());

        // A lock file nobody holds is what every finished or crashed run
        // leaves behind: not live.
        write(&root.join(LOCK), "");
        assert!(!live(&config(), root));

        // A held lock is a live run.
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();
        assert!(live(&config(), root));

        // The OS releases it on crash; dropping the descriptor is the same
        // release, and the root stops being live with no file changed.
        drop(holder);
        assert_released(&config(), root);
    }

    #[test]
    fn the_journal_says_which_tickets_a_run_holds() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join(JOURNAL),
            concat!(
                "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
                "2026-08-14T10:05:00Z  fix-gate-1  end@fix  runtime/logs/task-fix-gate-1-fix.log  exit=0,duration=5m,outcome=completed\n",
                // Usage accounting is a journal line too, and not a move.
                "2026-08-14T10:05:00Z  fix-gate-1  usage  invocation=fix-gate-1::fix agent=a cost=unpriced\n",
                // A transition release: the move spells from→to on both the
                // assignment and the release, and only the metadata tells.
                "2026-08-14T10:06:00Z  answer-1  fix→review  runtime/logs/task-answer-1-fix.log\n",
                "2026-08-14T10:09:00Z  answer-1  fix→review  runtime/logs/task-answer-1-fix.log  exit=0,duration=3m,outcome=completed\n",
                "2026-08-14T10:10:00Z  answer-1  start@review  runtime/logs/task-answer-1-review.log\n",
            ),
        );
        let held = holding(&config(), root);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].task, "answer-1");
        assert_eq!(held[0].states, vec!["review".to_string()]);
        assert_eq!(
            held[0].log,
            PathBuf::from("runtime/logs/task-answer-1-review.log")
        );
        // Either side of a journaled move counts as "still there": the
        // journal writes the machine's canonical name for a state the plan
        // may spell as an alias.
        assert!(held[0].still_at("review"));
        assert!(!held[0].still_at("fix"));

        // No journal: a run that has not written one holds nothing readable.
        let empty = tempfile::tempdir().unwrap();
        assert!(holding(&config(), empty.path()).is_empty());
    }

    /// Under fanout one task is several invocations, each with a log of its
    /// own: a release of one leaves the task held through the other, and two
    /// tasks can be held at once (§FS-005-dispatch.15).
    #[test]
    fn held_is_per_invocation_not_per_task() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join(JOURNAL),
            concat!(
                "2026-08-14T10:00:00Z  demo-1  start@fix  runtime/logs/task-demo-1-fix-alpha.log\n",
                "2026-08-14T10:00:01Z  demo-1  start@fix  runtime/logs/task-demo-1-fix-beta.log\n",
                "2026-08-14T10:00:02Z  other-1  start@fix  runtime/logs/task-other-1-fix.log\n",
                // The first invocation releases; the second still runs.
                "2026-08-14T10:05:00Z  demo-1  end@fix  runtime/logs/task-demo-1-fix-alpha.log  exit=0,duration=5m,outcome=completed\n",
            ),
        );
        let held = holding(&config(), root);
        assert_eq!(held.len(), 2);
        assert_eq!(held[0].task, "demo-1");
        assert_eq!(
            held[0].log,
            PathBuf::from("runtime/logs/task-demo-1-fix-beta.log")
        );
        assert_eq!(held[1].task, "other-1");
    }

    /// The change gate is the journal and the lock, never a walk of the
    /// logs: the runtime journals every slot move, and logs grow for the
    /// life of a project (§FS-005-dispatch.15.1). The quiet clock is the
    /// opposite: it wants the newest write of any kind, logs included.
    #[test]
    fn the_change_gate_is_bounded_and_the_quiet_clock_is_not() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join(JOURNAL), "journal\n");
        write(&root.join(LOCK), "");
        let log = root.join(LOGS).join("task-demo-1-fix.log");
        write(&log, "chatter");
        stamp(&root.join(JOURNAL), 1_000);
        stamp(&root.join(LOCK), 2_000);
        stamp(&log, 3_000);
        // The newest log write is invisible to the gate…
        assert_eq!(
            wrote_at(&config(), root),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(2_000))
        );
        // …and is exactly what the quiet clock reads.
        assert_eq!(
            last_write(&config(), root),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(3_000))
        );
    }

    /// The quiet badge is minutes of silence on a live run — never asserted
    /// by hand-building a Pulse: the clock itself is what can regress.
    #[test]
    fn a_live_run_that_stopped_writing_wears_the_quiet_badge() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join(JOURNAL), "journal\n");
        write(&root.join(LOCK), "");
        let log = root.join(LOGS).join("task-demo-1-fix.log");
        write(&log, "spoke once");
        for path in [&root.join(JOURNAL), &root.join(LOCK), &log] {
            stamp(path, 1_000); // long, long ago
        }
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();

        let alive = pulse(&config(), root);
        assert!(alive.live);
        let minutes = alive.quiet.expect("decades of silence is worth a badge");
        assert!(minutes >= QUIET_AFTER.as_secs() / 60, "{minutes}");

        // A fresh write clears it: the run is talking again.
        write(&log, "spoke just now");
        let talking = pulse(&config(), root);
        assert_eq!(talking.quiet, None);
        drop(holder);
    }

    #[test]
    fn the_dashboard_is_an_address_a_live_run_published() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert_eq!(dashboard(&config(), root), None);
        write(
            &root.join(DASHBOARD),
            r#"{ "url": "http://127.0.0.1:39114" }"#,
        );
        assert_eq!(
            dashboard(&config(), root).as_deref(),
            Some("http://127.0.0.1:39114")
        );
    }

    #[test]
    fn the_listing_is_split_into_plan_and_ticket() {
        let rows = parse_listing(
            r#"[
              {"id": "widget-42.fix-gate-1", "state": "fix", "assignee": "luna"},
              {"id": "widget-42.answer-1", "state": "done", "assignee": null},
              {"id": "no-dot-id", "state": "fix"}
            ]"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].plan, "widget-42");
        assert_eq!(rows[0].ticket, "fix-gate-1");
        assert_eq!(rows[0].assignee.as_deref(), Some("luna"));
        assert_eq!(rows[1].state.as_deref(), Some("done"));
        assert_eq!(rows[1].assignee, None);
        assert_eq!(parse_listing("not json"), None);
    }

    /// With no runner bound the listing is not attempted: the floor answers
    /// alone (§AR-007-runtime.3).
    #[test]
    fn the_listing_is_none_where_nothing_can_answer_it() {
        let tmp = tempfile::tempdir().unwrap();
        let absent = WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            ..WorkConfig::default()
        };
        assert_eq!(listed(&absent, tmp.path()), None);
        // Present but not a runner: the summons fails and the answer is the
        // same None, never an error the board would wear.
        assert_eq!(listed(&config(), tmp.path()), None);
    }

    fn root_with_plan(root: &Path, claimed: bool) -> RootPlans {
        write(
            &root.join("states.yaml"),
            concat!(
                "name: m\n",
                "states:\n",
                "  fix:\n    agent: x\n",
                "  needs-human:\n    gating: true\n",
                "  done:\n    final: true\n",
            ),
        );
        plan_only_root(root, claimed)
    }

    /// The same plan with no machine beside it, for the root that cannot
    /// judge finality or gating.
    fn plan_only_root(root: &Path, claimed: bool) -> RootPlans {
        let assignee = if claimed { "**Assignee:** luna\n" } else { "" };
        write(
            &root.join("widget-42.rhei.md"),
            &format!(
                "# Rhei: t\n**States:** m\n\n## Tasks\n\n\
                 ### Task fix-gate-1: fix the gate\n**State:** fix\n{assignee}\nwork\n\n\
                 ### Task answer-1: answer\n**State:** fix\n\nwork\n\n\
                 ### Task old-1: shipped\n**State:** done\n\nwork\n"
            ),
        );
        RootPlans {
            root: root.to_path_buf(),
            plans: vec![PlanRef {
                project: "widget".to_string(),
                plan_id: "widget-42".to_string(),
                path: root.join("widget-42.rhei.md"),
                item: Some("forge:widget/42".to_string()),
                title: "Widen the retry window".to_string(),
            }],
        }
    }

    fn park(root: &Path, ticket: &str) {
        let plan = root.join("widget-42.rhei.md");
        let text = fs::read_to_string(&plan).unwrap();
        write(
            &plan,
            &text.replacen(
                &format!("### Task {ticket}: answer\n**State:** fix"),
                &format!("### Task {ticket}: answer\n**State:** needs-human"),
                1,
            ),
        );
    }

    /// A held lock plus a journal is a running row, with the held ticket
    /// running and the rest queued; finished work is counted, not listed.
    /// And a ticket parked under the live run outranks everything else in
    /// the operation (§FS-005-dispatch.9).
    #[test]
    fn a_locked_root_is_a_running_operation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, false);
        write(
            &root.join(JOURNAL),
            "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
        );
        write(&root.join(LOCK), "");
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.refusal, None);
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert!(op.live);
        assert_eq!(op.done, 1);
        assert_eq!(op.tickets.len(), 2);
        assert_eq!(op.tickets[0].ticket, "fix-gate-1");
        assert_eq!(op.tickets[0].doing, Doing::Running);
        assert_eq!(op.tickets[1].ticket, "answer-1");
        assert_eq!(op.tickets[1].doing, Doing::Queued);
        assert_eq!(op.item(), Some("forge:widget/42"));

        // A parked ticket under a live run is waiting on the reader, and is
        // listed ahead of the running one (§FS-005-dispatch.9).
        park(root, "answer-1");
        let board = super::board(&config(), std::slice::from_ref(&group));
        let op = &board.operations[0];
        assert_eq!(op.tickets[0].ticket, "answer-1");
        assert_eq!(op.tickets[0].doing, Doing::Waiting);
        assert_eq!(op.tickets[1].doing, Doing::Running);
        drop(holder);
    }

    /// The payoff case (§FS-005-dispatch.9): a run parks a ticket, nothing
    /// else is schedulable, the run exits and the lock goes free — and the
    /// parked ticket keeps its row. `rhei transition` writes no assignee, so
    /// this root has no claim to make it a row any other way.
    #[test]
    fn a_parked_ticket_keeps_its_row_after_the_run_exits() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, false);
        park(root, "answer-1");
        // The lock file the exited run left behind, held by nobody.
        write(&root.join(LOCK), "");

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert!(!op.live);
        // Only the parked ticket is an operation: the open unclaimed one is
        // waiting work, and the finished one is history.
        assert_eq!(op.tickets.len(), 1);
        assert_eq!(op.tickets[0].ticket, "answer-1");
        assert_eq!(op.tickets[0].doing, Doing::Waiting);
        assert_eq!(op.done, 1);
    }

    /// A run that died mid-slot: the lock is free, the journal still holds
    /// the ticket, and the plan still has it where the journal put it. The
    /// ticket keeps a row — nobody else will move it — as its own flavour,
    /// dropped, never conflated with parked: one asks a question about the
    /// work, the other asks for the run back (§FS-005-dispatch.15). The
    /// moment the ticket's own state moves on, the stale entry stops
    /// counting and the root stops being a row.
    #[test]
    fn a_run_that_died_mid_slot_leaves_its_ticket_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, false);
        write(&root.join(LOCK), "");
        write(
            &root.join(JOURNAL),
            "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
        );
        write(
            &root.join(LOGS).join("task-fix-gate-1-fix.log"),
            "was working",
        );

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert!(!op.live);
        assert_eq!(op.tickets.len(), 1);
        assert_eq!(op.tickets[0].ticket, "fix-gate-1");
        assert_eq!(op.tickets[0].doing, Doing::Dropped);

        // Somebody moved the ticket on — completed by hand, or another round
        // took it up and finished: the journal entry is stale, and this root
        // holds no operation any more.
        let plan = root.join("widget-42.rhei.md");
        let text = fs::read_to_string(&plan).unwrap();
        write(
            &plan,
            &text.replacen(
                "### Task fix-gate-1: fix the gate\n**State:** fix",
                "### Task fix-gate-1: fix the gate\n**State:** done",
                1,
            ),
        );
        let board = super::board(&config(), std::slice::from_ref(&group));
        assert!(
            board.operations.is_empty(),
            "a moved ticket clears the journal's claim"
        );
    }

    /// The stale-journal case under a *later* run (§FS-005-dispatch.15): run
    /// one crashed holding a ticket, run two is live on the same root about
    /// a different one. The dead ticket's entry no longer matches its state
    /// once it moved, and a dead entry whose log predates the lock file is
    /// never believed — neither may read as running forever.
    #[test]
    fn a_crashed_runs_stale_entry_does_not_read_running_under_a_later_run() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, false);
        // Run one took fix-gate-1 up and never released it; run two is now
        // live, working answer-1. fix-gate-1 has since been moved to review
        // by hand — its state no longer matches the journaled one.
        let plan = root.join("widget-42.rhei.md");
        let text = fs::read_to_string(&plan).unwrap();
        write(
            &plan,
            &text.replacen(
                "### Task fix-gate-1: fix the gate\n**State:** fix",
                "### Task fix-gate-1: fix the gate\n**State:** needs-human",
                1,
            ),
        );
        write(
            &root.join(JOURNAL),
            concat!(
                "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
                "2026-08-15T09:00:00Z  answer-1  start@fix  runtime/logs/task-answer-1-fix.log\n",
            ),
        );
        write(&root.join(LOCK), "");
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();

        let board = board(&config(), std::slice::from_ref(&group));
        let op = &board.operations[0];
        assert!(op.live);
        let doing = |ticket: &str| {
            op.tickets
                .iter()
                .find(|row| row.ticket == ticket)
                .map(|row| row.doing.clone())
        };
        // The moved ticket is whatever its state says — here parked — never
        // running off a dead run's unreleased line.
        assert_eq!(doing("fix-gate-1"), Some(Doing::Waiting));
        assert_eq!(doing("answer-1"), Some(Doing::Running));
        drop(holder);
    }

    /// The conservative backstop: an entry whose journaled log was last
    /// written before the lock file existed cannot be the current run's — no
    /// run writes a log before the lock that guards every run was born
    /// (§FS-005-dispatch.15).
    #[test]
    fn an_entry_whose_log_predates_the_lock_is_never_held() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, false);
        write(
            &root.join(JOURNAL),
            "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
        );
        let log = root.join(LOGS).join("task-fix-gate-1-fix.log");
        write(&log, "another workspace's life");
        write(&root.join(LOCK), "");
        stamp(&log, 1_000);
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();

        let board = board(&config(), std::slice::from_ref(&group));
        let op = &board.operations[0];
        assert_eq!(op.tickets[0].ticket, "fix-gate-1");
        assert_eq!(op.tickets[0].doing, Doing::Queued);
        drop(holder);
    }

    /// The lock free and an assignee on a non-terminal ticket is the other
    /// row flavour: claimed, not scheduled — with the runner's own release
    /// words, reported and never run.
    #[test]
    fn a_claim_with_the_lock_free_is_claimed_not_scheduled() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, true);
        write(&root.join(LOCK), "");

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert!(!op.live);
        // Only the claimed ticket is an operation; the open unclaimed one is
        // waiting work, and the finished one is history.
        assert_eq!(op.tickets.len(), 1);
        assert_eq!(
            op.tickets[0].doing,
            Doing::Claimed {
                assignee: "luna".to_string(),
                free: "sh release widget-42.fix-gate-1".to_string(),
            }
        );
        assert_eq!(op.quiet, None);
        assert_eq!(op.dashboard, None);
    }

    /// A root with neither a live run nor a claim is not an operation at all.
    #[test]
    fn a_root_with_neither_lock_nor_claim_is_not_a_row() {
        let tmp = tempfile::tempdir().unwrap();
        let group = root_with_plan(tmp.path(), false);
        let board = board(&config(), std::slice::from_ref(&group));
        assert!(board.operations.is_empty());
        assert_eq!(board.refusal, None);
    }

    /// One board over several roots: each stays its own operation, in the
    /// order the caller grouped them, and nothing bleeds across.
    #[test]
    fn the_board_holds_one_operation_per_root() {
        let live_tmp = tempfile::tempdir().unwrap();
        let claimed_tmp = tempfile::tempdir().unwrap();
        let idle_tmp = tempfile::tempdir().unwrap();
        let running = root_with_plan(live_tmp.path(), false);
        let claimed = root_with_plan(claimed_tmp.path(), true);
        let idle = root_with_plan(idle_tmp.path(), false);
        write(
            &live_tmp.path().join(JOURNAL),
            "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
        );
        write(&live_tmp.path().join(LOCK), "");
        let holder = fs::File::open(live_tmp.path().join(LOCK)).unwrap();
        holder.lock().unwrap();

        let board = board(&config(), &[running.clone(), claimed.clone(), idle.clone()]);
        assert_eq!(board.operations.len(), 2);
        assert_eq!(board.operations[0].root, running.root);
        assert!(board.operations[0].live);
        assert_eq!(board.operations[0].tickets[0].doing, Doing::Running);
        assert_eq!(board.operations[1].root, claimed.root);
        assert!(!board.operations[1].live);
        assert!(matches!(
            board.operations[1].tickets[0].doing,
            Doing::Claimed { .. }
        ));
        drop(holder);
    }

    /// A root with no machine judges nothing it cannot judge
    /// (§FS-005-dispatch.15): what the lock and the journal prove still
    /// shows — live, running, claimed — but nothing is called queued and
    /// nothing is counted finished on a machine that is not there. And the
    /// withholding is carried as a fact on the operation, for the row to
    /// say, rather than left as a zero to be misread.
    #[test]
    fn a_root_with_no_machine_says_less_instead_of_guessing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = plan_only_root(root, false);
        write(
            &root.join(JOURNAL),
            "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
        );
        write(&root.join(LOCK), "");
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert!(op.live);
        // The journal-held ticket shows; the "done" one is not queued
        // forever and not counted — finality is the machine's word.
        assert_eq!(op.tickets.len(), 1);
        assert_eq!(op.tickets[0].ticket, "fix-gate-1");
        assert_eq!(op.tickets[0].doing, Doing::Running);
        assert_eq!(op.done, 0);
        assert_eq!(
            op.machine_unread.as_deref(),
            Some("no states.yaml — nothing judged queued or finished")
        );
        drop(holder);

        // A claim still reads without a machine: it is the plan's own word.
        let tmp = tempfile::tempdir().unwrap();
        let group = plan_only_root(tmp.path(), true);
        write(&tmp.path().join(LOCK), "");
        let board = super::board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        assert!(matches!(
            board.operations[0].tickets[0].doing,
            Doing::Claimed { .. }
        ));
        assert!(board.operations[0].machine_unread.is_some());

        // A machine that is there and readable carries no such fact.
        let tmp = tempfile::tempdir().unwrap();
        let group = root_with_plan(tmp.path(), true);
        write(&tmp.path().join(LOCK), "");
        let board = super::board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations[0].machine_unread, None);

        // A states.yaml that will not read — no machine name in it — is the
        // other way a machine goes missing, and says so in its own words.
        let tmp = tempfile::tempdir().unwrap();
        let group = plan_only_root(tmp.path(), true);
        write(&tmp.path().join("states.yaml"), "states:\n  fix:\n");
        write(&tmp.path().join(LOCK), "");
        let board = super::board(&config(), std::slice::from_ref(&group));
        assert_eq!(
            board.operations[0].machine_unread.as_deref(),
            Some("states.yaml unreadable — nothing judged queued or finished")
        );
    }

    /// A plan ephor never dispatched is watched exactly as one it did
    /// (§FS-005-dispatch.15): its ref carries no matter and no title — there
    /// is no item to borrow one from — so the board fills the title from the
    /// plan's own heading, and the operation leads to the plan itself.
    #[test]
    fn a_foreign_plan_lends_its_own_heading_and_leads_to_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut group = root_with_plan(root, false);
        group.plans[0].item = None;
        group.plans[0].title = String::new();
        write(
            &root.join("widget-42.rhei.md"),
            "# Rhei: Audit the retry paths\n**States:** m\n\n## Tasks\n\n\
             ### Task fix-gate-1: fix the gate\n**State:** needs-human\n\nwork\n",
        );
        write(&root.join(LOCK), "");

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert_eq!(op.item(), None, "nothing dispatched this, so no matter");
        assert_eq!(op.plan(), Some(root.join("widget-42.rhei.md").as_path()));
        assert_eq!(op.tickets[0].title, "Audit the retry paths");
    }

    /// A parked subtask on a root with no live run keeps its row: the floor
    /// reads every depth the runtime's language nests, so the operation does
    /// not vanish with the run that split the work (§FS-005-dispatch.15,
    /// §FS-005-dispatch.9).
    #[test]
    fn a_parked_subtask_keeps_its_row_on_an_idle_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, false);
        let plan = root.join("widget-42.rhei.md");
        let text = fs::read_to_string(&plan).unwrap();
        write(
            &plan,
            &text.replacen(
                "### Task answer-1: answer\n**State:** fix\n\nwork\n",
                "### Task answer-1: answer\n**State:** fix\n\nwork\n\n\
                 #### Task answer-1.1: the split-off question\n**State:** needs-human\n\nchild\n",
                1,
            ),
        );

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert!(!op.live);
        assert_eq!(op.tickets.len(), 1);
        assert_eq!(op.tickets[0].ticket, "answer-1.1");
        assert_eq!(op.tickets[0].doing, Doing::Waiting);
    }

    /// A live root whose tickets were all filtered out — here, unjudgeable
    /// under a missing machine — still answers for the plan behind it: the
    /// operation knows its root's plans, not only its tickets'
    /// (§FS-005-dispatch.15).
    #[test]
    fn an_operation_with_no_tickets_still_knows_its_plan() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = plan_only_root(root, false);
        write(&root.join(LOCK), "");
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();

        let board = board(&config(), std::slice::from_ref(&group));
        assert_eq!(board.operations.len(), 1);
        let op = &board.operations[0];
        assert!(op.live);
        assert!(op.tickets.is_empty());
        assert_eq!(op.plan(), Some(root.join("widget-42.rhei.md").as_path()));
        drop(holder);
    }

    /// With no runtime bound there are no operations — an operation is a
    /// run — and the sentence is the workable rung's own
    /// (§FS-005-dispatch.15).
    #[test]
    fn no_runner_is_no_rows_in_the_workable_rungs_words() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let group = root_with_plan(root, true);
        write(&root.join(LOCK), "");
        let absent = WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            ..WorkConfig::default()
        };
        let board = board(&absent, std::slice::from_ref(&group));
        assert!(board.operations.is_empty());
        let refusal = board.refusal.expect("the workable rung refuses");
        assert!(
            refusal.starts_with("no-such-runtime-anywhere is not on PATH"),
            "{refusal}"
        );
        // The floor is untouched: the plan is still read on disk
        // (§AR-007-runtime.3).
        let plan = Plan::read(&group.plans[0].path).unwrap().unwrap();
        assert_eq!(plan.tickets().len(), 3);
    }

    #[test]
    fn the_pulse_reprobes_without_reading_a_plan() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join(LOCK), "");
        let holder = fs::File::open(root.join(LOCK)).unwrap();
        holder.lock().unwrap();
        write(
            &root.join(DASHBOARD),
            r#"{ "url": "http://127.0.0.1:39114" }"#,
        );
        let alive = pulse(&config(), root);
        assert!(alive.live);
        assert_eq!(alive.dashboard.as_deref(), Some("http://127.0.0.1:39114"));

        drop(holder);
        assert_released(&config(), root);
        let gone = pulse(&config(), root);
        assert!(!gone.live);
        // The address file may outlive the run; the pulse does not report it.
        assert_eq!(gone.dashboard, None);
        assert_eq!(gone.quiet, None);
    }

    #[test]
    fn the_release_words_are_the_bound_runners() {
        assert_eq!(
            release_command(&WorkConfig::default(), "widget-42.fix-gate-1"),
            format!("{} release widget-42.fix-gate-1", super::super::RUNNER)
        );
        assert_eq!(
            release_command(&config(), "a.b"),
            "sh release a.b".to_string()
        );
    }
}
