//! AGENTS.md rendering for workspaces.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{registry_error, Result};
use crate::paths;
use crate::registry::{
    expand_template, extract_ticket, get_project_type, merge_objects, str_field, value_to_string,
    Workspace,
};

pub fn write_agents_file(
    registry: &Value,
    registry_path: &Path,
    workspace: &Workspace,
) -> Result<PathBuf> {
    let project_type = get_project_type(registry, &workspace.type_id)?;
    let root = paths::resolve_path(&workspace.root);
    let agents_path = root.join("AGENTS.md");
    if let Some(parent) = agents_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| registry_error(format!("Cannot create {}: {err}", parent.display())))?;
    }
    let content = render_agents_file(project_type, registry_path, workspace, &root)?;
    fs::write(&agents_path, content)
        .map_err(|err| registry_error(format!("Cannot write {}: {err}", agents_path.display())))?;
    Ok(agents_path)
}

pub fn render_agents_file(
    project_type: &Value,
    registry_path: &Path,
    workspace: &Workspace,
    root: &Path,
) -> Result<String> {
    let context = build_context(workspace, root);
    let agents = merge_objects(
        project_type.get("agents").unwrap_or(&Value::Null),
        &workspace.agents_overrides,
    );
    let template_field = agents
        .get("template")
        .and_then(Value::as_str)
        .ok_or_else(|| registry_error("Project type is missing an AGENTS template path."))?;
    let template_path = paths::resolve_registry_relative(registry_path, template_field);
    let template = fs::read_to_string(&template_path).map_err(|err| {
        registry_error(format!(
            "Cannot read AGENTS template {}: {err}",
            template_path.display()
        ))
    })?;

    let structure_intro = agents
        .get("structure_intro")
        .and_then(Value::as_str)
        .ok_or_else(|| registry_error("Project type is missing 'agents.structure_intro'."))?;

    let mut values = HashMap::new();
    values.insert(
        "summary".to_string(),
        select_summary_template(&agents, &context)?,
    );
    values.insert(
        "structure_intro".to_string(),
        expand_template(structure_intro, &context)?,
    );
    values.insert(
        "structure_bullets".to_string(),
        render_structure_bullets(project_type, workspace, &context)?,
    );
    values.insert(
        "validation_section".to_string(),
        render_validation_section(&agents, &context)?,
    );

    let rendered = expand_template(&template, &values)?;
    Ok(format!("{}\n", rendered.trim_end()))
}

pub fn render_structure_bullets(
    project_type: &Value,
    workspace: &Workspace,
    context: &HashMap<String, String>,
) -> Result<String> {
    let mut lines = Vec::new();
    for repo in iter_workspace_repos(project_type, workspace, context)? {
        let role = repo.get("role").and_then(Value::as_str).unwrap_or("");
        let description_template = repo
            .get("agents_description")
            .and_then(Value::as_str)
            .unwrap_or(role);
        let mut description = expand_template(description_template, context)?;
        if let Some(guide) = repo.get("agents_guide").and_then(Value::as_str) {
            description = format!(
                "{} See `{}`.",
                description,
                expand_template(guide, context)?
            );
        }
        let path = repo.get("path").and_then(Value::as_str).unwrap_or("");
        let label = if path == "." {
            "./".to_string()
        } else {
            format!("{}/", path.trim_end_matches('/'))
        };
        lines.push(format!("- `{label}`: {description}"));
    }
    Ok(lines.join("\n"))
}

pub fn render_validation_section(
    agents: &Map<String, Value>,
    context: &HashMap<String, String>,
) -> Result<String> {
    let validations = match agents.get("validations").and_then(Value::as_array) {
        Some(validations) if !validations.is_empty() => validations,
        _ => return Ok(String::new()),
    };

    let mut lines = vec![
        String::new(),
        String::new(),
        "## Required Validation".to_string(),
        String::new(),
    ];
    for validation in validations {
        let command_template = validation
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| registry_error("Validation entry is missing 'command'."))?;
        let command = expand_template(command_template, context)?;
        match validation.get("cwd").and_then(Value::as_str) {
            Some(cwd) => lines.push(format!(
                "- From `{}`: `{command}`",
                expand_template(cwd, context)?
            )),
            None => lines.push(format!("- `{command}`")),
        }
    }
    Ok(lines.join("\n"))
}

pub fn select_summary_template(
    agents: &Map<String, Value>,
    context: &HashMap<String, String>,
) -> Result<String> {
    let ticket = context.get("ticket").map(String::as_str).unwrap_or("");
    let template_for = |field: &str| {
        agents
            .get(field)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };

    if !ticket.is_empty() {
        if let Some(template) = template_for("summary_with_ticket_template") {
            return expand_template(template, context);
        }
    }
    if ticket.is_empty() {
        if let Some(template) = template_for("summary_without_ticket_template") {
            return expand_template(template, context);
        }
    }
    if let Some(template) = template_for("summary_template") {
        return expand_template(template, context);
    }
    Err(registry_error("No usable summary template found."))
}

pub fn iter_workspace_repos(
    project_type: &Value,
    workspace: &Workspace,
    context: &HashMap<String, String>,
) -> Result<Vec<Map<String, Value>>> {
    let repos = project_type
        .get("repos")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut result = Vec::new();
    for repo in &repos {
        let repo_id = str_field(repo, "id").unwrap_or("");
        let override_map = workspace
            .repo_overrides
            .get(repo_id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut merged = merge_objects(repo, &override_map);

        let path = merged
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        merged.insert(
            "path".to_string(),
            Value::String(expand_template(&path, context)?),
        );

        if let Some(default_branch) = merged.get("default_branch").cloned() {
            if !default_branch.is_null() && value_to_string(&default_branch) != "" {
                let branch = expand_template(&value_to_string(&default_branch), context)?;
                merged.insert("branch".to_string(), Value::String(branch));
            }
        }
        if let Some(branch_override) = override_map.get("branch") {
            let branch = expand_template(&value_to_string(branch_override), context)?;
            merged.insert("branch".to_string(), Value::String(branch));
        }
        result.push(merged);
    }
    Ok(result)
}

pub fn build_context(workspace: &Workspace, root: &Path) -> HashMap<String, String> {
    let default_gradle_java_home = paths::home_dir()
        .join(".sdkman")
        .join("candidates")
        .join("java")
        .join("current");
    let mut context = HashMap::new();
    context.insert("workspace_id".to_string(), workspace.id.clone());
    context.insert("display_name".to_string(), workspace.display_name.clone());
    context.insert(
        "workspace_root".to_string(),
        root.to_string_lossy().into_owned(),
    );
    context.insert(
        "gradle_java_home".to_string(),
        env::var("GRADLE_JAVA_HOME")
            .unwrap_or_else(|_| default_gradle_java_home.to_string_lossy().into_owned()),
    );
    for (key, value) in &workspace.vars {
        context.insert(key.clone(), value_to_string(value));
    }
    context.entry("branch".to_string()).or_default();
    if !context.contains_key("ticket") {
        let branch = context.get("branch").cloned().unwrap_or_default();
        context.insert("ticket".to_string(), extract_ticket(&branch));
    }
    context
}
