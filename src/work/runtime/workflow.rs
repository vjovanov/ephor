//! Workflows the binding offers, and rendering one into a plan
//! (§AR-007-runtime.1, §FS-005-dispatch.19).
//!
//! A workflow is the binding's: a named, parameterized plan that lays down
//! tasks of its own under a machine of its own. What leaves this module is an
//! id, a description, and typed inputs with none of the binding's grammar on
//! them — above here nobody knows what a template directory is, what renders
//! one, or what its manifest is called.
//!
//! Which workflows there are is asked of the binding rather than kept as a
//! list of ephor's, for the reason the roster is
//! (§DA-004-roster-is-asked-not-configured): the binding's own JSON listing,
//! honored by this one binding the way custom-status's stdout is
//! (§AR-002-summons.3). Where the binding keeps a workflow as a directory,
//! two things are read beside it: the input properties the listing leaves
//! out — which input is an execution target, which is the principal one —
//! scanned out of the manifest the way a states document is scanned in
//! [`super::plan`], right enough to fill an input and never the authority on
//! anything else; and the entry that makes the workflow an action, handed up
//! as bytes because this module knows where such a file sits and never what
//! it means.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{EphorError, Result};
use crate::seams::summons::{self, quote, Mode, Site, Summons};
use crate::work::recipe::WorkConfig;

use super::{runner, said};

/// The verb this fills, for messages.
pub const VERB: &str = "work.workflows";

/// The verb rendering one fills.
pub const LAY_VERB: &str = "work.lay";

/// Listing is a read of the binding's own registry; it answers or it does not.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// Rendering is a file render plus whatever validation the binding does on the
/// way out. Generous, and still bounded: a reader is holding the screen.
const LAY_TIMEOUT: Duration = Duration::from_secs(120);

/// What a workflow's inputs may be, in ephor's words. The binding's own type
/// names are mapped here and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Number,
    Flag,
    Path,
    List,
    Record,
}

impl Kind {
    fn of(name: &str) -> Kind {
        match name {
            "number" => Kind::Number,
            "boolean" => Kind::Flag,
            "path" => Kind::Path,
            "array" => Kind::List,
            "object" => Kind::Record,
            _ => Kind::Text,
        }
    }

    /// How a person is told what an input wants.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Number => "number",
            Kind::Flag => "true/false",
            Kind::Path => "path",
            Kind::List => "list",
            Kind::Record => "record",
        }
    }

    /// Whether a value for this can be typed on one line. A list or a record
    /// cannot, which is what sends the reader to a file instead
    /// (§FS-005-dispatch.19).
    pub fn is_scalar(self) -> bool {
        !matches!(self, Kind::List | Kind::Record)
    }
}

/// One input a workflow takes.
#[derive(Debug, Clone)]
pub struct Input {
    pub name: String,
    pub description: String,
    pub kind: Kind,
    pub required: bool,
    /// What it stands at when nobody says. Kept for what a preview shows; the
    /// binding applies it itself, so ephor never writes it back out.
    pub default: Option<serde_json::Value>,
    /// The workflow declares this input an execution target: a hand, in
    /// ephor's words (§DA-006-hands-fill-a-workflows-targets). True of a list
    /// whose elements are execution targets too — such an input is answered
    /// with hands, several at a time, which is what that record settles.
    pub hand: bool,
    /// The workflow names this its principal input — the one thing a person
    /// must say. Used to decide what a single line may answer.
    pub principal: bool,
    /// The values this input may take, where the binding's own check on it
    /// spells them out plainly. Empty where it does not: what is not a known
    /// set is typed, and this reading is a convenience rather than a second
    /// authority on what the binding accepts (§FS-005-dispatch.19).
    pub choices: Vec<String>,
    /// What one element of a list is, where the binding publishes it. None on
    /// everything else, and on a list it describes no further.
    pub of: Option<Element>,
}

/// One value inside another: an element of a list, described the way an input
/// is. Enough to answer it in its own shape and no deeper — a list of records
/// is a record's worth of nesting past what a row can carry, and goes to the
/// editor whole (§FS-005-dispatch.19).
#[derive(Debug, Clone)]
pub struct Element {
    pub kind: Kind,
    pub hand: bool,
    pub choices: Vec<String>,
}

/// Where a workflow came from, which is also where an entry naming it ranks
/// in the menu (§FS-005-dispatch.19). The binding's own words for these
/// places are mapped here and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The binding ships it, so it ranks with what ephor ships.
    Runtime,
    /// The project keeps it beside its checkout.
    Project,
    /// The person keeps it across projects.
    Person,
}

impl Source {
    fn of(word: &str) -> Source {
        match word {
            "project" => Source::Project,
            "user" => Source::Person,
            _ => Source::Runtime,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Runtime => "the runtime",
            Source::Project => "this project",
            Source::Person => "yours",
        }
    }
}

/// One workflow the binding offers.
#[derive(Debug, Clone)]
pub struct Workflow {
    pub id: String,
    pub description: String,
    pub version: String,
    /// Where the binding found it, and so where an entry naming it ranks.
    pub source: Source,
    /// Some where the binding keeps it as a directory a reader can write
    /// into; None where it carries the workflow inside itself.
    pub dir: Option<PathBuf>,
    pub inputs: Vec<Input>,
}

impl Workflow {
    pub fn input(&self, name: &str) -> Option<&Input> {
        self.inputs.iter().find(|input| input.name == name)
    }

    /// The inputs that must be answered for the workflow to render at all.
    pub fn required(&self) -> impl Iterator<Item = &Input> {
        self.inputs.iter().filter(|input| input.required)
    }

    /// How the binding is told which workflow to render: its directory where
    /// there is one — unambiguous, and independent of what is discoverable
    /// from the place the render runs in — and its name otherwise.
    fn reference(&self) -> String {
        match &self.dir {
            Some(dir) => dir.to_string_lossy().into_owned(),
            None => self.id.clone(),
        }
    }
}

/// Everything the binding offers, or the one sentence why it offers nothing.
#[derive(Debug, Clone, Default)]
pub struct Offered {
    pub workflows: Vec<Workflow>,
    /// The *workable* rung's own sentence where no runtime is bound
    /// (§FS-006-project-interface.10). The workflows are empty then for the
    /// reason the roster is (§AR-007-runtime.3).
    pub refusal: Option<String>,
}

impl Offered {
    pub fn find(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|workflow| workflow.id == id)
    }
}

/// What the binding offers, asked at `at` — the place matters, because a
/// project keeps workflows of its own beside its checkout and the binding
/// resolves those relative to where it is run.
pub fn offered(config: &WorkConfig, at: &Path) -> Offered {
    if let Some(refusal) = super::refusal(config) {
        return Offered {
            workflows: Vec::new(),
            refusal: Some(refusal),
        };
    }
    let command = format!("{} templates --json", runner(config));
    let listed = summons::run(
        &Summons::new(VERB, command),
        &Site::root(at),
        Mode::Captured(LIST_TIMEOUT),
    )
    .ok()
    .filter(summons::Answer::is_done)
    .and_then(|answer| answer.output)
    .and_then(|output| parse(&output))
    .unwrap_or_default();
    Offered {
        workflows: listed,
        refusal: None,
    }
}

/// The binding's listing, as workflows. None where it did not parse — the
/// binding answering something ephor does not understand is the binding
/// offering nothing, never a crash.
fn parse(output: &str) -> Option<Vec<Workflow>> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(output.trim()).ok()?;
    Some(rows.iter().filter_map(workflow_of).collect())
}

fn workflow_of(row: &serde_json::Value) -> Option<Workflow> {
    let word = |key: &str| {
        row.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let id = row.get("name")?.as_str()?.to_string();
    // A path the binding reports that is a directory on disk is one a reader
    // can put a file beside; anything else is the binding's own to keep.
    let dir = row
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .filter(|path| path.is_dir());
    let marks = dir.as_deref().map(marks_in).unwrap_or_default();
    let inputs = row
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .map(|inputs| {
            inputs
                .iter()
                .filter_map(|input| {
                    let name = input.get("name")?.as_str()?.to_string();
                    let mark = marks.iter().find(|mark| mark.name == name);
                    let own = element_of(input);
                    let of = input
                        .get("items")
                        .filter(|items| !items.is_null())
                        .map(element_of);
                    Some(Input {
                        kind: own.kind,
                        description: input
                            .get("description")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        required: input
                            .get("required")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                        default: input
                            .get("default")
                            .filter(|value| !value.is_null())
                            .cloned(),
                        // What the listing says, and what the manifest beside
                        // the workflow says where the listing is an older
                        // binding's and says nothing. A list of execution
                        // targets is an input answered with hands too
                        // (§DA-006-hands-fill-a-workflows-targets).
                        hand: own.hand
                            || of.as_ref().is_some_and(|element| element.hand)
                            || mark.is_some_and(|mark| mark.hand),
                        principal: input.get("positional").is_some_and(|slot| !slot.is_null())
                            || mark.is_some_and(|mark| mark.principal),
                        choices: own.choices,
                        of,
                        name,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Workflow {
        description: word("description"),
        version: word("version"),
        source: Source::of(&word("source")),
        dir,
        inputs,
        id,
    })
}

/// One value schema as the binding publishes it: what it is, whether it names
/// who does the work, and the set it is chosen from where its own check spells
/// one out. Read the same way at either depth, so an array of execution
/// targets is recognized by the same lines that recognize one
/// (§FS-005-dispatch.19).
fn element_of(schema: &serde_json::Value) -> Element {
    let word = |key: &str| schema.get(key).and_then(serde_json::Value::as_str);
    Element {
        kind: Kind::of(word("type").unwrap_or("string")),
        hand: word("format") == Some(TARGET_FORMAT),
        choices: word("validate").map(choices_in).unwrap_or_default(),
    }
}

/// The values a check permits, where it is plainly a list of them — an
/// anchored alternation of literal words and nothing else, which is how a
/// workflow spells a small set of choices in a grammar that has no other way
/// to say it. Anything with real pattern in it is no set at all and answers
/// with none, because half-reading somebody else's regular expression and
/// offering the reader four of the six values it permits is worse than
/// offering nothing (§FS-005-dispatch.19).
fn choices_in(pattern: &str) -> Vec<String> {
    let body = pattern
        .strip_prefix('^')
        .or_else(|| pattern.strip_prefix("\\A"))
        .unwrap_or(pattern);
    let body = body
        .strip_suffix('$')
        .or_else(|| body.strip_suffix("\\z"))
        .unwrap_or(body);
    // One pair of brackets around the whole of it is grouping, not pattern.
    let body = body
        .strip_prefix("(?:")
        .or_else(|| body.strip_prefix('('))
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(body);
    let plain = |word: &str| {
        !word.is_empty()
            && word
                .chars()
                .all(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.' | ' ' | '/'))
    };
    let words: Vec<String> = body.split('|').map(str::to_string).collect();
    match words.len() > 1 && words.iter().all(|word| plain(word)) {
        true => words,
        false => Vec::new(),
    }
}

/// What the listing leaves out about one input.
#[derive(Debug, Default)]
struct Mark {
    name: String,
    hand: bool,
    principal: bool,
}

/// The binding declares an input to be one of its execution targets with this
/// word (§DA-006-hands-fill-a-workflows-targets).
const TARGET_FORMAT: &str = "execution-target";

/// What the manifest says about each input that the listing does not: which
/// is an execution target, and which is the principal one. A line scan rather
/// than a parse, for the reason [`super::plan`] scans a states document: it
/// only has to be right enough to fill an input, and the binding's own
/// validation is the authority on everything else. Keys are read only at the
/// depth an input's own keys sit at, so a folded description that happens to
/// contain one of these words is not mistaken for a declaration.
fn marks_in(dir: &Path) -> Vec<Mark> {
    let Ok(text) = std::fs::read_to_string(dir.join(MANIFEST)) else {
        return Vec::new();
    };
    let mut marks: Vec<Mark> = Vec::new();
    let mut key_indent = 0usize;
    // The one place the scan goes deeper: what a list's elements are is a
    // block of its own, and an input whose elements are execution targets is
    // answered with hands like one that is (§DA-006-hands-fill-a-workflows-targets).
    let mut in_elements = false;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("- name:") {
            key_indent = indent + 2;
            in_elements = false;
            marks.push(Mark {
                name: scalar(rest),
                ..Mark::default()
            });
            continue;
        }
        if indent < key_indent {
            in_elements = false;
        }
        let Some(mark) = marks.last_mut() else {
            continue;
        };
        if indent > key_indent {
            if in_elements {
                if let Some(rest) = trimmed.strip_prefix("format:") {
                    mark.hand |= scalar(rest) == TARGET_FORMAT;
                }
            }
            continue;
        }
        if indent != key_indent {
            continue;
        }
        in_elements = trimmed == "items:";
        if let Some(rest) = trimmed.strip_prefix("format:") {
            mark.hand = scalar(rest) == TARGET_FORMAT;
        }
        if trimmed.starts_with("positional:") {
            mark.principal = true;
        }
    }
    marks
}

/// A scalar YAML value as it is written on one line.
fn scalar(rest: &str) -> String {
    rest.trim()
        .trim_matches(['"', '\''].as_slice())
        .trim()
        .to_string()
}

/// The binding's manifest for one workflow.
const MANIFEST: &str = "template.yaml";

/// The entry that makes a workflow an action, where one was written beside it
/// (§FS-005-dispatch.19). Bytes, not meaning: this module knows where such a
/// file sits, and core knows what is in it.
pub const ENTRY: &str = ".ephor.json";

/// What was written beside `workflow`, where anything was.
pub fn entry_beside(workflow: &Workflow) -> Option<String> {
    let dir = workflow.dir.as_deref()?;
    std::fs::read_to_string(dir.join(ENTRY)).ok()
}

/// Render a workflow into a plan of its own under `output`, with the values
/// ephor resolved (§FS-005-dispatch.19). Run from `at` — the checkout, not
/// the work root — because the binding resolves an input naming a file
/// relative to where it is asked.
///
/// `Ok` is the plan written; `Err` is the binding's own refusal in its own
/// first words, the way a cancel reports one
/// (§DA-005-cancel-is-the-runtimes-move). Nothing is kept behind a refusal:
/// half a workspace is something the operations board would report on
/// (§FS-005-dispatch.15).
pub fn lay(
    config: &WorkConfig,
    at: &Path,
    workflow: &Workflow,
    values: &Path,
    output: &Path,
    dry_run: bool,
) -> Result<String> {
    let answer = summons::run(
        &Summons::new(
            LAY_VERB,
            lay_command(config, workflow, values, output, dry_run),
        ),
        &Site::root(at),
        Mode::Captured(LAY_TIMEOUT),
    )?;
    if answer.is_done() {
        return Ok(answer.output.unwrap_or_default());
    }
    let said = said(answer.output.as_deref().unwrap_or(""));
    Err(EphorError::Command(match said.is_empty() {
        true => format!(
            "{} {} refused: {}",
            runner(config),
            INSTANTIATE,
            answer.refusal(LAY_VERB)
        ),
        false => format!("{} {} refused: {said}", runner(config), INSTANTIATE),
    }))
}

/// `<runner> instantiate <workflow> --values <file> --output <dir>`, quoted
/// for `sh`, with the binding's standard error folded into what is captured —
/// its refusal is written there, and that is the one thing worth reading back.
fn lay_command(
    config: &WorkConfig,
    workflow: &Workflow,
    values: &Path,
    output: &Path,
    dry_run: bool,
) -> String {
    let mut words = vec![
        runner(config).to_string(),
        INSTANTIATE.to_string(),
        quote(&workflow.reference()),
        VALUES_FLAG.to_string(),
        quote(&values.to_string_lossy()),
        OUTPUT_FLAG.to_string(),
        quote(&output.to_string_lossy()),
    ];
    if dry_run {
        words.push(DRY_RUN_FLAG.to_string());
    }
    words.push("2>&1".to_string());
    words.join(" ")
}

/// The plan a render left behind under `output`, whichever shape the workflow
/// has: the directory workspace itself, or the one plan file it wrote there.
/// Recognizing a plan is this module's grammar (§AR-007-runtime.1), so a
/// caller gets back an id and a path and never the suffix.
pub fn laid(output: &Path) -> Option<super::plan::FoundPlan> {
    let name = output.file_name()?.to_string_lossy().into_owned();
    let index = output.join(format!("index{}", super::plan::PLAN_SUFFIX));
    if index.is_file() {
        return Some(super::plan::FoundPlan {
            plan_id: name,
            path: index,
        });
    }
    super::plan::plans_in(output).into_iter().next()
}

/// The binding's verb for rendering a workflow, and the flags it takes. Part
/// of the coupling, and so part of this module (§AR-007-runtime.1).
const INSTANTIATE: &str = "instantiate";
const VALUES_FLAG: &str = "--values";
const OUTPUT_FLAG: &str = "--output";
const DRY_RUN_FLAG: &str = "--dry-run";

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"[
      {"name": "changeset-review", "version": "1.1.0", "source": "built-in",
       "path": "changeset-review", "description": "Review a code change.",
       "inputs": [
         {"name": "change_ref", "description": "The change.", "type": "string",
          "required": true, "default": null, "validate": null},
         {"name": "review_targets", "description": "Reviewers.", "type": "array",
          "required": false, "default": ["a", "b"], "validate": null}
       ]}
    ]"#;

    #[test]
    fn a_listing_becomes_workflows() {
        let workflows = parse(LISTING).expect("parses");
        assert_eq!(workflows.len(), 1);
        let workflow = &workflows[0];
        assert_eq!(workflow.id, "changeset-review");
        assert_eq!(workflow.source, Source::Runtime);
        // A path the binding names that is not a directory here is one it
        // keeps to itself: nothing can be written beside it.
        assert!(workflow.dir.is_none());
        assert_eq!(workflow.inputs.len(), 2);
        assert!(workflow.input("change_ref").expect("input").required);
        assert_eq!(
            workflow.input("review_targets").expect("input").kind,
            Kind::List
        );
        assert_eq!(workflow.required().count(), 1);
    }

    /// Every key the binding publishes about an input is read, so a form can
    /// be built from the listing alone — which is the only route open for a
    /// workflow the binding keeps inside itself and has no directory for
    /// (§DA-006-hands-fill-a-workflows-targets).
    #[test]
    fn the_listing_says_which_inputs_are_hands_and_what_a_set_holds() {
        const PUBLISHED: &str = r#"[
          {"name": "changeset-review", "version": "1.1.0", "source": "built-in",
           "path": "changeset-review", "description": "Review a code change.",
           "inputs": [
             {"name": "change_ref", "type": "string", "required": true,
              "positional": 1, "default": null, "validate": null,
              "format": null, "items": null, "properties": null},
             {"name": "review_targets", "type": "array", "required": false,
              "default": ["a", "b"], "validate": null, "format": null,
              "properties": null,
              "items": {"type": "string", "format": "execution-target",
                        "validate": null, "items": null, "properties": null}},
             {"name": "smart_target", "type": "string", "required": false,
              "default": "x", "format": "execution-target", "validate": null,
              "items": null, "properties": null},
             {"name": "fix_prepare", "type": "string", "required": false,
              "default": "none", "format": null, "items": null,
              "properties": null,
              "validate": "^(none|branch|worktree|fork)$"},
             {"name": "paper_id", "type": "string", "required": false,
              "default": "submission", "format": null, "items": null,
              "properties": null, "validate": "^[a-z]+-[0-9]{2,}$"}
           ]}
        ]"#;
        let workflows = parse(PUBLISHED).expect("parses");
        let workflow = &workflows[0];
        // A scalar execution target is a hand, and so is a list of them: such
        // an input is answered with hands, several at a time.
        assert!(workflow.input("smart_target").expect("input").hand);
        let several = workflow.input("review_targets").expect("input");
        assert!(several.hand);
        assert_eq!(several.kind, Kind::List);
        assert!(several.of.as_ref().expect("elements").hand);
        // The principal input, which the binding names by giving it a slot.
        assert!(workflow.input("change_ref").expect("input").principal);
        assert!(!workflow.input("smart_target").expect("input").principal);
        // A check that is plainly a set of words is one; a check with real
        // pattern in it is not a set at all.
        assert_eq!(
            workflow.input("fix_prepare").expect("input").choices,
            vec!["none", "branch", "worktree", "fork"]
        );
        assert!(workflow
            .input("paper_id")
            .expect("input")
            .choices
            .is_empty());
    }

    /// An older binding that publishes none of it still answers, and what the
    /// manifest beside the workflow says fills in what the listing left out.
    #[test]
    fn a_listing_that_says_none_of_it_falls_back_to_the_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(MANIFEST),
            "name: house-style\ninputs:\n  - name: reviewers\n    type: array\n    items:\n      \
             type: string\n      format: execution-target\n  - name: note\n    type: string\n",
        )
        .expect("write");
        let listing = format!(
            r#"[{{"name": "house-style", "source": "project", "path": {},
                 "description": "d",
                 "inputs": [{{"name": "reviewers", "type": "array", "required": false}},
                            {{"name": "note", "type": "string", "required": false}}]}}]"#,
            serde_json::Value::String(dir.path().to_string_lossy().into_owned())
        );
        let workflows = parse(&listing).expect("parses");
        let workflow = &workflows[0];
        assert!(workflow.input("reviewers").expect("input").hand);
        assert!(!workflow.input("note").expect("input").hand);
    }

    /// What a check permits is read only where it plainly says so: a pattern
    /// half-read is four of six values offered as if they were all of them.
    #[test]
    fn a_set_is_read_out_of_a_check_only_where_it_is_plainly_one() {
        assert_eq!(choices_in("^(a|b)$"), vec!["a", "b"]);
        assert_eq!(choices_in("a|b"), vec!["a", "b"]);
        assert_eq!(choices_in("^(?:pr|push)$"), vec!["pr", "push"]);
        assert!(choices_in("^[a-z]+$").is_empty());
        assert!(choices_in("^(a|b+)$").is_empty());
        assert!(choices_in("^none$").is_empty());
        assert!(choices_in("").is_empty());
    }

    #[test]
    fn nonsense_is_no_workflows_rather_than_a_crash() {
        assert!(parse("not json at all").is_none());
        assert!(parse("{}").is_none());
    }

    #[test]
    fn the_manifest_says_which_input_is_a_hand() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(MANIFEST),
            "name: probe\ninputs:\n  - name: conference\n    type: string\n    positional: 1\n  \
             - name: intake_target\n    type: string\n    format: execution-target\n  \
             - name: note\n    type: string\n    description: >-\n      a folded line that \
             says format: execution-target inside prose\n",
        )
        .expect("write");
        let marks = marks_in(dir.path());
        assert_eq!(marks.len(), 3);
        assert!(marks[0].principal && !marks[0].hand);
        assert!(marks[1].hand && !marks[1].principal);
        // The word inside a folded description is prose, not a declaration.
        assert!(!marks[2].hand);
    }

    #[test]
    fn a_missing_manifest_marks_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(marks_in(dir.path()).is_empty());
    }

    #[test]
    fn the_render_names_the_workflow_its_values_and_where_it_goes() {
        let workflow = Workflow {
            id: "changeset-review".to_string(),
            description: String::new(),
            version: String::new(),
            source: Source::Runtime,
            dir: None,
            inputs: Vec::new(),
        };
        let command = lay_command(
            &WorkConfig::default(),
            &workflow,
            Path::new("/state/values.json"),
            Path::new("/work/panta/pr-42-review"),
            false,
        );
        assert!(command.starts_with("rhei instantiate 'changeset-review' "));
        assert!(command.contains("--values '/state/values.json'"));
        assert!(command.contains("--output '/work/panta/pr-42-review'"));
        assert!(command.ends_with("2>&1"));
        assert!(!command.contains("--dry-run"));
    }

    #[test]
    fn a_workflow_on_disk_is_named_by_its_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = Workflow {
            id: "house-style".to_string(),
            description: String::new(),
            version: String::new(),
            source: Source::Project,
            dir: Some(dir.path().to_path_buf()),
            inputs: Vec::new(),
        };
        let command = lay_command(
            &WorkConfig::default(),
            &workflow,
            Path::new("/state/values.json"),
            Path::new("/work/out"),
            true,
        );
        assert!(command.contains(&dir.path().to_string_lossy().into_owned()));
        assert!(command.contains("--dry-run"));
    }

    #[test]
    fn an_entry_is_read_from_beside_the_workflow() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = Workflow {
            id: "house-style".to_string(),
            description: String::new(),
            version: String::new(),
            source: Source::Project,
            dir: Some(dir.path().to_path_buf()),
            inputs: Vec::new(),
        };
        assert!(entry_beside(&workflow).is_none());
        std::fs::write(dir.path().join(ENTRY), "{\"id\":\"house-style\"}").expect("write");
        assert_eq!(
            entry_beside(&workflow).as_deref(),
            Some("{\"id\":\"house-style\"}")
        );
    }
}
