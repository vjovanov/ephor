//! Answering a workflow's inputs, and the entry that makes one an action
//! (§FS-005-dispatch.19).
//!
//! The workflow itself belongs to the binding and is described by
//! [`crate::work::runtime::workflow`]; what is here is ephor's half — which
//! entry names it, what its inputs are answered with, and where each answer
//! came from, so a reader sees that before anything is written.
//!
//! Six steps per input, each displacing the ones after it: what the reader
//! answered explicitly for this instantiation alone, what the reader supplied
//! in values files, what the entry says, what ephor answers for an input that
//! names who does the work, the workflow's own default, and — where an input
//! is required and still unanswered — the reader, asked or refused by name.
//! The order is §FS-005-dispatch.14's, deliberately, so one resolution order
//! covers everything a dispatch settles.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::feed::config::{ActionConfig, WorkflowAsk};
use crate::work::runtime::workflow::{Input, Kind, Workflow};

/// The icon an entry written beside a workflow gets when it named none — it
/// still has to look like the entries beside it.
const WORKFLOW_ICON: &str = "⛬";

/// An entry written beside a workflow, in the workflow's own directory
/// (§FS-005-dispatch.19). Which workflow is not written down: it is the one
/// the file sits beside.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beside {
    /// Defaults to the workflow's own id — the menu's id is what a hands
    /// table answers by (§FS-006-project-interface.9), and a workflow already
    /// has a name worth using.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Defaults to the workflow's own description.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub when: crate::work::recipe::Selector,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub requires_checkout: bool,
    /// Which branch this workflow's work belongs on, where the matter has none
    /// (§FS-005-dispatch.25).
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub hands: Vec<String>,
    /// This work needs nobody to start it (§FS-005-dispatch.28) — the same
    /// key the other two homes carry, so a workflow that travels with its
    /// entry travels with that too.
    #[serde(default)]
    pub autorun: bool,
}

impl Beside {
    /// The menu entry this is, filled in from the workflow it sits beside.
    pub fn action(&self, workflow: &Workflow) -> ActionConfig {
        ActionConfig {
            id: self.id.clone().unwrap_or_else(|| workflow.id.clone()),
            icon: self
                .icon
                .clone()
                .unwrap_or_else(|| WORKFLOW_ICON.to_string()),
            description: self
                .description
                .clone()
                .unwrap_or_else(|| workflow.description.clone()),
            command: String::new(),
            agent: None,
            workflow: Some(WorkflowAsk {
                name: workflow.id.clone(),
                inputs: self.inputs.clone(),
                hands: self.hands.clone(),
                autorun: self.autorun,
            }),
            hand: None,
            cwd: None,
            kinds: Vec::new(),
            when: self.when.clone(),
            requires: self.requires.clone(),
            requires_checkout: self.requires_checkout,
            branch: self.branch.clone(),
            minted: None,
            confirm: self.confirm,
            // What a workflow lays down is files, and running it is the move
            // after (§FS-005-dispatch.19): there is no terminal to take, and so
            // no window to open either (§FS-005-dispatch.22).
            background: false,
            window: false,
        }
    }
}

/// The entry written beside a workflow, where one was and it could be read.
/// A file that does not parse is reported rather than ignored: an entry
/// somebody wrote and ephor silently dropped is the failure that matters
/// (§FS-004-quick-actions.3).
pub fn beside(workflow: &Workflow) -> Result<Option<ActionConfig>, String> {
    let Some(text) = crate::work::runtime::workflow::entry_beside(workflow) else {
        return Ok(None);
    };
    match serde_json::from_str::<Beside>(&text) {
        Ok(entry) => Ok(Some(entry.action(workflow))),
        Err(err) => Err(format!(
            "the entry beside workflow '{}' could not be read: {err}",
            workflow.id
        )),
    }
}

/// Where one input's answer came from, for the account a reader is shown
/// before anything is written (§FS-005-dispatch.19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum From {
    /// The reader, for this instantiation alone.
    Reader,
    /// A values file the reader supplied for this instantiation alone.
    Values,
    /// The entry that names the workflow.
    Entry,
    /// ephor's answer for an input that names who does the work
    /// (§DA-006-hands-fill-a-workflows-targets).
    Hand,
    /// The workflow's own default, which ephor does not write back out.
    Default,
    /// Required, and nobody has answered it.
    Nobody,
}

impl From {
    pub fn label(self) -> &'static str {
        match self {
            From::Reader => "you",
            From::Values => "a values file",
            From::Entry => "the entry",
            From::Hand => "the hand",
            From::Default => "the workflow",
            From::Nobody => "nobody",
        }
    }
}

/// One input, answered.
#[derive(Debug, Clone)]
pub struct Answer {
    pub input: String,
    /// What the answer looks like on one line.
    pub shown: String,
    pub from: From,
}

/// Every input of one workflow, answered.
#[derive(Debug, Clone, Default)]
pub struct Answered {
    /// What is written out for the binding to read. Inputs standing at their
    /// own default are absent: the binding applies those itself, and writing
    /// them back would freeze today's default into every workspace.
    pub values: serde_json::Map<String, Value>,
    /// Every input and where its answer came from, in the workflow's own
    /// order.
    pub answers: Vec<Answer>,
    /// Required inputs nobody answered. Non-empty is not a failure here — it
    /// is what sends the reader to a prompt or a file, and what a dispatch
    /// with nobody to ask refuses by name (§FS-005-dispatch.19).
    pub missing: Vec<String>,
    /// What could not stand: a hand a narrowing does not permit, a hand the
    /// roster refused. Stated before anything is written.
    pub refusals: Vec<String>,
}

impl Answered {
    pub fn answer(&self, input: &str) -> Option<&Answer> {
        self.answers.iter().find(|answer| answer.input == input)
    }
}

/// How a hand named in configuration is turned into what the binding reads.
/// `Ok(None)` is "nobody chose", which leaves the workflow's own default
/// standing; `Err` is the refusal, said where it was named.
pub type Rendering<'a> = &'a dyn Fn(&str) -> Result<Option<String>, String>;

/// Refuse values-file fields the selected workflow does not declare. This is
/// separate from answering because a reader's typo must be rejected before
/// ephor accounts for any answers or prepares anywhere to write them
/// (§FS-005-dispatch.19).
pub fn validate_file_values(
    workflow: &Workflow,
    file_values: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    for name in file_values.keys() {
        if workflow.input(name).is_none() {
            return Err(format!(
                "workflow '{}' has no input named '{name}'",
                workflow.id
            ));
        }
    }
    Ok(())
}

/// Answer every input of `workflow` (§FS-005-dispatch.19).
///
/// `typed` is what the reader said explicitly for this instantiation alone, by
/// input name. `file_values` are the merged values files the reader supplied.
/// `ask` is the entry. `values` are the matter's fields, for the placeholders
/// a string answer may name. `hand` renders the hand this entry resolved to,
/// and `named` renders one the entry named by id — both are the runtime's
/// rendering, passed in because it lives in the adapter alone.
pub fn answer(
    workflow: &Workflow,
    ask: &WorkflowAsk,
    typed: &BTreeMap<String, String>,
    values: &BTreeMap<&'static str, String>,
    hand: Option<&str>,
    named: Rendering<'_>,
) -> Answered {
    answer_with_values(
        workflow,
        ask,
        typed,
        &serde_json::Map::new(),
        values,
        hand,
        named,
    )
}

/// Answer every input, including values loaded from the reader's files.
pub fn answer_with_values(
    workflow: &Workflow,
    ask: &WorkflowAsk,
    typed: &BTreeMap<String, String>,
    file_values: &serde_json::Map<String, Value>,
    values: &BTreeMap<&'static str, String>,
    hand: Option<&str>,
    named: Rendering<'_>,
) -> Answered {
    let mut out = Answered::default();
    for input in &workflow.inputs {
        let is_hand = input.hand || ask.hands.iter().any(|name| *name == input.name);
        let (value, from) =
            match answer_one(input, is_hand, ask, typed, file_values, values, hand, named) {
                Ok(answered) => answered,
                Err(refusal) => {
                    out.refusals.push(refusal);
                    continue;
                }
            };
        let shown = match &value {
            Some(value) => shown(value),
            None => match &input.default {
                Some(default) => shown(default),
                None => String::new(),
            },
        };
        if let Some(value) = value {
            out.values.insert(input.name.clone(), value);
        }
        if from == From::Nobody {
            out.missing.push(input.name.clone());
        }
        out.answers.push(Answer {
            input: input.name.clone(),
            shown,
            from,
        });
    }
    out
}

/// One input's six steps.
#[allow(clippy::too_many_arguments)]
fn answer_one(
    input: &Input,
    is_hand: bool,
    ask: &WorkflowAsk,
    typed: &BTreeMap<String, String>,
    file_values: &serde_json::Map<String, Value>,
    values: &BTreeMap<&'static str, String>,
    hand: Option<&str>,
    named: Rendering<'_>,
) -> Result<(Option<Value>, From), String> {
    // 1. What the reader answered, for this instantiation alone. On an input
    //    that names who does the work it is a hand's name like any other, so
    //    a narrowing binds what the reader typed too
    //    (§DA-006-hands-fill-a-workflows-targets).
    if let Some(word) = typed.get(&input.name) {
        // As the input's own type either way: an input wanting several hands
        // is answered with several, and one line of `--set` says so as a list
        // (§DA-006-hands-fill-a-workflows-targets).
        let said = coerce(word, input.kind);
        return match is_hand {
            true => Ok((Some(hands(&said, input, named)?), From::Reader)),
            false => Ok((Some(said), From::Reader)),
        };
    }
    // 2. What the reader supplied in a values file. Explicit `--set` above
    //    deliberately wins over this mapping.
    if let Some(file_value) = file_values.get(&input.name) {
        return match is_hand {
            true => Ok((Some(hands(file_value, input, named)?), From::Values)),
            false => Ok((Some(fill(file_value, values)), From::Values)),
        };
    }
    // 3. What the entry says.
    if let Some(written) = ask.inputs.get(&input.name) {
        return match is_hand {
            true => Ok((Some(hands(written, input, named)?), From::Entry)),
            false => Ok((Some(fill(written, values)), From::Entry)),
        };
    }
    // 4. ephor's answer for who does the work — the hand this entry resolved
    //    to, in the binding's own spelling.
    if is_hand {
        if let Some(hand) = hand {
            let rendered = Value::String(hand.to_string());
            return Ok((
                Some(match input.kind {
                    Kind::List => Value::Array(vec![rendered]),
                    _ => rendered,
                }),
                From::Hand,
            ));
        }
    }
    // 5. The workflow's own default, left where it is.
    if input.default.is_some() || !input.required {
        return Ok((None, From::Default));
    }
    // 6. Nobody.
    Ok((None, From::Nobody))
}

/// A hand-shaped answer: hand names, one or several, each rendered into what
/// the binding reads. Configuration names a hand by its id and nothing else
/// (§FS-005-dispatch.14), so this is the only spelling accepted here.
fn hands(written: &Value, input: &Input, named: Rendering<'_>) -> Result<Value, String> {
    let one = |name: &str| -> Result<Value, String> {
        // An empty answer is nobody choosing, not a hand name: preserve its
        // spelling and do not let the resolved hand below it take its place
        // (§FS-005-dispatch.19).
        if name.trim().is_empty() {
            return Ok(Value::String(name.to_string()));
        }
        match named(name)? {
            Some(rendered) => Ok(Value::String(rendered)),
            None => Err(format!(
                "input '{}' names who does the work, and '{name}' is not a hand this runtime \
                 can be asked for",
                input.name
            )),
        }
    };
    match written {
        Value::String(name) => match input.kind {
            Kind::List => Ok(Value::Array(vec![one(name)?])),
            _ => one(name),
        },
        Value::Array(names) => {
            let mut rendered = Vec::with_capacity(names.len());
            for name in names {
                let Value::String(name) = name else {
                    return Err(format!(
                        "input '{}' names who does the work, so every entry in it is a hand's id",
                        input.name
                    ));
                };
                rendered.push(one(name)?);
            }
            match input.kind {
                Kind::List => Ok(Value::Array(rendered)),
                _ => rendered.into_iter().next().ok_or_else(|| {
                    format!("input '{}' was answered with no hand at all", input.name)
                }),
            }
        }
        _ => Err(format!(
            "input '{}' names who does the work, so it is answered with a hand's id",
            input.name
        )),
    }
}

/// A written answer with the matter's fields filled in. Strings anywhere in
/// the value are rendered — the fields an item carries are as useful inside a
/// structure as beside one (§FS-005-dispatch.19).
fn fill(written: &Value, values: &BTreeMap<&'static str, String>) -> Value {
    match written {
        Value::String(text) => Value::String(crate::work::dossier::render(text, values)),
        Value::Array(items) => Value::Array(items.iter().map(|item| fill(item, values)).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), fill(value, values)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// One line the reader typed, as the input's own type. What does not parse as
/// the type asked for stays the text they typed: the binding validates its own
/// inputs, and guessing on their behalf would refuse a value it would accept.
///
/// Public within the crate because the hands named in such a line are
/// resolved before the answering runs, and one line cannot be read two ways
/// (§DA-006-hands-fill-a-workflows-targets).
pub(crate) fn coerce(word: &str, kind: Kind) -> Value {
    let text = || Value::String(word.to_string());
    match kind {
        Kind::Number => word
            .parse::<serde_json::Number>()
            .map(Value::Number)
            .unwrap_or_else(|_| text()),
        Kind::Flag => match word {
            "true" | "yes" | "on" => Value::Bool(true),
            "false" | "no" | "off" => Value::Bool(false),
            _ => text(),
        },
        Kind::List | Kind::Record => serde_json::from_str(word).unwrap_or_else(|_| text()),
        Kind::Text | Kind::Path => text(),
    }
}

/// One value on one line, for the account of what is about to be written.
fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, kind: Kind, required: bool, hand: bool) -> Input {
        Input {
            name: name.to_string(),
            description: String::new(),
            kind,
            required,
            default: None,
            hand,
            principal: false,
            choices: Vec::new(),
            of: None,
        }
    }

    fn workflow(inputs: Vec<Input>) -> Workflow {
        Workflow {
            id: "changeset-review".to_string(),
            description: "Review a code change.".to_string(),
            version: "1".to_string(),
            source: crate::work::runtime::workflow::Source::Runtime,
            dir: None,
            inputs,
        }
    }

    fn matter() -> BTreeMap<&'static str, String> {
        BTreeMap::from([
            ("branch", "you/ABC-42-retry".to_string()),
            ("repo", "acme/widget".to_string()),
            ("number", "42".to_string()),
        ])
    }

    fn roster(name: &str) -> Result<Option<String>, String> {
        match name {
            "luna" => Ok(Some("claude-code[high]:anthropic:opus".to_string())),
            "sol" => Ok(Some("codex[xhigh]:openai:gpt".to_string())),
            "walled" => Err("hand 'walled' is not permitted on this project".to_string()),
            _ => Ok(None),
        }
    }

    /// The entry written beside a workflow says `autorun` the way the other
    /// two homes do, so a workflow that travels with its entry travels with
    /// that too (§FS-005-dispatch.28).
    #[test]
    fn an_entry_beside_a_workflow_may_ask_to_run_itself() {
        let flow = workflow(Vec::new());
        let asked: Beside = serde_json::from_str(
            r#"{ "id": "fix-issue", "autorun": true, "when": { "kinds": ["issue"] } }"#,
        )
        .unwrap();
        assert!(asked.action(&flow).workflow.unwrap().autorun);

        // Silence is the key here as everywhere.
        let quiet: Beside = serde_json::from_str(r#"{ "id": "fix-issue" }"#).unwrap();
        assert!(!quiet.action(&flow).workflow.unwrap().autorun);
    }

    #[test]
    fn the_entry_answers_with_the_matters_fields() {
        let flow = workflow(vec![input("change_ref", Kind::Text, true, false)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "change_ref".to_string(),
                Value::String("{repo}#{number}".to_string()),
            )]),
            hands: Vec::new(),
            autorun: false,
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &roster);
        assert_eq!(
            out.values["change_ref"],
            Value::String("acme/widget#42".into())
        );
        assert_eq!(
            out.answer("change_ref").expect("answered").from,
            From::Entry
        );
        assert!(out.missing.is_empty());
        assert!(out.refusals.is_empty());
    }

    #[test]
    fn the_reader_displaces_the_entry() {
        let flow = workflow(vec![input("change_ref", Kind::Text, true, false)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "change_ref".to_string(),
                Value::String("{branch}".to_string()),
            )]),
            hands: Vec::new(),
            autorun: false,
        };
        let typed = BTreeMap::from([("change_ref".to_string(), "HEAD~3..HEAD".to_string())]);
        let out = answer(&flow, &ask, &typed, &matter(), None, &roster);
        assert_eq!(
            out.values["change_ref"],
            Value::String("HEAD~3..HEAD".into())
        );
        assert_eq!(
            out.answer("change_ref").expect("answered").from,
            From::Reader
        );
    }

    /// Values files are reader answers below explicit `--set`, above the
    /// entry, and retain their structured types while matter placeholders and
    /// execution-target policy are still applied.
    #[test]
    fn values_files_are_structured_reader_answers_with_set_precedence() {
        let flow = workflow(vec![
            input("change_ref", Kind::Text, true, false),
            input("review_focus", Kind::List, false, false),
            input("settings", Kind::Record, false, false),
            input("smart_target", Kind::Text, false, true),
        ]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "change_ref".to_string(),
                Value::String("entry-ref".to_string()),
            )]),
            hands: Vec::new(),
            autorun: false,
        };
        let file_values = serde_json::Map::from_iter([
            (
                "change_ref".to_string(),
                Value::String("file-ref".to_string()),
            ),
            (
                "review_focus".to_string(),
                serde_json::json!(["{repo}", {"field": "{number}"}]),
            ),
            (
                "settings".to_string(),
                serde_json::json!({"enabled": true, "limit": 3}),
            ),
            (
                "smart_target".to_string(),
                Value::String("luna".to_string()),
            ),
        ]);
        let typed = BTreeMap::from([("change_ref".to_string(), "set-ref".to_string())]);
        let out = answer_with_values(&flow, &ask, &typed, &file_values, &matter(), None, &roster);

        assert_eq!(out.values["change_ref"], Value::String("set-ref".into()));
        assert_eq!(out.answer("change_ref").unwrap().from, From::Reader);
        assert_eq!(
            out.values["review_focus"],
            serde_json::json!(["acme/widget", {"field": "42"}])
        );
        assert_eq!(out.answer("review_focus").unwrap().from, From::Values);
        assert_eq!(
            out.values["settings"],
            serde_json::json!({"enabled": true, "limit": 3})
        );
        assert_eq!(out.answer("settings").unwrap().from, From::Values);
        assert_eq!(
            out.values["smart_target"],
            Value::String("claude-code[high]:anthropic:opus".into())
        );
        assert_eq!(out.answer("smart_target").unwrap().from, From::Values);
    }

    #[test]
    fn an_unknown_values_file_input_is_refused_by_name() {
        let flow = workflow(vec![input("ci_commands", Kind::List, false, false)]);
        let file_values = serde_json::Map::from_iter([(
            "ci_comands".to_string(),
            serde_json::json!(["cargo test"]),
        )]);

        let refusal = validate_file_values(&flow, &file_values).expect_err("the typo is refused");
        assert_eq!(
            refusal,
            "workflow 'changeset-review' has no input named 'ci_comands'"
        );
    }

    #[test]
    fn a_values_file_cannot_bypass_execution_target_policy() {
        let flow = workflow(vec![input("smart_target", Kind::Text, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            ..WorkflowAsk::default()
        };
        let file_values = serde_json::Map::from_iter([(
            "smart_target".to_string(),
            Value::String("walled".to_string()),
        )]);
        let out = answer_with_values(
            &flow,
            &ask,
            &BTreeMap::new(),
            &file_values,
            &matter(),
            None,
            &roster,
        );
        assert_eq!(out.values.get("smart_target"), None);
        assert!(out.refusals[0].contains("not permitted"));
    }

    /// An empty execution target is the reader saying nobody chose, not a
    /// hand name. It remains a values-file answer and must not fall through
    /// to ephor's resolved hand (§FS-005-dispatch.19).
    #[test]
    fn an_empty_values_file_execution_target_resolves_to_nobody() {
        let flow = workflow(vec![input("smart_target", Kind::Text, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            ..WorkflowAsk::default()
        };
        let file_values = serde_json::Map::from_iter([(
            "smart_target".to_string(),
            Value::String(String::new()),
        )]);
        let named = |_: &str| -> Result<Option<String>, String> {
            panic!("nobody must not be resolved as a named hand")
        };
        let out = answer_with_values(
            &flow,
            &ask,
            &BTreeMap::new(),
            &file_values,
            &matter(),
            Some("claude-code[high]:anthropic:opus"),
            &named,
        );

        assert_eq!(out.values["smart_target"], Value::String(String::new()));
        assert_eq!(out.answer("smart_target").unwrap().from, From::Values);
        assert!(out.refusals.is_empty());
    }

    /// An explicit empty line has the same nobody reading as an empty value
    /// from a file, while retaining its stronger provenance
    /// (§FS-005-dispatch.19).
    #[test]
    fn an_empty_set_execution_target_resolves_to_nobody() {
        let flow = workflow(vec![input("smart_target", Kind::Text, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            ..WorkflowAsk::default()
        };
        let typed = BTreeMap::from([("smart_target".to_string(), String::new())]);
        let named = |_: &str| -> Result<Option<String>, String> {
            panic!("nobody must not be resolved as a named hand")
        };
        let out = answer(
            &flow,
            &ask,
            &typed,
            &matter(),
            Some("claude-code[high]:anthropic:opus"),
            &named,
        );

        assert_eq!(out.values["smart_target"], Value::String(String::new()));
        assert_eq!(out.answer("smart_target").unwrap().from, From::Reader);
        assert!(out.refusals.is_empty());
    }

    /// Nobody can occupy one position in a list without changing the other
    /// positions: empty names pass as written and non-empty names still
    /// resolve through the roster (§FS-005-dispatch.19).
    #[test]
    fn an_empty_execution_target_list_element_resolves_to_nobody() {
        let flow = workflow(vec![input("review_targets", Kind::List, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "review_targets".to_string(),
                serde_json::json!(["", "  ", "luna"]),
            )]),
            ..WorkflowAsk::default()
        };
        let named = |name: &str| -> Result<Option<String>, String> {
            assert_eq!(name, "luna", "nobody must not be resolved as a named hand");
            roster(name)
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &named);

        assert_eq!(
            out.values["review_targets"],
            serde_json::json!(["", "  ", "claude-code[high]:anthropic:opus"])
        );
        assert!(out.refusals.is_empty());
    }

    /// An input wanting several hands is answered with several, said on one
    /// line — which is what the screen writes when the reader takes more than
    /// one, and what `--set <input>=["a","b"]` says on the command line
    /// (§DA-006-hands-fill-a-workflows-targets).
    #[test]
    fn one_line_may_name_several_hands_for_an_input_that_wants_several() {
        let flow = workflow(vec![input("review_targets", Kind::List, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::new(),
            hands: Vec::new(),
            autorun: false,
        };
        let typed = BTreeMap::from([(
            "review_targets".to_string(),
            r#"["luna","sol"]"#.to_string(),
        )]);
        let out = answer(&flow, &ask, &typed, &matter(), None, &roster);
        assert_eq!(
            out.values["review_targets"],
            Value::Array(vec![
                Value::String("claude-code[high]:anthropic:opus".into()),
                Value::String("codex[xhigh]:openai:gpt".into()),
            ])
        );

        // And one hand said plainly is still one hand, wrapped as the list the
        // input wants.
        let typed = BTreeMap::from([("review_targets".to_string(), "luna".to_string())]);
        let out = answer(&flow, &ask, &typed, &matter(), None, &roster);
        assert_eq!(
            out.values["review_targets"],
            Value::Array(vec![Value::String(
                "claude-code[high]:anthropic:opus".into()
            )])
        );

        // A hand the narrowing refuses is refused wherever it was named, list
        // or no list.
        let typed = BTreeMap::from([(
            "review_targets".to_string(),
            r#"["luna","walled"]"#.to_string(),
        )]);
        let out = answer(&flow, &ask, &typed, &matter(), None, &roster);
        assert!(out.values.is_empty());
        assert_eq!(out.refusals.len(), 1, "{:?}", out.refusals);
    }

    #[test]
    fn a_required_input_nobody_answered_is_missing_and_not_a_refusal() {
        let flow = workflow(vec![
            input("change_ref", Kind::Text, true, false),
            input("review_focus", Kind::List, false, false),
        ]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            ..WorkflowAsk::default()
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &roster);
        assert_eq!(out.missing, vec!["change_ref".to_string()]);
        // An optional input nobody answered stands at the workflow's own
        // default and is never written back out.
        assert!(!out.values.contains_key("review_focus"));
        assert_eq!(
            out.answer("review_focus").expect("answered").from,
            From::Default
        );
    }

    #[test]
    fn the_hand_fills_an_input_that_names_who_does_the_work() {
        let flow = workflow(vec![
            input("smart_target", Kind::Text, false, true),
            input("review_targets", Kind::List, false, true),
        ]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            ..WorkflowAsk::default()
        };
        let out = answer(
            &flow,
            &ask,
            &BTreeMap::new(),
            &matter(),
            Some("claude-code[high]:anthropic:opus"),
            &roster,
        );
        assert_eq!(
            out.values["smart_target"],
            Value::String("claude-code[high]:anthropic:opus".into())
        );
        // A list-shaped one takes the same hand as a list of one.
        assert_eq!(
            out.values["review_targets"],
            Value::Array(vec![Value::String(
                "claude-code[high]:anthropic:opus".into()
            )])
        );
        assert_eq!(
            out.answer("smart_target").expect("answered").from,
            From::Hand
        );
    }

    #[test]
    fn an_entry_names_hands_by_id_and_a_spread_is_restated_in_ephors_words() {
        let flow = workflow(vec![input("review_targets", Kind::List, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "review_targets".to_string(),
                Value::Array(vec![
                    Value::String("luna".to_string()),
                    Value::String("sol".to_string()),
                ]),
            )]),
            hands: Vec::new(),
            autorun: false,
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &roster);
        assert_eq!(
            out.values["review_targets"],
            Value::Array(vec![
                Value::String("claude-code[high]:anthropic:opus".into()),
                Value::String("codex[xhigh]:openai:gpt".into()),
            ])
        );
    }

    #[test]
    fn an_entry_may_say_which_input_is_a_hand_when_the_workflow_did_not() {
        let flow = workflow(vec![input("smart_target", Kind::Text, false, false)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "smart_target".to_string(),
                Value::String("luna".to_string()),
            )]),
            hands: vec!["smart_target".to_string()],
            autorun: false,
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &roster);
        assert_eq!(
            out.values["smart_target"],
            Value::String("claude-code[high]:anthropic:opus".into())
        );
    }

    #[test]
    fn a_narrowing_refuses_where_the_hand_was_named() {
        let flow = workflow(vec![input("smart_target", Kind::Text, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "smart_target".to_string(),
                Value::String("walled".to_string()),
            )]),
            hands: Vec::new(),
            autorun: false,
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &roster);
        assert_eq!(out.refusals.len(), 1);
        assert!(out.refusals[0].contains("not permitted"));
        assert!(!out.values.contains_key("smart_target"));
    }

    #[test]
    fn a_hand_nobody_has_heard_of_is_refused_by_name() {
        let flow = workflow(vec![input("smart_target", Kind::Text, false, true)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "smart_target".to_string(),
                Value::String("nobody".to_string()),
            )]),
            hands: Vec::new(),
            autorun: false,
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &roster);
        assert_eq!(out.refusals.len(), 1);
        assert!(out.refusals[0].contains("'nobody' is not a hand"));
    }

    #[test]
    fn a_typed_line_takes_the_inputs_own_type() {
        let flow = workflow(vec![
            input("review_passes", Kind::Number, false, false),
            input("harvest_pc", Kind::Flag, false, false),
            input("review_focus", Kind::List, false, false),
            input("agent_timeout", Kind::Text, false, false),
        ]);
        let typed = BTreeMap::from([
            ("review_passes".to_string(), "3".to_string()),
            ("harvest_pc".to_string(), "false".to_string()),
            ("review_focus".to_string(), "[\"perf\"]".to_string()),
            ("agent_timeout".to_string(), "30m".to_string()),
        ]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            ..WorkflowAsk::default()
        };
        let out = answer(&flow, &ask, &typed, &matter(), None, &roster);
        assert_eq!(out.values["review_passes"], serde_json::json!(3));
        assert_eq!(out.values["harvest_pc"], Value::Bool(false));
        assert_eq!(out.values["review_focus"], serde_json::json!(["perf"]));
        assert_eq!(out.values["agent_timeout"], Value::String("30m".into()));
    }

    #[test]
    fn a_line_that_is_not_the_type_asked_for_stays_what_was_typed() {
        let flow = workflow(vec![input("review_passes", Kind::Number, false, false)]);
        let typed = BTreeMap::from([("review_passes".to_string(), "twice".to_string())]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            ..WorkflowAsk::default()
        };
        let out = answer(&flow, &ask, &typed, &matter(), None, &roster);
        assert_eq!(out.values["review_passes"], Value::String("twice".into()));
    }

    #[test]
    fn strings_inside_a_structure_are_filled_too() {
        let flow = workflow(vec![input("personalities", Kind::List, false, false)]);
        let ask = WorkflowAsk {
            name: flow.id.clone(),
            inputs: BTreeMap::from([(
                "personalities".to_string(),
                serde_json::json!([{ "id": "a", "stance": "review {branch}" }]),
            )]),
            hands: Vec::new(),
            autorun: false,
        };
        let out = answer(&flow, &ask, &BTreeMap::new(), &matter(), None, &roster);
        assert_eq!(
            out.values["personalities"],
            serde_json::json!([{ "id": "a", "stance": "review you/ABC-42-retry" }])
        );
    }

    #[test]
    fn an_entry_beside_a_workflow_fills_itself_in_from_it() {
        let flow = workflow(vec![input("change_ref", Kind::Text, true, false)]);
        let entry: Beside = serde_json::from_str(
            "{\"when\": {\"kinds\": [\"pr\"]}, \"inputs\": {\"change_ref\": \"{branch}\"}}",
        )
        .expect("parses");
        let action = entry.action(&flow);
        assert_eq!(action.id, "changeset-review");
        assert_eq!(action.description, "Review a code change.");
        assert_eq!(action.icon, WORKFLOW_ICON);
        assert_eq!(action.when.kinds, vec!["pr".to_string()]);
        assert_eq!(
            action.workflow.expect("names one").inputs["change_ref"],
            Value::String("{branch}".into())
        );
        // Nothing said, nothing minted: the entry places its work through the
        // matter as it always did (§FS-005-dispatch.25).
        assert!(action.branch.is_none());
    }

    /// An entry beside a workflow may say which branch its work belongs on,
    /// for the matter that has none (§FS-005-dispatch.25) — the third home an
    /// entry lives in, reading the key the other two read.
    #[test]
    fn an_entry_beside_a_workflow_may_say_the_branch_its_work_belongs_on() {
        let flow = workflow(vec![input("change_ref", Kind::Text, true, false)]);
        let entry: Beside = serde_json::from_str(
            "{\"when\": {\"kinds\": [\"issue\"]}, \"branch\": \"fix/issue-{number}\"}",
        )
        .expect("parses");
        assert_eq!(
            entry.action(&flow).branch.as_deref(),
            Some("fix/issue-{number}")
        );
    }
}
