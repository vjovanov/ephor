//! The action menu: user-configured commands summoned on a feed item.
//!
//! Actions come from `status.json` (`actions` globally, plus per-project
//! `projects.<id>.actions`), each with an icon, a description, and a shell
//! command. The command runs via `sh -c` in the project's checkout with the
//! item's context exported as `EPHOR_*` environment variables.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use crate::feed::config::{ActionConfig, CheckoutConfig};
use crate::feed::model::{Item, ItemKind};

use super::{highlight_style, BranchInfo, WorkspaceState};

/// Actions applicable to one item: global first, then the project's own,
/// filtered by the item's kind.
pub(crate) fn applicable(
    global: &[ActionConfig],
    project: &[ActionConfig],
    item: &Item,
) -> Vec<ActionConfig> {
    global
        .iter()
        .chain(project)
        .filter(|action| {
            action.kinds.is_empty()
                || action
                    .kinds
                    .iter()
                    .any(|kind| kind_matches(item.kind, kind))
        })
        .cloned()
        .collect()
}

fn kind_matches(kind: ItemKind, name: &str) -> bool {
    // The Message label is "msg"; accept the config-friendly spelling too.
    name == kind.label() || (kind == ItemKind::Message && name == "message")
}

/// The item's context, exported to action commands. `branch` is the
/// registry branch the item was matched to (org → project → branch, the
/// same grouping the tree shows); `workspace` is the resolved checkout the
/// command runs in.
pub(crate) fn item_env(
    item: &Item,
    root: &Path,
    workspace: &Path,
    branch: Option<&BranchInfo>,
) -> Vec<(String, String)> {
    let string = |value: &Option<String>| value.clone().unwrap_or_default();
    // The provider-recorded branch is ground truth; the matched registry
    // branch fills in for providers that don't record one.
    let branch_name = item
        .raw
        .get("branch")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .or_else(|| branch.map(|branch| branch.branch.clone()))
        .unwrap_or_default();
    vec![
        ("EPHOR_PROJECT".to_string(), item.project.clone()),
        (
            "EPHOR_ROOT".to_string(),
            root.to_string_lossy().into_owned(),
        ),
        (
            "EPHOR_WORKSPACE".to_string(),
            workspace.to_string_lossy().into_owned(),
        ),
        ("EPHOR_ITEM_ID".to_string(), item.id.clone()),
        ("EPHOR_SOURCE".to_string(), item.source.clone()),
        ("EPHOR_KIND".to_string(), item.kind.label().to_string()),
        ("EPHOR_TITLE".to_string(), item.title.clone()),
        ("EPHOR_URL".to_string(), string(&item.url)),
        ("EPHOR_STATE".to_string(), string(&item.state)),
        ("EPHOR_BRANCH".to_string(), branch_name),
        (
            "EPHOR_TICKET".to_string(),
            branch
                .and_then(|branch| branch.ticket.clone())
                .unwrap_or_default(),
        ),
        ("EPHOR_REPO".to_string(), item.repo().unwrap_or_default()),
        (
            "EPHOR_NUMBER".to_string(),
            item.number().unwrap_or_default(),
        ),
        ("EPHOR_RAW".to_string(), item.raw.to_string()),
    ]
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
}

impl ActionMenu {
    pub fn new(
        item: Item,
        root: PathBuf,
        workspace: PathBuf,
        branch: Option<BranchInfo>,
        state: WorkspaceState,
        checkout: Option<CheckoutConfig>,
        actions: Vec<ActionConfig>,
    ) -> Self {
        let mut entries = Vec::new();
        // A missing workspace with a configured checkout is directly
        // runnable as its own entry.
        if let (WorkspaceState::Missing(_), Some(checkout)) = (&state, &checkout) {
            entries.push(MenuEntry {
                action: ActionConfig {
                    icon: checkout.icon.clone(),
                    description: checkout.description.clone(),
                    command: checkout.command.clone(),
                    kinds: Vec::new(),
                    requires_checkout: false,
                },
                is_checkout: true,
                gate: Gate::NeedsCheckout,
            });
        }
        for action in actions {
            let gate = if !action.requires_checkout {
                Gate::Ready
            } else {
                match &state {
                    WorkspaceState::Ready => Gate::Ready,
                    WorkspaceState::Missing(_) if checkout.is_some() => Gate::NeedsCheckout,
                    WorkspaceState::Missing(_) => Gate::Blocked(
                        "Branch is not checked out and no 'checkout' command is configured"
                            .to_string(),
                    ),
                    WorkspaceState::Unmatched => Gate::Blocked(
                        "Action needs a branch workspace, but the item's branch is unknown"
                            .to_string(),
                    ),
                }
            };
            entries.push(MenuEntry {
                action,
                is_checkout: false,
                gate,
            });
        }
        ActionMenu {
            item,
            root,
            workspace,
            branch,
            state,
            checkout,
            entries,
            selected: 0,
        }
    }

    /// The workspace directory a checkout must create.
    pub fn checkout_target(&self) -> Option<PathBuf> {
        match &self.state {
            WorkspaceState::Missing(target) => Some(target.clone()),
            _ => None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> MenuOutcome {
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('x') => MenuOutcome::Close,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                }
                MenuOutcome::Stay
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                MenuOutcome::Stay
            }
            KeyCode::Enter => MenuOutcome::Run(self.entries[self.selected].clone()),
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                let index = (digit as usize).wrapping_sub('1' as usize);
                match self.entries.get(index) {
                    Some(entry) => MenuOutcome::Run(entry.clone()),
                    None => MenuOutcome::Stay,
                }
            }
            _ => MenuOutcome::Stay,
        }
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
                match &entry.gate {
                    Gate::NeedsCheckout if !entry.is_checkout => spans.push(Span::styled(
                        "  (will check out first)",
                        Style::default().fg(Color::Yellow),
                    )),
                    Gate::Blocked(_) => spans.push(Span::styled(
                        "  (unavailable)",
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
    use chrono::Utc;
    use serde_json::json;

    fn action(description: &str, kinds: &[&str]) -> ActionConfig {
        ActionConfig {
            icon: "⚙".to_string(),
            description: description.to_string(),
            command: "true".to_string(),
            kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
            requires_checkout: false,
        }
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
        let names: Vec<String> = applicable(&global, &project, &pr)
            .into_iter()
            .map(|action| action.description)
            .collect();
        assert_eq!(names, ["everywhere", "prs only"]);

        let ci = item(ItemKind::Ci, "github-ci:acme/widget#42", json!({}));
        let names: Vec<String> = applicable(&global, &project, &ci)
            .into_iter()
            .map(|action| action.description)
            .collect();
        assert_eq!(names, ["everywhere", "project ci"]);
    }

    #[test]
    fn message_kind_accepts_both_spellings() {
        let message = item(ItemKind::Message, "slack:123", json!({}));
        assert_eq!(
            applicable(&[action("a", &["message"])], &[], &message).len(),
            1
        );
        assert_eq!(applicable(&[action("a", &["msg"])], &[], &message).len(), 1);
        assert_eq!(applicable(&[action("a", &["pr"])], &[], &message).len(), 0);
    }

    fn env_value(env: &[(String, String)], key: &str) -> String {
        env.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap()
    }

    #[test]
    fn env_extracts_github_number_and_repo() {
        let pr = item(ItemKind::Pr, "github-prs:acme/widget#42", json!({}));
        let env = item_env(
            &pr,
            Path::new("/tmp/widget"),
            Path::new("/tmp/widget/master"),
            None,
        );
        assert_eq!(env_value(&env, "EPHOR_NUMBER"), "42");
        assert_eq!(env_value(&env, "EPHOR_REPO"), "acme/widget");
        assert_eq!(env_value(&env, "EPHOR_ROOT"), "/tmp/widget");
        assert_eq!(env_value(&env, "EPHOR_WORKSPACE"), "/tmp/widget/master");
        assert_eq!(env_value(&env, "EPHOR_KIND"), "pr");
    }

    #[test]
    fn env_extracts_bitbucket_number_and_repo() {
        let pr = item(
            ItemKind::Pr,
            "bitbucket-prs:plugins/123",
            json!({ "repo": "plugins", "branch": "you/ABC-7-fix" }),
        );
        let env = item_env(
            &pr,
            Path::new("/tmp/widget"),
            Path::new("/tmp/widget"),
            None,
        );
        assert_eq!(env_value(&env, "EPHOR_NUMBER"), "123");
        assert_eq!(env_value(&env, "EPHOR_REPO"), "plugins");
        assert_eq!(env_value(&env, "EPHOR_BRANCH"), "you/ABC-7-fix");
    }

    #[test]
    fn env_fills_branch_and_ticket_from_matched_registry_branch() {
        let branch = BranchInfo {
            branch: "you/ABC-42-retry-window".to_string(),
            ticket: Some("ABC-42".to_string()),
            active: true,
            is_release: false,
        };
        // A github item records no branch: the registry match fills in.
        let pr = item(ItemKind::Pr, "github-prs:acme/widget#42", json!({}));
        let env = item_env(&pr, Path::new("/r"), Path::new("/r/b"), Some(&branch));
        assert_eq!(env_value(&env, "EPHOR_BRANCH"), "you/ABC-42-retry-window");
        assert_eq!(env_value(&env, "EPHOR_TICKET"), "ABC-42");

        // A provider-recorded branch wins over the registry match.
        let pr = item(
            ItemKind::Pr,
            "bitbucket-prs:app/123",
            json!({ "branch": "other" }),
        );
        let env = item_env(&pr, Path::new("/r"), Path::new("/r/b"), Some(&branch));
        assert_eq!(env_value(&env, "EPHOR_BRANCH"), "other");
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
            actions,
        )
    }

    fn checkout_config() -> CheckoutConfig {
        serde_json::from_value(json!({ "command": "gco \"$EPHOR_BRANCH\"" })).unwrap()
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
        assert_eq!(menu.checkout_target(), Some(target));

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

    #[test]
    fn gated_action_blocks_without_checkout_command() {
        let mut menu = menu(
            WorkspaceState::Missing(PathBuf::from("/tmp/ws/branch")),
            None,
            vec![requires_checkout("open ide")],
        );
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => assert!(matches!(entry.gate, Gate::Blocked(_))),
            _ => panic!("expected Run"),
        }

        let mut menu = menu_unmatched(vec![requires_checkout("open ide")]);
        match menu.handle_key(KeyCode::Char('1')) {
            MenuOutcome::Run(entry) => assert!(matches!(entry.gate, Gate::Blocked(_))),
            _ => panic!("expected Run"),
        }
    }

    fn menu_unmatched(actions: Vec<ActionConfig>) -> ActionMenu {
        menu(WorkspaceState::Unmatched, Some(checkout_config()), actions)
    }
}
