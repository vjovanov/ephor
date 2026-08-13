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

/// A branch as the registry declares it.
#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub branch: String,
    pub ticket: Option<String>,
    pub active: bool,
    pub is_release: bool,
}

/// Whether an item is work on this branch: by ticket key, by the branch name
/// the provider recorded, or by the branch name appearing in the title.
pub fn matches(item: &Item, branch: &BranchInfo) -> bool {
    if let Some(ticket) = &branch.ticket {
        if item.id.contains(ticket.as_str()) || item.title.contains(ticket.as_str()) {
            return true;
        }
    }
    if branch.branch.is_empty() {
        return false;
    }
    // Providers that know the item's source branch record it in raw.branch.
    if item.raw.get("branch").and_then(Value::as_str) == Some(branch.branch.as_str()) {
        return true;
    }
    item.title.contains(branch.branch.as_str())
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

/// Where an item's branch workspace stands, for gating work on it.
#[derive(Clone, Debug)]
pub enum WorkspaceState {
    /// The workspace exists (or the project root is the checkout).
    Ready,
    /// The project defines branch workspaces and this one is missing;
    /// carries the directory a checkout must create.
    Missing(PathBuf),
    /// The item is not linked to any registry branch.
    Unmatched,
}

/// One project's placement data: where it is checked out, how its branch
/// workspaces are named, and which branches the registry knows about.
#[derive(Clone, Debug)]
pub struct Placement {
    pub project: String,
    pub root: PathBuf,
    pub template: Option<String>,
    pub branches: Vec<BranchInfo>,
    /// What branches here are measured against, and replayed onto
    /// (§FS-004-quick-actions.6).
    pub main_branch: Option<String>,
    /// What the project type declares about the checkout's repositories. Empty
    /// where the registry does not say, which leaves the forest to be probed
    /// on disk instead (§AR-004-forest.2).
    pub repos: Vec<Declaration>,
}

/// Where one item's work belongs.
#[derive(Clone, Debug)]
pub struct Checkout {
    /// The directory a command about this item runs in: the branch workspace
    /// when it exists, otherwise the project root.
    pub workspace: PathBuf,
    /// The branch name — the provider-recorded one, or the matched registry
    /// branch's.
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
                        let extracted = registry::extract_ticket(&branch);
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
                });
            }
        }
        Some(Placement {
            project: project.to_string(),
            root,
            template,
            branches,
            main_branch: registry::str_field(entry, "main_branch").map(String::from),
            repos: declarations(registry_doc, entry),
        })
    }

    /// This project's forest at `checkout` — the thing every git-facing
    /// feature folds over (§AR-004-forest).
    pub fn forest(&self, checkout: &Path) -> Forest {
        Forest::resolve(checkout, self.main_branch.as_deref(), &self.repos)
    }

    /// The directory this project's workspace for `branch` belongs at, whether
    /// or not it is there yet.
    pub fn workspace_for(&self, branch: &str) -> Option<PathBuf> {
        expand_workspace(self.template.as_deref(), &self.root, branch)
    }

    /// An existing checkout to make a new workspace from
    /// (§FS-004-quick-actions.7). A working tree is added *from* a repository,
    /// so there has to be one already on disk: the main branch's workspace by
    /// preference — it is the one a new branch is grown from — then any other
    /// branch's, then the root for a project that keeps its repositories there.
    pub fn source_checkout(&self) -> Option<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(main) = self.main_branch.as_deref() {
            candidates.extend(self.workspace_for(main));
        }
        for branch in &self.branches {
            candidates.extend(self.workspace_for(&branch.branch));
        }
        candidates.push(self.root.clone());
        candidates
            .into_iter()
            .find(|path| !self.forest(path).is_empty())
    }

    pub fn matched(&self, item: &Item) -> Option<&BranchInfo> {
        self.branches.iter().find(|branch| matches(item, branch))
    }

    /// The item's branch name: what the provider recorded (ground truth), or
    /// the matched registry branch's.
    pub fn branch_name(&self, item: &Item) -> Option<String> {
        item.raw
            .get("branch")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(String::from)
            .or_else(|| self.matched(item).map(|branch| branch.branch.clone()))
    }

    pub fn checkout(&self, item: &Item) -> Checkout {
        let branch = self.branch_name(item);
        let ticket = self
            .matched(item)
            .and_then(|matched| matched.ticket.clone())
            .or_else(|| branch.as_deref().map(registry::extract_ticket))
            .filter(|ticket| !ticket.is_empty());
        let expanded = branch
            .as_deref()
            .and_then(|name| expand_workspace(self.template.as_deref(), &self.root, name));

        let (workspace, state) = match (&expanded, self.template.is_some()) {
            // A single-checkout project: the root is the workspace.
            (_, false) => (self.root.clone(), WorkspaceState::Ready),
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
            }],
            main_branch: Some("main".to_string()),
            repos: Vec::new(),
        }
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

    #[test]
    fn a_project_without_branch_workspaces_works_in_its_root() {
        let tmp = tempfile::tempdir().unwrap();
        let placement = placement(tmp.path(), None);
        let checkout = placement.checkout(&item("Unrelated", json!({})));
        assert_eq!(checkout.workspace, tmp.path());
        assert!(matches!(checkout.state, WorkspaceState::Ready));
    }
}
