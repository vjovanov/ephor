//! Data-driven update hooks.
//!
//! A hook set entry's `hooks` list contains either the built-in name
//! `"print-debug-context"` or an object:
//!
//! ```json
//! { "command": ["${USER_CODE}/tools/update-workspace", "update"], "cwd": "{workspace_root}", "pass_debug": false }
//! ```
//!
//! Command arguments and `cwd` are expanded with the workspace template
//! context (`workspace_id`, `workspace_root`, `branch`, ...) followed by
//! environment-variable expansion (`$VAR` / `${VAR}`).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::agents::build_context;
use crate::error::{registry_error, EphorError, Result};
use crate::paths::expand_user_vars;
use crate::registry::{expand_template, update_hooks_of, Workspace};

pub fn run_hook_sets(
    registry: &Value,
    project_type: &Value,
    phase: &str,
    workspace: &Workspace,
    root: &Path,
    debug: bool,
) -> Result<()> {
    let hook_sets: HashMap<&str, &Value> = registry
        .get("hook_sets")
        .and_then(Value::as_array)
        .map(|sets| {
            sets.iter()
                .map(|set| (set.get("id").and_then(Value::as_str).unwrap_or(""), set))
                .collect()
        })
        .unwrap_or_default();

    let update_hooks = match update_hooks_of(project_type) {
        Some(update_hooks) => update_hooks,
        None => return Ok(()),
    };
    let names = match update_hooks.get(phase).and_then(Value::as_array) {
        Some(names) => names,
        None => return Ok(()),
    };

    for hook_set_id in names {
        let hook_set_id = hook_set_id.as_str().unwrap_or("");
        let hook_set = hook_sets
            .get(hook_set_id)
            .ok_or_else(|| registry_error(format!("Unknown hook set '{hook_set_id}'.")))?;
        let hooks = hook_set
            .get("hooks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for hook in &hooks {
            run_hook(hook, workspace, root, debug)?;
        }
    }
    Ok(())
}

fn run_hook(hook: &Value, workspace: &Workspace, root: &Path, debug: bool) -> Result<()> {
    match hook {
        Value::String(name) if name == "print-debug-context" => {
            println!(
                "hook debug={} workspace={} root={}",
                if debug { "True" } else { "False" },
                workspace.id,
                root.display()
            );
            Ok(())
        }
        Value::String(name) => Err(registry_error(format!("Unknown hook '{name}'."))),
        Value::Object(spec) => run_command_hook(spec, workspace, root, debug),
        _ => Err(registry_error("Invalid hook entry.")),
    }
}

fn run_command_hook(
    spec: &serde_json::Map<String, Value>,
    workspace: &Workspace,
    root: &Path,
    debug: bool,
) -> Result<()> {
    let context = build_context(workspace, root);
    let argv = spec
        .get("command")
        .and_then(Value::as_array)
        .ok_or_else(|| registry_error("Hook object is missing 'command'."))?;
    let mut expanded: Vec<String> = Vec::with_capacity(argv.len() + 1);
    for arg in argv {
        let arg = arg
            .as_str()
            .ok_or_else(|| registry_error("Hook command arguments must be strings."))?;
        expanded.push(expand_user_vars(&expand_template(arg, &context)?));
    }
    if debug
        && spec
            .get("pass_debug")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        expanded.push("--debug".to_string());
    }

    let cwd = match spec.get("cwd").and_then(Value::as_str) {
        Some(cwd) => expand_user_vars(&expand_template(cwd, &context)?),
        None => root.to_string_lossy().into_owned(),
    };

    let status = Command::new(&expanded[0])
        .args(&expanded[1..])
        .current_dir(&cwd)
        .status()
        .map_err(|err| {
            EphorError::Command(format!("Failed to run hook {}: {err}", expanded.join(" ")))
        })?;
    if !status.success() {
        return Err(EphorError::Command(format!(
            "Hook failed with {}: {}",
            status,
            expanded.join(" ")
        )));
    }
    Ok(())
}
