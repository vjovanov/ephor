//! The action menu: what can be done about a feed item, from all four places
//! it can come from (§FS-006-project-interface.9, §FS-005-dispatch.1).
//!
//! Provenance orders the menu — what ephor itself recognized
//! (§FS-004-quick-actions.3), then the project's offers, then the person's own
//! from `status.json` (`actions` globally, plus per-project
//! `projects.<id>.actions`) — and where two entries share an id, the later
//! provenance wins in place. Then the recipes: the two lists are one menu, so
//! "what can I do about this row" has one answer rather than depending on
//! which key the reader knew to press (§FS-005-dispatch.1). Every entry is one
//! shape, selected by the same `when` language and gated by the same
//! capability rungs. A command entry runs via `sh -c` in the project's
//! checkout with the item's context exported as `EPHOR_*` environment
//! variables; an agent entry is handed over instead, through the one path the
//! work screen uses (§FS-005-dispatch.4).

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use crate::capabilities::CapabilitySet;
use crate::feed::config::{ActionConfig, CheckoutConfig};
use crate::feed::model::Item;
use crate::forest::{Forest, Upstream};
use crate::work::recipe::Facts;

use super::{highlight_style, BranchInfo, WorkspaceState};

/// Actions applicable to one item: global first, then the project's own,
/// selected by the shared language (§FS-006-project-interface.9).
pub(crate) fn applicable(
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
pub(crate) fn merge(provenances: Vec<Vec<ActionConfig>>) -> Vec<ActionConfig> {
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
pub(crate) fn add_unclaimed(menu: &mut Vec<ActionConfig>, entries: Vec<ActionConfig>) {
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
pub(crate) fn agent_entry(recipe: &crate::work::recipe::Recipe) -> ActionConfig {
    ActionConfig {
        id: recipe.id.clone(),
        icon: recipe.icon.clone(),
        description: recipe.description.clone(),
        agent: Some(recipe.clone()),
        ..ActionConfig::default()
    }
}

/// The rebase ephor offers on a checkout that trails its project's main
/// branch (§FS-004-quick-actions.6). It runs `ephor rebase`, so the key and
/// the state machine's program state are the same operation
/// (§FS-005-dispatch.12), and it says how far behind the branch is because
/// that is the fact the reader is being asked to act on.
pub(crate) fn rebase_action(main_branch: &str, behind: u64) -> ActionConfig {
    rebase_entry(
        "rebase",
        &format!("rebase onto {main_branch} ({behind} behind)"),
        "",
    )
}

/// The second rebase: onto the branch's own published copy, offered where the
/// checkout trails that instead (§FS-004-quick-actions.8). It names the ref,
/// because two entries reading `rebase onto …` differ in exactly that word —
/// and where the repositories of a forest are published under different names
/// there is no one ref to name, so it says what it is instead.
pub(crate) fn upstream_rebase_action(published: Option<&str>, behind: u64) -> ActionConfig {
    rebase_entry(
        "rebase-upstream",
        &format!(
            "rebase onto {} ({behind} behind)",
            published.unwrap_or("its published copy")
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
        ..ActionConfig::default()
    }
}

/// What one checkout says about itself, from a single fold: both distances
/// and what the published copy is called (§FS-004-quick-actions.8).
///
/// One fold rather than two, because the two offers stand next to each other in
/// the menu and counts measured a moment apart would eventually disagree
/// (§AR-004-forest.1).
pub(crate) struct Trailing {
    /// Commits the checkout trails its main branch, summed over the forest.
    pub behind: Option<u64>,
    /// Commits it trails its own published copies. None where nothing is
    /// published — which is not the same answer as level with a copy — and a
    /// repository whose copy is its base again already contributes nothing:
    /// the sum leaves that distance to `behind`, so the two entries cannot
    /// carry one distance under two names (§FS-004-quick-actions.8).
    pub behind_upstream: Option<u64>,
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
            behind: standing.staleness().total(),
            behind_upstream: standing.behind_upstream(),
            // Named only where the whole forest agrees: two different refs
            // have no one name, and an entry naming one of them would be
            // telling the reader about half its checkout.
            published: (published.len() == 1).then(|| published[0].clone()),
        }
    }

    /// The two distances in the shape a selector asks about them.
    pub fn facts(&self) -> Facts {
        Facts {
            behind: self.behind,
            behind_upstream: self.behind_upstream,
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
pub(crate) fn checkout_action(target: &Path) -> ActionConfig {
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
pub(crate) fn checkout_step(
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

pub(crate) enum MenuOutcome {
    Stay,
    Close,
    Run(MenuEntry),
}

/// What a menu is about. An item is the usual one; a branch row is the other,
/// because the rebase is offered wherever there is a branch on disk and a
/// branch row has no matter behind it (§FS-004-quick-actions.6). The menu is
/// one implementation either way — the two subjects differ only in what the
/// summons is told they are about (§AR-002-summons.1).
#[derive(Clone)]
pub(crate) enum Subject {
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
pub(crate) enum Gate {
    Ready,
    /// The branch workspace is missing; the checkout command runs first.
    NeedsCheckout,
    /// Cannot run; the reason is shown when chosen.
    Blocked(String),
}

#[derive(Clone)]
pub(crate) struct MenuEntry {
    pub action: ActionConfig,
    /// The synthetic "check out branch workspace" row.
    pub is_checkout: bool,
    /// The synthetic row with no command yet: the reader types one
    /// (§FS-005-dispatch.10).
    pub is_freehand: bool,
    pub gate: Gate,
}

pub(crate) struct ActionMenu {
    pub subject: Subject,
    /// The project root and the resolved checkout the action runs in (the
    /// branch workspace when one exists, otherwise the root).
    pub root: PathBuf,
    pub workspace: PathBuf,
    /// The registry branch the item was matched to, if any.
    pub branch: Option<BranchInfo>,
    /// The item's branch-workspace situation at menu-open time.
    state: WorkspaceState,
    pub checkout: Option<CheckoutConfig>,
    entries: Vec<MenuEntry>,
    selected: usize,
    /// An entry that asked to be confirmed and has been chosen once
    /// (§FS-006-project-interface.9): the next Enter on it runs it.
    confirming: Option<usize>,
}

impl ActionMenu {
    pub fn new(
        subject: Subject,
        root: PathBuf,
        workspace: PathBuf,
        branch: Option<BranchInfo>,
        state: WorkspaceState,
        checkout: Option<CheckoutConfig>,
        can: &CapabilitySet,
        actions: Vec<ActionConfig>,
    ) -> Self {
        let mut entries = Vec::new();
        // A missing workspace is directly runnable as its own entry. The
        // project's own command where it configured one, and ephor's otherwise
        // — the offer does not wait on anybody writing it down
        // (§FS-004-quick-actions.7).
        if let Some((action, _)) = checkout_step(&state, &checkout) {
            entries.push(MenuEntry {
                action,
                is_checkout: true,
                is_freehand: false,
                gate: Gate::NeedsCheckout,
            });
        }
        for action in actions {
            // What the entry said it needs, answered by the one table
            // (§AR-005-capabilities.2) — so a project's offer and a person's
            // action are refused in the same sentence, and a requirement
            // nobody recognizes is named rather than treated as met.
            let (rungs, unknown) = action.rungs();
            let gate = if let Some(name) = unknown.first() {
                Gate::Blocked(format!(
                    "'{name}' is not a capability ephor knows; it has: {}",
                    crate::capabilities::Rung::all()
                        .map(|rung| rung.name())
                        .join(", ")
                ))
            } else if let Some(reason) = can.refusal(&rungs) {
                Gate::Blocked(reason)
            // A hand that cannot stand is the entry refused, not the ticket
            // written and the choice quietly dropped
            // (§FS-006-project-interface.9): the reason is on the row and the
            // key is not advertised on it (§FS-004-quick-actions.2).
            } else if let Some(refusal) = action.hand.as_ref().and_then(|hand| hand.refusal.clone())
            {
                Gate::Blocked(refusal)
            } else if !action.requires_checkout {
                Gate::Ready
            } else {
                match &state {
                    WorkspaceState::Ready => Gate::Ready,
                    // There is always a checkout to run first now, configured
                    // or ephor's own (§FS-004-quick-actions.7).
                    WorkspaceState::Missing(_) => Gate::NeedsCheckout,
                    // A workspace the item cannot be resolved to is the
                    // branch-addressable rung failing on this item
                    // (§FS-006-project-interface.10).
                    WorkspaceState::Unmatched => Gate::Blocked(
                        "this action needs a branch workspace, and the item's branch is unknown"
                            .to_string(),
                    ),
                }
            };
            entries.push(MenuEntry {
                action,
                is_checkout: false,
                is_freehand: false,
                gate,
            });
        }
        // Last, and always there: what the reader wants to run once
        // (§FS-005-dispatch.10). It leads nothing and blocks nothing — a menu
        // whose first key is "type something" would be a menu that gave up.
        entries.push(MenuEntry {
            action: ActionConfig {
                icon: "⌨".to_string(),
                description: "run a command here…".to_string(),
                ..ActionConfig::default()
            },
            is_checkout: false,
            is_freehand: true,
            gate: Gate::Ready,
        });
        ActionMenu {
            subject,
            root,
            workspace,
            branch,
            state,
            checkout,
            entries,
            selected: 0,
            confirming: None,
        }
    }

    /// The checkout to run before an action that needs the workspace, and the
    /// directory it has to create (§FS-004-quick-actions.7).
    pub fn checkout_step(&self) -> Option<(ActionConfig, PathBuf)> {
        checkout_step(&self.state, &self.checkout)
    }

    /// Built from the selected entry, not from the menu
    /// (§FS-004-quick-actions.2): `Enter` does four different things here —
    /// run a command, check a workspace out, hand work over, ask for a command
    /// to run — and on an entry that cannot act it does none of them. A footer
    /// that said "run" over all of that would teach the key and leave the
    /// reader to find out what it meant from a line at the bottom of a screen
    /// they were not reading.
    pub fn footer(&self) -> String {
        let mut keys = String::from(" j/k move");
        if self.entries.len() > 1 {
            keys.push_str("  1-9 pick");
        }
        if let Some(entry) = self.entries.get(self.selected) {
            let verb = match &entry.gate {
                // The reason is on the row; there is nothing to press.
                Gate::Blocked(_) => None,
                _ if self.confirming == Some(self.selected) => {
                    Some("enter again to confirm".to_string())
                }
                _ if entry.is_freehand => Some("enter type a command".to_string()),
                _ if entry.is_checkout => Some("enter check out".to_string()),
                // An agent entry says who it goes to here as well as on the
                // row, because this is the line the reader reads last before
                // pressing (§FS-005-dispatch.14).
                _ if entry.action.agent.is_some() => Some(match &entry.action.hand {
                    Some(hand) => format!("enter hand over to {}", hand.says),
                    None => "enter hand it over".to_string(),
                }),
                _ => Some("enter run".to_string()),
            };
            if let Some(verb) = verb {
                keys.push_str(&format!("  {verb}"));
            }
        }
        keys.push_str("  esc cancel");
        keys
    }

    pub fn handle_key(&mut self, code: KeyCode) -> MenuOutcome {
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('x') => MenuOutcome::Close,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                }
                self.confirming = None;
                MenuOutcome::Stay
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.confirming = None;
                MenuOutcome::Stay
            }
            KeyCode::Enter => self.choose(self.selected),
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                let index = (digit as usize).wrapping_sub('1' as usize);
                match self.entries.get(index) {
                    Some(_) => self.choose(index),
                    None => MenuOutcome::Stay,
                }
            }
            _ => MenuOutcome::Stay,
        }
    }

    /// Choosing an entry runs it — unless it asked to be confirmed, and this
    /// is the first time it was chosen (§FS-006-project-interface.9). The
    /// second choice on the same entry runs it; choosing anything else drops
    /// the question, because a confirmation that survives the reader moving
    /// away is a trap.
    ///
    /// An entry that cannot run is chosen and nothing happens: the footer
    /// already teaches no key on it (§FS-004-quick-actions.2), and taking the
    /// menu down to repeat in the header the reason the row is carrying would
    /// answer a key that does nothing by hiding what the reader was reading.
    fn choose(&mut self, index: usize) -> MenuOutcome {
        let Some(entry) = self.entries.get(index) else {
            return MenuOutcome::Stay;
        };
        if matches!(entry.gate, Gate::Blocked(_)) {
            // Choosing it is still choosing something else, so a question
            // asked of another entry is dropped rather than left waiting to
            // catch the next Enter.
            self.confirming = None;
            self.selected = index;
            return MenuOutcome::Stay;
        }
        if entry.action.confirm && self.confirming != Some(index) {
            self.confirming = Some(index);
            self.selected = index;
            return MenuOutcome::Stay;
        }
        self.confirming = None;
        MenuOutcome::Run(entry.clone())
    }

    /// What an entry's gate says, by position.
    #[cfg(test)]
    fn gate(&self, index: usize) -> &Gate {
        &self.entries[index].gate
    }

    /// Centered overlay over the given area.
    pub fn draw(&self, frame: &mut ratatui::Frame, area: Rect) {
        let width = area.width.saturating_sub(4).min(72).max(20);
        let height = (self.entries.len() as u16 + 2).min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, rect);

        let rows: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let number = if index < 9 {
                    format!(" {} ", index + 1)
                } else {
                    "   ".to_string()
                };
                let mut spans = vec![
                    Span::styled(number, Style::default().fg(Color::DarkGray)),
                    Span::raw(format!(
                        "{}  {}",
                        entry.action.icon, entry.action.description
                    )),
                ];
                // Who would get this work, before the key is pressed
                // (§FS-005-dispatch.14). Where the choice was refused the row
                // already carries the whole reason below, so it is not said
                // twice.
                if let Some(hand) = entry
                    .action
                    .hand
                    .as_ref()
                    .filter(|hand| hand.refusal.is_none())
                {
                    spans.push(Span::styled(
                        format!("  → {}", hand.says),
                        Style::default().fg(Color::Cyan),
                    ));
                }
                if self.confirming == Some(index) {
                    spans.push(Span::styled(
                        "  press again to run",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                match &entry.gate {
                    Gate::NeedsCheckout if !entry.is_checkout => spans.push(Span::styled(
                        "  (will check out first)",
                        Style::default().fg(Color::Yellow),
                    )),
                    // The reason is rendered where the entry is, not saved for
                    // whoever presses it: an entry marked only "unavailable"
                    // teaches nothing (§AR-002-summons.4).
                    Gate::Blocked(reason) => spans.push(Span::styled(
                        format!("  (unavailable: {reason})"),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    )),
                    _ => {}
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let title: String = self
            .subject
            .title()
            .chars()
            .take(width as usize - 12)
            .collect();
        let list = List::new(rows)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" actions — {title} ")),
            )
            .highlight_style(highlight_style());
        let mut state = ListState::default();
        state.select(Some(self.selected));
        frame.render_stateful_widget(list, rect, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::ItemKind;
    use chrono::Utc;
    use serde_json::json;

    fn action(description: &str, kinds: &[&str]) -> ActionConfig {
        ActionConfig {
            icon: "⚙".to_string(),
            description: description.to_string(),
            command: "true".to_string(),
            kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
            ..ActionConfig::default()
        }
    }

    /// Nothing measured about the checkout — the answer for every item whose
    /// branch is not on this machine.
    fn facts() -> Facts {
        Facts::default()
    }

    /// A project that holds every rung, so gating is only what a test asks
    /// for.
    fn can_everything() -> CapabilitySet {
        let tmp = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let placement = crate::branches::Placement {
            project: "widget".to_string(),
            root: tmp.path().to_path_buf(),
            template: Some("{project_root}/{branch}".to_string()),
            branches: Vec::new(),
            main_branch: Some("main".to_string()),
            repos: Vec::new(),
            aliases: Vec::new(),
            territory: Vec::new(),
            trust: crate::manifest::Trust::Full,
        };
        std::fs::write(tmp.path().join("check.sh"), "#!/bin/sh\n").unwrap();
        std::fs::create_dir_all(tmp.path().join("panta")).unwrap();
        CapabilitySet::resolve(
            "widget",
            Some(&placement),
            &crate::capabilities::Bindings {
                sources: 1,
                answering: Some(1),
                checkout: Some("git worktree add"),
                runner: Some("sh"),
                gate_reported: true,
                manifest: None,
            },
        )
    }

    fn item(kind: ItemKind, id: &str, raw: serde_json::Value) -> Item {
        Item {
            id: id.to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind,
            role: None,
            title: "Fix condition errors".to_string(),
            url: Some("https://github.com/acme/widget/pull/42".to_string()),
            state: Some("open".to_string()),
            needs_response: false,
            updated_at: Utc::now(),
            raw,
        }
    }

    #[test]
    fn applicable_filters_by_kind_and_merges() {
        let global = [action("everywhere", &[]), action("prs only", &["pr"])];
        let project = [action("project ci", &["ci"])];

        let pr = item(ItemKind::Pr, "github-prs:acme/widget#42", json!({}));
        let names: Vec<String> = applicable(&global, &project, &pr, &facts())
            .into_iter()
            .map(|action| action.description)
            .collect();
        assert_eq!(names, ["everywhere", "prs only"]);

        let ci = item(ItemKind::Ci, "github-ci:acme/widget#42", json!({}));
        let names: Vec<String> = applicable(&global, &project, &ci, &facts())
            .into_iter()
            .map(|action| action.description)
            .collect();
        assert_eq!(names, ["everywhere", "project ci"]);
    }

    #[test]
    fn message_kind_accepts_both_spellings() {
        let message = item(ItemKind::Message, "slack:123", json!({}));
        assert_eq!(
            applicable(&[action("a", &["message"])], &[], &message, &facts()).len(),
            1
        );
        assert_eq!(
            applicable(&[action("a", &["msg"])], &[], &message, &facts()).len(),
            1
        );
        assert_eq!(
            applicable(&[action("a", &["pr"])], &[], &message, &facts()).len(),
            0
        );
    }

    fn menu(
        state: WorkspaceState,
        checkout: Option<CheckoutConfig>,
        actions: Vec<ActionConfig>,
    ) -> ActionMenu {
        let pr = item(ItemKind::Pr, "github-prs:acme/widget#42", json!({}));
        ActionMenu::new(
            Subject::Item(Box::new(pr)),
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            state,
            checkout,
            &can_everything(),
            actions,
        )
    }

    fn checkout_config() -> CheckoutConfig {
        serde_json::from_value(json!({
            "command": "git worktree add \"$EPHOR_WORKSPACE\" \"$EPHOR_BRANCH\""
        }))
        .unwrap()
    }

    #[test]
    fn menu_runs_by_digit_and_enter() {
        let actions = vec![action("first", &[]), action("second", &[])];
        let mut menu = menu(WorkspaceState::Ready, None, actions);

        match menu.handle_key(KeyCode::Char('2')) {
            MenuOutcome::Run(entry) => assert_eq!(entry.action.description, "second"),
            _ => panic!("expected Run"),
        }
        menu.handle_key(KeyCode::Char('j'));
        match menu.handle_key(KeyCode::Enter) {
            MenuOutcome::Run(entry) => assert_eq!(entry.action.description, "second"),
            _ => panic!("expected Run"),
        }
        assert!(matches!(
            menu.handle_key(KeyCode::Char('9')),
            MenuOutcome::Stay
        ));
        assert!(matches!(menu.handle_key(KeyCode::Esc), MenuOutcome::Close));
    }

    fn requires_checkout(description: &str) -> ActionConfig {
        let mut config = action(description, &[]);
        config.requires_checkout = true;
        config
    }

    #[test]
    fn missing_workspace_offers_checkout_row_and_gates_actions() {
        let target = PathBuf::from("/tmp/ws/branch");
        let mut menu = menu(
            WorkspaceState::Missing(target.clone()),
            Some(checkout_config()),
            vec![requires_checkout("open ide"), action("browser", &[])],
        );
        assert_eq!(menu.checkout_step().map(|(_, target)| target), Some(target));

        // Row 1 is the synthetic checkout entry.
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => {
                assert!(entry.is_checkout);
                assert_eq!(entry.action.description, "check out branch workspace");
            }
            _ => panic!("expected Run"),
        }
        // The gated action will check out first; the plain one is ready.
        match menu.handle_key(KeyCode::Char('2')) {
            MenuOutcome::Run(entry) => {
                assert!(!entry.is_checkout);
                assert!(matches!(entry.gate, Gate::NeedsCheckout));
            }
            _ => panic!("expected Run"),
        }
        match menu.handle_key(KeyCode::Char('3')) {
            MenuOutcome::Run(entry) => assert!(matches!(entry.gate, Gate::Ready)),
            _ => panic!("expected Run"),
        }
    }

    /// Nothing configured, and the checkout is still offered: it is one
    /// operation on every project, and ephor holds every input it takes
    /// (§FS-004-quick-actions.7).
    #[test]
    fn a_missing_workspace_is_offered_ephors_own_checkout() {
        let target = PathBuf::from("/tmp/ws/branch");
        let mut menu = menu(
            WorkspaceState::Missing(target.clone()),
            None,
            vec![requires_checkout("open ide")],
        );
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => {
                assert!(entry.is_checkout);
                assert!(entry.action.description.contains("/tmp/ws/branch"));
                assert!(entry.action.command.contains("checkout --project"));
                // The branch as well as the matter: on a branch row there is
                // no matter, and the item alone would leave `ephor checkout`
                // with nothing naming a branch (§FS-004-quick-actions.6).
                assert!(
                    entry.action.command.contains("--branch \"$EPHOR_BRANCH\""),
                    "{}",
                    entry.action.command
                );
            }
            _ => panic!("expected Run"),
        }
        // And the action that needs the workspace checks out first rather than
        // being refused for a command nobody wrote.
        match menu.handle_key(KeyCode::Char('2')) {
            MenuOutcome::Run(entry) => assert!(matches!(entry.gate, Gate::NeedsCheckout)),
            _ => panic!("expected Run"),
        }
        assert_eq!(menu.checkout_step().map(|(_, path)| path), Some(target));
    }

    /// An item matched to no branch has no workspace to make, so the refusal
    /// stands (§FS-004-quick-actions.2) — and choosing it does nothing rather
    /// than taking the menu down.
    #[test]
    fn gated_action_blocks_where_there_is_no_branch_to_check_out() {
        let mut menu = menu_unmatched(vec![requires_checkout("open ide")]);
        assert!(matches!(
            menu.handle_key(KeyCode::Char('1')),
            MenuOutcome::Stay
        ));
        assert!(matches!(menu.gate(0), Gate::Blocked(_)));
    }

    /// The footer teaches no key on an entry that cannot run, and the key
    /// keeps that promise: Enter leaves the menu standing, with the reason
    /// where it already was — on the row (§FS-004-quick-actions.2). Closing
    /// it to repeat the reason in the header would answer a key that does
    /// nothing by hiding what the reader was reading.
    #[test]
    fn a_blocked_entry_is_chosen_and_nothing_happens() {
        let mut menu = menu_unmatched(vec![requires_checkout("open ide"), action("browser", &[])]);
        assert!(!menu.footer().contains("enter"), "{}", menu.footer());
        assert!(matches!(menu.handle_key(KeyCode::Enter), MenuOutcome::Stay));
        // Chosen by number from anywhere: the cursor lands on it, so the
        // reason is under the reader rather than announced somewhere else.
        menu.handle_key(KeyCode::Char('j'));
        assert!(menu.footer().contains("enter run"), "{}", menu.footer());
        assert!(matches!(
            menu.handle_key(KeyCode::Char('1')),
            MenuOutcome::Stay
        ));
        assert!(!menu.footer().contains("enter"), "{}", menu.footer());
        // And the entries that can run still do.
        match menu.handle_key(KeyCode::Char('2')) {
            MenuOutcome::Run(entry) => assert_eq!(entry.action.description, "browser"),
            _ => panic!("expected the runnable entry to run"),
        }
    }

    fn menu_unmatched(actions: Vec<ActionConfig>) -> ActionMenu {
        menu(WorkspaceState::Unmatched, Some(checkout_config()), actions)
    }

    fn named(id: &str, description: &str) -> ActionConfig {
        ActionConfig {
            id: id.to_string(),
            ..action(description, &[])
        }
    }

    /// Provenance orders the menu and a repeated id replaces where it already
    /// sits, so the key that ran a thing goes on running that thing
    /// (§FS-006-project-interface.9).
    #[test]
    fn a_later_provenance_replaces_a_shared_id_where_it_already_sits() {
        let merged = merge(vec![
            vec![
                named("ci-failures", "see the CI failures"),
                named("rebase", "rebase"),
            ],
            vec![
                named("ci-failures", "the project's own failure view"),
                named("bench", "benchmark it"),
            ],
            vec![named("bench", "my benchmark"), named("ide", "open the ide")],
        ]);
        let described: Vec<&str> = merged
            .iter()
            .map(|action| action.description.as_str())
            .collect();
        assert_eq!(
            described,
            [
                // The project's view of the failures, still first because that
                // is where the shipped entry it replaced was.
                "the project's own failure view",
                "rebase",
                // The person's benchmark, at the position the project's took.
                "my benchmark",
                "open the ide",
            ]
        );
    }

    /// A recipe is an action: it joins the menu carrying its own icon, and the
    /// recipe itself rides on the entry so what is dispatched is the recipe
    /// and not a copy of the row (§FS-005-dispatch.1).
    #[test]
    fn a_recipe_joins_the_menu_as_an_entry_that_carries_it() {
        let rebase = crate::work::recipe::shipped()
            .into_iter()
            .find(|recipe| recipe.id == "rebase")
            .expect("the shipped rebase recipe");
        let entry = agent_entry(&rebase);
        assert_eq!(entry.id, "rebase");
        assert_eq!(entry.icon, rebase.icon);
        assert_eq!(entry.description, rebase.description);
        // Nothing runs here: the entry hands work over.
        assert!(entry.command.is_empty());
        let carried = entry.agent.as_ref().expect("the recipe rides along");
        assert_eq!(carried.brief, rebase.brief);
        // Including the deterministic move it opens with, which is the half a
        // rebuilt copy would have lost (§FS-005-dispatch.12).
        assert_eq!(
            carried.opens_with.as_deref(),
            Some(crate::work::recipe::OPENING_REBASE)
        );
    }

    /// Work whose name the menu already carries is that entry's own work, not
    /// a second row saying the same thing (§FS-005-dispatch.1). Everything
    /// else is added.
    #[test]
    fn work_is_added_where_nothing_has_claimed_the_name() {
        let recipe = |id: &str| crate::work::recipe::Recipe {
            id: id.to_string(),
            icon: "◆".to_string(),
            description: format!("do {id}"),
            state: "fix".to_string(),
            when: Default::default(),
            needs_checkout: false,
            brief: "b".to_string(),
            opens_with: None,
            hand: None,
            target: None,
            model: None,
        };
        let mut menu = vec![named("rebase", "rebase onto master (5 behind)")];
        add_unclaimed(
            &mut menu,
            vec![
                agent_entry(&recipe("rebase")),
                agent_entry(&recipe("answer")),
            ],
        );
        let described: Vec<&str> = menu
            .iter()
            .map(|action| action.description.as_str())
            .collect();
        assert_eq!(described, ["rebase onto master (5 behind)", "do answer"]);
        // And the one that stayed is still the command, not the ticket.
        assert!(menu[0].agent.is_none());
        assert!(menu[1].agent.is_some());
    }

    /// An entry with no id is nobody's to override — two anonymous entries
    /// are two entries.
    #[test]
    fn anonymous_entries_never_collapse_into_one() {
        let merged = merge(vec![
            vec![action("first", &[])],
            vec![action("second", &[])],
        ]);
        assert_eq!(merged.len(), 2);
    }

    fn requiring(rungs: &[&str]) -> ActionConfig {
        ActionConfig {
            requires: rungs.iter().map(|rung| rung.to_string()).collect(),
            ..action("rebuild it", &[])
        }
    }

    /// What an entry says it needs is answered by the one table, and the
    /// entry stays visible with the reason (§AR-005-capabilities.2).
    #[test]
    fn an_entry_is_gated_by_the_rungs_it_named() {
        // Every rung this fixture holds: nothing to refuse.
        let mut held = menu(
            WorkspaceState::Ready,
            None,
            vec![requiring(&["placed", "checkable"])],
        );
        match held.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => assert!(matches!(entry.gate, Gate::Ready)),
            _ => panic!("expected Run"),
        }

        // A rung the project does not hold: refused in the ladder's own words.
        let pr = item(ItemKind::Pr, "github-prs:acme/widget#42", json!({}));
        let unplaced = crate::capabilities::CapabilitySet::unknown("widget");
        let menu = ActionMenu::new(
            Subject::Item(Box::new(pr)),
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            WorkspaceState::Ready,
            None,
            &unplaced,
            vec![requiring(&["ticketed"])],
        );
        match menu.gate(0) {
            Gate::Blocked(reason) => assert!(reason.contains("no registry row"), "{reason}"),
            _ => panic!("expected the ladder's refusal"),
        }
    }

    /// A menu about a branch is the same menu: it names the branch, carries
    /// what it was given, and still ends with the row the reader types into
    /// (§FS-004-quick-actions.6, §FS-005-dispatch.10).
    #[test]
    fn a_branch_row_opens_the_same_menu_under_its_own_name() {
        let mut menu = ActionMenu::new(
            Subject::Branch {
                project: "widget".to_string(),
                branch: "you/ABC-42".to_string(),
            },
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            WorkspaceState::Ready,
            None,
            &can_everything(),
            vec![rebase_action("master", 3)],
        );
        assert_eq!(menu.subject.title(), "you/ABC-42");
        assert_eq!(menu.subject.project(), "widget");
        // No matter behind it, so the summons is told about the project and
        // the branch rather than about an item nobody filed.
        assert!(menu.subject.item().is_none());
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => {
                assert_eq!(entry.action.description, "rebase onto master (3 behind)");
                assert!(matches!(entry.gate, Gate::Ready));
            }
            _ => panic!("expected the rebase to run"),
        }
        match menu.handle_key(KeyCode::Char('2')) {
            MenuOutcome::Run(entry) => assert!(entry.is_freehand),
            _ => panic!("expected the freehand row"),
        }
    }

    /// A requirement ephor does not recognize is said out loud: a rung nobody
    /// checks is worse than a rung nobody wrote.
    #[test]
    fn a_requirement_nobody_recognizes_is_named_rather_than_met() {
        let menu = menu(WorkspaceState::Ready, None, vec![requiring(&["magic"])]);
        match menu.gate(0) {
            Gate::Blocked(reason) => {
                assert!(reason.contains("'magic' is not a capability"), "{reason}");
                assert!(reason.contains("checkout-able"), "{reason}");
            }
            _ => panic!("expected it to be blocked"),
        }
    }

    /// The hand rides on the entry, so the reader sees who would get the work
    /// before pressing the key — and a choice that cannot stand refuses the
    /// entry with its whole reason rather than being dropped
    /// (§FS-005-dispatch.14, §FS-006-project-interface.9).
    #[test]
    fn an_agent_entry_says_who_would_get_it_and_a_refused_hand_blocks_it() {
        let with_hand = |says: &str, refusal: Option<&str>| ActionConfig {
            hand: Some(crate::feed::config::Handed {
                says: says.to_string(),
                refusal: refusal.map(str::to_string),
            }),
            ..agent_entry(&crate::work::recipe::shipped()[0])
        };
        let mut ready = menu(
            WorkspaceState::Ready,
            None,
            vec![with_hand("luna at high", None)],
        );
        assert!(
            ready.footer().contains("hand over to luna at high"),
            "{}",
            ready.footer()
        );
        match ready.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => {
                assert!(matches!(entry.gate, Gate::Ready));
                assert!(entry.action.agent.is_some());
            }
            _ => panic!("expected the work to be handed over"),
        }

        // Refused: the entry stays, says why, the footer stops teaching a key
        // that would do nothing here, and the key does nothing
        // (§FS-004-quick-actions.2).
        let refused = "this project permits only sonnet";
        let mut blocked = menu(
            WorkspaceState::Ready,
            None,
            vec![with_hand(refused, Some(refused))],
        );
        assert!(matches!(
            blocked.handle_key(KeyCode::Char('1')),
            MenuOutcome::Stay
        ));
        match blocked.gate(0) {
            Gate::Blocked(reason) => assert_eq!(reason, refused),
            _ => panic!("expected the refusal to block it"),
        }
        assert!(!blocked.footer().contains("enter"), "{}", blocked.footer());
    }

    /// The footer is built from the selected entry, because `Enter` does four
    /// different things in this menu (§FS-004-quick-actions.2).
    #[test]
    fn the_menu_footer_says_what_enter_would_do_here() {
        let target = PathBuf::from("/tmp/ws/branch");
        let mut menu = menu(
            WorkspaceState::Missing(target),
            Some(checkout_config()),
            vec![action("browser", &[])],
        );
        // Row 1 is the synthetic checkout, row 2 the command, row 3 the one
        // the reader types into.
        assert!(
            menu.footer().contains("enter check out"),
            "{}",
            menu.footer()
        );
        menu.handle_key(KeyCode::Char('j'));
        assert!(menu.footer().contains("enter run"), "{}", menu.footer());
        menu.handle_key(KeyCode::Char('j'));
        assert!(
            menu.footer().contains("enter type a command"),
            "{}",
            menu.footer()
        );
        // And an entry that asked to be confirmed says so where the reader is
        // about to press.
        let mut asked = ActionMenu::new(
            Subject::Item(Box::new(item(
                ItemKind::Pr,
                "github-prs:acme/widget#42",
                json!({}),
            ))),
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            WorkspaceState::Ready,
            None,
            &can_everything(),
            vec![ActionConfig {
                confirm: true,
                ..action("wipe the build", &[])
            }],
        );
        asked.handle_key(KeyCode::Char('1'));
        assert!(
            asked.footer().contains("enter again to confirm"),
            "{}",
            asked.footer()
        );
    }

    /// An entry that asked to be confirmed runs on the second choice, and the
    /// question does not follow the reader around
    /// (§FS-006-project-interface.9).
    #[test]
    fn an_entry_that_asked_to_be_confirmed_runs_on_the_second_choice() {
        let asking = ActionConfig {
            confirm: true,
            ..action("wipe the build", &[])
        };
        let mut asked = menu(
            WorkspaceState::Ready,
            None,
            vec![asking, action("harmless", &[])],
        );
        assert!(matches!(
            asked.handle_key(KeyCode::Char('1')),
            MenuOutcome::Stay
        ));
        match asked.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => assert_eq!(entry.action.description, "wipe the build"),
            _ => panic!("expected it to run on the second choice"),
        }

        // Asked, then moved away from: the question is dropped rather than
        // waiting to catch the next Enter.
        let asking = ActionConfig {
            confirm: true,
            ..action("wipe the build", &[])
        };
        let mut moved_away = menu(
            WorkspaceState::Ready,
            None,
            vec![asking, action("harmless", &[])],
        );
        assert!(matches!(
            moved_away.handle_key(KeyCode::Char('1')),
            MenuOutcome::Stay
        ));
        moved_away.handle_key(KeyCode::Char('j'));
        match moved_away.handle_key(KeyCode::Enter) {
            MenuOutcome::Run(entry) => assert_eq!(entry.action.description, "harmless"),
            _ => panic!("expected the entry moved to"),
        }
        // And an entry that asked nothing runs on the first choice.
        match moved_away.handle_key(KeyCode::Char('2')) {
            MenuOutcome::Run(entry) => assert_eq!(entry.action.description, "harmless"),
            _ => panic!("expected Run"),
        }
    }
}
