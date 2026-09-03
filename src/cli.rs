use clap::{Args, Parser, Subcommand};

/// Manage project workspaces, root AGENTS.md files, and per-project status feeds.
///
/// The three scope selectors below are declared once and carried into every
/// subcommand's help, so a verb that will not read one refuses it by name
/// rather than parsing it and changing nothing (§FS-011-command-line.9).
/// `--all` is not among them: it is declared by the verbs that read it, so
/// nothing advertises it where it would mean nothing.
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
    List(ListArgs),
    /// Validate the project registry, or a project's own manifest.
    ///
    /// Selects managed workspaces with the scope selectors `--workspace`,
    /// `--tag` and `--org`; its own `--all` walks every branch entry rather
    /// than only the active ones (§FS-011-command-line.9).
    Validate(ValidateArgs),
    /// Run a project's own check verbs, from its checkout alone.
    Check(CheckArgs),
    /// Print one of the published schemas (manifest, answer, registry, forge, views).
    Schema(SchemaArgs),
    /// Render root AGENTS.md files.
    ///
    /// Selects managed workspaces with the scope selectors `--workspace`,
    /// `--tag` and `--org`; its own `--all` walks every branch entry rather
    /// than only the active ones (§FS-011-command-line.9).
    EnsureAgents(EnsureAgentsArgs),
    /// Update selected managed branches and regenerate their AGENTS.md files.
    ///
    /// Selects managed workspaces with the scope selectors `--workspace`,
    /// `--tag` and `--org`; its own `--all` walks every branch entry rather
    /// than only the active ones (§FS-011-command-line.9).
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
    /// Run a pull request's gate again — everything, or only what is not green.
    Restart(RestartArgs),
    /// Replay a checkout's branch onto its main branch.
    Rebase(RebaseArgs),
    /// Make the branch workspace that is not checked out yet.
    Checkout(CheckoutArgs),
    /// Hand items to the agent runtime, and see what came of it.
    Work(WorkArgs),
    /// What ephor is running beneath the screen, and what it wrote.
    Job(JobArgs),
    /// What may be done about a matter or a branch, and run one of it.
    #[command(visible_alias = "act")]
    Actions(ActionsArgs),
    /// A project's branches, and where each one stands.
    Branches(BranchesArgs),
    /// Every operation beneath the reading, in one place.
    #[command(visible_alias = "ops")]
    Operations(OperationsArgs),
    /// What this machine is spending on agents, over a window.
    ///
    /// Reads every project the site is configured with, to say which one a
    /// session was working in, so the scope selectors narrow nothing here
    /// (§FS-011-command-line.9).
    Burn(BurnArgs),
    /// A matter's recorded conversation, and the reply a run drafted.
    Thread(ThreadArgs),
    /// Post a reaction on one message of a matter.
    React(ReactArgs),
    /// Tick a task the source reported on a message.
    Tick(TickArgs),
    /// Send a reply: the one a run drafted, or one given in words.
    Reply(ReplyArgs),
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

    /// Keep up with a job that is still writing, as the interface's pager
    /// does. Ends when the job does.
    #[arg(long, short = 'f')]
    pub follow: bool,

    /// Emit the job and its whole log as JSON.
    #[arg(long)]
    pub json: bool,
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
    /// What could be handed over about one matter, and what already has been.
    Offers(WorkOffersArgs),
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
    /// What workflows the runtime offers, and what each one takes.
    Workflows(WorkWorkflowsArgs),
    /// Lay one of the runtime's workflows down about an item: a plan of its
    /// own, beside the item's, ready for the board to run.
    Lay(WorkLayArgs),
    /// Drop ledger entries; the plans they point at stay on disk.
    Forget(WorkForgetArgs),
    /// Print the state machine ephor's tickets run under, for editing or for
    /// installing into a runtime project it did not create.
    States(WorkStatesArgs),
}

#[derive(Args, Debug, Default)]
pub struct WorkStatesArgs {
    /// Print the machine and where it came from as JSON.
    #[arg(long)]
    pub json: bool,
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

/// `ephor work offers` (§FS-011-command-line.5): one matter's work screen —
/// the recipes that match it, the workflows that could be laid beside it, the
/// tickets that exist, and what ephor has run about it itself.
#[derive(Args, Debug)]
pub struct WorkOffersArgs {
    /// The matter, by its feed id.
    #[arg(long)]
    pub item: String,

    /// Show finished tickets too. They are folded away by default, exactly as
    /// the work screen folds them (§FS-005-dispatch.18).
    #[arg(long)]
    pub all: bool,

    /// Emit raw JSON instead of a listing.
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

    /// Restrict to one item kind (pr, ci, issue, task, message, status).
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

    /// Order this sweep by this ranking file instead of the configured one,
    /// for this dispatch alone: one item id per line, most important first
    /// (§FS-005-dispatch.26).
    #[arg(long, value_name = "PATH")]
    pub ranking: Option<String>,

    /// Dispatch at most this many items (opened, or would-open under
    /// `--dry-run`). Items skipped for another reason do not count against it
    /// (§FS-005-dispatch.26).
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,

    /// Report what would be opened without writing anything.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Default)]
pub struct WorkWorkflowsArgs {
    /// Ask about one project's checkout — a project keeps workflows of its
    /// own beside it. Omitted, the first configured project answers.
    #[arg(long)]
    pub project: Option<String>,

    /// Show one workflow's inputs in full.
    pub workflow: Option<String>,

    /// Emit raw JSON instead of a listing.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct WorkLayArgs {
    /// The item it is about, by its feed id.
    #[arg(long)]
    pub item: String,

    /// The entry to lay down — a menu entry that names a workflow, or a
    /// workflow's own id where no entry names it.
    pub entry: String,

    /// Answer one of the workflow's inputs, for this instantiation alone:
    /// `--set <input>=<value>`. Repeatable. An input naming who does the work
    /// is answered with a hand's id like everywhere else.
    #[arg(long = "set", value_name = "INPUT=VALUE")]
    pub set: Vec<String>,

    /// Load workflow input values from a YAML or JSON mapping. Repeatable;
    /// later files override earlier files, while `--set` wins over all files.
    #[arg(long, value_name = "FILE")]
    pub values: Vec<String>,

    /// Who does it, for this instantiation alone: a hand id from the roster,
    /// optionally at an effort (`<hand>[:<effort>]`).
    #[arg(long)]
    pub hand: Option<String>,

    /// Report what would be written, and what would answer every input,
    /// without writing the plan.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
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

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
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

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct WorkSyncArgs {
    /// Restrict to one project. May be passed multiple times.
    #[arg(long)]
    pub project: Vec<String>,

    /// Report what would be reopened without writing anything.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct WorkRunArgs {
    /// Restrict to one project. May be passed multiple times.
    #[arg(long)]
    pub project: Vec<String>,

    /// Run only the work of one item.
    #[arg(long)]
    pub item: Option<String>,

    /// Start a run on every work root that holds work asking to run itself
    /// and has none (§FS-005-dispatch.24). The sweep behind autorun: it reads
    /// the world rather than the ledger, starts nothing on a root a run
    /// already holds, and is safe to invoke as often as anything cares to.
    #[arg(long)]
    pub due: bool,

    /// Override the site's aggregate autorun ceiling for this due sweep.
    /// Project ceilings still apply inside it. Zero starts no new runs
    /// (§FS-005-dispatch.24).
    #[arg(long, value_name = "N", requires = "due")]
    pub max_concurrent: Option<usize>,

    /// Keep the terminal and watch the run, as this command always did
    /// (§FS-011-command-line.8). Without it the run starts detached and this
    /// prints the id it was given — which is also what a runner that cannot
    /// detach does unasked, saying so (§FS-005-dispatch.20).
    #[arg(long)]
    pub watch: bool,

    /// Start a run in a working tree a live run already holds. Without it such
    /// a plan is refused by name: one live run per checkout, because a second
    /// run there is a second agent editing the same files. This lifts that
    /// refusal and nothing else, for the run you asked for by name: `--due`
    /// is a sweep of what should be running anyway, and is never forced
    /// (§FS-005-dispatch.24).
    #[arg(long)]
    pub force: bool,

    /// Arguments passed through to the runtime, after `--`.
    #[arg(last = true)]
    pub runner_args: Vec<String>,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
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

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct EnsureAgentsArgs {
    /// Operate on every branch entry instead of only the active ones. Declared
    /// here rather than globally: it is a question about branch entries, and
    /// only the three verbs that walk managed workspaces ask it
    /// (§FS-011-command-line.9).
    #[arg(long)]
    pub all: bool,

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

    /// Print what was written as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Operate on every branch entry instead of only the active ones. Declared
    /// here rather than globally: it is a question about branch entries, and
    /// only the three verbs that walk managed workspaces ask it
    /// (§FS-011-command-line.9).
    #[arg(long)]
    pub all: bool,

    /// Pass debug mode through to hook implementations.
    #[arg(long)]
    pub debug: bool,

    /// Do not regenerate root AGENTS.md files.
    #[arg(long)]
    pub skip_agents: bool,

    /// Print what was updated as JSON.
    #[arg(long)]
    pub json: bool,
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

    /// Filter by item kind (pr, ci, issue, task, message, status).
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
    /// Operate on every branch entry instead of only the active ones. Declared
    /// here rather than globally: it is a question about branch entries, and
    /// only the three verbs that walk managed workspaces ask it
    /// (§FS-011-command-line.9).
    #[arg(long)]
    pub all: bool,

    /// Validate a project manifest (`ephor.json`) instead of the registry.
    /// Pass the file, or the forest root it sits at.
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<String>,

    /// Hold the registry to the published schema and stop, without looking
    /// for the checkouts its rows describe. What a repository carrying a
    /// committed registry can check in CI (FS-009-shipped-actions.1).
    #[arg(long)]
    pub schema_only: bool,

    /// Emit what was validated as JSON.
    #[arg(long)]
    pub json: bool,
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
    /// Which schema: manifest, answer, registry, forge, or views —
    /// what every command prints under `--json` (§FS-006-project-interface.11,
    /// §REQ-002-parity.4).
    pub name: String,
}

#[derive(Args, Debug)]
pub struct RefreshArgs {
    /// Projects to refresh (omit for all configured projects).
    pub projects: Vec<String>,

    /// Print nothing on success.
    #[arg(long)]
    pub quiet: bool,

    /// Emit what each project fetched as JSON.
    #[arg(long)]
    pub json: bool,
}

/// What the failing-CI quick action passes back to ephor. The item is named
/// by the four things its `EPHOR_*` environment already carries, so the action
/// is an ordinary shell command like every other one.
#[derive(Args, Debug)]
pub struct FailuresArgs {
    /// The matter, by its feed id — which is how a reader who just read the
    /// feed names it, rather than by taking it apart into four coordinates
    /// (§FS-011-command-line.6).
    #[arg(long, conflicts_with_all = ["project", "source", "repo", "number"])]
    pub item: Option<String>,

    /// Project the pull request belongs to.
    #[arg(long, requires_all = ["source", "repo", "number"])]
    pub project: Option<String>,

    /// Source that reported it (the provider name).
    #[arg(long)]
    pub source: Option<String>,

    /// Repository the pull request lives in.
    #[arg(long)]
    pub repo: Option<String>,

    /// Pull request number.
    #[arg(long)]
    pub number: Option<String>,

    /// Emit the gate and what failed as JSON.
    #[arg(long)]
    pub json: bool,
}

/// `ephor restart` (§FS-004-quick-actions.9): ask a pull request's gate to run
/// again. The scope is stated rather than inferred — the cheap re-run and the
/// expensive one are different questions, and one of them spends an hour of a
/// shared machine pool.
#[derive(Args, Debug)]
pub struct RestartArgs {
    /// The matter, by its feed id — which is how a reader who just read the
    /// feed names it, rather than by taking it apart into four coordinates
    /// (§FS-011-command-line.6).
    #[arg(long, conflicts_with_all = ["project", "source", "repo", "number"])]
    pub item: Option<String>,

    /// Project the pull request belongs to.
    #[arg(long, requires_all = ["source", "repo", "number"])]
    pub project: Option<String>,

    /// Source that reported it (the provider name).
    #[arg(long)]
    pub source: Option<String>,

    /// Repository the pull request lives in.
    #[arg(long)]
    pub repo: Option<String>,

    /// Pull request number.
    #[arg(long)]
    pub number: Option<String>,

    /// How much to run again: `failed` (the default — the failing gate and
    /// everything downstream of it) or `all`.
    #[arg(long, default_value = "failed")]
    pub scope: String,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
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

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
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

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct MarkReadArgs {
    /// Project whose items should be marked read.
    pub project: Option<String>,

    /// Every watched project's items, not one project's. `mark-read`'s own
    /// flag, declared here for the reason the managed-workspace verbs declare
    /// theirs: a flag belongs to the verbs that read it
    /// (§FS-011-command-line.9). Narrowed by `--org`, `--tag` and
    /// `--workspace` like every other project selection.
    #[arg(long)]
    pub all: bool,

    /// Mark a single item id as read.
    #[arg(long)]
    pub id: Option<String>,

    /// Restrict to one item kind (pr, ci, issue, task, message, status).
    #[arg(long)]
    pub kind: Option<String>,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

/// `ephor list`: the registry's rows, filtered by the global selectors.
#[derive(Args, Debug, Default)]
pub struct ListArgs {
    /// Emit the rows as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Which subject a command is about: a matter, or a branch that has none
/// behind it (§FS-004-quick-actions.6). The same three flags everywhere one
/// of the two is named, so a reader who learned them once has learned them.
#[derive(Args, Debug, Default, Clone)]
pub struct SubjectArgs {
    /// The matter, by its feed id — as `ephor feed` prints it.
    #[arg(long)]
    pub item: Option<String>,

    /// The project the branch belongs to. Needed with `--branch`.
    #[arg(long)]
    pub project: Option<String>,

    /// The branch, where the subject is a branch rather than a matter.
    #[arg(long, requires = "project")]
    pub branch: Option<String>,
}

/// `ephor actions` (§FS-011-command-line.1): the menu a matter or a branch
/// carries, and one entry of it run.
#[derive(Args, Debug)]
pub struct ActionsArgs {
    #[command(subcommand)]
    pub command: Option<ActionsCommand>,

    #[command(flatten)]
    pub subject: SubjectArgs,

    /// Emit the menu as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum ActionsCommand {
    /// What may be done here, in provenance order.
    List(ActionsListArgs),
    /// Run one entry, by the id the listing gives it.
    Run(ActionsRunArgs),
    /// Go to what is already going about one entry: follow its job's log,
    /// attach to its run, or bring its window forward (§FS-011-command-line.8).
    Open(ActionsOpenArgs),
}

/// `ephor actions open` (§FS-011-command-line.8): the key on a running row as
/// a command. It starts nothing — where the entry has nothing going it refuses
/// by name (§FS-005-dispatch.21).
#[derive(Args, Debug)]
pub struct ActionsOpenArgs {
    /// The entry, by the id `ephor actions` gives it.
    pub entry: String,

    #[command(flatten)]
    pub subject: SubjectArgs,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Default)]
pub struct ActionsListArgs {
    #[command(flatten)]
    pub subject: SubjectArgs,

    /// Emit raw JSON instead of a listing.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ActionsRunArgs {
    /// The entry, by the id `ephor actions` gives it. Left out with
    /// `--command`, which is the freehand row and names nothing
    /// (§FS-005-dispatch.10).
    pub entry: Option<String>,

    #[command(flatten)]
    pub subject: SubjectArgs,

    /// Run this instead of a configured entry: whatever you want to run once,
    /// in the resolved place, with the matter's context already exported
    /// (§FS-005-dispatch.10).
    #[arg(long, conflicts_with = "entry")]
    pub command: Option<String>,

    /// Who does it, for this dispatch alone: a hand id from the roster,
    /// optionally at an effort (`<hand>[:<effort>]`).
    #[arg(long)]
    pub hand: Option<String>,

    /// Answer one of a workflow entry's inputs, for this instantiation alone
    /// (`--set <input>=<value>`). Repeatable.
    #[arg(long = "set", value_name = "INPUT=VALUE")]
    pub set: Vec<String>,

    /// Agree to an entry that asked to be confirmed. A second keystroke has
    /// no meaning where there is no first one, so the confirmation is this
    /// (§FS-006-project-interface.9).
    #[arg(long)]
    pub yes: bool,

    /// Run it beneath the terminal as a job rather than here, whatever the
    /// entry declares (§FS-005-dispatch.17).
    #[arg(long)]
    pub background: bool,

    /// Report what would run, and where, without running it.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

/// `ephor branches` (§FS-011-command-line.2): what the registry knows about a
/// project's branches, and what the disk says about them.
#[derive(Args, Debug, Default)]
pub struct BranchesArgs {
    /// The project to read. With none, every configured project.
    pub project: Option<String>,

    /// Only branches whose workspace is on disk.
    #[arg(long)]
    pub checked_out: bool,

    /// Emit raw JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

/// `ephor operations` (§FS-011-command-line.3): the board, without the screen.
#[derive(Args, Debug, Default)]
pub struct OperationsArgs {
    #[command(subcommand)]
    pub command: Option<OperationsCommand>,

    /// Only what is running now.
    #[arg(long)]
    pub live: bool,

    /// Emit raw JSON instead of a listing.
    #[arg(long)]
    pub json: bool,
}

/// `ephor burn` (§FS-013-burn.8): the Burn page as a command. The window and
/// the grouping are the page's two keys, spelled as arguments — which is the
/// whole of the difference between the two surfaces (§REQ-002-parity.2).
#[derive(Args, Debug)]
pub struct BurnArgs {
    /// How far back to look.
    #[arg(long, value_enum, default_value_t = crate::burn::query::Window::Hour)]
    pub window: crate::burn::query::Window,

    /// What to group by. `project`, `model` and `session` read what this
    /// machine burned; `plan` and `matter` read what the runtime metered, and
    /// the two are never added together (§FS-013-burn.1).
    #[arg(long = "by", value_enum, default_value_t = crate::burn::query::By::Project)]
    pub by: crate::burn::query::By,

    /// Read the transcripts before answering, however fresh the store is.
    #[arg(long)]
    pub rescan: bool,

    /// Emit raw JSON instead of a reading.
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum OperationsCommand {
    /// Watch a live run by attaching to it (§FS-011-command-line.8). Leaving
    /// the surface detaches and never stops the run; stopping it is the
    /// runner's own command, which the board only ever shows
    /// (§FS-005-dispatch.20).
    Attach(OperationsAttachArgs),
}

#[derive(Args, Debug)]
pub struct OperationsAttachArgs {
    /// The run, by the id `ephor operations` prints on its row.
    pub run: String,

    /// Emit the outcome as JSON.
    #[arg(long)]
    pub json: bool,
}

/// `ephor thread` (§FS-011-command-line.4): a matter's recorded conversation.
#[derive(Args, Debug)]
pub struct ThreadArgs {
    /// The matter, by its feed id.
    pub item: String,

    /// Emit raw JSON instead of the conversation.
    #[arg(long)]
    pub json: bool,
}

/// `ephor react` (§FS-011-command-line.4).
#[derive(Args, Debug)]
pub struct ReactArgs {
    /// The matter, by its feed id.
    pub item: String,

    /// Which reaction: a palette name (`THUMBS_UP`, `HEART`, `EYES`, …).
    pub content: String,

    /// Which message, by the number `ephor thread` prints beside it. Required
    /// rather than defaulted: a reaction posted on the wrong message is a
    /// reaction nobody can take back through ephor.
    #[arg(long)]
    pub message: usize,

    /// Emit the outcome as JSON (§REQ-002-parity.3).
    #[arg(long)]
    pub json: bool,
}

/// `ephor tick` (§FS-004-quick-actions.5).
#[derive(Args, Debug)]
pub struct TickArgs {
    /// The matter, by its feed id.
    pub item: String,

    /// Which message, by the number `ephor thread` prints beside it.
    #[arg(long)]
    pub message: usize,

    /// Emit the outcome as JSON (§REQ-002-parity.3).
    #[arg(long)]
    pub json: bool,
}

/// `ephor reply` (§FS-005-dispatch.13, §FS-007-matters.4).
#[derive(Args, Debug)]
pub struct ReplyArgs {
    /// The matter, by its feed id.
    pub item: String,

    /// What to say. Left out, the reply a run drafted is sent as it stands —
    /// and posting is what retires the draft.
    pub words: Vec<String>,

    /// Print what would be sent, and where, without sending it.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit the outcome as JSON (§REQ-002-parity.3). With `--dry-run`, the
    /// words that would go out and where — the reading a program checks
    /// before letting the move happen.
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_lay_accepts_repeatable_values_files() {
        let cli = Cli::try_parse_from([
            "ephor",
            "work",
            "lay",
            "review-change",
            "--item",
            "github:repo/1",
            "--values",
            "base.yaml",
            "--values",
            "local.json",
        ])
        .expect("the public command accepts values files");
        let Command::Work(WorkArgs {
            command: Some(WorkCommand::Lay(args)),
        }) = cli.command
        else {
            panic!("expected work lay");
        };
        assert_eq!(args.values, ["base.yaml", "local.json"]);
    }
}
