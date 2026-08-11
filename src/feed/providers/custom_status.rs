//! Per-project custom status: run a shell command in the workspace and turn
//! its stdout into status items. This is the no-recompile extension point —
//! any script that prints `{"status": ..., "summary": ...}` is a provider.

use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use crate::feed::model::{Item, ItemKind};
use crate::feed::provider::{
    run_capture, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::parse_config;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    command: String,
    #[serde(default)]
    format: Format,
    /// Working directory; defaults to the project root. `{project_root}` is
    /// substituted.
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Format {
    #[default]
    Text,
    Json,
}

pub struct CustomStatus {
    config: Config,
}

impl CustomStatus {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(CustomStatus {
            config: parse_config(config)?,
        })
    }

    fn item_from_json(&self, ctx: &ProviderContext, value: &Value, index: usize) -> Item {
        let title = value
            .get("title")
            .or_else(|| value.get("summary"))
            .and_then(Value::as_str)
            .unwrap_or("status")
            .to_string();
        let suffix = if index == 0 {
            String::new()
        } else {
            format!(":{index}")
        };
        Item {
            id: format!("custom-status:{}{suffix}", ctx.project_id),
            project: ctx.project_id.clone(),
            source: "custom-status".to_string(),
            kind: ItemKind::Status,
            role: None,
            title,
            url: value.get("url").and_then(Value::as_str).map(String::from),
            state: value
                .get("status")
                .and_then(Value::as_str)
                .map(String::from),
            needs_response: value
                .get("needs_response")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            updated_at: chrono::Utc::now(),
            raw: value.clone(),
        }
    }
}

impl Provider for CustomStatus {
    fn name(&self) -> &'static str {
        "custom-status"
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let cwd = match &self.config.cwd {
            Some(cwd) => cwd.replace("{project_root}", &ctx.project_root.to_string_lossy()),
            None => ctx.project_root.to_string_lossy().into_owned(),
        };
        if !std::path::Path::new(&cwd).is_dir() {
            return Err(ProviderError(format!("cwd does not exist: {cwd}")));
        }
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(&self.config.command)
            .current_dir(&cwd);
        let stdout = run_capture(command, ctx.timeout, false)?;

        match self.config.format {
            Format::Text => {
                let text = stdout.trim();
                if text.is_empty() {
                    return Ok(Vec::new());
                }
                Ok(vec![Item {
                    id: format!("custom-status:{}", ctx.project_id),
                    project: ctx.project_id.clone(),
                    source: "custom-status".to_string(),
                    kind: ItemKind::Status,
                    role: None,
                    title: text.lines().next().unwrap_or("").to_string(),
                    url: None,
                    state: None,
                    needs_response: false,
                    updated_at: chrono::Utc::now(),
                    raw: Value::Null,
                }])
            }
            Format::Json => {
                let value: Value = serde_json::from_str(stdout.trim())
                    .map_err(|err| ProviderError(format!("invalid status JSON: {err}")))?;
                match value {
                    Value::Array(entries) => Ok(entries
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| self.item_from_json(ctx, entry, index))
                        .collect()),
                    other => Ok(vec![self.item_from_json(ctx, &other, 0)]),
                }
            }
        }
    }
}
