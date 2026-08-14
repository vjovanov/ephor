//! The action menu: what can be run on a feed item, from all three places it
//! can come from (§FS-006-project-interface.9).
//!
//! Provenance orders the menu — what ephor itself recognized
//! (§FS-004-quick-actions.3), then the project's offers, then the person's own
//! from `status.json` (`actions` globally, plus per-project
//! `projects.<id>.actions`) — and where two entries share an id, the later
//! provenance wins in place. Every entry is one shape, selected by the same
//! `when` language recipes use and gated by the same capability rungs. The
//! command runs via `sh -c` in the project's checkout with the item's context
//! exported as `EPHOR_*` environment variables.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use crate::capabilities::CapabilitySet;
use crate::feed::config::{ActionConfig, CheckoutConfig};
use crate::feed::model::Item;
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

/// The rebase ephor offers on a pull request whose checkout trails its main
/// branch (§FS-004-quick-actions.6). It runs `ephor rebase`, so the key and
/// the state machine's program state are the same operation
/// (§FS-005-dispatch.12), and it says how far behind the branch is because
/// that is the fact the reader is being asked to act on.
pub(crate) fn rebase_action(main_branch: &str, behind: u64) -> ActionConfig {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "ephor".to_string());
    ActionConfig {
        id: "rebase".to_string(),
        icon: "⤴".to_string(),
        description: format!("rebase onto {main_branch} ({behind} behind)"),
        // `--dispatch` is what makes a conflict work rather than a dead end:
        // where git stops, the ticket opens on the spot.
        command: format!(
            "{} rebase --project \"$EPHOR_PROJECT\" --checkout \"$EPHOR_WORKSPACE\" \
             --item \"$EPHOR_ITEM_ID\" --dispatch",
            crate::feed::providers::shell_quote(&exe)
        ),
        kinds: vec!["pr".to_string()],
        requires_checkout: true,
        ..ActionConfig::default()
    }
}

/// The checkout ephor offers on an item whose branch workspace is not on disk
/// (§FS-004-quick-actions.7). It runs `ephor checkout`, so the key and the
/// state machine's program state are the same operation (§FS-005-dispatch.12),
/// and it names the directory it is about to make because that is the thing
/// the reader is agreeing to.
pub(crate) fn checkout_action(target: &Path) -> ActionConfig {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "ephor".to_string());
    ActionConfig {
        id: "checkout".to_string(),
        icon: "⇣".to_string(),
        description: format!("check out {}", target.display()),
        command: format!(
            "{} checkout --project \"$EPHOR_PROJECT\" --item \"$EPHOR_ITEM_ID\"",
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
    pub item: Item,
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
        item: Item,
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
            item,
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
    fn choose(&mut self, index: usize) -> MenuOutcome {
        let Some(entry) = self.entries.get(index) else {
            return MenuOutcome::Stay;
        };
        if entry.action.confirm && self.confirming != Some(index) {
            self.confirming = Some(index);
            self.selected = index;
            return MenuOutcome::Stay;
        }
        self.confirming = None;
        MenuOutcome::Run(entry.clone())
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
        let title: String = self.item.title.chars().take(width as usize - 12).collect();
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
            pr,
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
    /// stands (§FS-004-quick-actions.2).
    #[test]
    fn gated_action_blocks_where_there_is_no_branch_to_check_out() {
        let mut menu = menu_unmatched(vec![requires_checkout("open ide")]);
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => assert!(matches!(entry.gate, Gate::Blocked(_))),
            _ => panic!("expected Run"),
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
        let mut menu = ActionMenu::new(
            pr,
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            WorkspaceState::Ready,
            None,
            &unplaced,
            vec![requiring(&["ticketed"])],
        );
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => match entry.gate {
                Gate::Blocked(reason) => assert!(reason.contains("no registry row"), "{reason}"),
                _ => panic!("expected the ladder's refusal"),
            },
            _ => panic!("expected Run"),
        }
    }

    /// A requirement ephor does not recognize is said out loud: a rung nobody
    /// checks is worse than a rung nobody wrote.
    #[test]
    fn a_requirement_nobody_recognizes_is_named_rather_than_met() {
        let mut menu = menu(WorkspaceState::Ready, None, vec![requiring(&["magic"])]);
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => match entry.gate {
                Gate::Blocked(reason) => {
                    assert!(reason.contains("'magic' is not a capability"), "{reason}");
                    assert!(reason.contains("checkout-able"), "{reason}");
                }
                _ => panic!("expected it to be blocked"),
            },
            _ => panic!("expected Run"),
        }
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
