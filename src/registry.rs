//! Registry loading, validation, and workspace selection.
//!
//! The registry is kept as loosely-typed JSON (`serde_json::Value`) to mirror
//! the merge/override semantics of the original Python implementation.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{registry_error, Result};
use crate::paths;

pub const EMBEDDED_SCHEMA: &str = include_str!("../assets/workspaces.schema.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchSection {
    ReleaseBranches,
    Branches,
    AdHoc,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: String,
    pub organization: Option<String>,
    pub type_id: String,
    pub display_name: String,
    pub root: String,
    pub active: bool,
    pub tags: Vec<String>,
    pub vars: Map<String, Value>,
    pub repo_overrides: Map<String, Value>,
    pub agents_overrides: Map<String, Value>,
    pub project_id: String,
    pub section: BranchSection,
}

pub fn load_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path)
        .map_err(|err| registry_error(format!("Cannot read {}: {err}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|err| registry_error(format!("Invalid JSON in {}: {err}", path.display())))
}

pub fn load_registry(registry_path: &Path, schema: &Value) -> Result<Value> {
    let registry = load_json(registry_path)?;
    validate_registry(&registry, schema)?;
    Ok(registry)
}

/// The same, from a document already read off disk. Split out for the caller
/// that keyed its cache on the bytes themselves and so is holding them
/// (§AR-009-surfaces.2): reading the file a second time to parse it would
/// leave a window in which the cache key and the parsed document came from two
/// different registries.
pub fn parse_registry(text: &str, registry_path: &Path, schema: &Value) -> Result<Value> {
    let registry: Value = serde_json::from_str(text).map_err(|err| {
        registry_error(format!(
            "Invalid JSON in {}: {err}",
            registry_path.display()
        ))
    })?;
    validate_registry(&registry, schema)?;
    Ok(registry)
}

pub fn validate_registry(registry: &Value, schema: &Value) -> Result<()> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(registry_error("Schema root must be an object."));
    }

    let validator = jsonschema::validator_for(schema)
        .map_err(|err| registry_error(format!("Invalid registry schema: {err}")))?;
    if let Some(error) = validator.iter_errors(registry).next() {
        return Err(registry_error(format!(
            "Registry does not match schema at '{}': {}",
            error.instance_path, error
        )));
    }

    for key in ["project_types", "hook_sets", "projects"] {
        match registry.get(key) {
            None => return Err(registry_error(format!("Registry is missing '{key}'."))),
            Some(value) if !value.is_array() => {
                return Err(registry_error(format!(
                    "Registry field '{key}' must be a list."
                )))
            }
            _ => {}
        }
    }

    let mut organizations: HashMap<String, &Value> = HashMap::new();
    if let Some(orgs) = registry.get("organizations") {
        let entries = orgs
            .as_array()
            .ok_or_else(|| registry_error("Registry field 'organizations' must be a list."))?;
        organizations = ensure_unique_ids(entries, "organization")?;
        for org in organizations.values() {
            if str_field(org, "name").is_none() {
                return Err(registry_error(format!(
                    "Organization '{}' must define a name.",
                    id_of(org)
                )));
            }
        }
    }

    let project_types = ensure_unique_ids(array_field(registry, "project_types"), "project type")?;
    let hook_sets = ensure_unique_ids(array_field(registry, "hook_sets"), "hook set")?;
    // Enforces unique project ids; the map itself is not needed afterwards.
    ensure_unique_ids(array_field(registry, "projects"), "project")?;

    for hook_set in hook_sets.values() {
        validate_hook_set(hook_set)?;
    }

    for project_type in project_types.values() {
        validate_project_type(project_type, &hook_sets)?;
    }

    let mut derived_ids = HashSet::new();
    for project in array_field(registry, "projects") {
        validate_project(project, &project_types, &organizations)?;
        for workspace in derive_project_workspaces(project)? {
            if !derived_ids.insert(workspace.id.clone()) {
                return Err(registry_error(format!(
                    "Duplicate derived workspace id: {}",
                    workspace.id
                )));
            }
        }
        validate_required_branch_ids(project)?;
    }

    Ok(())
}

fn validate_hook_set(hook_set: &Value) -> Result<()> {
    let id = id_of(hook_set);
    let hooks = hook_set.get("hooks").and_then(Value::as_array);
    let hooks = match hooks {
        Some(hooks) if !hooks.is_empty() => hooks,
        _ => {
            return Err(registry_error(format!(
                "Hook set '{id}' must define a non-empty 'hooks' list."
            )))
        }
    };
    for hook in hooks {
        match hook {
            Value::String(name) if !name.is_empty() => {}
            Value::Object(obj) => {
                let command = obj.get("command").and_then(Value::as_array);
                let valid = command.is_some_and(|cmd| {
                    !cmd.is_empty()
                        && cmd
                            .iter()
                            .all(|arg| arg.as_str().is_some_and(|s| !s.is_empty()))
                });
                if !valid {
                    return Err(registry_error(format!(
                        "Hook set '{id}' has a hook without a non-empty string 'command' list."
                    )));
                }
            }
            _ => {
                return Err(registry_error(format!(
                    "Hook set '{id}' hooks must be names or objects with a 'command' list."
                )))
            }
        }
    }
    Ok(())
}

fn validate_project_type(project_type: &Value, hook_sets: &HashMap<String, &Value>) -> Result<()> {
    let id = id_of(project_type);
    let layout = project_type.get("layout").and_then(Value::as_str);
    if !matches!(layout, Some("monorepo") | Some("polyrepo")) {
        return Err(registry_error(format!(
            "Project type '{id}' has invalid layout '{}'.",
            layout.unwrap_or("None")
        )));
    }

    let repos = project_type.get("repos").and_then(Value::as_array);
    let repos = match repos {
        Some(repos) if !repos.is_empty() => repos,
        _ => {
            return Err(registry_error(format!(
                "Project type '{id}' must define at least one repo."
            )))
        }
    };

    let mut repo_ids = HashSet::new();
    for repo in repos {
        let repo_id = match str_field(repo, "id") {
            Some(repo_id) => repo_id,
            None => {
                return Err(registry_error(format!(
                    "Project type '{id}' has a repo with an invalid id."
                )))
            }
        };
        if !repo_ids.insert(repo_id.to_string()) {
            return Err(registry_error(format!(
                "Project type '{id}' reuses repo id '{repo_id}'."
            )));
        }
        let update_mode = repo.get("update_mode").and_then(Value::as_str);
        if !matches!(update_mode, Some("branch") | Some("skip")) {
            return Err(registry_error(format!(
                "Project type '{id}' repo '{repo_id}' has invalid update_mode '{}'.",
                update_mode.unwrap_or("None")
            )));
        }
        if repo.get("required").and_then(Value::as_bool).is_none() {
            return Err(registry_error(format!(
                "Project type '{id}' repo '{repo_id}' must define boolean 'required'."
            )));
        }
        if str_field(repo, "path").is_none() {
            return Err(registry_error(format!(
                "Project type '{id}' repo '{repo_id}' must define a path."
            )));
        }
    }

    let agents = match project_type.get("agents") {
        Some(Value::Object(agents)) => agents,
        _ => {
            return Err(registry_error(format!(
                "Project type '{id}' must define 'agents'."
            )))
        }
    };
    if agents
        .get("template")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(registry_error(format!(
            "Project type '{id}' must define an AGENTS template path."
        )));
    }
    if agents
        .get("structure_intro")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(registry_error(format!(
            "Project type '{id}' must define 'agents.structure_intro'."
        )));
    }

    let has_summary = [
        "summary_template",
        "summary_with_ticket_template",
        "summary_without_ticket_template",
    ]
    .iter()
    .any(|field| {
        agents
            .get(*field)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    });
    if !has_summary {
        return Err(registry_error(format!(
            "Project type '{id}' must define at least one summary template."
        )));
    }

    let update_hooks = update_hooks_of(project_type);
    if let Some(update_hooks) = update_hooks {
        let update_hooks = update_hooks.as_object().ok_or_else(|| {
            registry_error(format!("Project type '{id}' has invalid update_hooks."))
        })?;
        for phase in ["pre", "post"] {
            let names = match update_hooks.get(phase) {
                None => continue,
                Some(Value::Array(names)) => names,
                Some(_) => {
                    return Err(registry_error(format!(
                        "Project type '{id}' update_hooks.{phase} must be a list."
                    )))
                }
            };
            for hook_set_id in names {
                let hook_set_id = hook_set_id.as_str().unwrap_or("");
                if !hook_sets.contains_key(hook_set_id) {
                    return Err(registry_error(format!(
                        "Project type '{id}' references unknown hook set '{hook_set_id}'."
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_project(
    project: &Value,
    project_types: &HashMap<String, &Value>,
    organizations: &HashMap<String, &Value>,
) -> Result<()> {
    let id = id_of(project);
    let type_id = project
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("None");
    if !project_types.contains_key(type_id) {
        return Err(registry_error(format!(
            "Project '{id}' references unknown project type '{type_id}'."
        )));
    }
    for field in ["display_name", "root", "main_branch"] {
        if str_field(project, field).is_none() {
            return Err(registry_error(format!(
                "Project '{id}' must define {field}."
            )));
        }
    }

    if let Some(org_id) = project.get("organization").and_then(Value::as_str) {
        if !organizations.is_empty() && !organizations.contains_key(org_id) {
            return Err(registry_error(format!(
                "Project '{id}' references unknown organization '{org_id}'."
            )));
        }
    }

    if let Some(clone_mode) = project.get("clone_mode").and_then(Value::as_str) {
        if !matches!(clone_mode, "worktree" | "full-clone") {
            return Err(registry_error(format!(
                "Project '{id}' has invalid clone_mode '{clone_mode}'."
            )));
        }
    }

    if !valid_tag_list(project.get("tags")) {
        return Err(registry_error(format!(
            "Project '{id}' tags must be a string list."
        )));
    }

    if let Some(template) = project.get("branch_root_template") {
        if template.as_str().is_none_or(str::is_empty) {
            return Err(registry_error(format!(
                "Project '{id}' branch_root_template must be a non-empty string."
            )));
        }
    }

    validate_branch_entries(project, "release_branches")?;
    validate_branch_entries(project, "branches")?;

    let mut branch_ids = HashSet::new();
    for entry in branch_entries(project, "release_branches")
        .iter()
        .chain(branch_entries(project, "branches"))
    {
        let branch_id = id_of(entry);
        if !branch_ids.insert(branch_id.to_string()) {
            return Err(registry_error(format!(
                "Project '{id}' reuses branch id '{branch_id}'."
            )));
        }
    }
    Ok(())
}

fn validate_branch_entries(project: &Value, field_name: &str) -> Result<()> {
    let project_id = id_of(project);
    let entries = match project.get(field_name) {
        None => return Ok(()),
        Some(Value::Array(entries)) => entries,
        Some(_) => {
            return Err(registry_error(format!(
                "Project '{project_id}' field '{field_name}' must be a list."
            )))
        }
    };
    for entry in entries {
        if str_field(entry, "id").is_none() {
            return Err(registry_error(format!(
                "Project '{project_id}' has an invalid branch id in '{field_name}'."
            )));
        }
        let entry_id = id_of(entry);
        if str_field(entry, "branch").is_none() {
            return Err(registry_error(format!(
                "Project '{project_id}' branch entry '{entry_id}' must define branch."
            )));
        }
        if entry.get("active").and_then(Value::as_bool).is_none() {
            return Err(registry_error(format!(
                "Project '{project_id}' branch entry '{entry_id}' must define boolean active."
            )));
        }
        if !valid_tag_list(entry.get("tags")) {
            return Err(registry_error(format!(
                "Project '{project_id}' branch entry '{entry_id}' tags must be a string list."
            )));
        }
        for object_field in ["vars", "repo_overrides", "agents_overrides"] {
            if let Some(value) = entry.get(object_field) {
                if !value.is_object() {
                    return Err(registry_error(format!(
                        "Project '{project_id}' branch entry '{entry_id}' field '{object_field}' must be an object."
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Generalized replacement for the hardcoded release-branch check: any
/// project may declare `required_branch_ids`, all of which must exist among
/// its release_branches or branches.
fn validate_required_branch_ids(project: &Value) -> Result<()> {
    let required = match project.get("required_branch_ids").and_then(Value::as_array) {
        Some(required) => required,
        None => return Ok(()),
    };
    let known: HashSet<&str> = branch_entries(project, "release_branches")
        .iter()
        .chain(branch_entries(project, "branches"))
        .map(|entry| id_of(entry))
        .collect();
    let mut missing: Vec<&str> = required
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| !known.contains(id))
        .collect();
    missing.sort_unstable();
    if !missing.is_empty() {
        return Err(registry_error(format!(
            "Project '{}' is missing required branches: {}",
            id_of(project),
            missing.join(", ")
        )));
    }
    Ok(())
}

fn ensure_unique_ids<'a, I>(entries: I, label: &str) -> Result<HashMap<String, &'a Value>>
where
    I: IntoIterator<Item = &'a Value>,
{
    let mut result = HashMap::new();
    for entry in entries {
        let entry_id = match str_field(entry, "id") {
            Some(entry_id) => entry_id,
            None => {
                return Err(registry_error(format!(
                    "Every {label} must define a non-empty string 'id'."
                )))
            }
        };
        if result.insert(entry_id.to_string(), entry).is_some() {
            return Err(registry_error(format!("Duplicate {label} id: {entry_id}")));
        }
    }
    Ok(result)
}

pub fn select_projects<'a>(
    registry: &'a Value,
    requested_ids: &[String],
    tags: &[String],
    organization: Option<&str>,
) -> Result<Vec<&'a Value>> {
    let id_filter: HashSet<&str> = requested_ids.iter().map(String::as_str).collect();
    let tag_filter: HashSet<&str> = tags.iter().map(String::as_str).collect();
    let mut selected = Vec::new();

    for project in array_field(registry, "projects") {
        let project_id = id_of(project);
        if !id_filter.is_empty() && !id_filter.contains(project_id) {
            continue;
        }
        if !tag_filter.is_empty() {
            let project_tags: HashSet<&str> = tag_list(project.get("tags")).into_iter().collect();
            if !tag_filter.is_subset(&project_tags) {
                continue;
            }
        }
        if let Some(org) = organization {
            if project.get("organization").and_then(Value::as_str) != Some(org) {
                continue;
            }
        }
        selected.push(project);
    }

    if !id_filter.is_empty() {
        let found: HashSet<&str> = selected.iter().map(|p| id_of(p)).collect();
        let mut missing: Vec<&str> = id_filter
            .iter()
            .filter(|id| !found.contains(**id))
            .copied()
            .collect();
        missing.sort_unstable();
        if !missing.is_empty() {
            // ids may still refer to derived workspaces; allow that case here
            let derived: HashSet<String> = iter_registered_workspaces(registry)?
                .into_iter()
                .map(|w| w.id)
                .collect();
            let unresolved: Vec<&str> = missing
                .into_iter()
                .filter(|id| !derived.contains(*id))
                .collect();
            if !unresolved.is_empty() {
                return Err(registry_error(format!(
                    "Unknown projects or workspaces: {}",
                    unresolved.join(", ")
                )));
            }
        }
    }

    Ok(selected)
}

pub fn select_managed_workspaces(
    registry: &Value,
    requested_ids: &[String],
    include_all: bool,
    tags: &[String],
    organization: Option<&str>,
) -> Result<Vec<Workspace>> {
    let requested: HashSet<&str> = requested_ids.iter().map(String::as_str).collect();
    let tag_filter: HashSet<&str> = tags.iter().map(String::as_str).collect();
    let mut selected = Vec::new();
    let mut matched: HashSet<String> = HashSet::new();

    for workspace in iter_registered_workspaces(registry)? {
        if !requested.is_empty()
            && !requested.contains(workspace.id.as_str())
            && !requested.contains(workspace.project_id.as_str())
        {
            continue;
        }
        if !tag_filter.is_empty() {
            let workspace_tags: HashSet<&str> = workspace.tags.iter().map(String::as_str).collect();
            if !tag_filter.is_subset(&workspace_tags) {
                continue;
            }
        }
        if let Some(org) = organization {
            if workspace.organization.as_deref() != Some(org) {
                continue;
            }
        }
        if workspace.section == BranchSection::Branches
            && !include_all
            && requested.is_empty()
            && !workspace.active
        {
            continue;
        }
        matched.insert(workspace.id.clone());
        matched.insert(workspace.project_id.clone());
        selected.push(workspace);
    }

    if !requested.is_empty() {
        let mut missing: Vec<&str> = requested
            .iter()
            .filter(|id| !matched.contains(**id))
            .copied()
            .collect();
        missing.sort_unstable();
        if !missing.is_empty() {
            return Err(registry_error(format!(
                "Unknown projects or workspaces: {}",
                missing.join(", ")
            )));
        }
    }

    Ok(dedupe_workspaces(selected))
}

pub fn iter_registered_workspaces(registry: &Value) -> Result<Vec<Workspace>> {
    let mut result = Vec::new();
    for project in array_field(registry, "projects") {
        result.extend(derive_project_workspaces(project)?);
    }
    Ok(result)
}

pub fn derive_project_workspaces(project: &Value) -> Result<Vec<Workspace>> {
    let mut result = Vec::new();
    for (section_name, section) in [
        ("release_branches", BranchSection::ReleaseBranches),
        ("branches", BranchSection::Branches),
    ] {
        for entry in branch_entries(project, section_name) {
            result.push(build_workspace_from_project(project, entry, section)?);
        }
    }
    Ok(result)
}

pub fn build_workspace_from_project(
    project: &Value,
    branch_entry: &Value,
    section: BranchSection,
) -> Result<Workspace> {
    let project_root = paths::resolve_path(str_field(project, "root").unwrap_or(""));
    let branch = str_field(branch_entry, "branch").unwrap_or("").to_string();
    let root = match project.get("branch_root_template").and_then(Value::as_str) {
        Some(template) => {
            let mut context = HashMap::new();
            context.insert(
                "project_root".to_string(),
                project_root.to_string_lossy().into_owned(),
            );
            context.insert("branch".to_string(), branch.clone());
            expand_template(template, &context)?
        }
        None => project_root.to_string_lossy().into_owned(),
    };

    let mut tags: Vec<String> = tag_list(project.get("tags"))
        .into_iter()
        .map(String::from)
        .collect();
    tags.extend(
        tag_list(branch_entry.get("tags"))
            .into_iter()
            .map(String::from),
    );

    let mut vars = object_field(project, "vars");
    for (key, value) in object_field(branch_entry, "vars") {
        vars.insert(key, value);
    }
    vars.insert("branch".to_string(), Value::String(branch));
    if let Some(ticket) = branch_entry.get("ticket").and_then(Value::as_str) {
        if !ticket.is_empty() {
            vars.insert("ticket".to_string(), Value::String(ticket.to_string()));
        }
    }

    let display_name = match branch_entry.get("display_name").and_then(Value::as_str) {
        Some(name) => name.to_string(),
        None => format!(
            "{} {}",
            str_field(project, "display_name").unwrap_or(""),
            str_field(branch_entry, "branch").unwrap_or("")
        ),
    };

    Ok(Workspace {
        id: id_of(branch_entry).to_string(),
        organization: project
            .get("organization")
            .and_then(Value::as_str)
            .map(String::from),
        type_id: str_field(project, "type").unwrap_or("").to_string(),
        display_name,
        root,
        active: branch_entry
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        tags: dedupe_list(tags),
        vars,
        repo_overrides: merge_maps(
            object_field(project, "repo_overrides"),
            object_field(branch_entry, "repo_overrides"),
        ),
        agents_overrides: merge_maps(
            object_field(project, "agents_overrides"),
            object_field(branch_entry, "agents_overrides"),
        ),
        project_id: id_of(project).to_string(),
        section,
    })
}

pub fn build_ad_hoc_workspace(
    project_type: &str,
    root: Option<&str>,
    display_name: &str,
    vars: &[String],
) -> Result<Workspace> {
    let root =
        root.ok_or_else(|| registry_error("--root is required when using ensure-agents --type."))?;
    let mut parsed = Map::new();
    for item in vars {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| registry_error(format!("Expected KEY=VALUE, got '{item}'.")))?;
        if key.is_empty() {
            return Err(registry_error(format!(
                "Invalid variable assignment '{item}'."
            )));
        }
        parsed.insert(key.to_string(), Value::String(value.to_string()));
    }
    Ok(Workspace {
        id: format!("adhoc-{project_type}"),
        organization: None,
        type_id: project_type.to_string(),
        display_name: display_name.to_string(),
        root: root.to_string(),
        active: true,
        tags: vec!["adhoc".to_string()],
        vars: parsed,
        repo_overrides: Map::new(),
        agents_overrides: Map::new(),
        project_id: "adhoc".to_string(),
        section: BranchSection::AdHoc,
    })
}

pub fn get_project_type<'a>(registry: &'a Value, project_type_id: &str) -> Result<&'a Value> {
    for project_type in array_field(registry, "project_types") {
        if id_of(project_type) == project_type_id {
            return Ok(project_type);
        }
    }
    Err(registry_error(format!(
        "Unknown project type '{project_type_id}'."
    )))
}

pub fn update_hooks_of(project_type: &Value) -> Option<&Value> {
    project_type.get("update_hooks").or_else(|| {
        project_type
            .get("agents")
            .and_then(|agents| agents.get("update_hooks"))
    })
}

/// Expand `{key}` placeholders (with `{{`/`}}` escapes) from the context,
/// erroring on unknown keys like Python's `str.format`.
pub fn expand_template(template: &str, context: &HashMap<String, String>) -> Result<String> {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    result.push('{');
                    continue;
                }
                let mut key = String::new();
                let mut closed = false;
                for inner in chars.by_ref() {
                    if inner == '}' {
                        closed = true;
                        break;
                    }
                    key.push(inner);
                }
                if !closed {
                    return Err(registry_error(format!(
                        "Unbalanced '{{' in template '{template}'."
                    )));
                }
                match context.get(&key) {
                    Some(value) => result.push_str(value),
                    None => {
                        return Err(registry_error(format!(
                            "Missing template variable '{key}' in '{template}'."
                        )))
                    }
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                result.push('}');
            }
            _ => result.push(ch),
        }
    }
    Ok(result)
}

/// Python `str(value)` semantics for the JSON scalar types used in `vars`.
pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

pub fn merge_maps(base: Map<String, Value>, overrides: Map<String, Value>) -> Map<String, Value> {
    let mut merged = base;
    for (key, value) in overrides {
        merged.insert(key, value);
    }
    merged
}

pub fn merge_objects(base: &Value, overrides: &Map<String, Value>) -> Map<String, Value> {
    let base_map = base.as_object().cloned().unwrap_or_default();
    merge_maps(base_map, overrides.clone())
}

fn dedupe_workspaces(workspaces: Vec<Workspace>) -> Vec<Workspace> {
    let mut seen = HashSet::new();
    workspaces
        .into_iter()
        .filter(|workspace| seen.insert(workspace.id.clone()))
        .collect()
}

fn dedupe_list(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

pub fn id_of(entry: &Value) -> &str {
    entry.get("id").and_then(Value::as_str).unwrap_or("")
}

pub fn str_field<'a>(entry: &'a Value, field: &str) -> Option<&'a str> {
    entry
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// Which organization each project belongs to, as the registry declares it
/// (§FS-005-dispatch.24). Membership is identity and lives in the project's
/// own row: it is the `organization` field there and nothing else, read here
/// and never written back (§REQ-001-boundary.2). A project whose row names no
/// organization is absent here, which is how it comes to be under no
/// organization ceiling.
pub fn organization_of_each_project(registry: &Value) -> BTreeMap<String, String> {
    array_field(registry, "projects")
        .iter()
        .filter_map(|project| {
            let organization = str_field(project, "organization")?;
            Some((id_of(project).to_string(), organization.to_string()))
        })
        .collect()
}

/// The organization ids named here that no registry row places a project
/// inside (§FS-005-dispatch.24). A binding written against an organization —
/// an autorun ceiling, say — reaches the projects whose rows join it, so an
/// id no row joins is a binding over nobody and the caller says so rather
/// than shrugging. This asks the membership the binding itself is resolved
/// through, so an id that is bounding somebody is never announced as
/// bounding nobody; an id the registry declares but no project has joined is
/// as empty as one it never declared, and is named the same way.
pub fn organizations_over_nobody<'a>(
    registry: &Value,
    named: impl Iterator<Item = &'a String>,
) -> Vec<String> {
    let inhabited: HashSet<String> = organization_of_each_project(registry)
        .into_values()
        .collect();
    named
        .filter(|id| !inhabited.contains(id.as_str()))
        .cloned()
        .collect()
}

pub fn array_field<'a>(registry: &'a Value, field: &str) -> &'a [Value] {
    registry
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub fn branch_entries<'a>(project: &'a Value, field: &str) -> &'a [Value] {
    array_field(project, field)
}

fn object_field(entry: &Value, field: &str) -> Map<String, Value> {
    entry
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn tag_list(value: Option<&Value>) -> Vec<&str> {
    value
        .and_then(Value::as_array)
        .map(|tags| tags.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn valid_tag_list(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Array(tags)) => tags
            .iter()
            .all(|tag| tag.as_str().is_some_and(|s| !s.is_empty())),
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_template_reports_missing_keys() {
        let mut context = HashMap::new();
        context.insert("branch".to_string(), "main".to_string());
        assert_eq!(expand_template("b={branch}", &context).unwrap(), "b=main");
        let err = expand_template("{missing}", &context).unwrap_err();
        assert!(err
            .to_string()
            .contains("Missing template variable 'missing'"));
    }

    #[test]
    fn expand_template_handles_escaped_braces() {
        let context = HashMap::new();
        assert_eq!(
            expand_template("{{literal}}", &context).unwrap(),
            "{literal}"
        );
    }

    /// §FS-005-dispatch.24: a binding written over an organization no
    /// registry row places a project inside is a binding over nobody, and the
    /// caller is told which name it was. Membership is the `organization`
    /// field on a project's row and nothing else, so the declaration array
    /// neither makes an organization inhabited nor is needed for one to be.
    #[test]
    fn an_organization_no_project_row_joins_is_named_back() {
        let named = ["acme".to_string(), "empty".to_string(), "typo".to_string()];
        // `empty` is declared and joined by nobody; `acme` is joined without
        // the array declaring anything at all, which the validator permits.
        let registry = serde_json::json!({
            "organizations": [{ "id": "empty", "name": "Empty" }],
            "projects": [{ "id": "widget", "organization": "acme" }]
        });
        assert_eq!(
            organizations_over_nobody(&registry, named.iter()),
            vec!["empty".to_string(), "typo".to_string()]
        );
        assert_eq!(
            organization_of_each_project(&registry),
            BTreeMap::from([("widget".to_string(), "acme".to_string())])
        );
        // A registry holding no project puts every name outside every
        // organization, and a caller that names none has nothing to be told.
        let barren = serde_json::json!({ "projects": [] });
        assert_eq!(organizations_over_nobody(&barren, named.iter()).len(), 3);
        assert!(organizations_over_nobody(&registry, [].iter()).is_empty());
    }
}
