//! Workspace update orchestration: per-repo git fetch/checkout/pull plus hooks.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Map, Value};

use crate::agents::{build_context, iter_workspace_repos, write_agents_file};
use crate::error::{registry_error, EphorError, Result};
use crate::hooks::run_hook_sets;
use crate::paths;
use crate::registry::{get_project_type, Workspace};

pub fn update_workspace(
    registry: &Value,
    registry_path: &Path,
    workspace: &Workspace,
    debug: bool,
    skip_agents: bool,
) -> Result<()> {
    let project_type = get_project_type(registry, &workspace.type_id)?;
    validate_workspace_paths(project_type, workspace)?;
    let root = paths::resolve_path(&workspace.root);
    let context = build_context(workspace, &root);

    run_hook_sets(registry, project_type, "pre", workspace, &root, debug)?;
    for repo in iter_workspace_repos(project_type, workspace, &context)? {
        update_repo(&root, &repo)?;
    }
    run_hook_sets(registry, project_type, "post", workspace, &root, debug)?;
    if !skip_agents {
        write_agents_file(registry, registry_path, workspace)?;
    }
    Ok(())
}

pub fn validate_workspace_paths(project_type: &Value, workspace: &Workspace) -> Result<()> {
    let root = paths::resolve_path(&workspace.root);
    if !root.exists() {
        return Err(registry_error(format!(
            "Managed workspace '{}' root does not exist: {}",
            workspace.id,
            root.display()
        )));
    }

    let context = build_context(workspace, &root);
    for repo in iter_workspace_repos(project_type, workspace, &context)? {
        let required = repo
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let repo_path = root.join(repo.get("path").and_then(Value::as_str).unwrap_or(""));
        if required && !repo_path.exists() {
            return Err(registry_error(format!(
                "Managed workspace '{}' is missing required repo path '{}': {}",
                workspace.id,
                repo.get("id").and_then(Value::as_str).unwrap_or(""),
                repo_path.display()
            )));
        }
    }
    Ok(())
}

fn update_repo(root: &Path, repo: &Map<String, Value>) -> Result<()> {
    if repo.get("update_mode").and_then(Value::as_str) == Some("skip") {
        return Ok(());
    }

    let required = repo
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let repo_path: PathBuf = root.join(repo.get("path").and_then(Value::as_str).unwrap_or(""));
    let branch = repo.get("branch").and_then(Value::as_str).unwrap_or("");

    if !repo_path.exists() {
        if required {
            return Err(registry_error(format!(
                "Required repo path does not exist: {}",
                repo_path.display()
            )));
        }
        return Ok(());
    }

    if !is_git_work_tree(&repo_path) {
        if required {
            return Err(registry_error(format!(
                "Required repo path is not a git repository: {}",
                repo_path.display()
            )));
        }
        return Ok(());
    }

    if branch.is_empty() {
        return Err(registry_error(format!(
            "Repo '{}' under '{}' is missing a branch.",
            repo.get("id").and_then(Value::as_str).unwrap_or(""),
            root.display()
        )));
    }

    let repo_path_str = repo_path.to_string_lossy();
    run_command(&["git", "-C", &repo_path_str, "fetch", "origin", branch])?;
    run_command(&["git", "-C", &repo_path_str, "checkout", branch])?;
    run_command(&["git", "-C", &repo_path_str, "pull", "origin", branch])?;
    Ok(())
}

pub fn is_git_work_tree(path: &Path) -> bool {
    Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn run_command(cmd: &[&str]) -> Result<()> {
    let status = Command::new(cmd[0])
        .args(&cmd[1..])
        .status()
        .map_err(|err| EphorError::Command(format!("Failed to run {}: {err}", cmd.join(" "))))?;
    if !status.success() {
        return Err(EphorError::Command(format!(
            "Command failed with {}: {}",
            status,
            cmd.join(" ")
        )));
    }
    Ok(())
}
