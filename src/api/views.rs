//! The view types: what a reading returns, and what a move reports
//! (§AR-009-surfaces.3).
//!
//! Plain data with no IO, so a surface renders a view rather than re-deriving
//! it, and `--json` prints the same answer a screen drew (§REQ-002-parity.3).
//! Each type here has a published schema (§AR-009-surfaces.3); a view added
//! without one fails the build.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// One entry of a subject's menu (§FS-011-command-line.1). What the interface
/// draws as a row and `ephor actions` prints as a line — one shape, so the
/// two cannot disagree about whether something can run.
#[derive(Debug, Clone, Serialize)]
pub struct Offer {
    /// How a command names it: the configured id, or the description where
    /// the entry is anonymous.
    pub id: String,
    pub icon: String,
    pub description: String,
    /// `command`, `agent`, `workflow`, `checkout`, `freehand`, `workflows`.
    pub kind: &'static str,
    /// `ready`, `needs-checkout`, or `blocked`.
    pub gate: &'static str,
    /// Why it cannot run, where it cannot. The ladder's own sentence, never a
    /// second opinion (§FS-006-project-interface.10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// Who the work would go to, on an entry that hands work over
    /// (§FS-005-dispatch.14).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hand: Option<String>,
    /// What it would run, where it runs a command. Printed because a person
    /// deciding whether to run an entry is deciding about this line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// What the ticket would ask for, on an entry that hands work over
    /// (§FS-005-dispatch.7) — the same words the work screen shows under the
    /// row. The `command` of an entry that asks somebody rather than running
    /// something: it is the thing a reader is deciding about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
    /// Where it runs — `workspace`, `root`, or `repo:<name>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// It runs beneath the screen as a job rather than taking the terminal
    /// (§FS-005-dispatch.17).
    pub background: bool,
    /// It asks before running (§FS-006-project-interface.9), which on the
    /// command line is `--yes`.
    pub confirm: bool,
    /// The capability rungs it named (§FS-006-project-interface.10).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// What is already going about this entry's subject, where anything is
    /// (§FS-005-dispatch.21). A program reading the menu learns it here, so it
    /// cannot start what a person reading it would have opened
    /// (§REQ-002-parity.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<Running>,
}

/// What is going about one entry's subject, and the way in
/// (§FS-005-dispatch.21, §FS-011-command-line.8).
///
/// The way in is printed because the way in is the ability, and spawning the
/// reader's own program on it is not (§REQ-002-parity.1).
#[derive(Debug, Clone, Serialize)]
pub struct Running {
    /// `job`, `run`, `queued`, or `window`.
    pub kind: &'static str,
    /// What it is at right now: the job's own last line, the ticket a run
    /// holds and the state it is in, `queued` where the root's run will reach
    /// it.
    pub says: String,
    /// How long it has been going, in seconds, where that is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since_seconds: Option<i64>,
    /// The job it is, on a job or a window — what `ephor job log` takes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
    /// What the job wrote, followed as it writes. Absent on a window, whose
    /// program's output went to a screen the reader was looking at
    /// (§FS-005-dispatch.22).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<PathBuf>,
    /// The execution root the run holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
    /// What the run calls itself, where it named itself (§AR-007-runtime.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// The runner's own attach command, in the runner's own words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    /// The address of the run's control, while it serves one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_url: Option<String>,
    /// The window's handle, brought forward by the bound opener
    /// (§FS-005-dispatch.22).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
}

/// A hand the roster offers, and the efforts it declares (§FS-005-dispatch.14).
#[derive(Debug, Clone, Serialize)]
pub struct HandOffer {
    pub id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub efforts: Vec<String>,
    /// Why it cannot be asked right now. Listed with its reason rather than
    /// hidden (§DA-004-roster-is-asked-not-configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

/// Everything `ephor actions` answers about one subject (§FS-011-command-line.1).
#[derive(Debug, Clone, Serialize)]
pub struct Actions {
    pub project: String,
    /// `item` or `branch`.
    pub subject: &'static str,
    /// The feed id, or the branch name.
    pub id: String,
    pub title: String,
    pub root: PathBuf,
    /// Where an entry runs: the branch workspace where there is one, the root
    /// otherwise (§AR-004-forest.3).
    pub workspace: PathBuf,
    /// `ready`, `missing`, or `unmatched`.
    pub workspace_state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub offers: Vec<Offer>,
    /// Who `--hand` may name here (§FS-005-dispatch.14). Empty where there is
    /// no work to hand over, or nobody to pick.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub roster: Vec<HandOffer>,
}

/// A distance and how fresh the comparison behind it is (§FS-004-quick-actions.6).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Distance {
    pub behind: u64,
    /// The day the local copy of what it was measured against last moved
    /// here. Nothing is invented to fill it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,
}

impl From<crate::forest::Trail> for Distance {
    fn from(trail: crate::forest::Trail) -> Distance {
        Distance {
            behind: trail.behind,
            as_of: trail.seen,
        }
    }
}

/// One branch of a project, and where it stands (§FS-011-command-line.2).
#[derive(Debug, Clone, Serialize)]
pub struct Branch {
    pub project: String,
    pub branch: String,
    /// Whether the registry row names it, or ephor found it on disk
    /// (§FS-008-attribution.1).
    pub declared: bool,
    pub active: bool,
    pub is_release: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// Where its workspace goes. None for a project that keeps one checkout
    /// at its root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    pub checked_out: bool,
    /// How far it trails the project's main branch, summed over the forest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<Distance>,
    /// How far it trails its own published copy — a different fact, never
    /// summed with the one above (§DA-003-upstream-is-the-published-copy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind_upstream: Option<Distance>,
    /// The ref every counted repository names, where they all name one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_branch: Option<String>,
    /// How many of the project's matters are on it.
    pub items: usize,
    /// The most urgent matching matter's link, which is what the row opens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// One row of the operations board (§FS-011-command-line.3): what the runtime
/// is at, and what ephor is running itself — one list, because "what is going
/// on" has one answer.
#[derive(Debug, Clone, Serialize)]
pub struct Operation {
    /// `job` for one of ephor's own (§FS-005-dispatch.17), `root` for an
    /// execution root the runtime holds (§FS-005-dispatch.15).
    pub kind: &'static str,
    pub id: String,
    pub project: String,
    pub says: String,
    /// Running, waiting, claimed, queued, over — what the row's state is
    /// called.
    pub state: String,
    pub live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<PathBuf>,
    /// The log a job wrote, read with `ephor job log`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<PathBuf>,
    /// Published only while a live run serves one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dashboard: Option<String>,
    /// The tickets this root holds, in the order the board puts them: what
    /// asks something of the reader first (§FS-005-dispatch.15).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<OperationTicket>,
}

/// One ticket under an execution root (§FS-005-dispatch.15).
#[derive(Debug, Clone, Serialize)]
pub struct OperationTicket {
    pub id: String,
    pub says: String,
    pub state: String,
    /// Running, queued, claimed, waiting on a person.
    pub doing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
}

/// The whole board, with the reason there is nothing on it where a runtime
/// cannot run any (§FS-005-dispatch.15).
#[derive(Debug, Clone, Serialize)]
pub struct Operations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    pub operations: Vec<Operation>,
}

/// One message of a matter's conversation (§FS-011-command-line.4).
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    /// Which conversation of the matter it belongs to (§FS-007-matters.4).
    pub thread: usize,
    pub author: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<DateTime<Utc>>,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<Reaction>,
    /// Whether a reaction can be posted on it — the source said how
    /// (§FS-004-quick-actions).
    pub can_react: bool,
    /// The task the source reported on it, where it reported one
    /// (§FS-004-quick-actions.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<MessageTask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reaction {
    pub emoji: String,
    pub users: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageTask {
    pub text: String,
    pub resolved: bool,
    pub source: String,
}

/// The reply a run drafted, waiting under the conversation it answers
/// (§FS-005-dispatch.13). It is a file until a person sends it, which is why
/// the path is here wherever the channel cannot carry it.
#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    pub text: String,
    pub path: PathBuf,
    pub thread: usize,
    /// Whether the channel declared that it can carry a reply
    /// (§FS-007-matters.4).
    pub sendable: bool,
}

/// A matter's recorded conversation (§FS-011-command-line.4).
#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub item: String,
    pub project: String,
    pub source: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<Draft>,
}

/// What is being done about one matter, and what could be
/// (§FS-011-command-line.5).
#[derive(Debug, Clone, Serialize)]
pub struct Work {
    pub item: String,
    pub project: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Why nothing here can be run, where a runtime cannot run it. Everything
    /// else on this reading goes on working with nothing bound — the plan is
    /// written, read and reopened either way (§FS-005-dispatch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// Why there is nothing to offer *at all* — the menu the offers come from
    /// could not be assembled, because the project is not placed
    /// (§AR-004-forest.3). A different fact from `refusal`, which is about
    /// running a plan: a matter can have offers nothing can run, and a matter
    /// nothing can even be asked about. Stated rather than left as an empty
    /// list, which reads exactly like an oversight (§REQ-001-boundary.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
    /// What could be handed over about it: the recipes that match and the
    /// workflows that could be laid beside it.
    pub offers: Vec<Offer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<WorkStatus>,
    /// What ephor has run about it itself (§FS-005-dispatch.17).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<Operation>,
}

/// The tickets that exist about a matter, and where its plan is
/// (§FS-005-dispatch.4).
#[derive(Debug, Clone, Serialize)]
pub struct WorkStatus {
    pub plan: PathBuf,
    pub plan_id: String,
    pub root: PathBuf,
    pub checkout: PathBuf,
    /// The item moved since the work was asked for (§FS-005-dispatch.5).
    pub stale: bool,
    /// The plan the ledger points at is gone.
    pub missing: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    pub tickets: Vec<Ticket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ticket {
    pub id: String,
    pub recipe: String,
    /// The state the plan says it is in. None where the plan does not say,
    /// which is not the same fact as a state called "unknown".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub finished: bool,
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
}

/// What one move changed (§REQ-002-parity.3): the same outcome the prose
/// describes, in the shape a program can act on.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub ok: bool,
    /// One line saying what happened — the same sentence the interface puts
    /// in its status line.
    pub says: String,
    /// What ran, in order, where the move ran steps.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
    /// The job it was started as, where it went beneath the screen
    /// (§FS-005-dispatch.17).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
    /// What it wrote, where it wrote a plan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<PathBuf>,
    /// The tickets it opened, where it opened any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<String>,
}

impl Outcome {
    pub fn ok(says: impl Into<String>) -> Outcome {
        Outcome {
            ok: true,
            says: says.into(),
            steps: Vec::new(),
            job: None,
            plan: None,
            tickets: Vec::new(),
        }
    }

    pub fn refused(says: impl Into<String>) -> Outcome {
        Outcome {
            ok: false,
            ..Outcome::ok(says)
        }
    }
}

/// One step of a move that ran several (§AR-002-summons.1).
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub icon: String,
    pub description: String,
    pub command: String,
    pub cwd: PathBuf,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
}
