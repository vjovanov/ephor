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

/// The plan language's **abandonment state**: the one final state that
/// satisfies no `**Prior:**`, and where a ticket the reader takes back goes
/// (§FS-005-dispatch.16). The name is the runtime's — its readiness rule
/// turns on this spelling and no other — so it is spelled here and nowhere
/// above this module (§REQ-001-boundary.5); a surface asks a [`WorkRoot`]
/// whether its machine declares it and a [`PlanTicket`] whether it sits in it.
pub const CANCELLED: &str = "cancelled";

/// The runtime's **built-in default machine**: what a project that declares no
/// `states.yaml` of its own runs its plans under — `pending`, and `completed`
/// final — as the runtime itself resolves it. The names and the shape are the
/// runtime's, so they are spelled here and nowhere above this module
/// (§REQ-001-boundary.5); a caller asks [`WorkRoot::in_force`] for the machine
/// a store's tasks actually run under (§FS-006-project-interface.7).
pub const DEFAULT_STATES: &str = "name: rhei\nstates:\n  pending:\n  completed:\n    final: true\n";

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

    /// The machine a directory's plans actually run under: the one it declares,
    /// or — where it declares none — the runtime's built-in default
    /// ([`DEFAULT_STATES`]), which is what the runtime resolves such a project
    /// to. This is the question a reader of somebody else's store asks, since a
    /// task's state means whatever the machine in force says it means
    /// (§FS-006-project-interface.7). [`open`](Self::open) keeps its own
    /// meaning: a surface that must withhold judgment where nothing is declared
    /// asks that one instead.
    pub fn in_force(dir: &Path) -> Result<WorkRoot> {
        match Self::open(dir)? {
            Some(root) => Ok(root),
            None => Self::from_states(dir, DEFAULT_STATES, "the runtime's default state machine"),
        }
    }

    fn read_root(dir: &Path, states: &Path) -> Result<WorkRoot> {
        let text = read(states)?;
        Self::from_states(dir, &text, &states.display().to_string())
    }

    /// A root from a states document, whatever it was read from — `origin`
    /// names that source in the one error this can raise.
    fn from_states(dir: &Path, text: &str, origin: &str) -> Result<WorkRoot> {
        Ok(WorkRoot {
            dir: dir.to_path_buf(),
            machine: machine_name(text).ok_or_else(|| {
                EphorError::Command(format!(
                    "{origin} declares no state machine name; ephor cannot write tickets that \
                     name one."
                ))
            })?,
            states: state_infos(text),
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

    /// The abandonment state this machine declares, where it declares one
    /// (§FS-005-dispatch.16): [`CANCELLED`], and final — a state under that
    /// name the work would leave again is not one a ticket can be taken back
    /// into. None is a refusal's cue, never a state to write.
    pub fn cancel_state(&self) -> Option<&str> {
        self.states
            .iter()
            .find(|info| info.name == CANCELLED && info.is_final)
            .map(|info| info.name.as_str())
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
    !plans_in(dir).is_empty()
}

/// One plan found in a work root: its id — the name the runtime knows it
/// by — and the file the floor reads (§AR-007-runtime.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundPlan {
    pub plan_id: String,
    pub path: PathBuf,
}

/// Every plan a work root holds, whoever wrote it (§FS-005-dispatch.15): the
/// `*.rhei.md` files and the directory workspaces among its direct non-hidden
/// children, exactly where the runtime looks for them. One directory listing,
/// no file read, no runner asked — recognizing a plan is this module's
/// grammar (§AR-007-runtime.1), and the callers get ids and paths, never the
/// suffix.
pub fn plans_in(dir: &Path) -> Vec<FoundPlan> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<FoundPlan> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "runtime" {
                return None;
            }
            if let Some(plan_id) = name.strip_suffix(PLAN_SUFFIX) {
                return Some(FoundPlan {
                    plan_id: plan_id.to_string(),
                    path: entry.path(),
                });
            }
            let index = entry.path().join(format!("index{PLAN_SUFFIX}"));
            index.is_file().then(|| FoundPlan {
                plan_id: name,
                path: index,
            })
        })
        .collect();
    found.sort_by(|a, b| a.plan_id.cmp(&b.plan_id));
    found
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

/// The execution line a ticket carries, where it carries one
/// (§FS-005-dispatch.14). The two rank differently against a run's flags: a
/// full line is resolved on its own, with the flags invisible to it, while a
/// model line takes its carrier from them — so only the latter keeps flags
/// off a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    /// `**Target:**` — the full execution identity, the ticket's alone.
    Target,
    /// `**Model:**` — a model with no carrier of its own.
    Model,
}

/// A ticket as the plan currently has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTicket {
    pub id: String,
    pub title: String,
    pub state: Option<String>,
    /// Who claimed the ticket, where anyone has. A claim makes the runtime
    /// skip the ticket — it is never a liveness signal (§FS-005-dispatch.15).
    pub assignee: Option<String>,
    /// The execution line the ticket carries, where it carries one
    /// (§FS-005-dispatch.14).
    pub pinned: Option<Pin>,
    /// The tickets this one is ordered after — its `**Prior:**` list, ids
    /// only, the kind word dropped as the runtime's own readers drop it. What
    /// a cancel names as left waiting (§FS-005-dispatch.16).
    pub prior: Vec<String>,
}

impl PlanTicket {
    /// Whether the ticket sits in the abandonment state (§FS-005-dispatch.16).
    pub fn cancelled(&self) -> bool {
        self.state.as_deref() == Some(CANCELLED)
    }
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

    /// The plan's own name: its leading heading, with the plan language's
    /// label stripped — what a board row can say about work ephor never
    /// dispatched, which has no matter to borrow a title from
    /// (§FS-005-dispatch.15). None where the file opens with no heading.
    pub fn title(&self) -> Option<String> {
        let heading = self
            .text
            .lines()
            .find_map(|line| line.trim().strip_prefix("# "))?;
        let title = heading.strip_prefix("Rhei:").unwrap_or(heading).trim();
        (!title.is_empty()).then(|| title.to_string())
    }

    /// Every ticket in the plan, in the order it was written — at every
    /// depth: the runtime nests a subtask one heading deeper per level, up to
    /// `######`, and a parked subtask is as much a ticket as its parent
    /// (§FS-005-dispatch.15). Fenced blocks
    /// are skipped: a dossier quotes conversations, and a conversation about a
    /// plan contains headings that are not this plan's. Metadata is read only
    /// from a ticket's header block — the `**Field:**` lines between its
    /// heading and its first content line, blank lines not closing it — which
    /// is exactly as far as the runtime reads them, so a dossier or a report
    /// quoted into a body cannot pin a ticket it merely mentions.
    pub fn tickets(&self) -> Vec<PlanTicket> {
        let mut tickets: Vec<PlanTicket> = Vec::new();
        let mut in_header = false;
        for line in unfenced(&self.text) {
            let trimmed = line.trim();
            if trimmed.starts_with("###") {
                match ticket_heading(trimmed) {
                    Some((id, title)) => {
                        tickets.push(PlanTicket {
                            id,
                            title,
                            state: None,
                            assignee: None,
                            pinned: None,
                            prior: Vec::new(),
                        });
                        in_header = true;
                    }
                    // A heading that is not a ticket is content, and content
                    // closes the header block it lands in.
                    None => in_header = false,
                }
                continue;
            }
            if !in_header {
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            let Some(last) = tickets.last_mut() else {
                in_header = false;
                continue;
            };
            if let Some(state) = trimmed.strip_prefix("**State:**") {
                if last.state.is_none() {
                    last.state = Some(state.trim().to_string());
                }
            } else if let Some(assignee) = trimmed.strip_prefix("**Assignee:**") {
                let assignee = assignee.trim();
                if last.assignee.is_none() && !assignee.is_empty() {
                    last.assignee = Some(assignee.to_string());
                }
            } else if let Some(prior) = trimmed.strip_prefix("**Prior:**") {
                if last.prior.is_empty() {
                    last.prior = prior_ids(prior);
                }
            } else if let Some(value) = trimmed.strip_prefix("**Target:**") {
                // The full line makes the ticket its own authority on who
                // runs it, whatever else it carries (§FS-005-dispatch.14).
                if !value.trim().is_empty() {
                    last.pinned = Some(Pin::Target);
                }
            } else if let Some(value) = trimmed.strip_prefix("**Model:**") {
                if !value.trim().is_empty() && last.pinned.is_none() {
                    last.pinned = Some(Pin::Model);
                }
            } else if !(trimmed.starts_with("**") && trimmed.contains(":**")) {
                // The first content line ends the header; any other
                // `**Field:**` line keeps it open, as the runtime reads it.
                in_header = false;
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

    /// The last top-level ticket in the plan that was not cancelled, which a
    /// new one follows: a new dispatch orders itself after the previous
    /// dispatch, never after a subtask the runtime nested under one — and
    /// never after an abandoned ticket, since the abandonment state satisfies
    /// no `**Prior:**` and a chain hung off one would never start
    /// (§FS-005-dispatch.16, §FS-005-dispatch.5).
    pub fn last_ticket(&self) -> Option<PlanTicket> {
        self.tickets()
            .into_iter()
            .filter(|ticket| !ticket.id.contains('.') && !ticket.cancelled())
            .next_back()
    }

    /// One ticket by id, as the plan has it.
    pub fn ticket(&self, id: &str) -> Option<PlanTicket> {
        self.tickets().into_iter().find(|ticket| ticket.id == id)
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

/// `Task fix-1: title` out of a node heading, ignoring headings that are not
/// tickets. The grammar is the runtime's own parser's: three to six hashes —
/// `###` is depth 1 — then a kind word (matched by shape alone: Title Case is
/// the runtime's convention, its matching is case-insensitive), then a dotted
/// id with exactly as many segments as the heading is deep, a colon, a title.
/// The depth match is what keeps a prose heading out of the tickets: the
/// runtime enforces it, so a heading that fails it is a ticket nowhere.
fn ticket_heading(line: &str) -> Option<(String, String)> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(3..=6).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    let body = rest.trim_start();
    if body.len() == rest.len() {
        // Nothing separated the hashes from the words: not a heading.
        return None;
    }
    let (kind, rest) = body.split_once(char::is_whitespace)?;
    if !kind
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        || !kind
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    let (id, title) = rest.trim_start().split_once(':')?;
    let id = id.trim_end();
    let depth = hashes - 2;
    if id.split('.').count() != depth || !id.split('.').all(id_segment) {
        return None;
    }
    Some((id.to_string(), title.trim().to_string()))
}

/// The ids a `**Prior:**` list names: `Task a-1, Task b-2` is `a-1`, `b-2`.
/// The kind word is decoration in the runtime's grammar — a reference
/// resolves on the id alone, and its own readers accept the bare form — so
/// both spellings read the same here.
fn prior_ids(list: &str) -> Vec<String> {
    list.split(',')
        .filter_map(|reference| {
            let reference = reference.trim();
            let id = match reference.split_once(char::is_whitespace) {
                Some((kind, id))
                    if kind
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') =>
                {
                    id.trim()
                }
                _ => reference,
            };
            (!id.is_empty()).then(|| id.to_string())
        })
        .collect()
}

/// One id segment, as the runtime's grammar has it: a name — a letter, then
/// letters, digits, `-`, `_` — or a canonical number, `0` or digits without a
/// leading zero, fitting in 32 bits.
fn id_segment(segment: &str) -> bool {
    match segment.bytes().next() {
        Some(first) if first.is_ascii_alphabetic() => segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
        Some(first) if first.is_ascii_digit() => {
            (segment.len() == 1 || first != b'0') && segment.parse::<u32>().is_ok()
        }
        _ => false,
    }
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

    /// The machine in force is the declared one where there is one and the
    /// runtime's built-in default where there is not — which is what the
    /// runtime itself resolves an undeclared project to, and so what a reader
    /// of somebody else's store must judge its tasks by
    /// (§FS-006-project-interface.7). `open` is unchanged: it still answers
    /// None, for the surfaces that must withhold judgment.
    #[test]
    fn the_machine_in_force_falls_back_to_the_runtimes_default() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(WorkRoot::open(tmp.path()).unwrap().is_none());
        let root = WorkRoot::in_force(tmp.path()).unwrap();
        assert!(root.declares("pending"), "{:?}", root.state_names());
        assert!(root.is_final("completed"));
        assert!(!root.is_final("pending"));

        // A declared machine is the one in force, and nothing of the default
        // leaks into it: `completed` is not one of its states at all.
        fs::write(
            tmp.path().join("states.yaml"),
            "name: custom\nstates:\n  todo:\n  verified:\n    final: true\n",
        )
        .unwrap();
        let root = WorkRoot::in_force(tmp.path()).unwrap();
        assert_eq!(root.machine, "custom");
        assert!(root.is_final("verified"));
        assert!(!root.is_final("completed"));

        // A machine that is there and cannot be read is an error, never the
        // default quietly standing in for it.
        fs::write(tmp.path().join("states.yaml"), "states:\n  todo:\n").unwrap();
        assert!(WorkRoot::in_force(tmp.path()).is_err());
    }

    /// The shipped machine declares the abandonment state and it is final;
    /// a machine spelling the name over a state it would leave again declares
    /// none, and neither does one without it (§FS-005-dispatch.16).
    #[test]
    fn the_abandonment_state_is_the_final_one_under_the_runtimes_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = WorkRoot::ensure(tmp.path(), SHIPPED_STATES).unwrap();
        assert_eq!(root.cancel_state(), Some(CANCELLED));
        assert!(root.is_final(CANCELLED));

        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("states.yaml"),
            "name: m\nstates:\n  fix:\n    agent: x\n  cancelled:\n    agent: y\n  done:\n    final: true\n",
        )
        .unwrap();
        let root = WorkRoot::ensure(tmp.path(), SHIPPED_STATES).unwrap();
        assert_eq!(root.cancel_state(), None, "not final is not abandonment");

        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("states.yaml"),
            "name: m\nstates:\n  fix:\n    agent: x\n  done:\n    final: true\n",
        )
        .unwrap();
        let root = WorkRoot::ensure(tmp.path(), SHIPPED_STATES).unwrap();
        assert_eq!(root.cancel_state(), None);
    }

    /// A ticket's `**Prior:**` list is read as ids, kind word or not; a
    /// cancelled ticket reads as such; and the ticket a new one follows is
    /// the last that was not taken back, so ephor's own chain never hangs
    /// off abandoned work (§FS-005-dispatch.16, §FS-005-dispatch.5).
    #[test]
    fn priors_are_read_and_a_new_ticket_follows_the_last_that_was_not_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.rhei.md");
        fs::write(
            &path,
            concat!(
                "# Rhei: p\n**States:** m\n\n## Tasks\n\n",
                "### Task fix-gate-1: one\n**State:** done\n\nbody\n\n",
                "### Task fix-gate-2: two\n**State:** cancelled\n**Prior:** Task fix-gate-1\n\nbody\n\n",
                "### Task fix-gate-3: three\n**State:** cancelled\n**Prior:** fix-gate-1, Task fix-gate-2\n\nbody\n\n",
                "#### Task fix-gate-3.1: sub\n**State:** fix\n\nbody\n",
            ),
        )
        .unwrap();
        let plan = Plan::read(&path).unwrap().unwrap();
        let tickets = plan.tickets();
        assert_eq!(tickets[1].prior, vec!["fix-gate-1"]);
        assert_eq!(tickets[2].prior, vec!["fix-gate-1", "fix-gate-2"]);
        assert!(tickets[1].cancelled());
        assert!(!tickets[0].cancelled());
        assert_eq!(
            plan.ticket("fix-gate-2").map(|t| t.id),
            Some("fix-gate-2".to_string())
        );
        assert!(plan.ticket("fix-gate-9").is_none());
        // Not the cancelled ones, and not the subtask: the finished one.
        assert_eq!(
            plan.last_ticket().map(|t| t.id),
            Some("fix-gate-1".to_string())
        );
        assert_eq!(
            prior_ids("Task a-1, Bug 2.3,  c-9 "),
            vec!["a-1", "2.3", "c-9"]
        );
        assert!(prior_ids("  ").is_empty());
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

    /// Every plan a work root holds is found whoever wrote it
    /// (§FS-005-dispatch.15): the plan files and the directory workspaces
    /// among the direct children, with ids and paths handed back so nothing
    /// above this module spells the suffix (§AR-007-runtime.1) — and nothing
    /// hidden, nothing under `runtime/`, and nothing that merely mentions a
    /// plan is one.
    #[test]
    fn every_plan_a_root_holds_is_found_with_its_id() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("zeta.rhei.md"), "# Rhei: Zeta\n").unwrap();
        fs::create_dir(dir.join("billing")).unwrap();
        fs::write(dir.join("billing/index.rhei.md"), "# Rhei: Billing\n").unwrap();
        // None of these is a plan: the manifest, a backup, a hidden file, the
        // runtime's own artifacts, and a bare directory.
        fs::write(dir.join("index.panta.md"), "# Panta: theirs\n").unwrap();
        fs::write(dir.join("zeta.rhei.md.bak"), "old").unwrap();
        fs::write(dir.join(".draft.rhei.md"), "hidden").unwrap();
        fs::create_dir_all(dir.join("runtime")).unwrap();
        fs::write(dir.join("runtime/echo.rhei.md"), "artifact").unwrap();
        fs::create_dir(dir.join("notes")).unwrap();

        let found = plans_in(dir);
        assert_eq!(
            found,
            vec![
                FoundPlan {
                    plan_id: "billing".to_string(),
                    path: dir.join("billing/index.rhei.md"),
                },
                FoundPlan {
                    plan_id: "zeta".to_string(),
                    path: dir.join("zeta.rhei.md"),
                },
            ]
        );
        // A directory that is not there answers empty, not an error: the
        // enumeration probes places that may hold nothing.
        assert!(plans_in(&dir.join("nowhere")).is_empty());
    }

    /// A plan lends its own heading where nothing dispatched it — a foreign
    /// plan has no matter to borrow a title from (§FS-005-dispatch.15).
    #[test]
    fn a_plan_says_its_own_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("widget-42.rhei.md");
        let created = Plan::create(
            &path,
            "ephor-work",
            "Widen the retry window",
            "dossier",
            &ticket("fix-1", "fix", "work"),
        );
        assert_eq!(created.title().as_deref(), Some("Widen the retry window"));

        // A hand-written plan with a plain heading, below frontmatter.
        fs::write(
            &path,
            "---\nowner: luna\n---\n\n# Audit the retry paths\n\n## Tasks\n",
        )
        .unwrap();
        let plain = Plan::read(&path).unwrap().unwrap();
        assert_eq!(plain.title().as_deref(), Some("Audit the retry paths"));

        // No heading, no title — never a guess.
        fs::write(&path, "just notes\n").unwrap();
        let bare = Plan::read(&path).unwrap().unwrap();
        assert_eq!(bare.title(), None);
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

    /// A claim is read where the runtime wrote one, and an unclaimed ticket
    /// answers None rather than an empty word (§FS-005-dispatch.15).
    #[test]
    fn a_claim_is_read_beside_the_state() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plan.rhei.md");
        let mut plan = Plan::create(
            &path,
            "ephor-work",
            "t",
            "## The item\n",
            &ticket("fix-gate-1", "fix", "work"),
        );
        plan.append(&ticket("fix-gate-2", "fix", "more work"));
        // The runtime's `next` wrote the claim; ephor only reads it.
        plan.text = plan.text.replacen(
            "**State:** fix\n",
            "**State:** fix\n**Assignee:** luna\n",
            1,
        );
        let tickets = plan.tickets();
        assert_eq!(tickets[0].assignee.as_deref(), Some("luna"));
        assert_eq!(tickets[1].assignee, None);
    }

    /// An execution line is a ticket's own only in its header — the
    /// `**Field:**` lines between the heading and the first content line,
    /// which is as far as the runtime reads metadata (§FS-005-dispatch.14). A
    /// body that merely mentions one, quoting a report or a dossier, pins
    /// nothing.
    #[test]
    fn an_execution_line_pins_only_from_the_tickets_header() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plan.rhei.md");
        let mut plan = Plan::create(
            &path,
            "ephor-work",
            "t",
            "## The item\n",
            &Ticket {
                target: Some("codex[yolo]:openai:gpt-5".to_string()),
                ..ticket("fix-gate-1", "fix", "work")
            },
        );
        plan.append(&Ticket {
            model: Some("sonnet".to_string()),
            ..ticket("answer-1", "fix", "reply")
        });
        plan.append(&ticket(
            "review-1",
            "fix",
            "The report said:\n\n**Target:** codex[yolo]:openai:gpt-5\n\nand stopped there.",
        ));
        let tickets = plan.tickets();
        assert_eq!(tickets[0].pinned, Some(Pin::Target));
        assert_eq!(tickets[1].pinned, Some(Pin::Model));
        // The quote sits past the header — the blank line after the body's
        // first content line has long closed it — so it pins nothing, and the
        // state past it is not re-read either.
        assert_eq!(tickets[2].pinned, None);
        assert_eq!(tickets[2].state.as_deref(), Some("fix"));
    }

    /// The floor reads every depth the runtime's language nests
    /// (§FS-005-dispatch.15): a subtask is a heading one level deeper with a
    /// dotted id, one segment per level, numeric or named — the runtime's own
    /// parser is the authority on that grammar.
    #[test]
    fn a_subtask_is_a_ticket_at_every_depth() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plan.rhei.md");
        let mut plan = Plan::create(
            &path,
            "ephor-work",
            "t",
            "## The item\n",
            &ticket("fix-gate-1", "fix", "work"),
        );
        plan.text.push_str(concat!(
            "\n#### Task fix-gate-1.1: split off\n**State:** needs-human\n\nchild\n",
            "\n##### Task fix-gate-1.1.re-check: deeper\n**State:** fix\n\ngrandchild\n",
            "\n###### Task fix-gate-1.1.re-check.0: as deep as the language goes\n",
            "**State:** fix\n\nleaf\n",
        ));
        let tickets = plan.tickets();
        let ids: Vec<&str> = tickets.iter().map(|ticket| ticket.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "fix-gate-1",
                "fix-gate-1.1",
                "fix-gate-1.1.re-check",
                "fix-gate-1.1.re-check.0"
            ]
        );
        assert_eq!(tickets[1].state.as_deref(), Some("needs-human"));
        // A new dispatch follows the last dispatch, never a subtask of one.
        assert_eq!(plan.last_ticket().unwrap().id, "fix-gate-1");
    }

    /// What the runtime's parser refuses is not a ticket here either: the
    /// heading's depth must match the id's segment count, a segment is a name
    /// or a canonical number, and seven hashes is past the language. Kind
    /// matching is case-insensitive — Title Case is the runtime's convention,
    /// not its grammar.
    #[test]
    fn a_heading_the_runtime_would_refuse_is_not_a_ticket() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plan.rhei.md");
        let mut plan = Plan::create(
            &path,
            "ephor-work",
            "t",
            "## The item\n",
            &ticket("fix-gate-1", "fix", "work"),
        );
        plan.text.push_str(concat!(
            "\n#### Task fix-gate-1-a: one segment, two headings deep\n**State:** fix\n\nx\n",
            "\n### Task a.b: two segments, one heading deep\n**State:** fix\n\nx\n",
            "\n#### Task fix-gate-1.01: a leading zero is not canonical\n**State:** fix\n\nx\n",
            "\n#### Task fix-gate-1.: an empty segment\n**State:** fix\n\nx\n",
            "\n####### Task d.d.d.d.d: past the language\n**State:** fix\n\nx\n",
            "\n#### The plan: prose, not a node\n\nx\n",
            "\n### task 9: a lowercase kind is valid grammar\n**State:** fix\n\nx\n",
        ));
        let ids: Vec<String> = plan.tickets().into_iter().map(|ticket| ticket.id).collect();
        assert_eq!(ids, ["fix-gate-1", "9"]);
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
