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
    /// Validate the project registry.
    Validate,
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
    /// Hand items to the agent runtime, and see what came of it.
    Work(WorkArgs),
    /// Interactive inbox: navigate the feed, open items, mark them done.
    #[command(alias = "inbox")]
    Tui,
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
    /// Reopen work whose item has moved since it was dispatched.
    Sync(WorkSyncArgs),
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

    /// Skip items with no activity in this many days.
    #[arg(long, value_name = "DAYS")]
    pub updated_within: Option<i64>,

    /// Report what would be opened without writing anything.
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
    pub rhei_args: Vec<String>,
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

    /// Emit raw JSON instead of a table.
    #[arg(long)]
    pub json: bool,
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
