//! What may be done here: the entries a matter or a branch carries, where each
//! one runs, and whether it can run at all (§AR-009-surfaces.1).
//!
//! This is the assembly half of the action menu, below both surfaces
//! (§REQ-002-parity.1): provenance ordering, the offers ephor synthesizes from
//! what is on disk, and the gate each entry is given. The interface draws the
//! result and `ephor actions` prints it; neither computes it.
//!
//! Provenance orders the menu — what ephor itself recognized
//! (§FS-004-quick-actions.3), then the project's offers, then the person's own
//! from `status.json` (`actions` globally, plus per-project
//! `projects.<id>.actions`) — and where two entries share an id, the later
//! provenance wins in place. Then the recipes: the two lists are one menu, so
//! "what can I do about this row" has one answer rather than depending on
//! which key the reader knew to press (§FS-005-dispatch.1). Every entry is one
//! shape, selected by the same `when` language and gated by the same
//! capability rungs.

use std::path::{Path, PathBuf};

use crate::branches::WorkspaceState;
use crate::capabilities::CapabilitySet;
use crate::feed::config::{ActionConfig, CheckoutConfig};
use crate::feed::model::Item;
use crate::forest::{Forest, Trail, Upstream};
use crate::work::recipe::{Facts, HandPin};

/// Actions applicable to one item: global first, then the project's own,
/// selected by the shared language (§FS-006-project-interface.9).
pub fn applicable(
    global: &[ActionConfig],
    project: &[ActionConfig],
    item: &Item,
    facts: &Facts,
) -> Vec<ActionConfig> {
    global
        .iter()
        .chain(project)
        .filter(|action| action.matches(item, facts))
        .cloned()
        .collect()
}

/// The menu, in provenance order: each list in turn, an entry whose id a later
/// list repeats **replacing it where it already sits**
/// (§FS-006-project-interface.9). Replacing in place rather than appending is
/// what keeps the numbering of a menu stable when a project starts offering an
/// entry the person had already written — the key that ran a thing goes on
/// running that thing.
pub fn merge(provenances: Vec<Vec<ActionConfig>>) -> Vec<ActionConfig> {
    let mut merged: Vec<ActionConfig> = Vec::new();
    for provenance in provenances {
        for action in provenance {
            match merged
                .iter()
                .position(|existing| !existing.id.is_empty() && existing.id == action.id)
            {
                Some(index) => merged[index] = action,
                None => merged.push(action),
            }
        }
    }
    merged
}

/// The work ephor can hand over about this item, added to the menu where
/// nothing has claimed the name and dropped where something has
/// (§FS-005-dispatch.1).
///
/// Dropped rather than appended, because an entry already carrying that name
/// is what hands this work over when it cannot finish: the key that replays a
/// branch runs `ephor rebase --dispatch`, which hands its conflict to the
/// recipe named `rebase` (§FS-005-dispatch.12). Two rows saying *rebase* would
/// be asking the reader to tell two spellings of one operation apart — which
/// is the thing this menu exists to stop.
pub fn add_unclaimed(menu: &mut Vec<ActionConfig>, entries: Vec<ActionConfig>) {
    for entry in entries {
        let claimed = menu
            .iter()
            .any(|existing| !existing.id.is_empty() && existing.id == entry.id);
        if !claimed {
            menu.push(entry);
        }
    }
}

/// A recipe as a menu entry (§FS-005-dispatch.1): its own icon and
/// description, and the recipe itself riding along, because what is dispatched
/// has to be the recipe and not a copy of what the row said about it — the
/// opening move and the hand it pins are on it.
pub fn agent_entry(recipe: &crate::work::recipe::Recipe) -> ActionConfig {
    ActionConfig {
        id: recipe.id.clone(),
        icon: recipe.icon.clone(),
        description: recipe.description.clone(),
        agent: Some(recipe.clone()),
        ..ActionConfig::default()
    }
}

/// The rebase ephor offers on a checkout with a main branch to name
/// (§FS-004-quick-actions.6). It runs `ephor rebase`, so the key and the state
/// machine's program state are the same operation (§FS-005-dispatch.12), and
/// it says how far behind the branch is and as of when, because a distance
/// with no day on it is a claim about now that nothing here measured.
pub fn rebase_action(main_branch: &str, trail: Trail) -> ActionConfig {
    rebase_entry(
        "rebase",
        &format!("rebase onto {main_branch} ({})", trail.label()),
        "",
    )
}

/// The second rebase: onto the branch's own published copy, offered where the
/// checkout trails that instead (§FS-004-quick-actions.8). It names the ref,
/// because two entries reading `rebase onto …` differ in exactly that word —
/// and where the repositories of a forest are published under different names
/// there is no one ref to name, so it says what it is instead.
pub fn upstream_rebase_action(published: Option<&str>, trail: Trail) -> ActionConfig {
    rebase_entry(
        "rebase-upstream",
        &format!(
            "rebase onto {} ({})",
            published.unwrap_or("its published copy"),
            // The copy's own day, not the base's: a fetch dates only the refs
            // it actually brought down (§FS-004-quick-actions.8).
            trail.label()
        ),
        " --upstream",
    )
}

/// One entry for both rebases, so the key the reader presses and the command a
/// state machine runs stay one operation (§FS-005-dispatch.12) and the two
/// offers cannot drift apart in how they are run.
fn rebase_entry(id: &str, description: &str, extra: &str) -> ActionConfig {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "ephor".to_string());
    ActionConfig {
        id: id.to_string(),
        icon: "⤴".to_string(),
        description: description.to_string(),
        // `--dispatch` is what makes a conflict work rather than a dead end:
        // where git stops, the ticket opens on the spot.
        command: format!(
            "{} rebase{extra} --project \"$EPHOR_PROJECT\" --checkout \"$EPHOR_WORKSPACE\" \
             --item \"$EPHOR_ITEM_ID\" --dispatch",
            crate::feed::providers::shell_quote(&exe)
        ),
        // No kind restriction: what the offer is about is a branch on disk
        // that trails something, never the kind of the row that mentions it
        // (§FS-004-quick-actions.6). The gate is the checkout resolving, and
        // it is applied where the entry is built.
        requires_checkout: true,
        // A replay asks nothing and decides nothing, so it costs no model
        // (§FS-005-dispatch.12) and no screen either: it runs beneath the
        // interface and is watched from its row (§FS-005-dispatch.17).
        background: true,
        ..ActionConfig::default()
    }
}

/// What one checkout says about itself, from a single fold: both distances
/// and what the published copy is called (§FS-004-quick-actions.8).
///
/// One fold rather than two, because the two offers stand next to each other in
/// the menu and counts measured a moment apart would eventually disagree
/// (§AR-004-forest.1).
pub struct Trailing {
    /// Commits the checkout trails its main branch, summed over the forest,
    /// with the day the oldest copy of that branch last moved here — the
    /// freshness every statement of the distance carries
    /// (§FS-004-quick-actions.6).
    pub behind: Option<Trail>,
    /// Commits it trails its own published copies, dated the same way from
    /// those copies. None where nothing is
    /// published — which is not the same answer as level with a copy — and a
    /// repository whose copy is its base again already contributes nothing:
    /// the sum leaves that distance to `behind`, so the two entries cannot
    /// carry one distance under two names (§FS-004-quick-actions.8).
    pub behind_upstream: Option<Trail>,
    /// The ref every counted repository names, where they all name one.
    pub published: Option<String>,
}

impl Trailing {
    pub fn of(forest: &Forest) -> Trailing {
        let standing = forest.standing();
        let mut published: Vec<String> = Vec::new();
        for repo in &standing.repos {
            // A copy that is the base again is not this offer's fact
            // (§FS-004-quick-actions.8), so it does not name the entry either.
            if repo.copies_the_base() {
                continue;
            }
            let Upstream::Published { remote, branch } = &repo.upstream else {
                continue;
            };
            let reference = format!("{remote}/{branch}");
            if !published.contains(&reference) {
                published.push(reference);
            }
        }
        Trailing {
            behind: standing.staleness().trail(),
            behind_upstream: standing.upstream_trail(),
            // Named only where the whole forest agrees: two different refs
            // have no one name, and an entry naming one of them would be
            // telling the reader about half its checkout.
            published: (published.len() == 1).then(|| published[0].clone()),
        }
    }

    /// The two distances in the shape a selector asks about them — bare
    /// counts, without the day. A recipe is dispatched on whether there is
    /// anything to do (§FS-005-dispatch.1), which is a different question from
    /// what the entry beside it is labelled with.
    pub fn facts(&self) -> Facts {
        Facts {
            behind: self.behind.map(|trail| trail.behind),
            behind_upstream: self.behind_upstream.map(|trail| trail.behind),
        }
    }
}

/// The checkout ephor offers on an item whose branch workspace is not on disk
/// (§FS-004-quick-actions.7). It runs `ephor checkout`, so the key and the
/// state machine's program state are the same operation (§FS-005-dispatch.12),
/// and it names the directory it is about to make because that is the thing
/// the reader is agreeing to.
///
/// It says the branch as well as the matter. A branch row has no matter behind
/// it (§FS-004-quick-actions.6), so `$EPHOR_ITEM_ID` is empty there and the
/// item alone would leave `ephor checkout` with nothing naming a branch — an
/// offer refused on the keystroke, on the one row it was added for
/// (§FS-004-quick-actions.2). Both are passed and either can be the empty
/// string: the command reads a flag or the environment and drops what is
/// blank, so the item path is unchanged by this.
pub fn checkout_action(target: &Path) -> ActionConfig {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "ephor".to_string());
    ActionConfig {
        id: "checkout".to_string(),
        icon: "⇣".to_string(),
        description: format!("check out {}", target.display()),
        command: format!(
            "{} checkout --project \"$EPHOR_PROJECT\" --item \"$EPHOR_ITEM_ID\" \
             --branch \"$EPHOR_BRANCH\"",
            crate::feed::providers::shell_quote(&exe)
        ),
        ..ActionConfig::default()
    }
}

/// The command that makes a missing branch workspace, and the directory it has
/// to end up creating: the project's own where it configured one, otherwise
/// ephor's (§FS-004-quick-actions.7). None where the workspace is not missing,
/// which is every project that keeps one checkout at its root.
///
/// One function so the row in the menu and the step that runs before an action
/// cannot come from two different commands.
pub fn checkout_step(
    state: &WorkspaceState,
    checkout: &Option<CheckoutConfig>,
) -> Option<(ActionConfig, PathBuf)> {
    let WorkspaceState::Missing(target) = state else {
        return None;
    };
    let action = match checkout {
        Some(checkout) => ActionConfig {
            id: "checkout".to_string(),
            icon: checkout.icon.clone(),
            description: checkout.description.clone(),
            command: checkout.command.clone(),
            ..ActionConfig::default()
        },
        None => checkout_action(target),
    };
    Some((action, target.clone()))
}
/// What a menu is about. An item is the usual one; a branch row is the other,
/// because the rebase is offered wherever there is a branch on disk and a
/// branch row has no matter behind it (§FS-004-quick-actions.6). The menu is
/// one implementation either way — the two subjects differ only in what the
/// summons is told they are about (§AR-002-summons.1).
#[derive(Clone)]
pub enum Subject {
    Item(Box<Item>),
    Branch { project: String, branch: String },
}

impl Subject {
    pub fn project(&self) -> &str {
        match self {
            Subject::Item(item) => &item.project,
            Subject::Branch { project, .. } => project,
        }
    }

    /// The matter this is about, where there is one. A branch row is not one,
    /// and saying so is what keeps a stand-in item out of the dossier.
    pub fn item(&self) -> Option<&Item> {
        match self {
            Subject::Item(item) => Some(item),
            Subject::Branch { .. } => None,
        }
    }

    /// What the menu's border says it is about.
    pub fn title(&self) -> &str {
        match self {
            Subject::Item(item) => &item.title,
            Subject::Branch { branch, .. } => branch,
        }
    }
}

/// Whether an entry can run right now.
#[derive(Clone)]
pub enum Gate {
    Ready,
    /// The branch workspace is missing; the checkout command runs first.
    NeedsCheckout,
    /// Cannot run; the reason is shown when chosen.
    Blocked(String),
}

/// What is already going about an entry's subject (§FS-005-dispatch.21).
///
/// Found by looking, never remembered from the keypress: a job is a held lock
/// and a record saying which entry it came from (§FS-005-dispatch.17), a run is
/// a held lock and the descriptor beside it (§FS-005-dispatch.20), and a window
/// is a job whose record carries the opener's handle (§FS-005-dispatch.22). A
/// second ephor opening the same menu sees the same marks, and a job that died
/// is not running whatever started it.
///
/// Each carries **the way in**, because the way in is the ability and spawning
/// the reader's own program on it is not (§FS-011-command-line.8,
/// §REQ-002-parity.1).
#[derive(Clone, Debug)]
pub enum Running {
    /// A job of ephor's own, still holding its lock. The way in is its log,
    /// followed as it writes (§FS-005-dispatch.17).
    Job {
        id: String,
        /// How long it has been going, in seconds.
        since: Option<i64>,
        /// What it is at right now: the last line its log said.
        says: String,
        log: PathBuf,
    },
    /// A run of the runtime holding this entry's work. The way in is the
    /// runner's own attach command (§FS-005-dispatch.20).
    Run {
        root: PathBuf,
        /// What the run calls itself, where it named itself
        /// (§AR-007-runtime.3).
        id: Option<String>,
        /// The address of its control, while it serves one.
        control_url: Option<String>,
        /// The runner's own attach command, in the runner's own words.
        attach: Option<String>,
        since: Option<i64>,
        /// The ticket the run holds and the state it is in, in the words the
        /// board already uses (§FS-005-dispatch.15).
        doing: String,
    },
    /// The root's run is live and will reach this work: the runtime schedules
    /// one run per root (§FS-005-dispatch.15). The way in is that run — it is
    /// the thing that is going, and the reader pressing the row meant it.
    Queued {
        root: PathBuf,
        id: Option<String>,
        attach: Option<String>,
        since: Option<i64>,
    },
    /// A program in a window of the reader's own (§FS-005-dispatch.22). The
    /// way in is the handle, brought forward: what the program wrote is on
    /// that screen and nowhere else, so there is no log to read
    /// (§AR-002-summons.6).
    Window {
        /// The job the window's supervisor is: liveness stays the lock.
        job: String,
        /// What the opener printed, and what focusing it takes.
        handle: String,
        since: Option<i64>,
        says: String,
    },
}

impl Running {
    /// What this is, in one word a reading can carry (§REQ-002-parity.3).
    pub fn name(&self) -> &'static str {
        match self {
            Running::Job { .. } => "job",
            Running::Run { .. } => "run",
            Running::Queued { .. } => "queued",
            Running::Window { .. } => "window",
        }
    }

    /// How long it has been going, in seconds, where that is known.
    pub fn since(&self) -> Option<i64> {
        match self {
            Running::Job { since, .. }
            | Running::Run { since, .. }
            | Running::Queued { since, .. }
            | Running::Window { since, .. } => *since,
        }
    }

    /// What it is at right now — the job's own last line, the ticket a run
    /// holds and the state it is in, *queued* where the root's run will reach
    /// it (§FS-005-dispatch.21). One sentence, so the screen and the command
    /// line phrase one situation one way (§AR-009-surfaces.1).
    pub fn says(&self) -> String {
        match self {
            Running::Job { says, .. } => says.clone(),
            Running::Run { doing, .. } => doing.clone(),
            Running::Queued { .. } => "queued".to_string(),
            // The job's own line already says where it is: a windowed program
            // writes to a screen and not to a file (§FS-005-dispatch.22).
            Running::Window { says, handle, .. } => match says.is_empty() {
                true => format!("running in window {handle}"),
                false => says.clone(),
            },
        }
    }

    /// What pressing this row opens, said in one line for a footer or a
    /// refusal (§FS-004-quick-actions.2). None where there is nothing to open:
    /// a run that named itself nothing has no surface to put on it
    /// (§AR-007-runtime.3).
    pub fn way_in(&self) -> Option<String> {
        match self {
            Running::Job { log, .. } => Some(log.display().to_string()),
            Running::Run { attach, .. } | Running::Queued { attach, .. } => attach.clone(),
            Running::Window { handle, .. } => Some(handle.clone()),
        }
    }
}

#[derive(Clone)]
pub struct MenuEntry {
    pub action: ActionConfig,
    /// The synthetic "check out branch workspace" row.
    pub is_checkout: bool,
    /// The synthetic row with no command yet: the reader types one
    /// (§FS-005-dispatch.10).
    pub is_freehand: bool,
    /// The synthetic row that opens the runtime's own workflows, for the ones
    /// no entry names (§FS-005-dispatch.19).
    pub is_workflows: bool,
    /// What the reader picked for this dispatch alone, where the picker was
    /// used (§FS-005-dispatch.14). It rides the one outcome that carries it
    /// to the dispatch and dies there: nothing records it, and the next
    /// dispatch resolves from the second step down.
    pub picked: Option<HandPin>,
    pub gate: Gate,
    /// What is already going about this entry's subject, where anything is
    /// (§FS-005-dispatch.21). Filled in by the session that assembles the
    /// menu, because it is a reading of the world and not a property of the
    /// entry (§AR-009-surfaces.1).
    pub running: Option<Running>,
}

/// The whole list a subject carries, gated: the checkout row where the branch
/// workspace is missing, then every configured and synthesized entry with the
/// one table's answer to what it said it needs, then the freehand row and —
/// where the runtime has any — the row that reaches its own workflows.
///
/// One assembly for both surfaces (§AR-009-surfaces.1): the menu draws this
/// and `ephor actions` prints it, so a gate cannot be computed twice and
/// answer differently on the two of them.
pub fn entries(
    state: &WorkspaceState,
    checkout: &Option<CheckoutConfig>,
    can: &CapabilitySet,
    actions: Vec<ActionConfig>,
    has_workflows: bool,
) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    // A missing workspace is directly runnable as its own entry. The
    // project's own command where it configured one, and ephor's otherwise
    // — the offer does not wait on anybody writing it down
    // (§FS-004-quick-actions.7).
    if let Some((action, _)) = checkout_step(state, checkout) {
        entries.push(MenuEntry {
            action,
            is_checkout: true,
            is_freehand: false,
            is_workflows: false,
            picked: None,
            gate: Gate::NeedsCheckout,
            running: None,
        });
    }
    for action in actions {
        let gate = gate_of(&action, state, can);
        // Nothing but ephor's own rows stands in ephor's namespace
        // ([`RESERVED`]). A person's configuration is refused where it is
        // written and a project's manifest by the schema's own pattern, so
        // what could still arrive here is a name from further out — a
        // workflow the runtime offers under one. It is shown, with the
        // reason, under a name of its own: a row a command cannot tell from
        // `@command` is a row that runs when `@command` is asked for, and an
        // entry silently dropped reads exactly like an oversight
        // (§REQ-001-boundary.1).
        let (action, gate) = match reserved(&action.id) {
            Some(why) => (
                ActionConfig {
                    id: String::new(),
                    ..action
                },
                Gate::Blocked(why),
            ),
            None => (action, gate),
        };
        entries.push(MenuEntry {
            action,
            is_checkout: false,
            is_freehand: false,
            is_workflows: false,
            picked: None,
            gate,
            running: None,
        });
    }
    // The row that reaches the runtime's workflows no entry names
    // (§FS-005-dispatch.19). Above the freehand row, because it is an offer
    // and that one is the escape hatch; below everything real, because most
    // of what it opens has nothing to do with this matter — which is the
    // whole reason those workflows are behind one row instead of in the menu.
    if has_workflows {
        entries.push(MenuEntry {
            action: ActionConfig {
                id: WORKFLOWS_ID.to_string(),
                icon: "▤".to_string(),
                description: "lay down a workflow…".to_string(),
                ..ActionConfig::default()
            },
            is_checkout: false,
            is_freehand: false,
            is_workflows: true,
            picked: None,
            gate: Gate::Ready,
            running: None,
        });
    }
    // Last, and always there: what the reader wants to run once
    // (§FS-005-dispatch.10). It leads nothing and blocks nothing — a menu
    // whose first key is "type something" would be a menu that gave up.
    entries.push(MenuEntry {
        action: ActionConfig {
            id: FREEHAND_ID.to_string(),
            icon: "⌨".to_string(),
            description: "run a command here…".to_string(),
            ..ActionConfig::default()
        },
        is_checkout: false,
        is_freehand: true,
        is_workflows: false,
        picked: None,
        gate: Gate::Ready,
        running: None,
    });
    entries
}

/// The id the freehand row answers to on the command line. The interface
/// reaches it with a cursor; a command needs a name for it, and a name it
/// could collide with a configured entry over would be worse than none —
/// hence the marker, which [`RESERVED`] keeps out of configuration.
pub const FREEHAND_ID: &str = "@command";

/// The same, for the row that opens the runtime's own workflows.
pub const WORKFLOWS_ID: &str = "@workflows";

/// The mark on every id ephor mints for a row of its own. Everything under it
/// is ephor's namespace, and configuration may not enter it.
///
/// This is a rule about *values*, and it has to be enforced as one.
/// `deny_unknown_fields` refuses an unknown key and says nothing about what a
/// known one holds, so an entry configured with `"id": "@command"` used to
/// stand beside the freehand row under the same name — and `ephor actions run
/// @command`, which finds the first entry answering to the id, ran the
/// impostor. Two rows that a command cannot tell apart is the failure the
/// marker exists to prevent (§FS-005-dispatch.10), so the id is refused where
/// it is written (§REQ-001-boundary.1: stated, not silently renamed) rather
/// than deduplicated afterwards.
pub const RESERVED: char = '@';

/// Why this id may not be configured, or None where it may be. Returned
/// rather than printed: the same sentence has to reach a configuration error
/// and a schema message (§AR-005-capabilities.2).
pub fn reserved(id: &str) -> Option<String> {
    id.starts_with(RESERVED).then(|| {
        format!(
            "'{id}' starts with '{RESERVED}', which is how ephor names the rows it mints itself \
             ({FREEHAND_ID} runs a command you type, {WORKFLOWS_ID} opens the runtime's \
             workflows). Give it a name of your own."
        )
    })
}

/// What the entry said it needs, answered by the one table
/// (§AR-005-capabilities.2) — so a project's offer and a person's action are
/// refused in the same sentence, and a requirement nobody recognizes is named
/// rather than treated as met.
pub fn gate_of(action: &ActionConfig, state: &WorkspaceState, can: &CapabilitySet) -> Gate {
    let (rungs, unknown) = action.rungs();
    if let Some(name) = unknown.first() {
        return Gate::Blocked(format!(
            "'{name}' is not a capability ephor knows; it has: {}",
            crate::capabilities::Rung::all()
                .map(|rung| rung.name())
                .join(", ")
        ));
    }
    if let Some(reason) = can.refusal(&rungs) {
        return Gate::Blocked(reason);
    }
    // A hand that cannot stand is the entry refused, not the ticket written
    // and the choice quietly dropped (§FS-006-project-interface.9): the
    // reason is on the row and the key is not advertised on it
    // (§FS-004-quick-actions.2).
    if let Some(refusal) = action.hand.as_ref().and_then(|hand| hand.refusal.clone()) {
        return Gate::Blocked(refusal);
    }
    if !action.requires_checkout {
        return Gate::Ready;
    }
    match state {
        WorkspaceState::Ready => Gate::Ready,
        // There is always a checkout to run first now, configured or
        // ephor's own (§FS-004-quick-actions.7).
        WorkspaceState::Missing(_) => Gate::NeedsCheckout,
        // A workspace the item cannot be resolved to is the branch-addressable
        // rung failing on this item (§FS-006-project-interface.10).
        WorkspaceState::Unmatched => Gate::Blocked(
            "this action needs a branch workspace, and the item's branch is unknown".to_string(),
        ),
    }
}

impl Gate {
    /// The sentence a blocked entry carries, for the surface that prints
    /// rather than greys (§REQ-002-parity.2).
    pub fn refusal(&self) -> Option<&str> {
        match self {
            Gate::Blocked(reason) => Some(reason),
            _ => None,
        }
    }

    /// What this gate is called in a reading (§REQ-002-parity.3).
    pub fn name(&self) -> &'static str {
        match self {
            Gate::Ready => "ready",
            Gate::NeedsCheckout => "needs-checkout",
            Gate::Blocked(_) => "blocked",
        }
    }
}

impl MenuEntry {
    /// How a command names this entry. A configured entry answers to its own
    /// id; an anonymous one — a person's action with no `id` — answers to its
    /// description, which is the only thing that distinguishes it on a screen
    /// either (§FS-006-project-interface.9).
    pub fn key(&self) -> String {
        if self.action.id.is_empty() {
            self.action.description.clone()
        } else {
            self.action.id.clone()
        }
    }

    /// What this entry does, as a reading names it: it runs a command here,
    /// hands work over, lays a workflow down, makes the workspace, or is one
    /// of the two rows that open something else.
    pub fn kind(&self) -> &'static str {
        if self.is_checkout {
            "checkout"
        } else if self.is_freehand {
            "freehand"
        } else if self.is_workflows {
            "workflows"
        } else if self.action.agent.is_some() {
            "agent"
        } else if self.action.workflow.is_some() {
            "workflow"
        } else {
            "command"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(id: &str) -> ActionConfig {
        ActionConfig {
            id: id.to_string(),
            icon: "▸".to_string(),
            description: format!("do {id}"),
            command: "true".to_string(),
            ..ActionConfig::default()
        }
    }

    /// The `@` namespace is ephor's, and an entry claiming it never reaches
    /// the menu under that name. `deny_unknown_fields` refuses an unknown
    /// *key* and says nothing about what a known one holds, so this is the
    /// rule about values enforced as one: two rows answering to `@command`
    /// would make `ephor actions run @command` run whichever came first.
    #[test]
    fn nothing_configured_stands_in_ephors_own_namespace() {
        let entries = entries(
            &WorkspaceState::Ready,
            &None,
            &CapabilitySet::unknown("demo"),
            vec![plain(FREEHAND_ID), plain(WORKFLOWS_ID), plain("note")],
            true,
        );
        let freehand: Vec<&MenuEntry> = entries
            .iter()
            .filter(|entry| entry.key() == FREEHAND_ID)
            .collect();
        assert_eq!(freehand.len(), 1, "one row answers to '{FREEHAND_ID}'");
        assert!(freehand[0].is_freehand, "and it is ephor's own");
        let workflows: Vec<&MenuEntry> = entries
            .iter()
            .filter(|entry| entry.key() == WORKFLOWS_ID)
            .collect();
        assert_eq!(workflows.len(), 1);
        assert!(workflows[0].is_workflows);
        // The impostors are still listed, refused by name: an entry that
        // vanished would read exactly like an oversight (§REQ-001-boundary.1).
        let refused: Vec<&str> = entries
            .iter()
            .filter_map(|entry| entry.gate.refusal())
            .collect();
        assert_eq!(
            refused.len(),
            2,
            "both are shown with the reason: {refused:?}"
        );
        assert!(refused
            .iter()
            .all(|why| why.contains("names the rows it mints itself")));
        // And an ordinary id is untouched.
        assert!(entries.iter().any(|entry| entry.key() == "note"));
    }

    /// A name of one's own is not in the namespace, whatever it contains.
    #[test]
    fn a_name_of_ones_own_is_left_alone() {
        assert!(reserved("note").is_none());
        assert!(reserved("rebase-upstream").is_none());
        assert!(reserved("mail@example").is_none());
        assert!(reserved(FREEHAND_ID).is_some());
        assert!(reserved(WORKFLOWS_ID).is_some());
        assert!(reserved("@anything").is_some());
    }
}
