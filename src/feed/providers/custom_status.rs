//! Per-project custom status: a summons on the project, whose answer becomes
//! status items. This is the no-recompile extension point — any script that
//! reports what it knows is a provider (§FS-006-project-interface.3).
//!
//! The command is run by the one executor (§AR-002-summons), so it is told
//! about the project in the same `EPHOR_*` vocabulary every other summons
//! receives and may answer in the published envelope by writing
//! `$EPHOR_ANSWER` (§FS-006-project-interface.4). Reading structure out of
//! standard output is the legacy binding option this provider predates the
//! envelope with, kept and marked as such: a contract that parses stdout would
//! make every honest log line a protocol violation.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::feed::model::{Item, ItemKind};
use crate::feed::provider::{Provider, ProviderContext, ProviderError, ProviderResult};
use crate::feed::providers::parse_config;
use crate::seams::dossier;
use crate::seams::summons::{self, Mode, Outcome, Place, Site, Summons};

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

/// How this binding reports. `Answer` is the envelope every other verb speaks;
/// `Text` and `Json` read standard output, which only this binding does and
/// only because it is older than the envelope.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Format {
    #[default]
    Text,
    Json,
    Answer,
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
        Item {
            id: self.id(ctx, index),
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

    fn id(&self, ctx: &ProviderContext, index: usize) -> String {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!(":{index}")
        };
        format!("custom-status:{}{suffix}", ctx.project_id)
    }

    /// One status row per matter the answer reported, and one from the
    /// envelope's own `summary` where it reported no matters at all — the
    /// common one-line case (§FS-006-project-interface.4).
    fn items_from_answer(
        &self,
        ctx: &ProviderContext,
        answer: &summons::Answer,
    ) -> Result<Vec<Item>, ProviderError> {
        let Some(normalized) = &answer.answer else {
            return Ok(Vec::new());
        };
        if !normalized.matters.is_empty() {
            return Ok(normalized
                .matters
                .iter()
                .enumerate()
                .map(|(index, matter)| Item {
                    id: matter.key.clone(),
                    project: ctx.project_id.clone(),
                    source: "custom-status".to_string(),
                    kind: ItemKind::Status,
                    role: None,
                    title: matter.title.clone().unwrap_or_else(|| matter.key.clone()),
                    url: matter.url.clone(),
                    state: matter.state.clone(),
                    needs_response: normalized.facts.needs_response.unwrap_or(false) && index == 0,
                    updated_at: chrono::Utc::now(),
                    raw: serde_json::to_value(&matter.data).unwrap_or(Value::Null),
                })
                .collect());
        }
        let Some(summary) = normalized.facts.summary.clone() else {
            return Ok(Vec::new());
        };
        Ok(vec![Item {
            id: self.id(ctx, 0),
            project: ctx.project_id.clone(),
            source: "custom-status".to_string(),
            kind: ItemKind::Status,
            role: None,
            title: summary,
            url: normalized.facts.url.clone(),
            state: None,
            needs_response: normalized.facts.needs_response.unwrap_or(false),
            updated_at: chrono::Utc::now(),
            raw: serde_json::to_value(&normalized.facts.data).unwrap_or(Value::Null),
        }])
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
        let answer = run(&self.config.command, ctx, Path::new(&cwd), ctx.timeout)?;
        // Parked is a source saying "nothing to report just now", which is a
        // status of none rather than a failure (§FS-006-project-interface.3).
        match answer.outcome {
            Outcome::Done => {}
            Outcome::Parked => return Ok(Vec::new()),
            Outcome::Failed => {
                return Err(ProviderError(format!(
                    "custom-status command {}",
                    match answer.exit_code {
                        Some(code) => format!("failed ({code})"),
                        None => "was killed".to_string(),
                    }
                )))
            }
        }

        // An envelope answers whatever the binding's format says: a command
        // that wrote one meant it.
        if answer.answer.is_some() || self.config.format == Format::Answer {
            return self.items_from_answer(ctx, &answer);
        }

        let stdout = answer.output.clone().unwrap_or_default();
        match self.config.format {
            // Configured for the envelope and silent: nothing to report.
            Format::Answer => Ok(Vec::new()),
            Format::Text => {
                let text = stdout.trim();
                if text.is_empty() {
                    return Ok(Vec::new());
                }
                Ok(vec![Item {
                    id: self.id(ctx, 0),
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

/// The summons this provider is: the project's own command, in the project's
/// place, captured because nobody is watching a refresh.
fn run(
    command: &str,
    ctx: &ProviderContext,
    cwd: &Path,
    timeout: Duration,
) -> Result<summons::Answer, ProviderError> {
    let site = Site::root(cwd);
    let dossier = dossier::of_project(&ctx.project_id, &ctx.project_root, cwd, None);
    let summons = Summons::new("custom-status", command)
        .at(Place::Workspace)
        .carrying(dossier);
    summons::run(&summons, &site, Mode::Captured(timeout))
        .map_err(|err| ProviderError(err.to_string()))
}
