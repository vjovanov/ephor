//! The plan file: one rhei per item, one ticket per dispatch
//! (§FS-005-dispatch.3).
//!
//! ephor writes a plain-text plan in the runtime's language and reads the
//! state back out of it. Nothing else is stored about how the work is going:
//! the runtime owns that, the plan is where it writes it, and a second copy in
//! ephor's ledger would be a watch reporting on itself
//! (§FS-005-dispatch.4).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{EphorError, Result};

/// The state machine ephor installs into a work root that has none.
pub const SHIPPED_STATES: &str = include_str!("../../../assets/ephor-work.states.yaml");

/// What surrounds the dossier, so a later sync can rewrite exactly that much
/// and leave every `**State:**` line the runtime owns untouched.
const DOSSIER_OPEN: &str = "<!-- ephor:dossier -->";
const DOSSIER_CLOSE: &str = "<!-- /ephor:dossier -->";

const TASKS_HEADING: &str = "## Tasks";

/// The directory a runtime project lives in, under the work root template
/// (§DA-001-runtime-bound-default). Part of the coupling, and so part of this
/// module (§REQ-001-boundary.5).
pub const PROJECT_DIR: &str = "panta";

/// What a plan file is called: `<plan id>` and this.
const PLAN_SUFFIX: &str = ".rhei.md";

/// A directory holding an item's plans: a rhei project, with the state machine
/// its tickets run under.
pub struct WorkRoot {
    pub dir: PathBuf,
    /// The machine's name, as its `states.yaml` declares it.
    pub machine: String,
    /// The states that machine declares, for refusing a recipe that names one
    /// it does not have (§FS-005-dispatch.6), and for telling a ticket that is
    /// still being worked from one that is over.
    states: Vec<StateInfo>,
}

pub struct StateInfo {
    pub name: String,
    pub is_final: bool,
    /// The runtime will not leave this state on its own: it is where work
    /// waits for a person (§FS-005-dispatch.9).
    pub is_gating: bool,
}

impl WorkRoot {
    /// Prepare `dir` to hold tickets: create the project manifest when it is
    /// not there, install `states_yaml` when the directory has no machine of
    /// its own, and read back whichever machine is in force. An existing
    /// machine is never replaced — a reader who edited it meant it.
    ///
    /// A project that already holds plans of its own and declares no machine
    /// is refused rather than filled in (§FS-005-dispatch.6): `states.yaml` is
    /// how every plan in a project resolves its states, so dropping one in
    /// changes what those plans run under. A project with no plans in it —
    /// which is exactly what the runtime's own `init` leaves behind — has
    /// nothing to disturb, and ephor moves in beside it.
    pub fn ensure(dir: &Path, states_yaml: &str) -> Result<WorkRoot> {
        create_dir(dir)?;
        let manifest = dir.join("index.panta.md");
        let states = dir.join("states.yaml");
        let ours = !manifest.exists();
        if ours {
            let title = dir
                .parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "work".to_string());
            write(&manifest, &format!("# Panta: {title}\n"))?;
        }
        // A directory that ignores itself needs no entry in the repository's
        // own .gitignore: work about a branch would otherwise show up as a
        // change to that branch.
        let ignore = dir.join(".gitignore");
        if !ignore.exists() {
            write(
                &ignore,
                "# ephor work — planning state, not repository content\n*\n",
            )?;
        }
        if !states.exists() {
            if !ours && holds_plans(dir) {
                return Err(EphorError::Command(format!(
                    "{} already holds plans of its own and declares no state machine — \
                     writing one there would change what they run under. Either install \
                     ephor's (`ephor work states > {}`) or give ephor a directory of its own \
                     with `work.root`.",
                    dir.display(),
                    states.display()
                )));
            }
            write(&states, states_yaml)?;
        }
        Self::read_root(dir, &states)
    }

    /// The machine in force in a work root, without creating anything. None
    /// when the directory holds no machine — a plan can still be read there,
    /// it is only finality that cannot be judged.
    pub fn open(dir: &Path) -> Result<Option<WorkRoot>> {
        let states = dir.join("states.yaml");
        if !states.is_file() {
            return Ok(None);
        }
        Self::read_root(dir, &states).map(Some)
    }

    fn read_root(dir: &Path, states: &Path) -> Result<WorkRoot> {
        let text = read(states)?;
        Ok(WorkRoot {
            dir: dir.to_path_buf(),
            machine: machine_name(&text).ok_or_else(|| {
                EphorError::Command(format!(
                    "{} declares no state machine name; ephor cannot write tickets that name one.",
                    states.display()
                ))
            })?,
            states: state_infos(&text),
        })
    }

    pub fn plan_path(&self, plan_id: &str) -> PathBuf {
        plan_path_in(&self.dir, plan_id)
    }

    /// Whether the machine in force declares a state, so a recipe pointing at
    /// one it does not have is refused rather than dispatched.
    pub fn declares(&self, state: &str) -> bool {
        self.states.iter().any(|info| info.name == state)
    }

    /// Whether a state is one the work does not leave.
    pub fn is_final(&self, state: &str) -> bool {
        self.flag(state, |info| info.is_final)
    }

    /// Whether a state is one the runtime will not leave on its own — work
    /// parked there is waiting for a person (§FS-005-dispatch.9).
    pub fn is_gating(&self, state: &str) -> bool {
        self.flag(state, |info| info.is_gating)
    }

    fn flag(&self, state: &str, of: impl Fn(&StateInfo) -> bool) -> bool {
        self.states
            .iter()
            .find(|info| info.name == state)
            .map(of)
            .unwrap_or(false)
    }

    pub fn state_names(&self) -> Vec<String> {
        self.states.iter().map(|info| info.name.clone()).collect()
    }
}

/// The plan file for an id inside a work root. The same path
/// [`WorkRoot::plan_path`] resolves, for callers that have the directory
/// before they have the root — a dry run promising where work would go
/// (§FS-005-dispatch.5).
pub fn plan_path_in(dir: &Path, plan_id: &str) -> PathBuf {
    dir.join(format!("{plan_id}{PLAN_SUFFIX}"))
}

/// Whether a runtime project has any plans in it: a `*.rhei.md` file, or a
/// directory workspace, among its direct non-hidden children — which is where
/// the runtime looks for them.
fn holds_plans(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "runtime" {
            return false;
        }
        name.ends_with(PLAN_SUFFIX) || entry.path().join(format!("index{PLAN_SUFFIX}")).is_file()
    })
}

/// The `name:` of a states document — the shallowest one, so a state called
/// `name` cannot be mistaken for it.
fn machine_name(yaml: &str) -> Option<String> {
    yaml.lines()
        .find_map(|line| line.strip_prefix("name:"))
        .map(|value| {
            value
                .trim()
                .trim_matches(['"', '\''].as_slice())
                .to_string()
        })
        .filter(|name| !name.is_empty())
}

/// The keys under `states:`, and which of them the work does not leave. A
/// line-scan rather than a YAML parse: this only has to be right enough to
/// refuse a recipe naming a state that is not there and to tell a ticket in
/// flight from a finished one, and the runtime's own validation is the
/// authority on everything else.
fn state_infos(yaml: &str) -> Vec<StateInfo> {
    let mut states: Vec<StateInfo> = Vec::new();
    let mut inside = false;
    for line in yaml.lines() {
        if line.starts_with("states:") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if !line.starts_with(' ') && !line.trim().is_empty() {
            break;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if indent == 2 && trimmed.ends_with(':') {
            states.push(StateInfo {
                name: trimmed.trim_end_matches(':').to_string(),
                is_final: false,
                is_gating: false,
            });
        } else if indent > 2 {
            let flag = |prefix: &str| {
                trimmed
                    .strip_prefix(prefix)
                    .map(|value| value.trim().eq_ignore_ascii_case("true"))
            };
            if let Some(last) = states.last_mut() {
                if let Some(value) = flag("final:") {
                    last.is_final = value;
                }
                if let Some(value) = flag("gating:") {
                    last.is_gating = value;
                }
            }
        }
    }
    states
}

/// One ticket, as it is written into a plan.
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub state: String,
    /// The ticket this one follows, so a reopened item's work stays ordered
    /// (§FS-005-dispatch.5).
    pub prior: Option<String>,
    pub target: Option<String>,
    pub model: Option<String>,
    pub body: String,
}

impl Ticket {
    fn render(&self) -> String {
        let mut out = format!("### Task {}: {}\n", self.id, one_line(&self.title));
        out.push_str(&format!("**State:** {}\n", self.state));
        if let Some(prior) = &self.prior {
            out.push_str(&format!("**Prior:** Task {prior}\n"));
        }
        // Mutually exclusive in the runtime's language: a target carries a
        // model already, and declaring both is a validation error there.
        match (&self.target, &self.model) {
            (Some(target), _) => out.push_str(&format!("**Target:** {target}\n")),
            (None, Some(model)) => out.push_str(&format!("**Model:** {model}\n")),
            (None, None) => {}
        }
        out.push('\n');
        out.push_str(self.body.trim_end());
        out.push('\n');
        out
    }
}

/// A ticket as the plan currently has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTicket {
    pub id: String,
    pub title: String,
    pub state: Option<String>,
}

pub struct Plan {
    pub path: PathBuf,
    text: String,
}

impl Plan {
    pub fn read(path: &Path) -> Result<Option<Plan>> {
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(Plan {
            path: path.to_path_buf(),
            text: read(path)?,
        }))
    }

    /// A fresh plan for one item: its title, the machine its tickets run
    /// under, the dossier, and the first ticket.
    pub fn create(path: &Path, machine: &str, title: &str, dossier: &str, ticket: &Ticket) -> Plan {
        let text = format!(
            "# Rhei: {}\n**States:** {machine}\n\n{DOSSIER_OPEN}\n{}\n{DOSSIER_CLOSE}\n\n\
             {TASKS_HEADING}\n\n{}",
            one_line(title),
            dossier.trim_end(),
            ticket.render()
        );
        Plan {
            path: path.to_path_buf(),
            text,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Every ticket in the plan, in the order it was written. Fenced blocks
    /// are skipped: a dossier quotes conversations, and a conversation about a
    /// plan contains headings that are not this plan's.
    pub fn tickets(&self) -> Vec<PlanTicket> {
        let mut tickets: Vec<PlanTicket> = Vec::new();
        for line in unfenced(&self.text) {
            if let Some(rest) = line.strip_prefix("### ") {
                if let Some((id, title)) = ticket_heading(rest) {
                    tickets.push(PlanTicket {
                        id,
                        title,
                        state: None,
                    });
                }
                continue;
            }
            if let Some(state) = line.trim().strip_prefix("**State:**") {
                if let Some(last) = tickets.last_mut() {
                    if last.state.is_none() {
                        last.state = Some(state.trim().to_string());
                    }
                }
            }
        }
        tickets
    }

    /// The next id for a recipe's tickets on this plan: `answer-1`, then
    /// `answer-2`. Ids are per recipe so the file reads as what was asked for.
    pub fn next_ticket_id(&self, recipe: &str) -> String {
        let highest = self
            .tickets()
            .iter()
            .filter_map(|ticket| {
                ticket
                    .id
                    .strip_prefix(recipe)?
                    .strip_prefix('-')?
                    .parse::<u32>()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        format!("{recipe}-{}", highest + 1)
    }

    /// The last ticket in the plan, which a new one follows.
    pub fn last_ticket(&self) -> Option<PlanTicket> {
        self.tickets().into_iter().next_back()
    }

    pub fn append(&mut self, ticket: &Ticket) {
        if !self.text.contains(TASKS_HEADING) {
            self.text.push_str(&format!("\n{TASKS_HEADING}\n"));
        }
        if !self.text.ends_with('\n') {
            self.text.push('\n');
        }
        self.text.push('\n');
        self.text.push_str(&ticket.render());
    }

    /// Record one ticket's item as structured metadata, where a program in the
    /// state machine can be handed it (§FS-005-dispatch.8).
    ///
    /// Merged, never rewritten: the runtime keeps its own per-task bookkeeping
    /// in this same block, and a ticket that replaced it would break every
    /// counted loop in the plan.
    pub fn set_metadata(&mut self, ticket: &str, values: &[(&str, String)]) {
        let entry = std::iter::once(format!("    {ticket}:\n"))
            .chain(
                values
                    .iter()
                    .filter(|(_, value)| !value.is_empty())
                    .map(|(key, value)| format!("      {key}: {}\n", yaml_string(value))),
            )
            .collect::<String>();

        let Some((open, close)) = self.frontmatter() else {
            // No block yet: one goes below the heading and its declaration,
            // which is where the runtime's language puts it.
            let body = format!("---\nmetadata:\n  tasks:\n{entry}---\n");
            let at = self.header_end();
            self.text.insert_str(at, &format!("\n{body}"));
            return;
        };
        let block = &self.text[open..close];
        let insert_at = |needle: &str| block.find(needle).map(|at| open + at + needle.len());
        match insert_at("\n  tasks:\n").or_else(|| insert_at("metadata:\n")) {
            // Under an existing `tasks:`, or as the first thing under
            // `metadata:` — YAML does not care about the order of keys.
            Some(at) if block.contains("\n  tasks:\n") => self.text.insert_str(at, &entry),
            Some(at) => self.text.insert_str(at, &format!("  tasks:\n{entry}")),
            None => self
                .text
                .insert_str(open, &format!("metadata:\n  tasks:\n{entry}")),
        }
    }

    /// The frontmatter body, as `(start, end)` byte offsets between its
    /// fences. None when the plan has none.
    fn frontmatter(&self) -> Option<(usize, usize)> {
        let header_end = self.header_end();
        let rest = &self.text[header_end..];
        let open = rest.find("\n---\n")? + header_end + "\n---\n".len();
        // Only a block that opens before any content is this plan's
        // frontmatter; a horizontal rule further down is prose.
        if self.text[header_end..open].trim().len() > "---".len() {
            return None;
        }
        let close = self.text[open..].find("\n---\n")? + open + 1;
        Some((open, close))
    }

    /// Just past the title and its `**States:**` declaration.
    fn header_end(&self) -> usize {
        let mut end = 0;
        for line in self.text.lines() {
            if line.starts_with("# ") || line.starts_with("**States:**") {
                end += line.len() + 1;
                continue;
            }
            break;
        }
        end
    }

    /// Rewrite the dossier and nothing else. Tickets are appended, never
    /// rewritten: their `**State:**` lines belong to the runtime, which may be
    /// advancing one right now (§FS-005-dispatch.4).
    pub fn set_dossier(&mut self, dossier: &str) -> bool {
        let (Some(open), Some(close)) =
            (self.text.find(DOSSIER_OPEN), self.text.find(DOSSIER_CLOSE))
        else {
            return false;
        };
        if close < open {
            return false;
        }
        let replacement = format!("{DOSSIER_OPEN}\n{}\n", dossier.trim_end());
        self.text.replace_range(open..close, &replacement);
        true
    }

    pub fn save(&self) -> Result<()> {
        write(&self.path, &self.text)
    }
}

/// `Task fix-1: title` out of a heading, ignoring headings that are not
/// tickets. The runtime's node kinds start with a capital and are followed by
/// an identifier and a colon.
fn ticket_heading(rest: &str) -> Option<(String, String)> {
    let (kind, rest) = rest.split_once(' ')?;
    if !kind
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
    {
        return None;
    }
    let (id, title) = rest.split_once(':')?;
    let id = id.trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    Some((id.to_string(), title.trim().to_string()))
}

/// The lines of a document that are not inside a fenced block.
fn unfenced(text: &str) -> impl Iterator<Item = &str> {
    let mut fence: Option<String> = None;
    text.lines().filter(move |line| {
        let trimmed = line.trim_start();
        let marker = trimmed
            .chars()
            .next()
            .filter(|ch| *ch == '`' || *ch == '~')
            .map(|ch| {
                trimmed
                    .chars()
                    .take_while(|candidate| *candidate == ch)
                    .collect::<String>()
            })
            .filter(|run| run.len() >= 3);
        match (&fence, marker) {
            (None, Some(open)) => {
                fence = Some(open);
                false
            }
            (Some(open), Some(close))
                if close.len() >= open.len() && close.starts_with(&open[..1]) =>
            {
                fence = None;
                false
            }
            (Some(_), _) => false,
            (None, None) => true,
        }
    })
}

/// A rhei id for an item: its own id, reduced to what the runtime's grammar
/// allows for a file stem, and never empty or leading with a digit.
pub fn plan_id(item_id: &str) -> String {
    let mut out = String::with_capacity(item_id.len());
    for ch in item_id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    match trimmed.chars().next() {
        Some(first) if first.is_ascii_alphabetic() => trimmed,
        Some(_) => format!("item-{trimmed}"),
        None => "item".to_string(),
    }
}

/// A value as a YAML scalar. Always quoted: a branch called `you/ABC-42` is a
/// string, a number like `24898` is a string too — a script comparing it
/// against what a forge printed should not have to care that YAML would have
/// made one of them an integer.
fn yaml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', r"\\").replace('"', "\\\""))
}

fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() <= 120 {
        return joined;
    }
    joined.chars().take(117).collect::<String>() + "…"
}

fn create_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)
        .map_err(|err| EphorError::Command(format!("Cannot create {}: {err}", dir.display())))
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .map_err(|err| EphorError::Command(format!("Cannot read {}: {err}", path.display())))
}

/// Write through a temporary file: the runtime may be reading a plan while
/// this runs, and half a plan is a plan that does not parse.
fn write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    let tmp = path.with_extension("ephor-tmp");
    fs::write(&tmp, content)
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", tmp.display())))?;
    fs::rename(&tmp, path)
        .map_err(|err| EphorError::Command(format!("Cannot rename {}: {err}", tmp.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(id: &str, state: &str, body: &str) -> Ticket {
        Ticket {
            id: id.to_string(),
            title: "fix the red gate".to_string(),
            state: state.to_string(),
            prior: None,
            target: None,
            model: None,
            body: body.to_string(),
        }
    }

    #[test]
    fn the_shipped_machine_declares_the_states_the_shipped_recipes_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = WorkRoot::ensure(tmp.path(), SHIPPED_STATES).unwrap();
        assert_eq!(root.machine, "ephor-work");
        assert!(root.declares("fix"), "{:?}", root.state_names());
        assert!(root.declares("review"));
        assert!(root.declares("done"));
        assert!(!root.declares("nonexistent"));
        // Finality is what tells work in flight from work that is over.
        assert!(root.is_final("done"));
        assert!(!root.is_final("fix"));
        assert!(!root.is_final("nonexistent"));

        // A machine with a state the runtime will not leave on its own: work
        // parked there is waiting for a person (§FS-005-dispatch.9).
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("states.yaml"),
            concat!(
                "name: m\n",
                "states:\n",
                "  fix:\n    agent: x\n",
                "  needs-human:\n    gating: true\n",
                "  done:\n    final: true\n",
            ),
        )
        .unwrap();
        let root = WorkRoot::ensure(tmp.path(), SHIPPED_STATES).unwrap();
        assert!(root.is_gating("needs-human"));
        assert!(!root.is_gating("fix"));
        assert!(!root.is_final("needs-human"));
        assert!(root.is_final("done"));
        // The project is set up, and ignores itself so the checkout stays clean.
        assert!(tmp.path().join("index.panta.md").is_file());
        assert!(fs::read_to_string(tmp.path().join(".gitignore"))
            .unwrap()
            .contains('*'));
    }

    #[test]
    fn an_existing_machine_is_read_and_never_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let mine = "name: mine\nversion: 2\nstates:\n  triage:\n    agent: x\n  shipped:\n    final: true\n";
        fs::write(tmp.path().join("states.yaml"), mine).unwrap();
        let root = WorkRoot::ensure(tmp.path(), SHIPPED_STATES).unwrap();
        assert_eq!(root.machine, "mine");
        assert!(root.declares("triage"));
        assert!(!root.declares("fix"));
        assert_eq!(
            fs::read_to_string(tmp.path().join("states.yaml")).unwrap(),
            mine
        );
    }

    /// Someone else's runtime project in the same checkout: filling in the
    /// machine it does not declare would change what its own plans run under
    /// (§FS-005-dispatch.6). An empty one — what the runtime's `init` leaves —
    /// has no plans to disturb, and is the common case in a checkout where a
    /// reader ran it once and never wrote a plan.
    #[test]
    fn only_a_project_with_plans_of_its_own_refuses_a_machine() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.panta.md"), "# Panta: theirs\n").unwrap();
        let root = WorkRoot::ensure(tmp.path(), SHIPPED_STATES)
            .unwrap_or_else(|err| panic!("an empty project has nothing to lose: {err}"));
        assert_eq!(root.machine, "ephor-work");

        // The same project once it holds a plan.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.panta.md"), "# Panta: theirs\n").unwrap();
        fs::write(tmp.path().join("auth.rhei.md"), "# Rhei: Auth\n").unwrap();
        let Err(err) = WorkRoot::ensure(tmp.path(), SHIPPED_STATES) else {
            panic!("their plans, their machine");
        };
        assert!(err.to_string().contains("ephor work states"), "{err}");
        assert!(!tmp.path().join("states.yaml").exists());

        // A workspace-shaped rhei counts as a plan too.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.panta.md"), "# Panta: theirs\n").unwrap();
        fs::create_dir(tmp.path().join("billing")).unwrap();
        fs::write(
            tmp.path().join("billing/index.rhei.md"),
            "# Rhei: Billing\n",
        )
        .unwrap();
        assert!(WorkRoot::ensure(tmp.path(), SHIPPED_STATES).is_err());
    }

    #[test]
    fn a_plan_holds_the_dossier_and_its_tickets_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("widget-42.rhei.md");
        let mut plan = Plan::create(
            &path,
            "ephor-work",
            "acme/widget#42 — Retry\nwindow",
            "## The item\n\n- **project** widget\n",
            &ticket("fix-gate-1", "fix", "make it green"),
        );
        assert_eq!(plan.next_ticket_id("fix-gate"), "fix-gate-2");

        plan.append(&Ticket {
            prior: Some("fix-gate-1".to_string()),
            target: Some("claude-code[yolo]:anthropic:sonnet".to_string()),
            ..ticket("fix-gate-2", "fix", "it changed")
        });
        plan.save().unwrap();

        let reread = Plan::read(&path).unwrap().unwrap();
        let tickets = reread.tickets();
        assert_eq!(tickets.len(), 2);
        assert_eq!(tickets[0].id, "fix-gate-1");
        assert_eq!(tickets[0].state.as_deref(), Some("fix"));
        assert_eq!(reread.last_ticket().unwrap().id, "fix-gate-2");
        assert!(reread.text().contains("**Prior:** Task fix-gate-1"));
        assert!(reread.text().contains("**Target:** claude-code[yolo]"));
        // The title survives as one line.
        assert!(reread
            .text()
            .starts_with("# Rhei: acme/widget#42 — Retry window\n"));
    }

    /// A program in the state machine is handed the item as `{meta.*}`, and
    /// the runtime keeps its own bookkeeping in the same block
    /// (§FS-005-dispatch.8).
    #[test]
    fn metadata_is_merged_into_the_frontmatter_the_runtime_shares() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plan.rhei.md");
        let mut plan = Plan::create(
            &path,
            "ephor-work",
            "acme/widget#42",
            "## The item\n",
            &ticket("fix-gate-1", "collect", "work"),
        );
        plan.set_metadata(
            "fix-gate-1",
            &[
                ("repo", "acme/widget".to_string()),
                ("number", "42".to_string()),
                ("branch", r#"you/"odd"\name"#.to_string()),
                ("empty", String::new()),
            ],
        );
        let text = plan.text().to_string();
        // Below the heading and its declaration, which is where the runtime's
        // language puts frontmatter.
        assert!(
            text.starts_with("# Rhei: acme/widget#42\n**States:** ephor-work\n\n---\nmetadata:\n  tasks:\n    fix-gate-1:\n"),
            "{text}"
        );
        assert!(text.contains(r#"      number: "42""#), "{text}");
        assert!(
            text.contains(r#"      branch: "you/\"odd\"\\name""#),
            "{text}"
        );
        // Nothing is said about a field the item does not have.
        assert!(!text.contains("empty:"), "{text}");
        // The dossier and the ticket still follow it.
        assert!(text.contains("\n---\n\n<!-- ephor:dossier -->"), "{text}");
        assert_eq!(plan.tickets().len(), 1);

        // The runtime has since written its own counter into the block; a
        // second ticket joins it rather than replacing it.
        plan.text = plan.text.replace(
            "  tasks:\n",
            "  tasks:\n    fix-gate-1:\n      stateVisits:\n        collect: 1\n",
        );
        plan.set_metadata("answer-1", &[("repo", "acme/widget".to_string())]);
        let text = plan.text().to_string();
        assert!(text.contains("stateVisits:"), "{text}");
        assert!(
            text.contains("    answer-1:\n      repo: \"acme/widget\""),
            "{text}"
        );
        assert_eq!(text.matches("metadata:").count(), 1, "{text}");
        assert_eq!(text.matches("  tasks:").count(), 1, "{text}");
    }

    #[test]
    fn a_conversation_quoting_a_plan_is_not_read_as_one() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plan.rhei.md");
        let dossier =
            "## The conversation\n\n````\n### Task ghost: not a ticket\n**State:** done\n````\n";
        let plan = Plan::create(
            &path,
            "ephor-work",
            "t",
            dossier,
            &ticket("fix-gate-1", "fix", "real work"),
        );
        let tickets = plan.tickets();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].id, "fix-gate-1");
        assert_eq!(tickets[0].state.as_deref(), Some("fix"));
    }

    #[test]
    fn refreshing_the_dossier_leaves_every_ticket_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plan.rhei.md");
        let mut plan = Plan::create(
            &path,
            "ephor-work",
            "t",
            "## The item\n\n- **state** open\n",
            &ticket("fix-gate-1", "fix", "work"),
        );
        // The runtime has advanced the ticket since it was written.
        plan.text = plan.text.replace("**State:** fix", "**State:** review");
        assert!(plan.set_dossier("## The item\n\n- **state** merged\n"));
        assert!(plan.text().contains("- **state** merged"));
        assert!(!plan.text().contains("- **state** open"));
        assert_eq!(plan.tickets()[0].state.as_deref(), Some("review"));
        assert!(plan.text().contains(DOSSIER_CLOSE));
    }

    #[test]
    fn an_items_id_becomes_a_file_the_runtime_will_accept() {
        assert_eq!(
            plan_id("github-prs:acme/widget#42"),
            "github-prs-acme-widget-42"
        );
        assert_eq!(plan_id("forge:repo/123"), "forge-repo-123");
        assert_eq!(plan_id("42"), "item-42");
        assert_eq!(plan_id("///"), "item");
    }
}
