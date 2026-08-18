use clap::{Args, Parser, Subcommand};

/// Manage project workspaces, root AGENTS.md files, and per-project status feeds.
#[derive(Parser, Debug)]
#[command(name = "ephor", version, about)]
pub struct Cli {
    /// Path to the project registry JSON file.
    #[arg(long, global = true)]
    pub registry: Option<String>,

    /// Path to the project registry schema file.
    #[arg(long, global = true)]
    pub schema: Option<String>,

    /// Project id or derived workspace id to operate on. May be passed multiple times.
    #[arg(long, global = true)]
    pub workspace: Vec<String>,

    /// Operate on every branch entry instead of only active ones.
    #[arg(long, global = true)]
    pub all: bool,

    /// Restrict operations to projects containing this tag.
    #[arg(long, global = true)]
    pub tag: Vec<String>,

    /// Restrict operations to projects in this organization.
    #[arg(long = "org", visible_alias = "organization", global = true)]
    pub organization: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List registered projects.
    List,
    /// Validate the project registry, or a project's own manifest.
    Validate(ValidateArgs),
    /// Run a project's own check verbs, from its checkout alone.
    Check(CheckArgs),
    /// Print one of the published schemas (manifest, answer, registry, forge).
    Schema(SchemaArgs),
    /// Render root AGENTS.md files.
    EnsureAgents(EnsureAgentsArgs),
    /// Update selected managed branches and regenerate their AGENTS.md files.
    Update(UpdateArgs),
    /// Show the cached information stream for a project (or a summary of all).
    Status(StatusArgs),
    /// Flat aggregate feed of items across projects, newest first.
    Feed(FeedArgs),
    /// Fetch provider data into the cache (what the systemd timer runs).
    Refresh(RefreshArgs),
    /// Mark feed items as read.
    MarkRead(MarkReadArgs),
    /// Show what failed under a pull request's red gate.
    Failures(FailuresArgs),
    /// Replay a checkout's branch onto its main branch.
    Rebase(RebaseArgs),
    /// Make the branch workspace that is not checked out yet.
    Checkout(CheckoutArgs),
    /// Hand items to the agent runtime, and see what came of it.
    Work(WorkArgs),
    /// What ephor is running beneath the screen, and what it wrote.
    Job(JobArgs),
    /// What a project can do, rung by rung, and why a rung is missing.
    #[command(visible_alias = "caps")]
    Capabilities(CapabilitiesArgs),
    /// Is this still working? The whole site, then ephor itself.
    Doctor(DoctorArgs),
    /// Interactive inbox: navigate the feed, open items, mark them done.
    #[command(alias = "inbox")]
    Tui,
}

/// `ephor job` (§FS-005-dispatch.17): a move that needs nobody watching runs
/// beneath the screen, and this is the same job from the command line — what is
/// going, what it wrote, and the supervisor the interface itself starts.
#[derive(Args, Debug)]
pub struct JobArgs {
    #[command(subcommand)]
    pub command: Option<JobCommand>,
}

#[derive(Subcommand, Debug)]
pub enum JobCommand {
    /// What is running, and what recently ran.
    List(JobListArgs),
    /// Everything one job wrote, in order.
    Log(JobLogArgs),
    /// Run a job that has already been written down. This is the supervisor
    /// the interface starts (§AR-002-summons.5); its own output is the log, so
    /// running it by hand prints what the job would have written.
    #[command(hide = true)]
    Run(JobRunArgs),
}

#[derive(Args, Debug, Default)]
pub struct JobListArgs {
    /// Only what is running now.
    #[arg(long)]
    pub live: bool,

    /// Emit raw JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct JobLogArgs {
    /// The job, as `ephor job list` names it.
    pub id: String,
}

#[derive(Args, Debug)]
pub struct JobRunArgs {
    /// The job directory, holding the record written before it started.
    pub dir: std::path::PathBuf,
}

/// `ephor capabilities` (§FS-010-doctor.2): the ladder of
/// §FS-006-project-interface.10 for one project or all of them, so that "why
/// is this action not offered here" has a cheap answer.
#[derive(Args, Debug)]
pub struct CapabilitiesArgs {
    /// The project to read. With none, every configured project.
    pub project: Option<String>,

    /// Print the ladder as JSON.
    #[arg(long)]
    pub json: bool,
}

/// `ephor doctor` (§FS-010-doctor): the site pass asks the world, the self
/// pass asks the binary, and the exit code is the answer a timer reads —
/// `0` well, `4` degraded, `3` nothing reachable, `1` ephor itself is wrong.
#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// Look at one project rather than every configured one.
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,

    /// Skip the self pass: ask the world and nothing else.
    #[arg(long)]
    pub skip_self: bool,

    /// Run only the self pass. Reaches no forge and reads nothing of yours,
    /// so it is the one half that works on a machine with no site.
    #[arg(long, conflicts_with_all = ["skip_self", "project"])]
    pub self_only: bool,

    /// Print the diagnosis as JSON.
    #[arg(long)]
    pub json: bool,
}

/// `ephor work` (§FS-005-dispatch). With no subcommand: what has been
/// dispatched and what it reached.
#[derive(Args, Debug)]
pub struct WorkArgs {
    #[command(subcommand)]
    pub command: Option<WorkCommand>,
}

#[derive(Subcommand, Debug)]
pub enum WorkCommand {
    /// What has been dispatched, and what the runtime has made of it.
    List(WorkListArgs),
    /// Open tickets for items that match a recipe and have no work yet.
    Dispatch(WorkDispatchArgs),
    /// Ask one item for something no recipe covers, in your own words.
    Ask(WorkAskArgs),
    /// Reopen work whose item has moved since it was dispatched.
    Sync(WorkSyncArgs),
    /// Take a ticket back: the runtime moves it into its cancelled state, with
    /// your reason as its result. The plan keeps it.
    Cancel(WorkCancelArgs),
    /// Run the runtime over every work root that still has an open ticket.
    Run(WorkRunArgs),
    /// Drop ledger entries; the plans they point at stay on disk.
    Forget(WorkForgetArgs),
    /// Print the state machine ephor's tickets run under, for editing or for
    /// installing into a runtime project it did not create.
    States,
}

#[derive(Args, Debug, Default)]
pub struct WorkListArgs {
    /// Restrict to one project. May be passed multiple times.
    #[arg(long)]
    pub project: Vec<String>,

    /// Only work that is still open or has gone stale.
    #[arg(long)]
    pub open: bool,

    /// Emit raw JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct WorkDispatchArgs {
    /// Restrict to one project. May be passed multiple times.
    #[arg(long)]
    pub project: Vec<String>,

    /// Dispatch one item by its feed id.
    #[arg(long)]
    pub item: Option<String>,

    /// Use this recipe instead of the first one that matches.
    #[arg(long)]
    pub recipe: Option<String>,

    /// Restrict to one item kind (pr, ci, issue, message, status).
    #[arg(long)]
    pub kind: Option<String>,

    /// Dispatch onto items that already have work, adding a ticket.
    #[arg(long)]
    pub again: bool,

    /// Who does it, for this dispatch alone: a hand id from the roster,
    /// optionally at an effort (`<hand>[:<effort>]`). Displaces every table
    /// for exactly this dispatch and is remembered by nothing.
    #[arg(long)]
    pub hand: Option<String>,

    /// Skip items with no activity in this many days.
    #[arg(long, value_name = "DAYS")]
    pub updated_within: Option<i64>,

    /// Report what would be opened without writing anything.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct WorkAskArgs {
    /// The item to ask about, by its feed id.
    #[arg(long)]
    pub item: String,

    /// What to ask for. Omitted, it is read from stdin — which is how a
    /// longer ask composed in an editor gets in.
    pub words: Vec<String>,

    /// Start the ticket in this state instead of the machine's working one.
    #[arg(long)]
    pub state: Option<String>,

    /// Report what would be written without writing it.
    #[arg(long)]
    pub dry_run: bool,
}

/// `ephor work cancel` (§FS-005-dispatch.16): one item's tickets, taken back
/// through the runtime's own transition. A ticket a live run holds, one
/// already over, and a machine with no cancelled state are refused by name;
/// tickets ordered after a cancelled one are named as left waiting.
#[derive(Args, Debug)]
pub struct WorkCancelArgs {
    /// The item whose tickets these are, by its feed id.
    #[arg(long)]
    pub item: String,

    /// The ticket(s) to take back, by id inside the item's plan (`fix-gate-2`).
    #[arg(value_name = "TICKET", required = true)]
    pub tickets: Vec<String>,

    /// Why — one line, recorded as the ticket's result. Left out, the result
    /// says the reason was left unsaid.
    #[arg(long, value_name = "WORDS")]
    pub why: Option<String>,

    /// Report what would be cancelled without asking the runtime to move
    /// anything.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct WorkSyncArgs {
    /// Restrict to one project. May be passed multiple times.
    #[arg(long)]
    pub project: Vec<String>,

    /// Report what would be reopened without writing anything.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct WorkRunArgs {
    /// Restrict to one project. May be passed multiple times.
    #[arg(long)]
    pub project: Vec<String>,

    /// Run only the work of one item.
    #[arg(long)]
    pub item: Option<String>,

    /// Arguments passed through to the runtime, after `--`.
    #[arg(last = true)]
    pub runner_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct WorkForgetArgs {
    /// Forget one item's work.
    #[arg(long)]
    pub item: Option<String>,

    /// Forget every entry whose tickets are all finished.
    #[arg(long)]
    pub done: bool,

    /// Forget every entry whose plan is gone.
    #[arg(long)]
    pub missing: bool,
}

#[derive(Args, Debug)]
pub struct EnsureAgentsArgs {
    /// Project type id for an ad hoc workspace.
    #[arg(long = "type")]
    pub project_type: Option<String>,

    /// Workspace root for an ad hoc workspace.
    #[arg(long)]
    pub root: Option<String>,

    /// Display name for an ad hoc workspace.
    #[arg(long, default_value = "workspace")]
    pub display_name: String,

    /// Template variable for an ad hoc workspace. May be passed multiple times.
    #[arg(long = "var", value_name = "KEY=VALUE")]
    pub vars: Vec<String>,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Pass debug mode through to hook implementations.
    #[arg(long)]
    pub debug: bool,

    /// Do not regenerate root AGENTS.md files.
    #[arg(long)]
    pub skip_agents: bool,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Project id to show (omit for a summary of all configured projects).
    pub project: Option<String>,

    /// Force a fetch regardless of cache age.
    #[arg(long, conflicts_with = "cached")]
    pub refresh: bool,

    /// Never fetch; use only cached data.
    #[arg(long)]
    pub cached: bool,

    /// Maximum acceptable cache age in seconds (overrides the configured TTL).
    #[arg(long, value_name = "SECONDS")]
    pub max_age: Option<u64>,

    /// Emit raw JSON instead of a table.
    #[arg(long)]
    pub json: bool,

    /// Exit with code 4 when unread needs-response items exist.
    #[arg(long)]
    pub check: bool,
}

#[derive(Args, Debug)]
pub struct FeedArgs {
    /// Restrict to one project. May be passed multiple times.
    #[arg(long)]
    pub project: Vec<String>,

    /// Only show unread items.
    #[arg(long)]
    pub unread: bool,

    /// Filter by item kind (pr, ci, issue, message, status).
    #[arg(long)]
    pub kind: Option<String>,

    /// Show what nothing claimed instead: the conversations attribution could
    /// not place, and the ones two projects claimed equally
    /// (FS-008-attribution.4).
    #[arg(long)]
    pub unattributed: bool,

    /// Emit raw JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Default)]
pub struct ValidateArgs {
    /// Validate a project manifest (`ephor.json`) instead of the registry.
    /// Pass the file, or the forest root it sits at.
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<String>,

    /// Hold the registry to the published schema and stop, without looking
    /// for the checkouts its rows describe. What a repository carrying a
    /// committed registry can check in CI (FS-009-shipped-actions.1).
    #[arg(long)]
    pub schema_only: bool,
}

/// `ephor check`: the project's own gate, derived from the project's own
/// declaration (FS-006-project-interface.5).
#[derive(Args, Debug)]
pub struct CheckArgs {
    /// The forest root to check. Defaults to the working directory.
    #[arg(long, value_name = "PATH")]
    pub root: Option<String>,

    /// Which verbs to run — `check`, `style`, `smoke` — repeatable. With none
    /// named, the aggregate runs where the project declares one, and whatever
    /// else it declares where it does not.
    #[arg(long = "verb", value_name = "VERB")]
    pub verbs: Vec<String>,

    /// Run one feature's smoke rather than the whole of it
    /// (FS-006-project-interface.5).
    #[arg(long, value_name = "ID")]
    pub feature: Option<String>,

    /// Print the features this project's smoke enumerates and stop — one per
    /// line, or `--json` for a workflow matrix.
    #[arg(long)]
    pub list_features: bool,

    /// Print the feature list as a JSON array, which is what a workflow
    /// matrix reads.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct SchemaArgs {
    /// Which schema: manifest, answer, registry, or forge
    /// (FS-006-project-interface.11).
    pub name: String,
}

#[derive(Args, Debug)]
pub struct RefreshArgs {
    /// Projects to refresh (omit for all configured projects).
    pub projects: Vec<String>,

    /// Print nothing on success.
    #[arg(long)]
    pub quiet: bool,
}

/// What the failing-CI quick action passes back to ephor. The item is named
/// by the four things its `EPHOR_*` environment already carries, so the action
/// is an ordinary shell command like every other one.
#[derive(Args, Debug)]
pub struct FailuresArgs {
    /// Project the pull request belongs to.
    #[arg(long)]
    pub project: String,

    /// Source that reported it (the provider name).
    #[arg(long)]
    pub source: String,

    /// Repository the pull request lives in.
    #[arg(long)]
    pub repo: String,

    /// Pull request number.
    #[arg(long)]
    pub number: String,
}

/// `ephor rebase` (§FS-004-quick-actions.6): fetch, and replay every
/// repository under a checkout onto its main branch — or, with `--upstream`,
/// onto the branch's own published copy (§FS-004-quick-actions.8).
///
/// It exits `0` where every repository is on the base — replayed, already
/// there, or with nothing published to replay onto — `3` where one stopped in
/// a conflict, and `1` where it could not run at all. A state machine reads
/// those to decide who works next (§FS-005-dispatch.12).
///
/// Every argument can arrive as the environment variable named beside it
/// instead, which is how a program state passes it `{meta.*}`.
#[derive(Args, Debug)]
pub struct RebaseArgs {
    /// The checkout to rebase (`CHECKOUT`). Defaults to the working
    /// directory, which is where an action already runs.
    #[arg(long)]
    pub checkout: Option<String>,

    /// Project the checkout belongs to (`PROJECT`); its registry entry says
    /// which branch to replay onto and which repositories track it.
    #[arg(long)]
    pub project: Option<String>,

    /// Replay onto this branch instead of the project's main branch (`ONTO`).
    #[arg(long)]
    pub onto: Option<String>,

    /// Replay each repository onto its own branch's published copy rather
    /// than onto a base (`UPSTREAM`, set to any non-empty value)
    /// (§FS-004-quick-actions.8). A different ref per repository, so it takes
    /// no branch name — and none to give, which is why it excludes `--onto`
    /// rather than being one of its values. clap sees only the flags, so the
    /// same refusal is repeated where the two arrive as `UPSTREAM` and
    /// `ONTO`: asked for together in any spelling, the rebase refuses rather
    /// than silently picking one.
    #[arg(long, conflicts_with = "onto")]
    pub upstream: bool,

    /// The feed item this is about (`ITEM`), so a conflict can be handed over
    /// as work.
    #[arg(long)]
    pub item: Option<String>,

    /// Where a conflict stops the rebase, open a ticket about it
    /// (§FS-005-dispatch.12). Needs `--item`.
    #[arg(long)]
    pub dispatch: bool,

    /// Who resolves a conflict this rebase hands over (`HAND`), for this
    /// dispatch alone: a hand id from the roster, optionally at an effort
    /// (`<hand>[:<effort>]`). Rides `--dispatch` (§FS-005-dispatch.14).
    #[arg(long)]
    pub hand: Option<String>,

    /// Also write the outcome as markdown here (`REPORT`), for a state to
    /// hand on to the one that resolves it.
    #[arg(long)]
    pub report: Option<String>,
}

/// `ephor checkout` (§FS-004-quick-actions.7): make the branch workspace that
/// is not there, one working tree per repository of the project.
///
/// It exits `0` where every repository has a working tree, and `1` where any
/// of them was refused — a branch another working tree is holding is the
/// common one.
///
/// Every argument can arrive as the environment variable named beside it
/// instead, which is how a program state passes it `{meta.*}`.
#[derive(Args, Debug)]
pub struct CheckoutArgs {
    /// Project the branch belongs to (`PROJECT`); its registry entry says
    /// where the workspace goes, which repositories it holds, and what a new
    /// branch is grown from.
    #[arg(long)]
    pub project: Option<String>,

    /// The branch to check out (`BRANCH`).
    #[arg(long)]
    pub branch: Option<String>,

    /// The feed item this is about (`ITEM`); its project and branch are used
    /// where they are not given.
    #[arg(long)]
    pub item: Option<String>,

    /// Grow a branch the repository does not have from this instead of the
    /// project's main branch (`FROM`).
    #[arg(long)]
    pub from: Option<String>,

    /// Also write the outcome as markdown here (`REPORT`).
    #[arg(long)]
    pub report: Option<String>,
}

#[derive(Args, Debug)]
pub struct MarkReadArgs {
    /// Project whose items should be marked read (or pass the global --all).
    pub project: Option<String>,

    /// Mark a single item id as read.
    #[arg(long)]
    pub id: Option<String>,

    /// Restrict to one item kind (pr, ci, issue, message, status).
    #[arg(long)]
    pub kind: Option<String>,
}
