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
    /// Interactive inbox: navigate the feed, open items, mark them done.
    #[command(alias = "inbox")]
    Tui,
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
