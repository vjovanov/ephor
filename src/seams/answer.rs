//! The answer envelope: the structure a summoned command may write to the file
//! named by `$EPHOR_ANSWER` (§FS-006-project-interface.4).
//!
//! The envelope is the seam's structured half — materials, never linked code
//! (§REQ-001-boundary.1) — so its authority is a published JSON schema rather
//! than these types: the schema ships embedded, a project can validate against
//! it with no ephor present (§FS-006-project-interface.11), and every answer is
//! checked against it before anything here reads a field. The types below
//! mirror that schema and speak the model's nouns (§FS-007-matters).
//!
//! Unknown fields are ignored everywhere, which is what lets the envelope grow
//! by addition: a command written for a later ephor stays readable by this one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::error::{EphorError, Result};

/// The published envelope schema (§FS-006-project-interface.11). Embedded so
/// that validation needs no file on disk and `ephor schema` can print it.
pub const ANSWER_SCHEMA: &str = include_str!("../../assets/ephor-answer.schema.json");

/// What a command wrote, exactly as the schema describes it.
#[derive(Debug, Clone, Deserialize)]
pub struct Envelope {
    /// Envelope version. Bumps only on incompatible change.
    pub v: u32,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub needs_response: Option<bool>,
    #[serde(default)]
    pub gate: Option<Gate>,
    #[serde(default)]
    pub failures: Vec<Failure>,
    #[serde(default)]
    pub features: Vec<Feature>,
    #[serde(default)]
    pub matters: Vec<Matter>,
    #[serde(default)]
    pub discussions: Vec<Discussion>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub data: Map<String, Value>,
}

/// A subject the command reports (§FS-007-matters.1).
#[derive(Debug, Clone, Deserialize)]
pub struct Matter {
    pub key: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub terminal: Option<bool>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub data: Map<String, Value>,
}

/// Messages grouped in one channel (§FS-007-matters.3).
#[derive(Debug, Clone, Deserialize)]
pub struct Discussion {
    /// The subject key this belongs to; absent means the matter of the summons.
    #[serde(default)]
    pub matter: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub author: String,
    pub text: String,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub mine: Option<bool>,
    #[serde(default)]
    pub task: Option<Task>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

/// A task the venue tracks on a message (§FS-003-feed-categories.4). Everything
/// beyond `state` is the source's own resolve descriptor, handed back verbatim
/// when the person ticks the box.
#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub state: String,
    #[serde(flatten)]
    pub descriptor: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub mine: Option<bool>,
}

/// What a channel can carry (§FS-007-matters.4). An undeclared capability is
/// display-only — the surfaces offer no key for it.
#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(default)]
    pub can: Vec<String>,
}

/// An observation about a matter (§FS-007-matters.5).
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    #[serde(default)]
    pub matter: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub gate: Option<Gate>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub data: Map<String, Value>,
}

/// The gate's standing, per repository of the forest it covers
/// (§AR-004-forest.1).
#[derive(Debug, Clone, Deserialize)]
pub struct Gate {
    #[serde(default)]
    pub repos: Vec<GateRepo>,
    #[serde(default)]
    pub blocked: Option<bool>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateRepo {
    pub repo: String,
    #[serde(default)]
    pub passed: Option<u64>,
    #[serde(default)]
    pub failed: Option<u64>,
    #[serde(default)]
    pub running: Option<u64>,
    #[serde(default)]
    pub url: Option<String>,
}

/// What actually failed (§FS-006-project-interface.5, §FS-006-project-interface.6).
#[derive(Debug, Clone, Deserialize)]
pub struct Failure {
    pub job: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub trace: Option<String>,
    /// A log the command wrote, named relative to where it ran; normalization
    /// resolves it (§FS-006-project-interface.4).
    #[serde(default)]
    pub log: Option<PathBuf>,
    #[serde(default)]
    pub jobs: Option<u64>,
}

/// One feature a project's checks enumerate (§FS-006-project-interface.5).
#[derive(Debug, Clone, Deserialize)]
pub struct Feature {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
}

/// An answer with its verb-level conveniences expanded: `failures` and `gate`
/// become events, `features` and the one-line fields become facts
/// (§FS-006-project-interface.4). Readers downstream see one shape whether the
/// command wrote sugar or spelled the events out.
#[derive(Debug, Clone)]
pub struct Normalized {
    pub facts: Facts,
    pub events: Vec<Observation>,
    pub matters: Vec<Matter>,
    pub discussions: Vec<Discussion>,
    pub channels: Vec<Channel>,
}

/// What the answer says about the matter of the summons itself, and about the
/// project — the parts that are not observations of a moment.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub summary: Option<String>,
    pub url: Option<String>,
    pub needs_response: Option<bool>,
    pub features: Vec<Feature>,
    /// Free passthrough, returned wherever the dossier's metadata goes.
    pub data: Map<String, Value>,
}

/// An event with the failure list it may have arrived as. `matter` absent means
/// the matter of the summons, which the `EPHOR_*` environment already named.
#[derive(Debug, Clone)]
pub struct Observation {
    pub matter: Option<String>,
    pub kind: String,
    pub time: Option<String>,
    pub gate: Option<Gate>,
    pub state: Option<String>,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub failures: Vec<Failure>,
    pub data: Map<String, Value>,
}

/// The event kind a `failures` list normalizes to: a check finished, and this
/// is what it found.
pub const CHECK: &str = "check";
/// The event kind a `gate` normalizes to.
pub const GATE: &str = "gate";

impl Envelope {
    /// Expand the conveniences and resolve the answer's paths against the
    /// directory the summons ran in (§FS-006-project-interface.4).
    pub fn normalize(self, place: &Path) -> Normalized {
        let mut events: Vec<Observation> = Vec::new();
        if let Some(gate) = self.gate {
            events.push(Observation {
                matter: None,
                kind: GATE.to_string(),
                time: None,
                gate: Some(gate),
                state: None,
                summary: None,
                url: None,
                failures: Vec::new(),
                data: Map::new(),
            });
        }
        if !self.failures.is_empty() {
            events.push(Observation {
                matter: None,
                kind: CHECK.to_string(),
                time: None,
                gate: None,
                state: None,
                summary: None,
                url: None,
                failures: self
                    .failures
                    .into_iter()
                    .map(|failure| failure.resolved(place))
                    .collect(),
                data: Map::new(),
            });
        }
        events.extend(self.events.into_iter().map(|event| Observation {
            matter: event.matter,
            kind: event.kind,
            time: event.time,
            gate: event.gate,
            state: event.state,
            summary: event.summary,
            url: event.url,
            failures: Vec::new(),
            data: event.data,
        }));
        Normalized {
            facts: Facts {
                summary: self.summary,
                url: self.url,
                needs_response: self.needs_response,
                features: self.features,
                data: self.data,
            },
            events,
            matters: self.matters,
            discussions: self.discussions,
            channels: self.channels,
        }
    }
}

impl Failure {
    fn resolved(mut self, place: &Path) -> Self {
        self.log = self.log.map(|log| {
            if log.is_absolute() {
                log
            } else {
                place.join(log)
            }
        });
        self
    }
}

impl Normalized {
    /// The metadata a program reads back, flattened under one vocabulary — the
    /// passthrough half of §FS-005-dispatch.8.
    pub fn metadata(&self) -> BTreeMap<String, String> {
        self.facts
            .data
            .iter()
            .map(|(key, value)| {
                let rendered = match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                (key.clone(), rendered)
            })
            .collect()
    }
}

/// Read one answer: parse it, hold it to the published schema, and expand its
/// conveniences. A command that wrote something the schema refuses is reported
/// rather than half-read — the exit code has already said whether the work
/// succeeded, and a malformed envelope is the command breaking the contract,
/// not the work failing.
pub fn parse(text: &str, verb: &str, place: &Path) -> Result<Normalized> {
    let value: Value = serde_json::from_str(text)
        .map_err(|err| EphorError::Command(format!("{verb}: answer is not JSON: {err}")))?;
    if let Some(error) = validator().iter_errors(&value).next() {
        return Err(EphorError::Command(format!(
            "{verb}: answer does not match the envelope schema at '{}': {error}",
            error.instance_path
        )));
    }
    let envelope: Envelope = serde_json::from_value(value)
        .map_err(|err| EphorError::Command(format!("{verb}: answer could not be read: {err}")))?;
    Ok(envelope.normalize(place))
}

/// The compiled schema. Built once: an answer is read per verb per refresh, and
/// compiling a schema per read would cost more than the read.
fn validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(ANSWER_SCHEMA).expect("embedded answer schema is valid JSON");
        jsonschema::validator_for(&schema).expect("embedded answer schema is a valid schema")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Normalized> {
        parse(text, "check", Path::new("/forest"))
    }

    #[test]
    fn the_embedded_schema_compiles() {
        // The schema ships to projects; a broken one would only be found by
        // the first command that wrote an answer.
        let _ = validator();
    }

    #[test]
    fn one_line_fields_become_facts() {
        let answer = read(r#"{"v":1,"summary":"3 tests failed","url":"https://ci/1"}"#).unwrap();
        assert_eq!(answer.facts.summary.as_deref(), Some("3 tests failed"));
        assert_eq!(answer.facts.url.as_deref(), Some("https://ci/1"));
        assert!(answer.events.is_empty());
    }

    #[test]
    fn failures_normalize_into_one_check_event() {
        let answer =
            read(r#"{"v":1,"failures":[{"job":"style","trace":"line too long"},{"job":"build"}]}"#)
                .unwrap();
        assert_eq!(answer.events.len(), 1);
        let event = &answer.events[0];
        assert_eq!(event.kind, CHECK);
        assert_eq!(event.matter, None);
        assert_eq!(event.failures.len(), 2);
        assert_eq!(event.failures[0].job, "style");
    }

    #[test]
    fn a_gate_normalizes_into_a_gate_event_with_its_repositories() {
        let answer = read(
            r#"{"v":1,"gate":{"repos":[{"repo":"acme/ce","failed":2},{"repo":"acme/ee"}],
                 "blocked":true}}"#,
        )
        .unwrap();
        let gate = answer.events[0].gate.as_ref().unwrap();
        assert_eq!(answer.events[0].kind, GATE);
        assert_eq!(gate.repos.len(), 2);
        assert_eq!(gate.repos[0].failed, Some(2));
        assert_eq!(gate.blocked, Some(true));
    }

    #[test]
    fn a_log_path_resolves_against_where_the_command_ran() {
        let answer =
            read(r#"{"v":1,"failures":[{"job":"build","log":"target/build.log"}]}"#).unwrap();
        assert_eq!(
            answer.events[0].failures[0].log.as_deref(),
            Some(Path::new("/forest/target/build.log"))
        );
    }

    #[test]
    fn an_absolute_log_path_is_left_alone() {
        let answer =
            read(r#"{"v":1,"failures":[{"job":"build","log":"/var/log/build.log"}]}"#).unwrap();
        assert_eq!(
            answer.events[0].failures[0].log.as_deref(),
            Some(Path::new("/var/log/build.log"))
        );
    }

    #[test]
    fn features_are_facts_not_events() {
        let answer =
            read(r#"{"v":1,"features":[{"id":"reflection","description":"Reflection"}]}"#).unwrap();
        assert_eq!(answer.facts.features.len(), 1);
        assert_eq!(answer.facts.features[0].id, "reflection");
        assert!(answer.events.is_empty());
    }

    #[test]
    fn spelled_out_events_keep_their_own_matter_key() {
        let answer = read(
            r#"{"v":1,"events":[{"matter":"ticket:GR-1","kind":"state","state":"resolved"}]}"#,
        )
        .unwrap();
        assert_eq!(answer.events[0].matter.as_deref(), Some("ticket:GR-1"));
        assert_eq!(answer.events[0].state.as_deref(), Some("resolved"));
    }

    #[test]
    fn discussions_and_their_tasks_survive_the_read() {
        let answer = read(
            r#"{"v":1,"discussions":[{"channel":"review","messages":[
                 {"author":"ada","text":"why?","task":{"state":"open","id":"t1"}}]}],
               "channels":[{"id":"review","can":["reply","react"]}]}"#,
        )
        .unwrap();
        let message = &answer.discussions[0].messages[0];
        let task = message.task.as_ref().unwrap();
        assert_eq!(task.state, "open");
        // The source's own resolve descriptor comes back verbatim.
        assert_eq!(task.descriptor.get("id").unwrap(), "t1");
        assert_eq!(answer.channels[0].can, vec!["reply", "react"]);
    }

    #[test]
    fn unknown_fields_are_ignored_so_the_envelope_can_grow() {
        let answer = read(r#"{"v":1,"summary":"ok","invented_later":{"a":1}}"#).unwrap();
        assert_eq!(answer.facts.summary.as_deref(), Some("ok"));
    }

    #[test]
    fn passthrough_data_returns_as_metadata() {
        let answer = read(r#"{"v":1,"data":{"build":"4711","flaky":true}}"#).unwrap();
        let metadata = answer.metadata();
        assert_eq!(metadata.get("build").map(String::as_str), Some("4711"));
        assert_eq!(metadata.get("flaky").map(String::as_str), Some("true"));
    }

    #[test]
    fn an_answer_the_schema_refuses_names_the_verb_and_the_place_in_it() {
        // `failures[].job` is required: a failure nobody can name is not one.
        let err = read(r#"{"v":1,"failures":[{"url":"https://ci/1"}]}"#).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("check"), "{message}");
        assert!(message.contains("/failures/0"), "{message}");
    }

    #[test]
    fn an_answer_of_the_wrong_version_is_refused() {
        let err = read(r#"{"v":2,"summary":"ok"}"#).unwrap_err();
        assert!(err.to_string().contains("envelope schema"), "{err}");
    }

    #[test]
    fn an_answer_that_is_not_json_is_refused_with_the_verb_named() {
        let err = read("not json at all").unwrap_err();
        assert!(
            err.to_string().starts_with("check: answer is not JSON"),
            "{err}"
        );
    }
}
