//! `ephor checkout` — making the workspace that is not there
//! (§FS-004-quick-actions.7).
//!
//! The reader presses a key for it and a state machine runs it as a program;
//! both arrive here, so there is one answer to where a project's branch
//! workspace goes and what it holds (§FS-005-dispatch.12). Everything it needs
//! is already in the registry — the directory template, the repositories, the
//! main branch — which is why nobody has to configure a command for it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::branches::Placement;
use crate::cli::CheckoutArgs;
use crate::error::{EphorError, Result};
use crate::feed::cache;
use crate::feed::config::load_config;
use crate::feed::model::Item;
use crate::git;
use crate::given;

/// What making one branch workspace came to (§FS-004-quick-actions.7), in the
/// shape every caller of [`make`] needs: what was already there, what git did
/// with the rest, and what the store came to.
pub struct Made {
    /// The directory the workspace belongs at — made now, or found there.
    pub target: PathBuf,
    /// Every declared repository was already on disk, so there was no tree
    /// left to make and the store was all this had left to do
    /// (§FS-004-quick-actions.7.1).
    pub already: bool,
    /// The repositories that were absent from a directory that was there, for
    /// the line a reader is owed about what is being made.
    pub missing: Vec<String>,
    /// What git came to, where anything was made.
    pub outcome: Option<git::Creation>,
    /// None where the tree is half-made: a work root inside one would be a
    /// place for plans that cannot be worked (§FS-006-project-interface.7).
    pub store: Option<Store>,
}

impl Made {
    /// Why this is not a workspace, where it is not: nothing to make it from,
    /// or a repository the checkout refused. Half a workspace is not one —
    /// whatever is missing, the next thing to run in here would fail on it.
    ///
    /// Returned rather than printed, because the two callers answer for it
    /// differently: the command has already reported every repository and
    /// stops at an exit code, and a dispatch has written nothing yet and
    /// refuses in the checkout's own words (§FS-005-dispatch.25).
    pub fn refusal(&self, source: &Path) -> Option<String> {
        let outcome = self.outcome.as_ref()?;
        if outcome.repos.is_empty() {
            return Some(format!(
                "No repository under {} to make a workspace from.",
                source.display()
            ));
        }
        (!outcome.refused().is_empty()).then(|| outcome.report())
    }
}

/// Make the branch workspace `branch` belongs in, or find it already made
/// (§FS-004-quick-actions.7).
///
/// The one implementation of that operation, for every caller: the key the
/// reader presses, the command a state machine runs, and the dispatch that
/// makes the workspace a `branch` template named (§FS-005-dispatch.25). Two
/// of them would eventually disagree about what a checked-out workspace is —
/// which repositories it holds, what its branches are grown from, whether it
/// has a store — and the disagreement would be discovered by work landing in
/// a directory that is not one.
///
/// It writes nothing to the registry and pushes nothing: a workspace is found
/// on disk like every other (§FS-008-attribution.2), and publishing a branch
/// is the work's move.
pub fn make(
    placement: &Placement,
    project: &str,
    branch: &str,
    from: Option<&str>,
) -> Result<(Made, PathBuf)> {
    // Where the workspace goes is settled before the first directory can be
    // created (§FS-004-quick-actions.7.3), and here rather than at the command
    // line alone: this is the implementation every caller shares
    // (§FS-005-dispatch.12), so the name a reader typed, the one a state
    // machine set and the one a `branch` template minted are all held to it.
    if !crate::forest::is_branch_name(branch) {
        return Err(EphorError::Registry(format!(
            "'{branch}' is a placeholder nothing filled, not a branch to check out."
        )));
    }
    if let Some(why) = crate::branches::why_git_refuses(branch) {
        return Err(EphorError::Registry(format!(
            "git will not take '{branch}' as a branch name: {why}."
        )));
    }
    let target = placement.workspace_for(branch).ok_or_else(|| {
        EphorError::Command(format!(
            "{project} does not use a checkout per branch (no branch_root_template), so \
             there is no workspace to make for {branch} — its root is the checkout."
        ))
    })?;
    // Read once and answered twice: the work root says whether this name lands
    // on the place plans go, and the same reading puts the store there once the
    // tree is whole (§FS-006-project-interface.7).
    let work = Work::read(placement, project);
    if let Some(why) =
        crate::branches::why_the_workspace_is_refused(placement, branch, &work.root())
    {
        return Err(EphorError::Registry(why));
    }
    // A directory is not a workspace: the declared repositories are what make
    // it one. This is the operation whose answer says whether the workspace is
    // whole (§AR-004-forest.1), so a directory that is there is asked which of
    // them are — by path, which is what tells presence (§AR-004-forest.3) —
    // and only a whole one stops here. A project that declares no forest has
    // nothing to be missing and answers as it always did.
    let mut missing = Vec::new();
    if target.is_dir() {
        missing = placement.forest(&target).absent;
        if missing.is_empty() {
            // Every repository is here, so there is no tree left to make. The
            // store still may be: a workspace made before ephor made stores at
            // all, or made by the project's own checkout command, holds every
            // repository it should and has nowhere for a plan to land
            // (§FS-004-quick-actions.7.1). Asking again is what repairs it.
            let store = init_store(&work, placement, project, &target);
            return Ok((
                Made {
                    target: target.clone(),
                    already: true,
                    missing,
                    outcome: None,
                    store: Some(store),
                },
                target,
            ));
        }
    }

    // A working tree is added from a repository, so one has to be on disk —
    // and not this one: a half-made workspace cannot supply the repositories it
    // is itself missing, and taking it as the source would answer *no
    // repository at* per repository instead of saying there is nothing to grow
    // them from.
    let source = placement
        .source_checkout()
        .filter(|source| source != &target)
        .ok_or_else(|| {
            EphorError::Command(format!(
                "{project} has no checkout on disk to make {} from — clone the project first.",
                target.display()
            ))
        })?;

    // The shape of the workspace being made is the shape of the one it is made
    // from: the declared forest where the row declares one, the source
    // checkout's own repositories otherwise (§AR-004-forest.1).
    let forest = placement.forest(&source);
    let base = match from
        .map(str::to_string)
        .or_else(|| placement.main_branch.clone())
    {
        Some(base) => base,
        None => forest
            .repos
            .first()
            .and_then(|repo| git::default_base(&repo.path, &repo.remote))
            .ok_or_else(|| {
                EphorError::Command(format!(
                    "Nothing says what to grow {branch} from — pass --from, or give \
                     {project} a main_branch in the registry."
                ))
            })?,
    };

    let outcome = git::create(&source, &target, &forest, branch, &base);
    // The store goes in only where the tree it belongs to is whole: a half-made
    // workspace is refused by the caller, and a work root inside one would be a
    // place for plans that cannot be worked (§FS-006-project-interface.7).
    let store = (outcome.refused().is_empty() && !outcome.repos.is_empty())
        .then(|| init_store(&work, placement, project, &target));
    Ok((
        Made {
            target,
            already: false,
            missing,
            outcome: Some(outcome),
            store,
        },
        source,
    ))
}

pub fn checkout(args: &CheckoutArgs) -> Result<ExitCode> {
    // Every value this command takes is honoured or refused naming the input it
    // came in on, before the first of them is acted on
    // (§FS-011-command-line.9). What the reader passed decides the answer, so
    // one they passed and ephor cannot use may not become one they did not.
    let item = match given::value(&args.item, "ITEM")? {
        Some(id) => Some(find_item(&id)?),
        None => None,
    };
    let project = given::value(&args.project, "PROJECT")?
        .or_else(|| item.as_ref().map(|item| item.project.clone()))
        .ok_or_else(|| {
            EphorError::Command(
                "Nothing says which project this branch belongs to — pass --project or --item."
                    .to_string(),
            )
        })?;

    let registry = crate::feed::commands::load_registry_doc()?;
    let placement = Placement::load(&registry, &project).ok_or_else(|| {
        EphorError::Command(format!(
            "{project}: no root in the registry, so there is nowhere to put a checkout."
        ))
    })?;

    let branch = given::branch(&args.branch, "BRANCH")?
        .or_else(|| item.as_ref().and_then(|item| placement.branch_name(item)))
        .ok_or_else(|| {
            EphorError::Command(
                "Nothing says which branch to check out — pass --branch, or --item for one \
                 the feed knows the branch of."
                    .to_string(),
            )
        })?;

    // Read before the making rather than where each is used: a value this
    // command will not act on is refused instead of the workspace being made
    // and the refusal arriving afterwards (§FS-004-quick-actions.7.3).
    let from = given::branch(&args.from, "FROM")?;
    let report = given::value(&args.report, "REPORT")?;

    // Everything below is reporting: what the workspace came to is the one
    // operation's answer, and this command is the reading of it
    // (§AR-009-surfaces.1).
    let (made, source) = make(&placement, &project, &branch, from.as_deref())?;
    if made.already {
        let summary = format!("{} is already checked out", made.target.display());
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "workspace": made.target,
                    "branch": branch,
                    "ready": true,
                    "summary": summary,
                    "repos": [],
                    "store": made.store.as_ref().map(Store::view),
                }))
                .unwrap_or_else(|_| "null".to_string())
            );
        } else {
            println!("{summary}.");
            if let Some(store) = &made.store {
                store.say();
            }
        }
        return Ok(ExitCode::SUCCESS);
    }
    let outcome = made.outcome.as_ref().expect("a workspace that was made");
    if !args.json && !made.missing.is_empty() {
        println!(
            "{} is missing {} — making {}.",
            made.target.display(),
            made.missing.join(", "),
            if made.missing.len() == 1 {
                "it"
            } else {
                "them"
            }
        );
    }
    if args.json {
        let mut view = outcome.view();
        if let Some(object) = view.as_object_mut() {
            object.insert(
                "report".to_string(),
                serde_json::Value::String(outcome.report()),
            );
            if let Some(store) = &made.store {
                object.insert("store".to_string(), store.view());
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&view).unwrap_or_else(|_| "null".to_string())
        );
    } else {
        print!("{}", outcome.report());
    }
    if let Some(path) = report {
        write_report(&path, &outcome.report())?;
    }

    if let Some(why) = made.refusal(&source) {
        // Nothing to make it from is this command refusing. Half a workspace is
        // not one either — whatever is missing, the next thing to run in here
        // would fail on it — but every repository has already been reported
        // above, so that one stops at the exit code.
        if outcome.repos.is_empty() {
            return Err(EphorError::Command(why));
        }
        return Ok(ExitCode::from(1));
    }
    if !args.json {
        println!("{}", outcome.summary());
        if let Some(store) = &made.store {
            store.say();
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// This project's work configuration, read once at the three tiers a work root
/// resolves through — the site's, its organization's, and its own
/// (§FS-005-dispatch.6.1).
///
/// The checkout asks for it twice and must get one answer both times: before
/// anything is made, to know whether the branch name lands on the work root
/// (§FS-004-quick-actions.7.3), and after, to put the store there
/// (§FS-006-project-interface.7). Two readings of one configuration would
/// eventually name two directories, and the second of them would be where the
/// plans went.
struct Work<'a> {
    /// None where no site configuration can be read: the shipped default
    /// answers then, which is `ephor checkout` working on a machine that has a
    /// registry and nothing else.
    config: Option<crate::feed::config::StatusConfig>,
    placement: &'a Placement,
    project: &'a str,
}

impl<'a> Work<'a> {
    fn read(placement: &'a Placement, project: &'a str) -> Work<'a> {
        Work {
            config: load_config().ok(),
            placement,
            project,
        }
    }

    fn global(&self) -> crate::work::recipe::WorkConfig {
        self.config
            .as_ref()
            .map(|config| config.work.clone())
            .unwrap_or_default()
    }

    fn per_project(&self) -> Option<&crate::work::recipe::ProjectWorkConfig> {
        self.config
            .as_ref()
            .and_then(|config| config.projects.get(self.project))
            .map(|project| &project.work)
    }

    /// The organization tier, read through the membership the registry declares
    /// (§FS-005-dispatch.6.1) — the same resolution dispatch makes, because the
    /// work root is one answer for both.
    fn per_organization(&self) -> Option<&crate::work::recipe::OrganizationWorkConfig> {
        let placed_in = self.placement.organization.as_ref()?;
        self.config
            .as_ref()?
            .organizations
            .get(&placed_in.id)
            .map(|organization| &organization.work)
    }

    /// Where this project's work goes, as the template says it.
    fn root(&self) -> String {
        crate::work::root_template(&self.global(), self.per_organization(), self.per_project())
    }
}

/// A workspace ephor makes gets a task store, so the first dispatch into
/// this branch has somewhere to land and what is under way is visible from
/// the moment the tree exists (§FS-006-project-interface.7). A workspace that
/// was already there is owed it just the same: *already checked out* answers
/// the question about repositories, not the one about work
/// (§FS-004-quick-actions.7.1).
///
/// Reported and never fatal: the workspace is made either way, and a checkout
/// that failed because a convenience did is a checkout that did not need to
/// fail. Where no site configuration can be read, the shipped default answers
/// — this is `ephor checkout` working on a machine that has a registry and
/// nothing else.
fn init_store(
    work: &Work,
    placement: &Placement,
    project: &str,
    workspace: &std::path::Path,
) -> Store {
    match crate::work::ensure_store(
        &work.global(),
        work.per_organization(),
        work.per_project(),
        project,
        placement.organization.as_ref(),
        workspace,
        &placement.root,
    ) {
        Ok(store) => Store {
            dir: Some(store.dir),
            made: store.made,
            note: store.note,
        },
        Err(err) => Store {
            dir: None,
            made: false,
            note: Some(format!("no task store was made — {err}")),
        },
    }
}

/// What the store came to, in the shape both answers need: the reading prints
/// it and `--json` carries it, so a runtime is told what a reader is told
/// (§REQ-002-parity.3).
pub struct Store {
    /// None where none could be made; the note says why.
    pub dir: Option<PathBuf>,
    pub made: bool,
    pub note: Option<String>,
}

impl Store {
    /// The line worth printing, where there is one: what was made now, and
    /// whatever could not be. A store that was already there says nothing —
    /// the reader asked for a checkout and it changed nothing.
    pub fn say(&self) {
        if let (true, Some(dir)) = (self.made, &self.dir) {
            println!("  task store at {}", dir.display());
        }
        if let Some(note) = &self.note {
            eprintln!("note: {note}");
        }
    }

    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({
            "dir": self.dir,
            "made": self.made,
            "note": self.note,
        })
    }
}

fn write_report(path: &str, contents: &str) -> Result<()> {
    let path = PathBuf::from(crate::paths::resolve_path(path));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            EphorError::Command(format!("Cannot create {}: {err}", parent.display()))
        })?;
    }
    std::fs::write(&path, contents)
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", path.display())))
}

/// The item by its feed id, out of whatever the last refresh cached.
fn find_item(id: &str) -> Result<Item> {
    let config = load_config()?;
    for project in config.projects.keys() {
        let Some(feed) = cache::load_feed(project)? else {
            continue;
        };
        let found = feed.items().find(|item| item.id == id);
        if let Some(item) = found {
            return Ok(item);
        }
    }
    Err(EphorError::Command(format!(
        "{id} is not in any cached feed — run `ephor refresh` first."
    )))
}
