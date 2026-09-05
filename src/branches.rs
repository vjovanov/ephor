//! Which registry branch a feed item belongs to, and where that branch is
//! checked out.
//!
//! One item is one piece of work on one branch, and both halves of ephor need
//! to agree about which: the inbox groups items under the branch they belong
//! to, and dispatch writes a ticket into the checkout that branch resolves to
//! (§FS-005-dispatch.3). Two answers to that question would put the ticket
//! somewhere other than where the reader was told the work was.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::feed::model::Item;
use crate::forest::{Declaration, Forest};
use crate::registry;

/// A branch of a project: one the registry row declares, or one found checked
/// out under the workspace base (§FS-008-attribution.2).
#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub branch: String,
    pub ticket: Option<String>,
    pub active: bool,
    pub is_release: bool,
    /// Whether the row names this branch. A discovered one is placed and
    /// worked like any other, but it may not widen the project's identity —
    /// a checkout must not be able to claim another project's conversations
    /// (§FS-008-attribution.1).
    pub declared: bool,
}

/// Whether an item is work on this branch: by ticket key, by the branch name
/// the provider recorded, or by the branch name appearing in the title.
pub fn matches(item: &Item, branch: &BranchInfo) -> bool {
    // The provider-recorded branch is ground truth and is not a match to be
    // argued about: the forge said which branch this is on.
    if item.raw.get("branch").and_then(Value::as_str) == Some(branch.branch.as_str())
        && !branch.branch.is_empty()
    {
        return true;
    }
    // Everything else is the one matching engine at its second scope
    // (§AR-003-attribution.3): the same evidence, the project's branches as
    // the identity table. [`place`] is what the surfaces ask — it puts the
    // whole table in front of the engine at once, which is both cheaper and
    // the engine's own ranking rather than list order.
    crate::attribution::branch(
        &evidence(item),
        &[(branch.branch.clone(), branch.ticket.clone())],
    )
    .is_some()
}

/// Which of a project's branches an item belongs to, by index. The branch the
/// provider recorded is taken before any that merely resembles it: two branch
/// names can carry one ticket key — `you/ABC-42` and `you/ABC-42-retry` are
/// both real trees on disk — and resemblance must not take an item off the
/// branch the forge named (§FS-008-attribution.3).
///
/// One answer for one item, so the group a row is filed under and the count
/// the branch above it shows cannot disagree.
///
/// The engine is asked once for the whole table rather than once per branch.
/// An item's evidence is its entire recorded conversation joined into one
/// string, so building it per branch is the same answer at N times the price —
/// and N is now every workspace on disk, not the handful somebody wrote down.
pub fn place(item: &Item, branches: &[BranchInfo]) -> Option<usize> {
    let recorded = item
        .raw
        .get("branch")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty());
    if let Some(name) = recorded {
        if let Some(exact) = branches.iter().position(|branch| branch.branch == name) {
            return Some(exact);
        }
    }
    let table: Vec<(String, Option<String>)> = branches
        .iter()
        .map(|branch| (branch.branch.clone(), branch.ticket.clone()))
        .collect();
    let chosen = crate::attribution::branch(&evidence(item), &table)?;
    branches.iter().position(|branch| branch.branch == chosen)
}

/// What the matching engine is given about an item, at the branch scope
/// (§AR-003-attribution.3): everything the matter carries, plus the ticket the
/// id names for sources that put it there and nowhere else.
fn evidence(item: &Item) -> crate::attribution::Evidence {
    let mut evidence = crate::matter::evidence_of(item);
    evidence
        .tickets
        .extend(crate::ticket_ids::tickets_in(&item.id));
    evidence
}

/// A branch name's workspace directory per the project's
/// `branch_root_template`, whether or not it exists. None when the project has
/// no branch workspaces — the root is the checkout then.
pub fn expand_workspace(template: Option<&str>, root: &Path, branch: &str) -> Option<PathBuf> {
    Some(PathBuf::from(
        template?
            .replace("{project_root}", &root.to_string_lossy())
            .replace("{branch}", branch),
    ))
}

/// A name no branch has, so the part of an expanded template that came from
/// `{branch}` is identifiable however many path separators the branch itself
/// contains.
const MARK: &str = "\u{1}workspace\u{1}";

/// What the project type declares about the repositories of a checkout
/// (§AR-004-forest.2). A repository the type says to skip is not tracking the
/// branch, so it is not part of the forest a branch workspace folds over.
fn declarations(registry_doc: &Value, project: &Value) -> Vec<Declaration> {
    let Some(type_id) = registry::str_field(project, "type") else {
        return Vec::new();
    };
    let Ok(project_type) = registry::get_project_type(registry_doc, type_id) else {
        return Vec::new();
    };
    registry::array_field(project_type, "repos")
        .iter()
        .filter(|repo| repo.get("update_mode").and_then(Value::as_str) != Some("skip"))
        .filter_map(|repo| {
            Some(Declaration {
                path: registry::str_field(repo, "path")?.to_string(),
                role: registry::str_field(repo, "role").map(String::from),
                main: registry::str_field(repo, "default_branch").map(String::from),
            })
        })
        .collect()
}

/// A row's list-of-strings field, empty where it says nothing.
fn strings(entry: &Value, field: &str) -> Vec<String> {
    entry
        .get(field)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Where an item's branch workspace stands, for gating work on it.
#[derive(Clone, Debug)]
pub enum WorkspaceState {
    /// The workspace exists (or the project root is the checkout).
    Ready,
    /// The project defines branch workspaces and this one is missing;
    /// carries the directory a checkout must create.
    Missing(PathBuf),
    /// The project keeps one checkout for every branch and it is standing on
    /// another one; carries the branch it is actually on
    /// (§FS-005-dispatch.3). Unlike [`Missing`](Self::Missing) there is no
    /// checkout to offer — the directory is right there, holding different
    /// code.
    Elsewhere(String),
    /// The item is not linked to any registry branch.
    Unmatched,
}

/// One project's placement data: where it is checked out, how its branch
/// workspaces are named, and which branches it has.
#[derive(Clone, Debug, Default)]
pub struct Placement {
    pub project: String,
    pub root: PathBuf,
    pub template: Option<String>,
    /// The row's branches first, then every workspace found on disk that the
    /// row does not already name (§FS-008-attribution.2). Read the `declared`
    /// flag rather than the position for anything that must not treat the two
    /// alike.
    pub branches: Vec<BranchInfo>,
    /// What branches here are measured against, and replayed onto
    /// (§FS-004-quick-actions.6).
    pub main_branch: Option<String>,
    /// What the project type declares about the checkout's repositories. Empty
    /// where the registry does not say, which leaves the forest to be probed
    /// on disk instead (§AR-004-forest.2).
    pub repos: Vec<Declaration>,
    /// Names the project answers to (§FS-008-attribution.1).
    pub aliases: Vec<String>,
    /// Repositories and organizations that are its business without being in
    /// its forest — what places a mention or an issue filed in its ecosystem
    /// (§FS-008-attribution.1).
    pub territory: Vec<String>,
    /// How much of the project's own manifest the row is willing to believe
    /// (§FS-006-project-interface.2).
    pub trust: crate::manifest::Trust,
    /// The organization the registry row places this project in, where it
    /// names one (§FS-005-dispatch.6.1). A work root may reach above the
    /// project to it, so the answer belongs beside the project's own place
    /// rather than being read again wherever a template is rendered.
    pub organization: Option<Organization>,
}

/// An organization a project belongs to, as the registry declares it: its id,
/// and where it is rooted (§FS-005-dispatch.6.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Organization {
    pub id: String,
    /// The organization's `root`, resolved the way every registry path is.
    /// None where the organization declares none — which a placement reaching
    /// above the project refuses on by name, rather than rendering the gap
    /// into a path.
    pub root: Option<PathBuf>,
}

/// The placeholders a `branch` template may not name, because they are what
/// it decides (§FS-005-dispatch.25). Rendering one of them would answer the
/// template's own question with the answer it has not given yet.
const DECIDED: [&str; 3] = ["branch", "workspace", "reply"];

/// Why an entry's branch template does not serve this matter, where it names a
/// real field the matter has empty (§FS-005-dispatch.25). None where the
/// matter already owns a branch — the template does not apply then — or where
/// every field it names has a value. Unknown and decided fields are author
/// errors for [`minted`] to refuse on every matter, not reasons to withhold an
/// entry from only this one.
pub fn why_not_served(placement: &Placement, item: &Item, template: &str) -> Option<String> {
    if placement.own_branch(item).is_some() {
        return None;
    }
    let values = placeholder_values(placement, item);
    missing_field(template, &values).map(|name| not_served_reason(template, &name))
}

/// What an entry's `branch` template comes to on one matter
/// (§FS-005-dispatch.25): the branch it names, and where that branch's
/// workspace stands — [`WorkspaceState::Ready`] where it is already on disk,
/// [`WorkspaceState::Missing`] where dispatch would make it.
///
/// The rendering *is* the resolution and nothing is written down, so asking
/// again about the same matter gives the same branch and the same directory.
/// The caller applies it only where the matter has no branch of its own: the
/// forge's answer is never displaced by a template ([`Placement::checkout`]).
///
/// Refused rather than rendered where the template names what it produces or
/// where the project keeps no workspace per branch to put it in. An entry
/// naming a field this matter has not got does not serve it and is normally
/// withheld before this point; direct resolution keeps the same answer rather
/// than rendering a branch with a hole in it (§FS-005-dispatch.25).
pub fn minted(
    placement: &Placement,
    item: &Item,
    template: &str,
) -> std::result::Result<Checkout, String> {
    let values = placeholder_values(placement, item);
    if let Some(name) = missing_field(template, &values) {
        return Err(not_served_reason(template, &name));
    }
    for name in crate::work::dossier::named(template) {
        if DECIDED.contains(&name.as_str()) {
            return Err(format!(
                "the branch template '{template}' names {{{name}}}, which is what it decides: a \
                 branch template is rendered from the matter's own fields — {{number}}, {{repo}}, \
                 {{kind}}, {{title}} — and never from the branch, the workspace or the reply that \
                 follow from it."
            ));
        }
        // A name the vocabulary has not got at all: `render` would leave it
        // standing and the branch would be refused for holding a brace, which
        // points at the rendering rather than at the typo (§FS-005-dispatch.25).
        if !values.contains_key(name.as_str()) {
            return Err(format!(
                "the branch template '{template}' names {{{name}}}, which is not a field a branch \
                 template may name; it may name: {}.",
                nameable(&values).join(", ")
            ));
        }
    }
    let branch = crate::work::dossier::render(template, &values)
        .trim()
        .to_string();
    if !crate::forest::is_branch_name(&branch) {
        return Err(format!(
            "the branch template '{template}' rendered '{branch}', which is not a branch name."
        ));
    }
    if let Some(why) = why_git_refuses(&branch) {
        return Err(format!(
            "the branch template '{template}' rendered '{branch}', which git will not take as a \
             branch name: {why}."
        ));
    }
    let target = placement.workspace_for(&branch).ok_or_else(|| {
        format!(
            "{project} does not use a checkout per branch (no branch_root_template), so there is \
             nowhere to put {branch} — give '{project}' a branch_root_template in the registry, so \
             its branches get workspaces of their own.",
            project = placement.project
        )
    })?;
    let state = match target.is_dir() {
        true => WorkspaceState::Ready,
        false => WorkspaceState::Missing(target.clone()),
    };
    Ok(Checkout {
        workspace: target,
        ticket: Some(crate::ticket_ids::extract_ticket(&branch)).filter(|key| !key.is_empty()),
        branch: Some(branch),
        state,
    })
}

/// The one placeholder vocabulary branch rendering and serving both read.
fn placeholder_values(
    placement: &Placement,
    item: &Item,
) -> std::collections::BTreeMap<&'static str, String> {
    let here = placement.checkout(item);
    crate::work::dossier::Subject {
        item,
        checkout: &here,
        root: &placement.root,
        organization: placement.organization.as_ref(),
    }
    .placeholders()
}

/// The first real field a template names that this matter has empty. Author
/// errors are deliberately absent: they are refused wherever they are read.
fn missing_field(
    template: &str,
    values: &std::collections::BTreeMap<&'static str, String>,
) -> Option<String> {
    let names = crate::work::dossier::named(template);
    // A template with an author error must stay visible and refuse loudly,
    // even where another name in it is a real field this matter lacks.
    if names
        .iter()
        .any(|name| DECIDED.contains(&name.as_str()) || !values.contains_key(name.as_str()))
    {
        return None;
    }
    let missing: Vec<_> = names
        .into_iter()
        .filter(|name| values[name.as_str()].is_empty())
        .collect();
    let first = missing.first()?.clone();

    // Fill only the absent fields with a Git-safe value. If the rest of the
    // rendering is already invalid, that is still an author error to refuse,
    // not a matter-specific reason to withhold the entry.
    let mut complete = values.clone();
    for name in missing {
        if let Some(value) = complete.get_mut(name.as_str()) {
            *value = "field".to_string();
        }
    }
    let branch = crate::work::dossier::render(template, &complete)
        .trim()
        .to_string();
    if !crate::forest::is_branch_name(&branch) || why_git_refuses(&branch).is_some() {
        return None;
    }
    Some(first)
}

fn not_served_reason(template: &str, name: &str) -> String {
    format!("the branch template '{template}' needs {{{name}}}, and this matter has none")
}

/// The fields a branch template may name, spelled as it would name them: every
/// placeholder a matter carries but the three the template decides
/// (§FS-005-dispatch.25). What a refusal offers instead of the name it refused.
fn nameable(values: &std::collections::BTreeMap<&'static str, String>) -> Vec<String> {
    values
        .keys()
        .filter(|name| !DECIDED.contains(name))
        .map(|name| format!("{{{name}}}"))
        .collect()
}

/// Why git will not take `branch` as a branch name, as a clause a refusal can
/// carry — None where it will (§FS-005-dispatch.25). These are the rules
/// `git check-ref-format --branch` applies, asked here rather than left to the
/// checkout: git's own answer comes from inside the making, by which time the
/// directories leading to the workspace are already there, so an author's bad
/// template would be reported and still leave part of a tree behind.
pub(crate) fn why_git_refuses(branch: &str) -> Option<String> {
    /// What git refuses anywhere in a ref, beyond the control characters.
    const FORBIDDEN: [char; 9] = [' ', '~', '^', ':', '?', '*', '[', '\\', '\u{7f}'];
    if branch.is_empty() {
        return Some("it is empty".to_string());
    }
    if branch == "HEAD" {
        return Some("'HEAD' is not a branch name".to_string());
    }
    if branch.starts_with('-') {
        return Some("it begins with '-'".to_string());
    }
    if branch.ends_with('.') {
        return Some("it ends with '.'".to_string());
    }
    if branch.contains("..") {
        return Some("it holds '..'".to_string());
    }
    if branch.contains("@{") {
        return Some("it holds '@{'".to_string());
    }
    if let Some(bad) = branch
        .chars()
        .find(|held| FORBIDDEN.contains(held) || held.is_ascii_control())
    {
        return Some(format!("it holds {bad:?}"));
    }
    for part in branch.split('/') {
        if part.is_empty() {
            return Some("one of its path components is empty".to_string());
        }
        if part.starts_with('.') {
            return Some(format!("its component '{part}' begins with '.'"));
        }
        if part.ends_with(".lock") {
            return Some(format!("its component '{part}' ends with '.lock'"));
        }
    }
    None
}

/// Why the workspace this branch renders to is not a place to make one, as a
/// clause a refusal can carry — None where it is (§FS-004-quick-actions.7.3).
///
/// The two questions the checkout has to settle before it makes anything: does
/// the rendered path stay in the area this project's own template puts branch
/// workspaces in, and does it land on the project's work root or inside it.
/// `work_root` is that project's resolved work-root template
/// ([`crate::work::root_template`]), rendered here against the candidate
/// workspace rather than read from disk, because neither directory exists yet.
///
/// Standing apart from [`expand_workspace`] and [`Placement::workspace_for`] on
/// purpose: those two answer *where an already known branch's workspace is* for
/// every surface that asks, and are called with a sentinel that is deliberately
/// not a branch name, so a refusal inside them would silently stop branch
/// discovery finding workspaces on disk.
pub fn why_the_workspace_is_refused(
    placement: &Placement,
    branch: &str,
    work_root: &str,
) -> Option<String> {
    // A project whose checkout is its root has no workspace to place and no
    // area to leave; where the branch belongs is the caller's own refusal.
    let target = lexical(&placement.workspace_for(branch)?);
    let base = lexical(&placement.workspace_base()?);
    if !target.starts_with(&base) {
        return Some(format!(
            "'{branch}' renders to {}, which is not inside {} — the area {} puts its branch \
             workspaces in.",
            target.display(),
            base.display(),
            placement.project
        ));
    }

    // Where this project's work goes, rendered against the workspace this
    // branch would be: the same values the store is made with, so the two
    // cannot disagree about the directory (§FS-006-project-interface.7).
    let mut values = std::collections::BTreeMap::from([
        ("workspace", target.to_string_lossy().into_owned()),
        ("root", placement.root.to_string_lossy().into_owned()),
        ("project", placement.project.clone()),
    ]);
    if let Some(organization) = &placement.organization {
        values.insert("org", organization.id.clone());
        if let Some(root) = &organization.root {
            values.insert("org_root", root.to_string_lossy().into_owned());
        }
    }
    let rendered = crate::work::dossier::render(work_root, &values);
    // A template naming a field only a matter can fill describes no fixed
    // place, so there is nothing here for a branch to land on. Passed over
    // rather than guessed at, exactly as the board's walk passes it over
    // (§FS-005-dispatch.15.1).
    if rendered.contains('{') {
        return None;
    }
    let root = lexical(&crate::paths::resolve_path(&rendered));
    // A work root that holds the whole workspace area refuses every branch, so
    // the branch is not what is wrong: naming it would send the reader off to
    // rename a branch when what has to move is the project's `work.root`. Said
    // before either collision below, which are about one name landing on the
    // work root rather than about none being able to miss it.
    if base.starts_with(&root) {
        return Some(format!(
            "{}'s work root {} covers the whole area it puts its branch workspaces in, so \
             every branch renders inside the work root and none can miss it — it is the \
             work root that has to move, not '{branch}'.",
            placement.project,
            root.display()
        ));
    }
    if target == root {
        return Some(format!(
            "'{branch}' is {}'s work root, not a branch: {} is where its work goes, and a \
             workspace made there would be a checkout and a work root in one directory.",
            placement.project,
            root.display()
        ));
    }
    if target.starts_with(&root) {
        return Some(format!(
            "'{branch}' renders inside {}'s work root {}, where a workspace would sit among \
             the plans of every branch rather than beside them.",
            placement.project,
            root.display()
        ));
    }
    None
}

/// A path with its `.` and `..` folded out, without asking the disk: the
/// workspace this is about does not exist yet, so there is nothing to
/// canonicalize and the question is what the name *says*
/// (§FS-004-quick-actions.7.3).
///
/// A `..` with nothing left to climb is kept rather than dropped, so a name
/// that reaches above whatever it is compared against stays outside it.
fn lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut folded = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !folded.pop() {
                    folded.push("..");
                }
            }
            other => folded.push(other.as_os_str()),
        }
    }
    folded
}

/// Where an entry's work resolves, with no refusal to answer for: the matter's
/// own checkout where the forge or the registry gave it a branch, the workspace
/// a `branch` template names where it did not, and the matter's own again where
/// that template will not render (§FS-005-dispatch.25).
///
/// [`minted`] is the answer that decides; this is the one every surface asking
/// *where would this work go* reads, so the root a hand is resolved against and
/// the root the dispatch writes into cannot be two different directories. A
/// template that will not render is reported where it is read — carried onto
/// the entry as [`crate::feed::config::Minted::Refused`], which blocks it — and
/// not by moving the work somewhere else.
pub fn placed_through(placement: &Placement, item: &Item, branch: Option<&str>) -> Checkout {
    let here = placement.own_checkout(item);
    if here.branch.is_some() {
        return here;
    }
    match branch {
        Some(template) => minted(placement, item, template).unwrap_or(here),
        None => here,
    }
}

/// Where one item's work belongs.
#[derive(Clone, Debug)]
pub struct Checkout {
    /// The directory a command about this item runs in: the branch workspace
    /// when it exists, otherwise the project root. A workspace a `branch`
    /// template named is this even before it is made ([`minted`]): dispatch
    /// makes it, and everything the work is told about where it is has to
    /// agree with where the work will actually be (§FS-005-dispatch.25).
    pub workspace: PathBuf,
    /// The branch name — the provider-recorded one, or the matched registry
    /// branch's. From [`Placement::checkout`] this includes the project's
    /// main branch, because it answers where the matter's code lives right
    /// now; from [`Placement::own_checkout`] it does not, because that
    /// answers which branch the matter owns for placement
    /// (§FS-005-dispatch.25).
    pub branch: Option<String>,
    /// The registry branch the item matched, and its ticket key.
    pub ticket: Option<String>,
    pub state: WorkspaceState,
}

impl Placement {
    /// Read one project's placement out of the registry document. None when
    /// the project is not in the registry or declares no root — either way
    /// there is nowhere to put work.
    pub fn load(registry_doc: &Value, project: &str) -> Option<Placement> {
        let entry = registry::array_field(registry_doc, "projects")
            .into_iter()
            .find(|candidate| registry::id_of(candidate) == project)?;
        let root = crate::paths::resolve_path(registry::str_field(entry, "root")?);
        let template = registry::str_field(entry, "branch_root_template").map(String::from);

        let mut branches = Vec::new();
        for (section, is_release) in [("release_branches", true), ("branches", false)] {
            for branch_entry in registry::branch_entries(entry, section) {
                let branch = registry::str_field(branch_entry, "branch")
                    .unwrap_or("")
                    .to_string();
                let ticket = registry::str_field(branch_entry, "ticket")
                    .map(String::from)
                    .or_else(|| {
                        let extracted = crate::ticket_ids::extract_ticket(&branch);
                        (!extracted.is_empty()).then_some(extracted)
                    });
                branches.push(BranchInfo {
                    branch,
                    ticket,
                    active: branch_entry
                        .get("active")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    is_release,
                    declared: true,
                });
            }
        }
        let organization = registry::organization_of(registry_doc, project)
            .map(|(id, root)| Organization { id, root });
        let mut placement = Placement {
            project: project.to_string(),
            root,
            template,
            organization,
            branches,
            main_branch: registry::str_field(entry, "main_branch").map(String::from),
            repos: declarations(registry_doc, entry),
            aliases: strings(entry, "aliases"),
            territory: strings(entry, "territory"),
            trust: registry::str_field(entry, "manifest_trust")
                .map(crate::manifest::Trust::parse)
                .transpose()
                .ok()
                .flatten()
                .unwrap_or_default(),
        };
        // What the row wrote down, then what is actually on disk. The row
        // first, so a branch it describes keeps its ticket and its `active`
        // flag rather than the bare facts a directory can offer
        // (§FS-008-attribution.2).
        let named: Vec<String> = placement
            .branches
            .iter()
            .map(|branch| branch.branch.clone())
            .collect();
        let discovered: Vec<BranchInfo> = placement
            .discovered_branches()
            .into_iter()
            .filter(|found| !named.contains(&found.branch))
            .collect();
        placement.branches.extend(discovered);
        Some(placement)
    }

    /// The branches this project has a workspace for on disk, whether or not
    /// the row names them (§FS-008-attribution.2). Empty for a project whose
    /// checkout is its root — there are no branch workspaces to find.
    ///
    /// A branch is named by the directory it was found in, not by what that
    /// checkout has at `HEAD`: [`Placement::workspace_for`] has to lead back to
    /// the directory the branch was discovered in, and a tree at `…/GR-1`
    /// holding `you/GR-1-retry` would otherwise resolve forward to a directory
    /// it is not in.
    fn discovered_branches(&self) -> Vec<BranchInfo> {
        let Some(base) = self.workspace_base() else {
            return Vec::new();
        };
        // The template split at `{branch}`: what a workspace path starts with,
        // and what it ends with. Whatever a found directory has between them
        // is the branch name that expands back to it.
        let expanded = self.workspace_for(MARK).unwrap_or_default();
        let expanded = expanded.to_string_lossy().into_owned();
        let Some((prefix, suffix)) = expanded.split_once(MARK) else {
            return Vec::new();
        };
        let layout = self.layout();
        let mut found = Vec::new();
        collect_workspaces(&base, BRANCH_DEPTH, &layout, &mut found);
        found.sort();
        found.dedup();
        found
            .iter()
            .filter_map(|path| {
                let path = path.to_string_lossy();
                let branch = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
                (!branch.is_empty()).then(|| BranchInfo {
                    branch: branch.to_string(),
                    ticket: {
                        let ticket = crate::ticket_ids::extract_ticket(branch);
                        (!ticket.is_empty()).then_some(ticket)
                    },
                    // Nobody wrote this one down, so nobody said it was what
                    // they are working on; it is a place, not a plan.
                    active: false,
                    is_release: false,
                    declared: false,
                })
            })
            .collect()
    }

    /// The repository paths a checkout of this project holds: the row's layout
    /// where it declares one, the manifest's where it does not
    /// (§AR-004-forest.2). Empty where neither says, which leaves a checkout to
    /// be recognized by holding a repository at all.
    fn layout(&self) -> Vec<String> {
        if !self.repos.is_empty() {
            return self.repos.iter().map(|repo| repo.path.clone()).collect();
        }
        self.manifest()
            .map(|manifest| {
                manifest
                    .forest
                    .iter()
                    .map(|repo| repo.path.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The signals by which this project's matters are recognized
    /// (§FS-008-attribution.1), compiled from the row. The row has the last
    /// word: a checkout must not be able to claim another project's
    /// conversations.
    /// What the project says about itself, where it says anything and the row
    /// lets it be read (§FS-006-project-interface.2). Errors are not fatal
    /// here: a manifest ephor cannot read must not stop the project being
    /// watched, which is the whole of §DF-001-manifest-offered.
    pub fn manifest(&self) -> Option<crate::manifest::Manifest> {
        crate::manifest::Manifest::read(&self.root, self.trust)
            .ok()
            .flatten()
    }

    pub fn identity(&self) -> crate::attribution::Identity {
        let manifest = self.manifest();
        let hint = |pick: fn(&crate::manifest::Identity) -> &Vec<String>| -> Vec<String> {
            manifest
                .as_ref()
                .map(|manifest| pick(&manifest.identity).clone())
                .unwrap_or_default()
        };
        // The row adopts a hint where it says nothing of its own, and
        // overrides it where it does: a checkout must not be able to claim
        // another project's conversations (§FS-008-attribution.1).
        let adopt =
            |own: Vec<String>, hinted: Vec<String>| if own.is_empty() { hinted } else { own };
        crate::attribution::Identity {
            project: self.project.clone(),
            // The prefixes its branches' ticket keys carry — what the row
            // already says about which tickets are this project's. Declared
            // branches only: a stray checkout under the workspace base would
            // otherwise teach this project a whole ticket prefix, which is
            // exactly the claim §FS-008-attribution.1 reserves to the row.
            tickets: self
                .branches
                .iter()
                .filter(|branch| branch.declared)
                .filter_map(|branch| branch.ticket.as_deref())
                .filter_map(|ticket| ticket.split_once('-').map(|(prefix, _)| prefix.to_string()))
                .fold(Vec::new(), |mut prefixes, prefix| {
                    if !prefixes.contains(&prefix) {
                        prefixes.push(prefix);
                    }
                    prefixes
                }),
            repos: adopt(
                self.repos.iter().map(|repo| repo.path.clone()).collect(),
                hint(|identity| &identity.repos),
            ),
            territory: adopt(self.territory.clone(), hint(|identity| &identity.territory)),
            aliases: adopt(self.aliases.clone(), hint(|identity| &identity.aliases)),
            addresses: hint(|identity| &identity.addresses),
        }
    }

    /// This project's forest at `checkout` — the thing every git-facing
    /// feature folds over (§AR-004-forest).
    pub fn forest(&self, checkout: &Path) -> Forest {
        // The row's layout where it declares one, the manifest's where it does
        // not, probing where neither says (§AR-004-forest lead,
        // §FS-006-project-interface.1).
        if !self.repos.is_empty() {
            return Forest::resolve(checkout, self.main_branch.as_deref(), &self.repos);
        }
        let declared: Vec<Declaration> = self
            .manifest()
            .map(|manifest| {
                manifest
                    .forest
                    .iter()
                    .map(|repo| Declaration {
                        path: repo.path.clone(),
                        role: repo.role.clone(),
                        main: repo.main.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Forest::resolve(checkout, self.main_branch.as_deref(), &declared)
    }

    /// The directory this project's workspace for `branch` belongs at, whether
    /// or not it is there yet.
    pub fn workspace_for(&self, branch: &str) -> Option<PathBuf> {
        expand_workspace(self.template.as_deref(), &self.root, branch)
    }

    /// The directories holding a task store for this project
    /// (§FS-006-project-interface.7): the forest root, and every branch
    /// workspace on disk that has one.
    ///
    /// Work about a change belongs in that change's working tree
    /// (§FS-005-dispatch.3), so a branch-addressable project keeps a store per
    /// workspace rather than one at the root — and looking only at the root
    /// left such a project writing its plans where nothing read them again.
    ///
    /// Verified on disk rather than derived from the registry: the row names
    /// the branches somebody wrote down, and the stores are wherever branches
    /// were actually checked out. A project whose row names five branches had
    /// nine workspaces holding work, none of them the five.
    pub fn task_stores(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        if holds_store(&self.root) {
            found.push(self.root.clone());
        }
        if let Some(base) = self.workspace_base() {
            collect_stores(&base, WORKSPACE_DEPTH, &mut found);
        }
        found.sort();
        found.dedup();
        found
    }

    /// Where this project's branch workspaces are rooted — the fixed part of
    /// the template, above wherever `{branch}` lands. None for a project whose
    /// checkout is its root.
    fn workspace_base(&self) -> Option<PathBuf> {
        let expanded = self.workspace_for(MARK)?;
        let mut base = expanded.as_path();
        while base.to_string_lossy().contains(MARK) {
            base = base.parent()?;
        }
        Some(base.to_path_buf())
    }
}

/// How far under the workspace base a store is looked for. A branch name may
/// carry separators — `you/ABC-42` is two components — so the store under it
/// is three deep, and stopping there keeps this a bounded walk of the
/// workspace area rather than a search of the repositories inside it
/// (§AR-005-capabilities.1).
const WORKSPACE_DEPTH: usize = 3;

/// How far under the workspace base a branch workspace is looked for. Same
/// bound as [`WORKSPACE_DEPTH`] and for the same reason — a branch name may
/// carry separators, so its directory is a few components down — but reached
/// one component sooner, since this walk stops at the workspace itself rather
/// than at a store inside it.
const BRANCH_DEPTH: usize = 3;

/// Whether a directory is a checkout of a project with this layout: it holds a
/// declared repository, or — where the layout is unknown — a repository at all,
/// the same shape [`crate::git::probe`] looks for.
///
/// Tested through `.git` rather than by asking git: discovery looks at every
/// directory in the workspace area on every load, and a process per directory
/// is a subprocess storm to answer what a path already answers
/// (§AR-004-forest.3).
///
/// An unknown layout cannot tell a workspace of one repository from the
/// directory above it — both have a git working tree directly under them. The
/// shallower one wins, because that is already the answer
/// [`crate::git::probe`] gives for the same tree, and one wrong answer beats
/// two that disagree. A declared layout — which every project with a registry
/// type has — has no such ambiguity.
fn holds_checkout(dir: &Path, layout: &[String]) -> bool {
    if !layout.is_empty() {
        return layout
            .iter()
            .any(|repo| is_repository(&crate::forest::under(dir, repo)));
    }
    is_repository(dir)
        || std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| is_repository(&entry.path()))
}

/// Whether a directory is a git working tree, by the marker it carries: a
/// directory for a clone, a file for a linked working tree.
fn is_repository(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Walk for branch workspaces, to a bounded depth. What is under a workspace
/// is its repositories, not more workspaces, so the walk stops descending the
/// moment it finds one.
fn collect_workspaces(dir: &Path, depth: usize, layout: &[String], found: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Nothing hidden is a branch workspace, and descending into `.git`
        // would be a walk of a repository rather than of the workspace area.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if holds_checkout(&path, layout) {
            found.push(path);
            continue;
        }
        collect_workspaces(&path, depth - 1, layout, found);
    }
}

/// Whether a directory holds a store ephor recognizes. The names are the
/// stores' own, so they are asked of the adapter rather than spelled here
/// (§REQ-001-boundary.5).
fn holds_store(dir: &Path) -> bool {
    crate::seams::tasks::Kind::all()
        .iter()
        .any(|kind| dir.join(kind.probed()).is_dir())
}

/// Walk for directories holding a store, to a bounded depth.
fn collect_stores(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // A store may be a dotted name of its own, but nothing else hidden is
        // a workspace — and descending into `.git` would be a walk of the
        // repository rather than of the workspace area.
        if name.starts_with('.') {
            continue;
        }
        if holds_store(&path) {
            found.push(path.clone());
        }
        collect_stores(&path, depth - 1, found);
    }
}

impl Placement {
    /// Where this project's checkouts would be, in the order one of them
    /// stands for the project: the main branch's workspace first — it is the
    /// one a question about the project rather than about a change is about,
    /// and the one a new branch is grown from — then every other branch's,
    /// then the root, which is the checkout itself for a project the registry
    /// gives no `branch_root_template`.
    ///
    /// A project may have several checkouts, and this is the single ladder
    /// that decides which one answers for it. Whether a candidate counts as
    /// being there is left to the caller: what has to be true of the directory
    /// depends on what is wanted from it.
    fn checkout_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(main) = self.main_branch.as_deref() {
            candidates.extend(self.workspace_for(main));
        }
        for branch in &self.branches {
            candidates.extend(self.workspace_for(&branch.branch));
        }
        candidates.push(self.root.clone());
        candidates
    }

    /// An existing checkout to make a new workspace from
    /// (§FS-004-quick-actions.7). A working tree is added *from* a repository,
    /// so the first candidate holding one is the answer — a directory with no
    /// repository under it is nothing to grow a working tree from.
    pub fn source_checkout(&self) -> Option<PathBuf> {
        self.checkout_candidates()
            .into_iter()
            .find(|path| !self.forest(path).is_empty())
    }

    /// The checkout that answers for this project when its own files are the
    /// question — where it keeps the toolchain's configuration
    /// (§FS-006-project-interface.12). The first candidate on disk, so a
    /// branch-addressable project answers with the main branch's workspace and
    /// never with the registry root above it: that root is the *container* the
    /// workspaces sit in, and the files are one level down in each of them.
    ///
    /// The same ladder as [`Placement::source_checkout`] behind a weaker
    /// filter, deliberately rather than by oversight: reading a file asks only
    /// that the directory be there, while adding a working tree asks for a
    /// repository to add it from. `None` where no candidate is on disk at all —
    /// a project with nothing placed has no checkout to read, which `doctor`
    /// already reports as the missing *placed* rung rather than as a sentence
    /// about the layout.
    ///
    /// The one place the two ladders part: a project the registry gives a
    /// `branch_root_template` does not fall back to its root, even where a
    /// repository of its own happens to sit there. The registry has said its
    /// checkouts live one level down, so what is at the container is not a
    /// checkout this project keeps, and reporting the container's files as the
    /// project's layout names a path no checkout maintains. `None` is the
    /// honest answer, and `doctor` already reports the branch workspaces as
    /// missing. [`Placement::source_checkout`] keeps the root candidate: a
    /// working tree may be added from any repository, wherever it stands
    /// (§FS-004-quick-actions.7).
    pub fn project_checkout(&self) -> Option<PathBuf> {
        self.checkout_candidates()
            .into_iter()
            .filter(|path| self.template.is_none() || path != &self.root)
            .find(|path| path.is_dir())
    }

    pub fn matched(&self, item: &Item) -> Option<&BranchInfo> {
        self.branches.get(place(item, &self.branches)?)
    }

    /// Whether `name` is this project's configured main branch — the trunk
    /// every workspace is grown from, never a matter's own to place through
    /// (§FS-005-dispatch.25).
    pub fn is_main_branch(&self, name: &str) -> bool {
        self.main_branch.as_deref() == Some(name)
    }

    /// The item's branch name: what the provider recorded (ground truth), or
    /// the matched registry branch's — the project's main branch included,
    /// because this answers where the matter's code lives right now, not
    /// which branch it owns for placement. [`Placement::own_branch`] answers
    /// that question instead (§FS-005-dispatch.25).
    pub fn branch_name(&self, item: &Item) -> Option<String> {
        item.raw
            .get("branch")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(String::from)
            .or_else(|| self.matched(item).map(|branch| branch.branch.clone()))
    }

    /// The branch this matter owns for placement: what the provider
    /// recorded (ground truth), or the matched registry branch's — except
    /// the project's main branch, which a registry match keeps only by
    /// resemblance and which no matter owns (§FS-005-dispatch.25). The
    /// forge-recorded branch is untouched even where it happens to equal the
    /// main branch: that is the forge's own fact, not a resemblance to argue
    /// with.
    ///
    /// This decides whether a `branch` template applies and whether editing
    /// work is refused — [`Placement::branch_name`] answers the different
    /// question of where the matter's code lives right now, main branch
    /// included, for read-only resolution and display.
    pub fn own_branch(&self, item: &Item) -> Option<String> {
        item.raw
            .get("branch")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(String::from)
            .or_else(|| {
                self.matched(item)
                    .map(|branch| branch.branch.clone())
                    .filter(|name| !self.is_main_branch(name))
            })
    }

    /// Where an item's code lives right now: the checkout its branch name
    /// resolves to, main branch included (§FS-005-dispatch.25). What
    /// read-only resolution and display use; [`Placement::own_checkout`]
    /// answers the placement question instead.
    pub fn checkout(&self, item: &Item) -> Checkout {
        self.checkout_of(item, self.branch_name(item))
    }

    /// Where an item's work belongs for placement: the checkout of the
    /// branch it owns, main branch excluded (§FS-005-dispatch.25). What the
    /// mint gate, the editing refusal, and the offers surface use.
    pub fn own_checkout(&self, item: &Item) -> Checkout {
        self.checkout_of(item, self.own_branch(item))
    }

    fn checkout_of(&self, item: &Item, branch: Option<String>) -> Checkout {
        let ticket = self
            .matched(item)
            .and_then(|matched| matched.ticket.clone())
            .or_else(|| branch.as_deref().map(crate::ticket_ids::extract_ticket))
            .filter(|ticket| !ticket.is_empty());
        let expanded = branch
            .as_deref()
            .and_then(|name| expand_workspace(self.template.as_deref(), &self.root, name));

        let (workspace, state) = match (&expanded, self.template.is_some()) {
            // A single-checkout project: the root is the workspace, and it is
            // this branch's working tree only while it is standing on the
            // branch (§FS-005-dispatch.3). Only a branch that can be read and
            // disagrees says so; an unreadable or detached HEAD leaves the
            // root as ready, because refusing on a fact nobody could establish
            // is worse than the ticket.
            (_, false) => {
                let state = match (&branch, crate::git::head_branch(&self.root)) {
                    (Some(wanted), Some(head)) if &head != wanted => {
                        WorkspaceState::Elsewhere(head)
                    }
                    _ => WorkspaceState::Ready,
                };
                (self.root.clone(), state)
            }
            (Some(target), true) if target.is_dir() => (target.clone(), WorkspaceState::Ready),
            (Some(target), true) => (self.root.clone(), WorkspaceState::Missing(target.clone())),
            (None, true) => (self.root.clone(), WorkspaceState::Unmatched),
        };
        Checkout {
            workspace,
            branch,
            ticket,
            state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::ItemKind;
    use serde_json::json;

    fn item(title: &str, raw: Value) -> Item {
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: title.to_string(),
            url: None,
            state: Some("open".to_string()),
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw,
        }
    }

    fn placement(root: &Path, template: Option<&str>) -> Placement {
        Placement {
            project: "widget".to_string(),
            root: root.to_path_buf(),
            template: template.map(String::from),
            branches: vec![BranchInfo {
                branch: "you/ABC-42-retry-window".to_string(),
                ticket: Some("ABC-42".to_string()),
                active: true,
                is_release: false,
                declared: true,
            }],
            main_branch: Some("main".to_string()),
            repos: Vec::new(),
            aliases: Vec::new(),
            territory: Vec::new(),
            trust: crate::manifest::Trust::Full,
            organization: None,
        }
    }

    /// A poly-repo workspace on disk: `ce` and `ee` under one directory, the
    /// shape a branch workspace of several repositories has.
    fn workspace_on_disk(base: &Path, branch: &str, repos: &[&str]) -> PathBuf {
        let dir = base.join(branch);
        for repo in repos {
            std::fs::create_dir_all(dir.join(repo).join(".git")).unwrap();
        }
        dir
    }

    fn poly_repo(root: &Path) -> Placement {
        let mut placement = placement(root, Some("{project_root}/{branch}"));
        placement.repos = vec![Declaration::at("ce"), Declaration::at("ee")];
        placement
    }

    #[test]
    fn a_workspace_on_disk_is_a_branch_the_row_never_named() {
        let tmp = tempfile::tempdir().unwrap();
        workspace_on_disk(tmp.path(), "main", &["ce", "ee"]);
        workspace_on_disk(tmp.path(), "you/ABC-7-tidy", &["ce"]);
        // Neither a workspace nor on the way to one.
        std::fs::create_dir_all(tmp.path().join("scratch/notes")).unwrap();

        let found: Vec<String> = poly_repo(tmp.path())
            .discovered_branches()
            .into_iter()
            .map(|branch| branch.branch)
            .collect();
        // `you` is the way to a workspace, not one; `main/ce` is a repository
        // inside one, and the walk stopped before it.
        assert_eq!(found, vec!["main", "you/ABC-7-tidy"]);
    }

    #[test]
    fn a_discovered_branch_resolves_back_to_the_directory_it_was_found_in() {
        let tmp = tempfile::tempdir().unwrap();
        // The tree holds a branch its directory is not named for. The branch
        // is the directory's, so that one workspace function keeps one answer.
        let dir = workspace_on_disk(tmp.path(), "you/ABC-7", &["ce"]);
        let placement = poly_repo(tmp.path());

        let found = placement.discovered_branches();
        let branch = found.first().expect("the workspace was found");
        assert_eq!(branch.branch, "you/ABC-7");
        assert_eq!(branch.ticket.as_deref(), Some("ABC-7"));
        assert!(!branch.declared);
        assert!(!branch.active);
        assert_eq!(placement.workspace_for(&branch.branch), Some(dir));
    }

    #[test]
    fn a_project_whose_checkout_is_its_root_has_nothing_to_discover() {
        let tmp = tempfile::tempdir().unwrap();
        workspace_on_disk(tmp.path(), "main", &["ce"]);
        let mut placement = poly_repo(tmp.path());
        placement.template = None;
        assert!(placement.discovered_branches().is_empty());
    }

    fn registry(root: &Path) -> Value {
        json!({
            "projects": [{
                "id": "widget",
                "root": root.to_string_lossy(),
                "branch_root_template": "{project_root}/{branch}",
                "main_branch": "main",
                "branches": [
                    { "id": "widget-retry", "branch": "you/ABC-42-retry-window",
                      "ticket": "ABC-42", "active": true }
                ]
            }]
        })
    }

    /// A work root may reach above the project, so the placement carries the
    /// organization the registry puts the project in and where that
    /// organization is rooted, resolved the way every registry path is
    /// (§FS-005-dispatch.6.1). A row naming no organization carries none, and
    /// an organization declaring no root carries the membership without one —
    /// the two gaps a placement above the project refuses on separately.
    #[test]
    fn the_placement_carries_the_organization_the_registry_puts_it_in() {
        let doc = json!({
            "organizations": [
                { "id": "foundation", "name": "Foundation", "root": "$HOME/f" },
                { "id": "personal", "name": "Personal" }
            ],
            "projects": [
                { "id": "widget", "root": "/w/widget", "organization": "foundation" },
                { "id": "gadget", "root": "/w/gadget", "organization": "personal" },
                { "id": "sprocket", "root": "/w/sprocket" }
            ]
        });
        let widget = Placement::load(&doc, "widget").expect("a row");
        let organization = widget.organization.expect("the row names one");
        assert_eq!(organization.id, "foundation");
        assert_eq!(
            organization.root,
            Some(crate::paths::resolve_path("$HOME/f")),
            "the organization's root is resolved, not carried raw"
        );

        let gadget = Placement::load(&doc, "gadget").expect("a row");
        let organization = gadget.organization.expect("the row names one");
        assert_eq!(organization.id, "personal");
        assert_eq!(organization.root, None, "it declares no root");

        let sprocket = Placement::load(&doc, "sprocket").expect("a row");
        assert_eq!(sprocket.organization, None, "the row names no organization");
    }

    #[test]
    fn the_row_keeps_the_last_word_on_a_branch_it_also_names() {
        let tmp = tempfile::tempdir().unwrap();
        workspace_on_disk(tmp.path(), "you/ABC-42-retry-window", &["ce"]);
        workspace_on_disk(tmp.path(), "you/DEF-9-spike", &["ce"]);

        let placement = Placement::load(&registry(tmp.path()), "widget").expect("a row");
        let named: Vec<(&str, bool, bool)> = placement
            .branches
            .iter()
            .map(|branch| (branch.branch.as_str(), branch.declared, branch.active))
            .collect();
        // One entry for the branch both name, and the row's word on it.
        assert_eq!(
            named,
            vec![
                ("you/ABC-42-retry-window", true, true),
                ("you/DEF-9-spike", false, false),
            ]
        );
    }

    /// The graal-workspace shape: every repository of the project type
    /// declares its base as `{branch}`, one per branch workspace, and nothing
    /// expands it on the way into the forest. Read verbatim it sent every fold
    /// looking for a ref called `{branch}`, so no branch of such a project ever
    /// showed a count and nothing that depends on one was ever offered. The
    /// declaration says where to look; the base is settled where it is used
    /// (§AR-004-forest.2).
    #[test]
    fn a_project_type_whose_base_is_a_template_measures_against_the_projects_main() {
        let tmp = tempfile::tempdir().unwrap();
        for repo in ["ce", "ee"] {
            let path = tmp.path().join("main").join(repo);
            std::fs::create_dir_all(&path).unwrap();
            assert!(std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&path)
                .status()
                .unwrap()
                .success());
        }
        let doc = json!({
            "project_types": [{
                "id": "poly",
                "repos": [
                    { "id": "ce", "path": "ce", "default_branch": "{branch}" },
                    { "id": "ee", "path": "ee", "default_branch": "{branch}" }
                ]
            }],
            "projects": [{
                "id": "widget",
                "type": "poly",
                "root": tmp.path().to_string_lossy(),
                "branch_root_template": "{project_root}/{branch}",
                "main_branch": "master"
            }]
        });

        let placement = Placement::load(&doc, "widget").expect("a row");
        // The row is read as it was written — the template is not laundered on
        // the way in, so anything else reading it sees what the type said.
        assert_eq!(placement.repos[0].main.as_deref(), Some("{branch}"));

        let forest = placement.forest(&tmp.path().join("main"));
        assert_eq!(forest.names(), vec!["ce", "ee"]);
        for repo in &forest.repos {
            assert_eq!(forest.base(repo).as_deref(), Some("master"));
        }
    }

    #[test]
    fn a_checkout_may_not_teach_the_project_a_ticket_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        workspace_on_disk(tmp.path(), "you/DEF-9-spike", &["ce"]);

        let placement = Placement::load(&registry(tmp.path()), "widget").expect("a row");
        // The branch is placed and worked like any other, but `DEF` is not
        // this project's to claim — only the row may say that
        // (§FS-008-attribution.1).
        assert!(placement
            .branches
            .iter()
            .any(|branch| branch.branch == "you/DEF-9-spike"));
        assert_eq!(placement.identity().tickets, vec!["ABC".to_string()]);
    }

    #[test]
    fn an_item_is_placed_on_the_branch_its_ticket_names() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), Some("{project_root}/{branch}"));

        // Matched by ticket key in the title, workspace not on disk yet.
        let checkout = placement.checkout(&item("[ABC-42] Fix condition errors", json!({})));
        assert_eq!(checkout.branch.as_deref(), Some("you/ABC-42-retry-window"));
        assert_eq!(checkout.ticket.as_deref(), Some("ABC-42"));
        assert_eq!(checkout.workspace, tmp.path());
        assert!(matches!(checkout.state, WorkspaceState::Missing(_)));

        // Once it exists, that is where the work goes.
        std::fs::create_dir_all(tmp.path().join("you/ABC-42-retry-window")).unwrap();
        let checkout = placement.checkout(&item("[ABC-42] Fix condition errors", json!({})));
        assert_eq!(
            checkout.workspace,
            tmp.path().join("you/ABC-42-retry-window")
        );
        assert!(matches!(checkout.state, WorkspaceState::Ready));
    }

    #[test]
    fn a_recorded_branch_beats_the_registry_and_an_unknown_one_is_unmatched() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), Some("{project_root}/{branch}"));

        let checkout = placement.checkout(&item("Unrelated", json!({ "branch": "you/other" })));
        assert_eq!(checkout.branch.as_deref(), Some("you/other"));
        assert!(matches!(checkout.state, WorkspaceState::Missing(_)));

        let checkout = placement.checkout(&item("Unrelated", json!({})));
        assert!(checkout.branch.is_none());
        assert!(matches!(checkout.state, WorkspaceState::Unmatched));
    }

    /// A registry match to the project's configured main branch is not the
    /// matter's own: it is the trunk every workspace is grown from, so it
    /// yields no placement branch and the matter resolves as unmatched, the
    /// same as one with no branch at all — but it is still where the
    /// matter's code lives right now, so the read resolution keeps it
    /// (§FS-005-dispatch.25).
    #[test]
    fn a_registry_match_to_the_main_branch_is_not_the_matters_own() {
        let tmp = tempfile::tempdir().unwrap();
        let mut placement = placement(tmp.path(), Some("{project_root}/{branch}"));
        placement.branches.push(BranchInfo {
            branch: "main".to_string(),
            ticket: None,
            active: true,
            is_release: false,
            declared: true,
        });

        let matter = item("Document the main workflow", json!({}));
        assert!(placement.is_main_branch("main"));

        // Read resolution: main branch included, because this is where the
        // matter's code lives right now.
        assert_eq!(placement.branch_name(&matter), Some("main".to_string()));

        // Placement: main branch excluded, because it is not the matter's
        // own — it resolves as unmatched, the same as no branch at all.
        assert_eq!(placement.own_branch(&matter), None);
        let owned = placement.own_checkout(&matter);
        assert!(owned.branch.is_none());
        assert!(matches!(owned.state, WorkspaceState::Unmatched));
    }

    /// The forge's own record of a pull request's branch keeps winning even
    /// where that branch happens to be the main branch — a forge fact, not a
    /// resemblance to carve out (§FS-005-dispatch.25).
    #[test]
    fn a_forge_recorded_branch_equal_to_main_still_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let mut placement = placement(tmp.path(), Some("{project_root}/{branch}"));
        placement.branches.push(BranchInfo {
            branch: "main".to_string(),
            ticket: None,
            active: true,
            is_release: false,
            declared: true,
        });

        let matter = item("A pull request from main", json!({ "branch": "main" }));
        assert_eq!(placement.branch_name(&matter), Some("main".to_string()));
        assert_eq!(placement.own_branch(&matter), Some("main".to_string()));
    }

    #[test]
    fn a_project_without_branch_workspaces_works_in_its_root() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), None);
        let checkout = placement.checkout(&item("Unrelated", json!({})));
        assert_eq!(checkout.workspace, tmp.path());
        assert!(matches!(checkout.state, WorkspaceState::Ready));
    }

    /// Write a `.git/HEAD` standing on `branch`, which is all
    /// [`crate::git::head_branch`] reads.
    fn standing_on(root: &Path, head: &str) {
        let git_dir = root.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), format!("ref: refs/heads/{head}\n")).unwrap();
    }

    /// The single checkout is the branch's working tree only while it is
    /// standing on the branch (§FS-005-dispatch.3). A directory that exists is
    /// not the same fact as the change being in it — which is how work was
    /// once dispatched against a root sitting on the main branch.
    #[test]
    fn a_single_checkout_standing_on_another_branch_is_not_this_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), None);
        let on_a_branch = |name: &str| item("Unrelated", json!({ "branch": name }));

        standing_on(tmp.path(), "main");
        let checkout = placement.checkout(&on_a_branch("fissile-0.6.0"));
        // Still the root — a refusal names where the branch should have been,
        // and the workspace is where every other reader of this looks.
        assert_eq!(checkout.workspace, tmp.path());
        match &checkout.state {
            WorkspaceState::Elsewhere(head) => assert_eq!(head, "main"),
            other => panic!("standing on main, not {other:?}"),
        }

        // The branch it is actually on is ready, as it always was.
        assert!(matches!(
            placement.checkout(&on_a_branch("main")).state,
            WorkspaceState::Ready
        ));
    }

    /// A fact nobody can establish is not a refusal: a detached HEAD, an
    /// unreadable one, or no repository at all leaves the root ready, because
    /// refusing there would block work that would have run
    /// (§FS-005-dispatch.3).
    #[test]
    fn a_head_that_names_no_branch_refuses_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), None);
        let item = item("Unrelated", json!({ "branch": "fissile-0.6.0" }));

        // No repository at all.
        assert!(matches!(
            placement.checkout(&item).state,
            WorkspaceState::Ready
        ));

        // Detached: a commit id, naming no branch to disagree with.
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "de2700aeb4c0ffee\n").unwrap();
        assert!(matches!(
            placement.checkout(&item).state,
            WorkspaceState::Ready
        ));
    }

    /// An item with no branch — an issue — is placed by the template the entry
    /// carries, and the workspace it names is the registry's own directory for
    /// that branch (§FS-005-dispatch.25).
    #[test]
    fn a_branch_template_names_the_branch_and_the_workspace_it_belongs_in() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), Some("{project_root}/{branch}"));
        let issue = issue();

        let named = minted(&placement, &issue, "fix/issue-{number}").unwrap();
        assert_eq!(named.branch.as_deref(), Some("fix/issue-95"));
        assert_eq!(named.workspace, tmp.path().join("fix/issue-95"));
        // Not there yet, and the state carries the directory to make.
        assert!(matches!(
            &named.state,
            WorkspaceState::Missing(target) if target == &tmp.path().join("fix/issue-95")
        ));

        // Rendering it again is the whole of the resolution: the same matter
        // gets the same directory, and one that is there is used as it stands.
        std::fs::create_dir_all(&named.workspace).unwrap();
        let again = minted(&placement, &issue, "fix/issue-{number}").unwrap();
        assert_eq!(again.workspace, named.workspace);
        assert!(matches!(again.state, WorkspaceState::Ready));
    }

    /// The three placeholders a branch template produces are refused inside it
    /// by name. A field this matter has not got makes an entry not serve it,
    /// while direct rendering still refuses rather than minting a branch with a
    /// hole in it (§FS-005-dispatch.25).
    #[test]
    fn a_template_naming_what_it_decides_is_refused_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), Some("{project_root}/{branch}"));
        let issue = issue();

        for name in ["branch", "workspace", "reply"] {
            let why = minted(&placement, &issue, &format!("fix/{{{name}}}")).unwrap_err();
            assert!(why.contains(&format!("{{{name}}}")), "{why}");
        }
        // A field the matter has not got. This issue carries no ticket key.
        assert_eq!(
            why_not_served(&placement, &issue, "fix/{ticket}"),
            Some(
                "the branch template 'fix/{ticket}' needs {ticket}, and this matter has none"
                    .to_string()
            )
        );
        let why = minted(&placement, &issue, "fix/{ticket}").unwrap_err();
        assert!(why.contains("{ticket}"), "{why}");
        // And one nobody has heard of: not a bad branch, a name that is no
        // field of a matter — said as that, with the ones it may name.
        let why = minted(&placement, &issue, "fix/{sprint}").unwrap_err();
        assert!(
            why.contains("not a field a branch template may name"),
            "{why}"
        );
        assert!(why.contains("{number}") && why.contains("{repo}"), "{why}");
        // And never as one of the three it decides, which have their own
        // refusal above.
        assert!(!why.contains("{workspace}"), "{why}");
        // Those author errors keep winning where the same template also names
        // the missing field, and a statically invalid rendering does too.
        for template in [
            "fix/{ticket}/{sprint}",
            "fix/{ticket}/{branch}",
            "bad name/{ticket}",
        ] {
            assert_eq!(why_not_served(&placement, &issue, template), None);
        }
    }

    /// A template that renders a name git will not take is refused by name
    /// too, before anything is made: git's own answer comes from inside the
    /// making, and by then the directories leading to the workspace are there
    /// (§FS-005-dispatch.25).
    #[test]
    fn a_template_rendering_what_git_refuses_is_refused_before_anything_is_made() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), Some("{project_root}/{branch}"));
        let issue = issue();

        // The issue's title, which holds spaces and a '#'.
        let why = minted(&placement, &issue, "fix/{title}").unwrap_err();
        assert!(why.contains("git will not take"), "{why}");
        assert!(!tmp.path().join("fix").exists(), "nothing was made");
    }

    /// The rules are git's own, not ephor's: every name here is what
    /// `git check-ref-format --branch` answers for it, so a template git would
    /// take is never refused early and one it would not is refused by name
    /// (§FS-005-dispatch.25).
    #[test]
    fn a_rendered_branch_is_held_to_what_git_takes() {
        for name in [
            "fix/issue-95",
            "issue-95",
            "fix/x.lockx",
            "fix/x#y",
            "fix/ünïcode",
            "fix/x./y",
            "fix/HEAD",
            "HEAD/x",
            "fix/x.y",
            "@fix",
            "fix@",
        ] {
            assert_eq!(why_git_refuses(name), None, "git takes '{name}'");
        }
        for name in [
            "",
            "fix/",
            "/fix",
            "fix//issue",
            ".hidden/x",
            "fix/.hidden",
            "fix/x.lock",
            "x.lock/y",
            "fix/x..y",
            "fix/x@{1}",
            "-fix",
            "fix.",
            "fix/x~y",
            "fix/x^y",
            "fix/x:y",
            "fix/x?y",
            "fix/x*y",
            "fix/x[y",
            "fix/x\\y",
            "fix/x y",
            "fix/issue-95/",
            "HEAD",
        ] {
            assert!(why_git_refuses(name).is_some(), "git refuses '{name}'");
        }
    }

    /// Nothing is minted into a root that is itself the checkout: a project
    /// with no `branch_root_template` is refused by name
    /// (§FS-005-dispatch.25).
    #[test]
    fn a_project_with_no_workspace_per_branch_is_refused_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), None);

        let why = minted(&placement, &issue(), "fix/issue-{number}").unwrap_err();
        assert!(why.contains("branch_root_template"), "{why}");
        assert!(why.contains("widget"), "{why}");
    }

    /// An issue: the matter a branch template is for, with no branch of its
    /// own and no ticket key in it.
    fn issue() -> Item {
        let mut item = item("Humanize durations", json!({}));
        item.id = "github-issues:acme/widget#95".to_string();
        item.kind = ItemKind::Issue;
        item.raw = json!({ "repo": "widget", "number": "95" });
        item
    }

    /// The shipped work root is a directory *inside* each branch workspace, so
    /// it can never be one: a project on the default configuration keeps
    /// `ephor checkout --branch panta` (§FS-004-quick-actions.7.3).
    #[test]
    fn the_shipped_work_root_under_each_workspace_never_collides() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = poly_repo(tmp.path());
        assert_eq!(
            why_the_workspace_is_refused(&placement, "panta", "{workspace}/panta"),
            None
        );
        assert_eq!(
            why_the_workspace_is_refused(&placement, "feature", "{workspace}/panta"),
            None
        );
    }

    /// A work root written beside the branch workspaces rather than inside each
    /// of them is a directory a branch name can land on, and the branch named
    /// for it is refused rather than checked out on top of the work root
    /// (§FS-004-quick-actions.7.3).
    #[test]
    fn a_work_root_beside_the_workspaces_refuses_the_branch_named_for_it() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = poly_repo(tmp.path());
        let why = why_the_workspace_is_refused(&placement, "panta", "{root}/panta")
            .expect("a branch named for the work root is refused");
        assert!(why.contains("panta"), "{why}");
        assert!(why.contains("work root"), "{why}");
    }

    /// Landing *inside* the work root is the same collision: `panta/sub`
    /// renders under it, and a workspace there would hold the plans of every
    /// branch (§FS-004-quick-actions.7.3).
    #[test]
    fn a_branch_under_the_work_root_is_refused_too() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = poly_repo(tmp.path());
        let why = why_the_workspace_is_refused(&placement, "panta/sub", "{root}/panta")
            .expect("a branch inside the work root is refused");
        assert!(why.contains("work root"), "{why}");
    }

    /// A work root that swallows the whole workspace area refuses every branch
    /// there is, so the refusal is about the project's configuration and says
    /// so: a message naming the branch would send the reader off to rename one
    /// when what has to move is `work.root` (§FS-004-quick-actions.7.3).
    #[test]
    fn a_work_root_holding_the_whole_workspace_area_blames_itself_and_not_the_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = poly_repo(tmp.path());
        let why = why_the_workspace_is_refused(&placement, "brand-new", "{root}")
            .expect("a work root holding the workspace area refuses the branch");
        assert!(why.contains("work root"), "{why}");
        assert!(why.contains("has to move"), "{why}");
        // The same answer for any name, which is what makes it the
        // configuration's and not the branch's.
        let other = why_the_workspace_is_refused(&placement, "feature", "{root}")
            .expect("every branch is refused, not one");
        assert!(other.contains("work root"), "{other}");
    }

    /// A work-root template naming a field only a matter can fill describes no
    /// fixed place, so there is nothing for a branch to collide with and it is
    /// passed over rather than guessed at (§FS-004-quick-actions.7.3).
    #[test]
    fn a_work_root_only_a_matter_can_render_is_passed_over() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = poly_repo(tmp.path());
        assert_eq!(
            why_the_workspace_is_refused(&placement, "panta", "{root}/{ticket}/panta"),
            None
        );
    }

    /// A name that climbs out of the area the template puts workspaces in is
    /// refused before anything is made — the directories on the way to it are
    /// what git's own refusal would have left behind
    /// (§FS-004-quick-actions.7.3).
    #[test]
    fn a_branch_that_climbs_out_of_the_workspace_area_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = poly_repo(tmp.path());
        let why = why_the_workspace_is_refused(&placement, "../escaped", "{workspace}/panta")
            .expect("a name that leaves the workspace area is refused");
        assert!(why.contains("escaped"), "{why}");
    }
}
