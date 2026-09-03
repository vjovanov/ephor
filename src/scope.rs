//! The global scope selectors, and the rule that a verb honours one or
//! refuses it (§FS-011-command-line.9).
//!
//! `--workspace`, `--tag` and `--org` are declared once on [`Cli`] and
//! carried into every subcommand's help, so every verb advertises all three.
//! For most of ephor's life exactly three sites read them, and every other
//! verb parsed a selector and changed nothing — a scope that looked honoured
//! and was not, which is worse than an error because the output of the scoped
//! run and the site-wide one are identical.
//!
//! `--all` was the fourth, and it is gone from here: it said which *branch
//! entries* a managed-workspace verb walks, `mark-read` read the same flag as
//! "every project", and one global flag meaning two things is a scope nobody
//! can read off the command line. It is now declared by each verb that reads
//! it, which is also why nothing advertises it where it would mean nothing.
//!
//! What lives here is the other half of that: [`honoured`] says, for every
//! variant of [`Command`], which selectors that verb reads, and [`Scope`]
//! refuses the rest by name. The match in [`honoured`] is total on purpose —
//! a variant added without a line here does not compile, so the fault this
//! module exists to end cannot come back by omission.
//!
//! [`sweeps`] is the second axis, and the one §FS-011-command-line.10 turns
//! on: which projects a verb reads says nothing about what it then writes in
//! them. A read at any width is free; a verb that changes a work root in every
//! project the scope reaches is a different kind of act above one checkout, so
//! [`Act`] holds it to the gate — report by default, act under `--act`. Total
//! for the same reason [`honoured`] is.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::cli::{Cli, Command, WorkArgs, WorkCommand};
use crate::error::{registry_error, Result};
use crate::registry;

/// One of the three selectors declared on [`Cli`] (§FS-011-command-line.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector {
    Workspace,
    Tag,
    Organization,
}

impl Selector {
    /// The flag as a caller types it, which is how a refusal names it.
    pub fn flag(self) -> &'static str {
        match self {
            Selector::Workspace => "--workspace",
            Selector::Tag => "--tag",
            Selector::Organization => "--org",
        }
    }
}

/// What a verb does with the selectors (§FS-011-command-line.9), and where the
/// selection is made. There is no third answer to the first question: a verb
/// honours all three selectors or refuses every one of them. The two refusing
/// variants differ only in what the refusal *says* — a verb about one target
/// and a verb about the whole site both refuse, and neither may say something
/// untrue about what it reads on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Honours {
    /// Nothing: the verb is about what it is given — one item, one checkout,
    /// one file — so there is no set of projects for a selector to narrow.
    Nothing,
    /// Nothing either, but for the other reason: the verb reads every project
    /// the site is configured with and answers for the site itself, not for a
    /// group the registry names. `doctor`, `capabilities` and `operations`
    /// are here, and they say so when they refuse: a refusal that told a
    /// reader these verbs read no projects would send them looking for a
    /// project set that is right there in the output.
    NothingOverTheSite,
    /// The projects the site watches, which is a different list from the
    /// registry's: these verbs pick from `status.json` while the selectors
    /// name registry rows. The bridge between the two is [`Scope::projects`],
    /// resolved once here and carried to where each verb picks.
    Watched,
    /// The registry's own rows, selected by the verb itself out of the
    /// registry it loads — [`crate::registry::select_projects`] for `list`,
    /// [`crate::registry::select_managed_workspaces`] for the three verbs
    /// about the branch entries behind a project. Nothing is resolved here
    /// for them: there is no second list to bridge to, and loading the
    /// registry twice is two chances to answer from two registries.
    Registry,
}

impl Honours {
    pub fn selectors(self) -> &'static [Selector] {
        const PROJECTS: &[Selector] = &[Selector::Workspace, Selector::Tag, Selector::Organization];
        match self {
            Honours::Nothing | Honours::NothingOverTheSite => &[],
            Honours::Watched | Honours::Registry => PROJECTS,
        }
    }
}

/// Which selectors each verb honours (§FS-011-command-line.9).
///
/// Total on [`Command`], and deliberately written out rather than defaulted:
/// a variant added without a line here fails to compile, so a new verb has to
/// say which side of the rule it is on before it can ship. The guarded arms
/// are the verbs that are two verbs wearing one name — `validate --manifest`
/// reads a file where `validate` reads the registry, `ensure-agents --type`
/// renders a workspace that is in no registry at all — and each half answers
/// for itself.
pub fn honoured(command: &Command) -> (String, Honours) {
    let said = |name: &str, honours| (name.to_string(), honours);
    match command {
        Command::List(_) => said("list", Honours::Registry),
        Command::Validate(args) if args.manifest.is_some() => {
            said("validate --manifest", Honours::Nothing)
        }
        // The registry held to the published schema and no checkout looked
        // for (§FS-009-shipped-actions.1): it selects no workspace, so a
        // selector would narrow nothing.
        Command::Validate(args) if args.schema_only => {
            said("validate --schema-only", Honours::Nothing)
        }
        Command::Validate(_) => said("validate", Honours::Registry),
        Command::Check(_) => said("check", Honours::Nothing),
        Command::Schema(_) => said("schema", Honours::Nothing),
        Command::EnsureAgents(args) if args.project_type.is_some() => {
            said("ensure-agents --type", Honours::Nothing)
        }
        Command::EnsureAgents(_) => said("ensure-agents", Honours::Registry),
        Command::Update(_) => said("update", Honours::Registry),
        Command::Status(_) => said("status", Honours::Watched),
        // What nothing claimed belongs to no project (§FS-008-attribution.4),
        // so there is nothing here for a project selector to narrow.
        Command::Feed(args) if args.unattributed => said("feed --unattributed", Honours::Nothing),
        Command::Feed(_) => said("feed", Honours::Watched),
        Command::Refresh(_) => said("refresh", Honours::Watched),
        Command::MarkRead(_) => said("mark-read", Honours::Watched),
        Command::Failures(_) => said("failures", Honours::Nothing),
        Command::Restart(_) => said("restart", Honours::Nothing),
        Command::Rebase(_) => said("rebase", Honours::Nothing),
        Command::Checkout(_) => said("checkout", Honours::Nothing),
        Command::Work(args) => work_honoured(args),
        Command::Job(_) => said("job", Honours::Nothing),
        Command::Actions(_) => said("actions", Honours::Nothing),
        Command::Branches(_) => said("branches", Honours::Watched),
        Command::Operations(_) => said("operations", Honours::NothingOverTheSite),
        Command::Thread(_) => said("thread", Honours::Nothing),
        Command::React(_) => said("react", Honours::Nothing),
        Command::Tick(_) => said("tick", Honours::Nothing),
        Command::Reply(_) => said("reply", Honours::Nothing),
        // The three that answer for the site itself: they read every
        // configured project, so their refusal says that rather than denying
        // it (§FS-011-command-line.9).
        Command::Capabilities(_) => said("capabilities", Honours::NothingOverTheSite),
        Command::Doctor(_) => said("doctor", Honours::NothingOverTheSite),
        Command::Tui => said("tui", Honours::Watched),
    }
}

/// The same classification one level down: `ephor work` is eleven verbs, and
/// four of them read a set of projects.
fn work_honoured(args: &WorkArgs) -> (String, Honours) {
    let said = |name: &str, honours| (name.to_string(), honours);
    // No subcommand is `work list` (§FS-005-dispatch.4), so it is classified
    // as what it runs rather than as a name of its own.
    match args.command.as_ref() {
        None | Some(WorkCommand::List(_)) => said("work list", Honours::Watched),
        Some(WorkCommand::Dispatch(_)) => said("work dispatch", Honours::Watched),
        Some(WorkCommand::Sync(_)) => said("work sync", Honours::Watched),
        Some(WorkCommand::Run(_)) => said("work run", Honours::Watched),
        Some(WorkCommand::Offers(_)) => said("work offers", Honours::Nothing),
        Some(WorkCommand::Ask(_)) => said("work ask", Honours::Nothing),
        Some(WorkCommand::Cancel(_)) => said("work cancel", Honours::Nothing),
        Some(WorkCommand::Workflows(_)) => said("work workflows", Honours::Nothing),
        Some(WorkCommand::Lay(_)) => said("work lay", Honours::Nothing),
        Some(WorkCommand::Forget(_)) => said("work forget", Honours::Nothing),
        Some(WorkCommand::States(_)) => said("work states", Honours::Nothing),
    }
}

/// What a verb writes *across* the scope it resolved (§FS-011-command-line.10).
///
/// The other axis to [`Honours`]: that one says which projects a verb reads,
/// this one says whether it changes a work root in every one of them. Total on
/// [`Command`] for the same reason, and written out rather than defaulted — a
/// new mutating verb that shipped without a line here would be the one thing
/// this rule exists to make impossible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sweeps {
    /// Nothing the width changes: the verb reads, or it writes about the one
    /// checkout, matter or record it was named, or it writes ephor's own
    /// memory — a read mark, a ledger row — which is the same act at every
    /// width. `--act` would mean nothing here, so it is refused by name.
    Nothing,
    /// A work root in every project the scope reaches, held to the gate:
    /// above one project it reports what it would do and writes only under
    /// `--act`.
    Gated,
    /// The same sweep, and deliberately *not* held to the gate — the whole
    /// content of this variant is that the exemption is written down.
    /// `update` and `ensure-agents` rewrite files in every managed workspace
    /// their scope reaches, and the issue that built the gate did not name
    /// them; classifying them with the read verbs would have hidden that
    /// behind a word that was not true. Joining the gate is one word on this
    /// line (§FS-011-command-line.10).
    Ungated,
}

/// What each verb writes across its scope (§FS-011-command-line.10).
///
/// Beside [`honoured`] rather than folded into it: the two questions have
/// different answers for the same verb — `work list` and `work dispatch` read
/// the same projects and only one of them writes in them — and a reader
/// checking one rule should not have to read the other's arms to find it.
pub fn sweeps(command: &Command) -> Sweeps {
    match command {
        // The managed-workspace sweeps. `validate` only reads the paths it
        // walks; the other two rewrite a file in every workspace, and are the
        // deferral this rule records rather than hides.
        Command::Validate(_) => Sweeps::Nothing,
        // One ad hoc workspace, named on the command line and in no registry:
        // there is no set of projects here to be above (§FS-011-command-line.9).
        Command::EnsureAgents(args) if args.project_type.is_some() => Sweeps::Nothing,
        Command::EnsureAgents(_) => Sweeps::Ungated,
        Command::Update(_) => Sweeps::Ungated,
        Command::Work(args) => work_sweeps(args),
        // Everything else. The readings write nothing at all; `rebase`,
        // `checkout`, `restart`, `react`, `reply` and `tick` change the one
        // thing they were given, which is the same act however wide the site
        // is; `mark-read` and the screen write ephor's own memory of what has
        // been seen, not a project's work root.
        Command::List(_)
        | Command::Check(_)
        | Command::Schema(_)
        | Command::Status(_)
        | Command::Feed(_)
        | Command::Refresh(_)
        | Command::MarkRead(_)
        | Command::Failures(_)
        | Command::Restart(_)
        | Command::Rebase(_)
        | Command::Checkout(_)
        | Command::Job(_)
        | Command::Actions(_)
        | Command::Branches(_)
        | Command::Operations(_)
        | Command::Thread(_)
        | Command::React(_)
        | Command::Tick(_)
        | Command::Reply(_)
        | Command::Capabilities(_)
        | Command::Doctor(_)
        | Command::Tui => Sweeps::Nothing,
    }
}

/// The same question one level down: three of `ephor work`'s eleven verbs
/// write a work root in every project they reach (§FS-011-command-line.10).
fn work_sweeps(args: &WorkArgs) -> Sweeps {
    match args.command.as_ref() {
        Some(WorkCommand::Dispatch(_)) | Some(WorkCommand::Sync(_)) | Some(WorkCommand::Run(_)) => {
            Sweeps::Gated
        }
        // `work list` reads; the rest are about one matter, which the width
        // of the site does not widen — `lay`, `ask`, `cancel` and `forget`
        // included, since each names the matter it is about.
        None
        | Some(WorkCommand::List(_))
        | Some(WorkCommand::Offers(_))
        | Some(WorkCommand::Ask(_))
        | Some(WorkCommand::Cancel(_))
        | Some(WorkCommand::Workflows(_))
        | Some(WorkCommand::Lay(_))
        | Some(WorkCommand::Forget(_))
        | Some(WorkCommand::States(_)) => Sweeps::Nothing,
    }
}

/// The word that says act at a scope wider than one project
/// (§FS-011-command-line.10).
///
/// Read off the command line beside [`Scope`] and held to the verb the same
/// way: a flag accepted where it can change nothing is the fault
/// §FS-011-command-line.9 exists to end, arriving one rule later.
#[derive(Clone, Copy, Debug, Default)]
pub struct Act {
    asked: bool,
}

impl Act {
    pub fn of(cli: &Cli) -> Act {
        Act { asked: cli.act }
    }

    /// The flag as if it had been given, for the callers that are not a
    /// command line — the interface dispatches one matter at a time and is
    /// never above the gate, and a test that is about something else should
    /// not have to say so.
    pub fn asked() -> Act {
        Act { asked: true }
    }

    /// Hold a verb to the flag: `--act` is taken exactly where the gate can
    /// fire, and refused by name everywhere else (§FS-011-command-line.10).
    pub fn held_to(&self, verb: &str, sweeps: Sweeps) -> Result<()> {
        if !self.asked || sweeps == Sweeps::Gated {
            return Ok(());
        }
        let mut says = format!("{verb} does not take --act.");
        match sweeps {
            Sweeps::Nothing => says.push_str(&format!(
                " --act lets a sweep act at a scope wider than one project, and {verb} \
                 sweeps no set of projects: it is the same act however many the site has."
            )),
            // Said plainly rather than as an apology: the reader is owed the
            // fact that this verb acts at every width, which is what makes
            // the flag meaningless on it rather than merely unimplemented.
            Sweeps::Ungated => says.push_str(&format!(
                " --act lets a sweep act at a scope wider than one project, and {verb} is \
                 not held to that gate: it acts at every width, as it always has."
            )),
            Sweeps::Gated => unreachable!("returned above"),
        }
        says.push_str(" `work dispatch`, `work sync` and `work run` take it.");
        // Exits 2 like a refused selector: both halves of the scope rule are
        // one configuration refusal for the caller (§FS-011-command-line.9).
        Err(registry_error(says))
    }

    /// The gate for a run of `verb` that resolved to `projects` projects.
    pub fn over(&self, verb: &str, projects: usize) -> Gate {
        if self.asked || projects <= 1 {
            return Gate::acting();
        }
        Gate {
            held: Some(format!(
                "Nothing was written: {verb} reaches {projects} projects, and above one \
                 it reports what it would do. Pass --act to do it."
            )),
        }
    }
}

/// Whether this run acts, or reports what it would do
/// (§FS-011-command-line.10).
///
/// A sentence rather than a flag, because the report is only half the rule:
/// the other half is that the reader is told the word that acts, which is why
/// nothing here can be held without something to say.
#[derive(Clone, Debug, Default)]
pub struct Gate {
    held: Option<String>,
}

impl Gate {
    /// Nothing wide enough to hold: what this command line did before the
    /// rule, byte for byte.
    pub fn acting() -> Gate {
        Gate { held: None }
    }

    /// Whether this run must report rather than write.
    pub fn holds(&self) -> bool {
        self.held.is_some()
    }

    /// What a held run says it did instead, naming the flag that does it.
    pub fn says(&self) -> Option<&str> {
        self.held.as_deref()
    }
}

/// The four selectors as they were given, before anything is resolved.
///
/// Kept apart from the resolved [`Projects`] because refusing must not need a
/// registry: `schema`, `check` and `validate --manifest` answer without one,
/// and a verb that refuses a selector has to be able to say so on a machine
/// where the registry is missing (§FS-011-command-line.9).
#[derive(Clone, Debug, Default)]
pub struct Scope {
    workspaces: Vec<String>,
    tags: Vec<String>,
    organization: Option<String>,
}

impl Scope {
    pub fn of(cli: &Cli) -> Scope {
        Scope {
            workspaces: cli.workspace.clone(),
            tags: cli.tag.clone(),
            organization: cli.organization.clone(),
        }
    }

    /// The selectors actually given, in the order the help lists them.
    fn given(&self) -> Vec<Selector> {
        let mut given = Vec::new();
        if !self.workspaces.is_empty() {
            given.push(Selector::Workspace);
        }
        if !self.tags.is_empty() {
            given.push(Selector::Tag);
        }
        if self.organization.is_some() {
            given.push(Selector::Organization);
        }
        given
    }

    /// Whether anything was given that names a set of projects.
    fn selects_projects(&self) -> bool {
        !self.workspaces.is_empty() || !self.tags.is_empty() || self.organization.is_some()
    }

    /// What was typed, so a refusal quotes the caller rather than paraphrasing
    /// them: `--org graal`, `--workspace ephor --tag rust`.
    fn said(&self) -> String {
        let mut said = Vec::new();
        for workspace in &self.workspaces {
            said.push(format!("--workspace {workspace}"));
        }
        for tag in &self.tags {
            said.push(format!("--tag {tag}"));
        }
        if let Some(organization) = &self.organization {
            said.push(format!("--org {organization}"));
        }
        said.join(" ")
    }

    /// Hold a verb to the rule: every selector it was given that it does not
    /// read is refused by name (§FS-011-command-line.9).
    ///
    /// The sentence names the verb, the flags, and what the verb *does* take,
    /// because a caller who reached this was scoping something and still needs
    /// to know how.
    pub fn held_to(&self, verb: &str, honours: Honours) -> Result<()> {
        let refused: Vec<Selector> = self
            .given()
            .into_iter()
            .filter(|selector| !honours.selectors().contains(selector))
            .collect();
        if refused.is_empty() {
            return Ok(());
        }
        let flags: Vec<&str> = refused.iter().map(|selector| selector.flag()).collect();
        let mut says = format!("{verb} does not take {}.", spoken(&flags));
        match honours {
            Honours::Nothing => says.push_str(&format!(
                " {verb} takes no scope selector: it is about what it is given, \
                 not about a set of projects."
            )),
            Honours::NothingOverTheSite => says.push_str(&format!(
                " {verb} takes no scope selector: it reads every project the \
                 site is configured with and answers for the site itself, not \
                 for a group the registry names."
            )),
            _ => {
                let takes: Vec<&str> = honours
                    .selectors()
                    .iter()
                    .map(|selector| selector.flag())
                    .collect();
                says.push_str(&format!(" {verb} takes {}.", spoken(&takes)));
            }
        }
        // A refusal of the second kind exits like a refusal of the first: an
        // empty selection is 2, and so is every other usage-shaped refusal
        // ephor makes, so "the scope was refused" is one comparison for the
        // caller rather than two (§FS-011-command-line.9).
        Err(registry_error(says))
    }

    /// The projects these selectors name, resolved against the registry.
    ///
    /// Cheap and registry-free where nothing was given: the answer is then
    /// "every project the verb would have read anyway", which is what the
    /// verbs did before this rule existed.
    pub fn projects(&self) -> Result<Projects> {
        if !self.selects_projects() {
            return Ok(Projects::every());
        }
        let registry = crate::feed::commands::load_registry_doc()?;
        self.against(&registry)
    }

    /// The same, against a registry document already in hand — which is what
    /// the tests use, and what a caller that has just loaded one should.
    pub fn against(&self, registry: &Value) -> Result<Projects> {
        let said = self.said();
        let mut named: BTreeSet<String> =
            registry::select_projects(registry, &[], &self.tags, self.organization.as_deref())?
                .into_iter()
                .map(|project| registry::id_of(project).to_string())
                .collect();

        if !self.workspaces.is_empty() {
            let asked = self.asked_for(registry)?;
            named.retain(|project| asked.contains(project));
        }

        if named.is_empty() {
            return Err(registry_error(format!(
                "{said} matches no project in {}.",
                crate::paths::default_registry_path().display()
            )));
        }
        Ok(Projects {
            named: Some(named),
            said,
        })
    }

    /// Which projects `--workspace` named, by their own id or by the id of a
    /// workspace derived from them: a branch workspace belongs to its
    /// project, and a reader who named one meant that project.
    ///
    /// An id that names neither is the registry's own refusal in the
    /// registry's own words, raised here so a typo is a message rather than an
    /// empty selection.
    fn asked_for(&self, registry: &Value) -> Result<BTreeSet<String>> {
        let projects: BTreeSet<String> = registry::select_projects(registry, &[], &[], None)?
            .into_iter()
            .map(|project| registry::id_of(project).to_string())
            .collect();
        let derived = registry::iter_registered_workspaces(registry)?;
        let mut asked = BTreeSet::new();
        let mut unknown: Vec<&str> = Vec::new();
        for id in &self.workspaces {
            if projects.contains(id) {
                asked.insert(id.clone());
                continue;
            }
            let owners: Vec<&str> = derived
                .iter()
                .filter(|workspace| &workspace.id == id)
                .map(|workspace| workspace.project_id.as_str())
                .collect();
            if owners.is_empty() {
                unknown.push(id.as_str());
                continue;
            }
            asked.extend(owners.into_iter().map(str::to_string));
        }
        unknown.sort_unstable();
        if !unknown.is_empty() {
            return Err(registry_error(format!(
                "Unknown projects or workspaces: {}",
                unknown.join(", ")
            )));
        }
        Ok(asked)
    }
}

/// `--org` / `--org and --tag` / `--org, --tag and --all`.
fn spoken(flags: &[&str]) -> String {
    match flags {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Which projects a run may touch, resolved once and applied where each verb
/// picks its projects (§FS-011-command-line.9).
///
/// `None` is not "no projects" but "no selector was given": the verb reads
/// what it always read. That distinction is the whole type — a scope that
/// resolved to the full list and a scope nobody asked for behave the same,
/// but only one of them may refuse an empty intersection.
#[derive(Clone, Debug, Default)]
pub struct Projects {
    named: Option<BTreeSet<String>>,
    said: String,
}

impl Projects {
    /// No selector was given.
    pub fn every() -> Projects {
        Projects {
            named: None,
            said: String::new(),
        }
    }

    /// Whether a selector narrowed anything.
    pub fn narrowed(&self) -> bool {
        self.named.is_some()
    }

    /// Whether this project is in scope.
    pub fn holds(&self, project: &str) -> bool {
        match &self.named {
            Some(named) => named.contains(project),
            None => true,
        }
    }

    /// A project named on the command line, held to the scope. Refused by
    /// name rather than filtered away: a reader who named a project and a
    /// selector that excludes it asked two things, and printing one of them
    /// silently is the fault this rule ends.
    pub fn admit(&self, project: &str) -> Result<()> {
        if self.holds(project) {
            return Ok(());
        }
        Err(registry_error(format!(
            "Project '{project}' is outside {}.",
            self.said
        )))
    }

    /// The watched projects this run may read, in the order they were given.
    ///
    /// An empty answer under a selector is refused, never printed: a mistyped
    /// organization and one whose projects nobody watches otherwise print the
    /// same quiet table as a site with nothing to say
    /// (§FS-011-command-line.9).
    pub fn over<'a, I>(&self, watched: I) -> Result<Vec<&'a String>>
    where
        I: IntoIterator<Item = &'a String>,
    {
        let watched: Vec<&String> = watched.into_iter().collect();
        let Some(_) = &self.named else {
            return Ok(watched);
        };
        let kept: Vec<&String> = watched
            .iter()
            .copied()
            .filter(|project| self.holds(project))
            .collect();
        if kept.is_empty() {
            return Err(registry_error(format!(
                "{} matches no watched project (watched: {}).",
                self.said,
                watched
                    .iter()
                    .map(|project| project.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        Ok(kept)
    }

    /// The `--project` restriction a verb should read: what it was asked for,
    /// held to the scope. An empty answer keeps the meaning it always had —
    /// every watched project — and comes back only where no selector narrowed
    /// anything.
    pub fn narrow<'a, I>(&self, asked: &[String], watched: I) -> Result<Vec<String>>
    where
        I: IntoIterator<Item = &'a String>,
    {
        if !self.narrowed() {
            return Ok(asked.to_vec());
        }
        if asked.is_empty() {
            return Ok(self.over(watched)?.into_iter().cloned().collect());
        }
        for project in asked {
            self.admit(project)?;
        }
        Ok(asked.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use serde_json::json;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("ephor").chain(args.iter().copied())).expect("parses")
    }

    fn scope(args: &[&str]) -> Scope {
        Scope::of(&cli(args))
    }

    fn two_orgs() -> Value {
        json!({
            "project_types": [],
            "organizations": [{ "id": "foundation" }, { "id": "graal" }],
            "projects": [
                { "id": "ephor", "organization": "foundation", "tags": ["rust"] },
                { "id": "rhei", "organization": "foundation", "tags": ["rust"] },
                { "id": "graal", "organization": "graal", "tags": ["java"] }
            ]
        })
    }

    /// §FS-011-command-line.9: a verb that reads no project set refuses every
    /// selector it was given, and says which ones it does take. The refusal
    /// is a registry error, so it exits 2 like the empty selection at the
    /// other end of the same rule rather than making the caller test for two
    /// codes to learn one thing.
    #[test]
    fn a_verb_that_reads_no_projects_refuses_the_selector() {
        let err = scope(&["--org", "graal", "rebase"])
            .held_to("rebase", Honours::Nothing)
            .unwrap_err();
        assert!(
            matches!(err, crate::error::EphorError::Registry(_)),
            "a refused selector must exit 2: {err:?}"
        );
        let refused = err.to_string();
        assert!(refused.contains("rebase does not take --org"), "{refused}");
        assert!(refused.contains("takes no scope selector"), "{refused}");
        assert!(scope(&["rebase"])
            .held_to("rebase", Honours::Nothing)
            .is_ok());
    }

    /// §FS-011-command-line.9: `doctor`, `capabilities` and `operations`
    /// refuse the selectors too, but they *do* read every configured project
    /// — so the sentence they refuse with says that instead of denying it.
    /// A reader told these verbs are about no set of projects would go
    /// looking for the project set their own output prints.
    #[test]
    fn a_verb_that_answers_for_the_site_says_what_it_reads() {
        for verb in ["doctor", "capabilities", "operations"] {
            let (named, honours) = honoured(&cli(&[verb]).command);
            assert_eq!(
                (named.as_str(), honours),
                (verb, Honours::NothingOverTheSite)
            );
            let refused = scope(&["--org", "graal", verb])
                .held_to(&named, honours)
                .unwrap_err()
                .to_string();
            assert!(
                refused.contains(&format!("{verb} does not take --org")),
                "{refused}"
            );
            assert!(
                refused.contains("reads every project the site is configured with"),
                "{refused}"
            );
            assert!(
                !refused.contains("not about a set of projects"),
                "{refused}"
            );
        }
    }

    /// §FS-011-command-line.9: `--all` is not a scope selector any more. It
    /// belongs to the verbs that read it, so a verb that would ignore it does
    /// not parse it — clap refuses it before ephor is asked.
    #[test]
    fn all_belongs_to_the_verbs_that_read_it() {
        for verb in ["validate", "ensure-agents", "update", "mark-read"] {
            assert!(
                Cli::try_parse_from(["ephor", verb, "--all"]).is_ok(),
                "{verb} lost its own --all"
            );
        }
        for verb in ["status", "rebase", "checkout", "list"] {
            assert!(
                Cli::try_parse_from(["ephor", verb, "--all"]).is_err(),
                "{verb} still parses --all"
            );
        }
        assert!(Cli::try_parse_from(["ephor", "--all", "update"]).is_err());
    }

    /// §FS-011-command-line.9: the selectors resolve to a project set through
    /// the registry, and `--workspace` may name a derived branch workspace.
    #[test]
    fn the_selectors_resolve_to_the_projects_they_name() {
        let registry = two_orgs();
        let foundation = scope(&["--org", "foundation", "status"])
            .against(&registry)
            .unwrap();
        assert!(foundation.holds("ephor") && foundation.holds("rhei"));
        assert!(!foundation.holds("graal"));

        let tagged = scope(&["--tag", "java", "status"])
            .against(&registry)
            .unwrap();
        assert!(tagged.holds("graal") && !tagged.holds("ephor"));

        let one = scope(&["--workspace", "rhei", "status"])
            .against(&registry)
            .unwrap();
        assert!(one.holds("rhei") && !one.holds("ephor"));
    }

    /// §FS-011-command-line.9: an empty selection is refused rather than
    /// printed, and the sentence says which end came out empty.
    #[test]
    fn an_empty_selection_is_refused_at_the_end_that_is_empty() {
        let registry = two_orgs();
        let nowhere = scope(&["--org", "graall", "status"])
            .against(&registry)
            .unwrap_err()
            .to_string();
        assert!(
            nowhere.contains("--org graall matches no project"),
            "{nowhere}"
        );

        let unwatched = scope(&["--org", "graal", "status"])
            .against(&registry)
            .unwrap()
            .over(["ephor".to_string(), "rhei".to_string()].iter())
            .unwrap_err()
            .to_string();
        assert!(
            unwatched.contains("matches no watched project (watched: ephor, rhei)"),
            "{unwatched}"
        );
    }

    /// §FS-011-command-line.9: a project named beside a selector that excludes
    /// it is refused by name rather than quietly dropped.
    #[test]
    fn a_named_project_outside_the_scope_is_refused() {
        let foundation = scope(&["--org", "foundation", "status"])
            .against(&two_orgs())
            .unwrap();
        let refused = foundation.admit("graal").unwrap_err().to_string();
        assert!(
            refused.contains("Project 'graal' is outside --org foundation."),
            "{refused}"
        );
        assert!(foundation.admit("ephor").is_ok());
    }

    /// With nothing given, every verb reads what it always read.
    #[test]
    fn nothing_given_narrows_nothing() {
        let every = scope(&["status"]).projects().unwrap();
        assert!(!every.narrowed());
        assert!(every.holds("anything"));
        let watched = vec!["ephor".to_string()];
        assert_eq!(
            every.narrow(&[], watched.iter()).unwrap(),
            Vec::<String>::new()
        );
    }

    /// §FS-011-command-line.9: every verb is classified, and the ones that are
    /// two verbs under one name answer for themselves.
    #[test]
    fn every_verb_is_on_one_side_of_the_rule() {
        let classified = |args: &[&str]| honoured(&cli(args).command);
        assert_eq!(classified(&["status"]), ("status".into(), Honours::Watched));
        assert_eq!(classified(&["rebase"]), ("rebase".into(), Honours::Nothing));
        assert_eq!(
            classified(&["update"]),
            ("update".into(), Honours::Registry)
        );
        assert_eq!(
            classified(&["validate"]),
            ("validate".into(), Honours::Registry)
        );
        assert_eq!(
            classified(&["validate", "--manifest", "ephor.json"]),
            ("validate --manifest".into(), Honours::Nothing)
        );
        assert_eq!(
            classified(&["validate", "--schema-only"]),
            ("validate --schema-only".into(), Honours::Nothing)
        );
        assert_eq!(
            classified(&["ensure-agents", "--type", "monorepo"]),
            ("ensure-agents --type".into(), Honours::Nothing)
        );
        assert_eq!(
            classified(&["feed", "--unattributed"]),
            ("feed --unattributed".into(), Honours::Nothing)
        );
        assert_eq!(
            classified(&["work"]),
            ("work list".into(), Honours::Watched)
        );
        assert_eq!(
            classified(&["work", "dispatch"]),
            ("work dispatch".into(), Honours::Watched)
        );
        assert_eq!(
            classified(&["work", "offers", "--item", "x"]),
            ("work offers".into(), Honours::Nothing)
        );
    }

    /// §FS-011-command-line.10: every verb says what it writes across its
    /// scope, and the three that sweep a work root are the ones the gate can
    /// fire on. `update` and `ensure-agents` are the written-down deferral —
    /// classified as the sweeps they are and left outside the gate, so
    /// joining it later is one word rather than a rediscovery.
    #[test]
    fn every_verb_says_what_it_sweeps() {
        let swept = |args: &[&str]| sweeps(&cli(args).command);
        for gated in [
            vec!["work", "dispatch"],
            vec!["work", "sync"],
            vec!["work", "run"],
        ] {
            assert_eq!(swept(&gated), Sweeps::Gated, "{gated:?}");
        }
        assert_eq!(swept(&["update"]), Sweeps::Ungated);
        assert_eq!(swept(&["ensure-agents"]), Sweeps::Ungated);
        // Two verbs under one name answer for themselves here too: one ad hoc
        // workspace is no set of projects to be above.
        assert_eq!(
            swept(&["ensure-agents", "--type", "monorepo"]),
            Sweeps::Nothing
        );
        for reading in [
            vec!["status"],
            vec!["feed"],
            vec!["refresh"],
            vec!["list"],
            vec!["validate"],
            vec!["rebase"],
            vec!["checkout"],
            vec!["mark-read"],
            vec!["tui"],
            vec!["work"],
            vec!["work", "forget", "--done"],
            vec!["work", "lay", "--item", "x", "entry"],
        ] {
            assert_eq!(swept(&reading), Sweeps::Nothing, "{reading:?}");
        }
    }

    /// §FS-011-command-line.10: `--act` is taken where the gate can fire and
    /// refused by name everywhere else — including the two sweeps left
    /// outside the gate, whose refusal says *that* rather than denying they
    /// sweep. It exits 2, like a refused selector at the other end of the
    /// same rule.
    #[test]
    fn act_is_refused_where_the_gate_cannot_fire() {
        let act = Act::asked();
        assert!(act.held_to("work dispatch", Sweeps::Gated).is_ok());
        assert!(act.held_to("work sync", Sweeps::Gated).is_ok());

        let err = act.held_to("rebase", Sweeps::Nothing).unwrap_err();
        assert!(
            matches!(err, crate::error::EphorError::Registry(_)),
            "a refused --act must exit 2: {err:?}"
        );
        let refused = err.to_string();
        assert!(refused.contains("rebase does not take --act"), "{refused}");
        assert!(refused.contains("sweeps no set of projects"), "{refused}");

        let deferred = act
            .held_to("update", Sweeps::Ungated)
            .unwrap_err()
            .to_string();
        assert!(
            deferred.contains("update does not take --act"),
            "{deferred}"
        );
        assert!(
            deferred.contains("it acts at every width, as it always has"),
            "{deferred}"
        );
        assert!(
            !deferred.contains("sweeps no set of projects"),
            "the deferral must not deny that it sweeps: {deferred}"
        );

        // Nothing said, nothing to hold: every verb is free of the flag it
        // was not given.
        let silent = Act::default();
        for sweeps in [Sweeps::Nothing, Sweeps::Gated, Sweeps::Ungated] {
            assert!(silent.held_to("rebase", sweeps).is_ok());
        }
    }

    /// §FS-011-command-line.10: the gate counts the resolved project set. One
    /// project is what this command line always did; more than one reports,
    /// and says the word that acts.
    #[test]
    fn the_gate_counts_the_resolved_projects() {
        let silent = Act::default();
        assert!(!silent.over("work dispatch", 1).holds());
        // A site watching nothing is not a sweep either.
        assert!(!silent.over("work dispatch", 0).holds());

        let held = silent.over("work dispatch", 4);
        assert!(held.holds());
        let says = held
            .says()
            .expect("a held gate always has something to say");
        assert!(says.contains("work dispatch reaches 4 projects"), "{says}");
        assert!(
            says.contains("--act"),
            "a gated report must name the flag that acts: {says}"
        );

        // And with the word said, today's behaviour at any width.
        assert!(!Act::asked().over("work dispatch", 4).holds());
        assert!(Gate::acting().says().is_none());
    }

    /// Each verb's `--all` is that verb's own, and says what that verb means
    /// by it (§FS-011-command-line.9). This is why the flag could not stay
    /// global: clap propagates a subcommand's value up to the global arg of
    /// the same name, so ephor could not have told `mark-read --all` from a
    /// global `--all` it was supposed to refuse.
    #[test]
    fn each_verbs_all_is_that_verbs_own() {
        match &cli(&["mark-read", "--all"]).command {
            Command::MarkRead(args) => assert!(args.all),
            other => panic!("parsed as {other:?}"),
        }
        match &cli(&["update", "--all"]).command {
            Command::Update(args) => assert!(args.all),
            other => panic!("parsed as {other:?}"),
        }
        match &cli(&["work", "offers", "--item", "x", "--all"]).command {
            Command::Work(args) => match args.command.as_ref() {
                Some(WorkCommand::Offers(offers)) => assert!(offers.all),
                other => panic!("parsed as {other:?}"),
            },
            other => panic!("parsed as {other:?}"),
        }
    }
}
