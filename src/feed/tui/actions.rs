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

use std::path::PathBuf;

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use crate::api::offers;
pub(crate) use crate::api::offers::{Gate, MenuEntry, Subject};
use crate::capabilities::CapabilitySet;
use crate::feed::config::{ActionConfig, CheckoutConfig};
use crate::work::recipe::HandPin;
use crate::work::runtime::roster::Hand;

use super::{highlight_style, BranchInfo, WorkspaceState};

pub(crate) enum MenuOutcome {
    Stay,
    Close,
    Run(MenuEntry),
    /// The key on a row that says *running* goes to the thing that is running
    /// and never starts a second one (§FS-005-dispatch.21).
    Open(MenuEntry),
}

/// The reader's own pick, made over one entry at the moment of asking
/// (§FS-005-dispatch.14): two columns, not three — the hands, and the efforts
/// the selected hand declares, the second absent where it declares none,
/// which is every hand on a machine with no model profiles. It holds indices
/// into the menu's roster; the hands themselves stay on the menu.
///
/// The choice it produces is always one the resolution can stand: a hand
/// that declares efforts is picked at the highlighted one, so nothing here
/// can assemble the effort-less ask of a several-effort hand that the
/// resolution refuses.
struct HandPicker {
    selected: usize,
    /// The reader is in the efforts column.
    on_efforts: bool,
    effort: usize,
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
    pub state: WorkspaceState,
    pub checkout: Option<CheckoutConfig>,
    entries: Vec<MenuEntry>,
    selected: usize,
    /// An entry that asked to be confirmed and has been chosen once
    /// (§FS-006-project-interface.9): the next Enter on it runs it.
    confirming: Option<usize>,
    /// The hands `t` may offer on this menu's agent entries
    /// (§FS-005-dispatch.14): the roster's, already without what the
    /// project's narrowing excludes. Empty where there is nobody to pick
    /// from, which is what withholds the picker entirely — the entry still
    /// dispatches as if nothing had been picked.
    roster: Vec<Hand>,
    /// The picker, open over the selected entry — the menu's second level,
    /// like `confirming`.
    picker: Option<HandPicker>,
}

impl ActionMenu {
    /// The menu over a list the session already assembled and gated
    /// (§AR-009-surfaces.1). This file draws that list and translates keys on
    /// it; what is in it, and whether each row can run, is not decided here.
    pub fn over(
        subject: Subject,
        root: PathBuf,
        workspace: PathBuf,
        branch: Option<BranchInfo>,
        state: WorkspaceState,
        checkout: Option<CheckoutConfig>,
        entries: Vec<MenuEntry>,
    ) -> Self {
        // What is already going stands first (§FS-005-dispatch.21). A stable
        // partition, so provenance still orders everything within each half
        // (§FS-006-project-interface.9) — and the numbers are the rows', so
        // the digit that picks a row goes on picking the row it is beside.
        let mut entries = entries;
        entries.sort_by_key(|entry| entry.running.is_none());
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
            roster: Vec::new(),
            picker: None,
        }
    }

    /// A menu built from a raw list, for the tests that exercise gating
    /// through the drawing. It goes through the same assembly the session
    /// uses (§AR-009-surfaces.1) — a fixture that gated its own entries would
    /// be testing a second implementation.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
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
        let entries = offers::entries(&state, &checkout, can, actions, false, &mut offers::Unnamed);
        ActionMenu::over(subject, root, workspace, branch, state, checkout, entries)
    }

    /// The same menu over a different list — how the workflows row replaces
    /// the menu with the runtime's own offers (§FS-005-dispatch.19). The
    /// gating is the session's, as it is for every other list, and so is the
    /// naming the gating reads: this list is about the same matter, and an
    /// entry offered here has to be offered in the shape the menu behind it
    /// offered it (§REQ-002-parity.3).
    pub fn rebuilt(
        &self,
        actions: Vec<ActionConfig>,
        can: &CapabilitySet,
        naming: &mut dyn offers::Naming,
    ) -> ActionMenu {
        ActionMenu::over(
            self.subject.clone(),
            self.root.clone(),
            self.workspace.clone(),
            self.branch.clone(),
            self.state.clone(),
            self.checkout.clone(),
            offers::entries(&self.state, &self.checkout, can, actions, false, naming),
        )
    }

    /// The entry about a workspace that is not there, where this subject has
    /// one (§FS-004-quick-actions.7.2). What the key on the row runs, so that
    /// the key and the row in this menu are the same move rather than two
    /// spellings of it (§AR-009-surfaces.1).
    pub fn checkout_entry(&self) -> Option<&MenuEntry> {
        self.entries.iter().find(|entry| entry.is_checkout)
    }

    /// The hands `t` may offer here (§FS-005-dispatch.14). Separate from the
    /// constructor because most menus — a branch row's, a project with no
    /// recipes — carry no agent entry to pick for and need no roster read.
    pub fn with_roster(mut self, roster: Vec<Hand>) -> Self {
        self.roster = roster;
        self
    }

    /// Built from the selected entry, not from the menu
    /// (§FS-004-quick-actions.2): `Enter` does four different things here —
    /// run a command, check a workspace out, hand work over, ask for a command
    /// to run — and on an entry that cannot act it does none of them. A footer
    /// that said "run" over all of that would teach the key and leave the
    /// reader to find out what it meant from a line at the bottom of a screen
    /// they were not reading.
    pub fn footer(&self) -> String {
        // The picker's own keys, built from its selection the same way
        // (§FS-004-quick-actions.2): the column key appears only where there
        // is a column to enter, and Enter is not taught on a hand that
        // cannot be chosen.
        if let Some(picker) = &self.picker {
            let hand = &self.roster[picker.selected];
            let mut keys = String::from(" j/k move");
            if hand.available.is_none() && !hand.efforts.is_empty() {
                keys.push_str("  ←/→ column");
            }
            if hand.available.is_none() {
                let at = hand
                    .efforts
                    .get(picker.effort)
                    .map(|effort| format!(" at {effort}"))
                    .unwrap_or_default();
                keys.push_str(&format!("  enter hand over to {}{at}", hand.id));
            }
            keys.push_str("  esc back");
            return keys;
        }
        let mut keys = String::from(" j/k move");
        if self.entries.len() > 1 {
            keys.push_str("  1-9 pick");
        }
        if let Some(entry) = self.entries.get(self.selected) {
            let verb = match &entry.gate {
                // Built from the row and not from the key
                // (§FS-004-quick-actions.2): on a row that says *running* the
                // key opens what is going, so the footer says *open*
                // (§FS-005-dispatch.21). Ahead of the blocked arm, because a
                // running row's way in does not depend on the capability its
                // *start* needed — and the footer, Enter and `l` have to
                // answer such a row the same way (§FS-004-quick-actions.2).
                _ if entry.running.is_some() => Some(match &entry.running {
                    Some(running) => match running.way_in() {
                        Some(way) => format!("enter open {way}"),
                        None => format!("enter open the {}", running.name()),
                    },
                    None => "enter open".to_string(),
                }),
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
            // The pick for this dispatch alone (§FS-005-dispatch.14), taught
            // only where there is work to hand over and somebody to pick —
            // with an empty roster the picker is not offered at all.
            if self.picker_offered(entry) {
                keys.push_str("  t pick the hand");
            }
        }
        keys.push_str("  esc cancel");
        keys
    }

    /// Whether `t` has anything to open on this entry: work to hand over,
    /// not refused, nothing already going about it, and a roster with somebody
    /// on it (§FS-005-dispatch.14).
    ///
    /// Not on a row that says *running* (§FS-005-dispatch.21). The picker's
    /// whole purpose is to start a dispatch with a hand pinned to it, so
    /// offering it there would teach — in the footer, no less — the second copy
    /// that pressing such a row must never make. Where somebody does mean it
    /// the command line starts it and the refusal is the lock's own sentence.
    fn picker_offered(&self, entry: &MenuEntry) -> bool {
        entry.action.agent.is_some()
            && entry.running.is_none()
            && !matches!(entry.gate, Gate::Blocked(_))
            && !self.roster.is_empty()
    }

    pub fn handle_key(&mut self, code: KeyCode) -> MenuOutcome {
        // The picker is the menu's second level: while it is open, the keys
        // are its (§FS-005-dispatch.14).
        if self.picker.is_some() {
            return self.pick_key(code);
        }
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
            // `l` goes *in*, which on a row that says *running* is the thing
            // that is running (§FS-005-dispatch.21). It starts nothing: on
            // every other row there is nothing to go into.
            KeyCode::Char('l') => match self.entries.get(self.selected) {
                Some(entry) if entry.running.is_some() => MenuOutcome::Open(entry.clone()),
                _ => MenuOutcome::Stay,
            },
            // `t`: the pick for this dispatch alone (§FS-005-dispatch.14).
            // Only over an entry that hands work over, and only where there
            // is somebody to pick — with an empty roster the picker is not
            // offered and the entry dispatches as if nothing had been picked.
            KeyCode::Char('t') => {
                let offered = self
                    .entries
                    .get(self.selected)
                    .is_some_and(|entry| self.picker_offered(entry));
                if offered {
                    self.confirming = None;
                    self.picker = Some(HandPicker {
                        selected: 0,
                        on_efforts: false,
                        effort: 0,
                    });
                }
                MenuOutcome::Stay
            }
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

    /// One key of the open picker. Arrows move between the columns, `j`/`k`
    /// within one, Enter runs the entry with what is selected, Esc returns
    /// to the menu (§FS-005-dispatch.14).
    fn pick_key(&mut self, code: KeyCode) -> MenuOutcome {
        let Some(picker) = &mut self.picker else {
            return MenuOutcome::Stay;
        };
        let hand = &self.roster[picker.selected];
        match code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Char('j') | KeyCode::Down => {
                if picker.on_efforts {
                    if picker.effort + 1 < hand.efforts.len() {
                        picker.effort += 1;
                    }
                } else if picker.selected + 1 < self.roster.len() {
                    picker.selected += 1;
                    picker.effort = 0;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if picker.on_efforts {
                    picker.effort = picker.effort.saturating_sub(1);
                } else if picker.selected > 0 {
                    picker.selected -= 1;
                    picker.effort = 0;
                }
            }
            // Into the efforts where the selected hand declares any and can
            // be asked at all; a hand declaring none has no second column —
            // it is asked plainly, and a dead column would teach an axis
            // that is not there (§FS-005-dispatch.14).
            KeyCode::Right => {
                if hand.available.is_none() && !hand.efforts.is_empty() {
                    picker.on_efforts = true;
                }
            }
            KeyCode::Left => picker.on_efforts = false,
            KeyCode::Enter => {
                // An unavailable hand is shown with its reason and cannot be
                // chosen — the refusal was computed when the roster was read,
                // not discovered on the dispatch (§AR-002-summons.4).
                if hand.available.is_some() {
                    return MenuOutcome::Stay;
                }
                // A hand with efforts is picked at the highlighted one, so
                // this can never assemble the effort-less ask the resolution
                // refuses (§FS-005-dispatch.14).
                let pin = HandPin::Named {
                    id: hand.id.clone(),
                    effort: hand.efforts.get(picker.effort).cloned(),
                };
                let mut entry = self.entries[self.selected].clone();
                self.picker = None;
                // A row that says *running* opens what is going, whatever was
                // picked on the way here (§FS-005-dispatch.21). The picker is
                // not offered on such a row at all; this is the second line of
                // defence, so that no path through this screen can start a
                // second copy of work the row said was already going.
                if entry.running.is_some() {
                    self.confirming = None;
                    return MenuOutcome::Open(entry);
                }
                entry.picked = Some(pin);
                // Opening the picker and choosing in it is already the
                // deliberate second step a `confirm` entry asks for.
                self.confirming = None;
                return MenuOutcome::Run(entry);
            }
            _ => {}
        }
        MenuOutcome::Stay
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
        // A row that says *running* opens what is going; it never starts it
        // again (§FS-005-dispatch.21). A second copy is not what a reader
        // pressing such a row meant, and where somebody does mean it the
        // command line starts it and the refusal is the lock's own sentence.
        //
        // Asked before the gate, because opening what is going asks nothing of
        // the capability the *start* needed: a row the footer says *open* on
        // has to open on Enter as it does on `l` (§FS-004-quick-actions.2).
        if entry.running.is_some() {
            self.confirming = None;
            self.selected = index;
            return MenuOutcome::Open(entry.clone());
        }
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
        // The line that says what the rows above the rest are
        // (§FS-005-dispatch.21). It is not a row: the cursor never lands on it
        // and no number picks it.
        let heading = self.entries.iter().any(|entry| entry.running.is_some()) as u16;
        let width = area.width.saturating_sub(4).min(72).max(20);
        let height = (self.entries.len() as u16 + heading + 2).min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, rect);

        let mut rows: Vec<ListItem> = Vec::new();
        if heading == 1 {
            rows.push(ListItem::new(Line::from(Span::styled(
                "  running",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ))));
        }
        rows.extend(self.entries.iter().enumerate().map(|(index, entry)| {
            let number = if index < 9 {
                format!(" {} ", index + 1)
            } else {
                "   ".to_string()
            };
            // One colour reserved for what is going and used for nothing
            // else on this screen (§FS-005-dispatch.21) — and a step
            // further in, so the running rows are set apart as a group.
            let going = Style::default().fg(Color::Cyan);
            let mut spans = vec![
                Span::styled(number, Style::default().fg(Color::DarkGray)),
                match &entry.running {
                    Some(_) => Span::styled(
                        format!("  ▶ {}  {}", entry.action.icon, entry.action.description),
                        going,
                    ),
                    None => Span::raw(format!(
                        "{}  {}",
                        entry.action.icon, entry.action.description
                    )),
                },
            ];
            // How long it has been going and what it is at right now, in
            // the words the board already uses (§FS-005-dispatch.21).
            if let Some(running) = &entry.running {
                let since = running
                    .since()
                    .map(|seconds| format!("{} · ", crate::feed::render::span(seconds)))
                    .unwrap_or_default();
                spans.push(Span::styled(format!("   {since}{}", running.says()), going));
            }
            // Who would get this work, before the key is pressed
            // (§FS-005-dispatch.14). Dim, because cyan means running and
            // nothing else here. Where the choice was refused the row
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
                    Style::default().fg(Color::DarkGray),
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
        }));
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
        // Past the heading, which is a line and not a row.
        state.select(Some(self.selected + heading as usize));
        frame.render_stateful_widget(list, rect, &mut state);
        if let Some(picker) = &self.picker {
            self.draw_picker(frame, area, picker);
        }
    }

    /// The picker over the menu (§FS-005-dispatch.14): the hands, and beside
    /// a hand that declares efforts, those efforts — the second column drawn
    /// only where there is one, because on a machine with no model profiles
    /// no hand declares any and a dead column would teach an axis that is
    /// not there.
    fn draw_picker(&self, frame: &mut ratatui::Frame, area: Rect, picker: &HandPicker) {
        let width = area.width.saturating_sub(4).min(72).max(20);
        let height = (self.roster.len() as u16 + 2).min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" who does it — this dispatch alone ");
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let chosen = &self.roster[picker.selected];
        // A column only where the selected hand has one to enter.
        let efforts_width = match chosen.available.is_none() && !chosen.efforts.is_empty() {
            true => chosen
                .efforts
                .iter()
                .map(|effort| effort.chars().count() as u16 + 3)
                .max()
                .unwrap_or(0)
                .min(inner.width / 2),
            false => 0,
        };
        let hands_rect = Rect {
            width: inner.width.saturating_sub(efforts_width),
            ..inner
        };

        let dim = Style::default().fg(Color::DarkGray);
        let rows: Vec<ListItem> = self
            .roster
            .iter()
            .map(|hand| {
                let mut spans = vec![
                    Span::raw(format!(" {}", hand.id)),
                    Span::styled(format!("  {}", hand.resolves_to()), dim),
                ];
                // The reason is on the row, never saved for whoever presses
                // it (§AR-002-summons.4).
                if let Some(why) = &hand.available {
                    spans.push(Span::styled(
                        format!("  (unavailable: {why})"),
                        dim.add_modifier(Modifier::ITALIC),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        // The active column carries the highlight; the other keeps a dim one,
        // so where the next `j` lands is readable before it is pressed.
        let hands = List::new(rows).highlight_style(match picker.on_efforts {
            true => dim.add_modifier(Modifier::BOLD),
            false => highlight_style(),
        });
        let mut state = ListState::default();
        state.select(Some(picker.selected));
        frame.render_stateful_widget(hands, hands_rect, &mut state);

        if efforts_width > 0 {
            let efforts_rect = Rect {
                x: inner.x + hands_rect.width,
                width: efforts_width,
                ..inner
            };
            let rows: Vec<ListItem> = chosen
                .efforts
                .iter()
                .map(|effort| ListItem::new(format!(" {effort}")))
                .collect();
            let efforts = List::new(rows).highlight_style(match picker.on_efforts {
                true => highlight_style(),
                false => dim.add_modifier(Modifier::BOLD),
            });
            let mut state = ListState::default();
            state.select(Some(picker.effort));
            frame.render_stateful_widget(efforts, efforts_rect, &mut state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The assembly these exercise now lives below the screen
    // (§AR-009-surfaces.1); the menu they build it into is still here.
    use crate::api::offers::{add_unclaimed, agent_entry, applicable, merge, rebase_action};
    use crate::feed::model::{Item, ItemKind};
    use crate::forest::Trail;
    use crate::work::recipe::Facts;
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
        let entries = offers::entries(
            &state,
            &checkout,
            &can_everything(),
            actions,
            false,
            &mut offers::Unnamed,
        );
        ActionMenu::over(
            Subject::Item(Box::new(pr)),
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            state,
            checkout,
            entries,
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

    /// What is already going stands first, is opened rather than started
    /// again, and says so on the footer (§FS-005-dispatch.21). The key on such
    /// a row goes to the thing that is running; a second copy is not what a
    /// reader pressing a row that says *running* meant.
    #[test]
    fn a_running_entry_stands_first_and_opens_instead_of_running() {
        let actions = vec![action("first", &[]), action("second", &[])];
        let entries = offers::entries(
            &WorkspaceState::Ready,
            &None,
            &can_everything(),
            actions,
            false,
            &mut offers::Unnamed,
        );
        // The second entry has a job going about it.
        let mut entries = entries;
        let at = entries
            .iter()
            .position(|entry| entry.action.description == "second")
            .expect("the entry is there");
        entries[at].running = Some(offers::Running::Job {
            id: "20260822T090000.000Z-second".to_string(),
            since: Some(90),
            says: "still replaying".to_string(),
            log: PathBuf::from("/state/ephor/jobs/j/log"),
        });
        let pr = item(ItemKind::Pr, "github-prs:acme/widget#42", json!({}));
        let mut menu = ActionMenu::over(
            Subject::Item(Box::new(pr)),
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            WorkspaceState::Ready,
            None,
            entries,
        );
        // It leads, whatever provenance put it where it was.
        assert_eq!(menu.entries[0].action.description, "second");
        assert!(menu.entries[0].running.is_some());
        assert!(menu.entries[1..]
            .iter()
            .all(|entry| entry.running.is_none()));

        // The footer is built from the row, so it says *open* and names the
        // way in (§FS-004-quick-actions.2).
        let footer = menu.footer();
        assert!(
            footer.contains("enter open /state/ephor/jobs/j/log"),
            "{footer}"
        );

        // Enter opens it; it never runs it again.
        match menu.handle_key(KeyCode::Enter) {
            MenuOutcome::Open(entry) => {
                assert_eq!(entry.action.description, "second");
            }
            _ => panic!("expected Open"),
        }
        // And so does going *in* on it.
        match menu.handle_key(KeyCode::Char('l')) {
            MenuOutcome::Open(entry) => assert_eq!(entry.action.description, "second"),
            _ => panic!("expected Open"),
        }
        // On a row with nothing going, `l` is not a key at all.
        menu.handle_key(KeyCode::Char('j'));
        assert!(matches!(
            menu.handle_key(KeyCode::Char('l')),
            MenuOutcome::Stay
        ));
        let footer = menu.footer();
        assert!(footer.contains("enter run"), "{footer}");
    }

    /// A row that says *running* names the way in on every flavour of running
    /// there is (§FS-011-command-line.8): a job's log, a run's attach command,
    /// a window's handle.
    #[test]
    fn every_flavour_of_running_names_the_way_in() {
        let job = offers::Running::Job {
            id: "j".to_string(),
            since: Some(30),
            says: "fetching".to_string(),
            log: PathBuf::from("/state/ephor/jobs/j/log"),
        };
        assert_eq!(job.name(), "job");
        assert_eq!(job.says(), "fetching");
        assert_eq!(job.way_in().as_deref(), Some("/state/ephor/jobs/j/log"));

        let run = offers::Running::Run {
            root: PathBuf::from("/w/demo/panta"),
            id: Some("3f9a2c".to_string()),
            control_url: Some("http://127.0.0.1:54321".to_string()),
            attach: Some("the-runner attach '3f9a2c'".to_string()),
            since: Some(600),
            doing: "widget-42.fix-gate-1 [fix]".to_string(),
        };
        assert_eq!(run.name(), "run");
        assert_eq!(run.says(), "widget-42.fix-gate-1 [fix]");
        assert_eq!(run.way_in().as_deref(), Some("the-runner attach '3f9a2c'"));

        let queued = offers::Running::Queued {
            root: PathBuf::from("/w/demo/panta"),
            id: Some("3f9a2c".to_string()),
            attach: Some("the-runner attach '3f9a2c'".to_string()),
            since: None,
        };
        assert_eq!(queued.name(), "queued");
        assert_eq!(queued.says(), "queued");
        assert_eq!(
            queued.way_in().as_deref(),
            Some("the-runner attach '3f9a2c'")
        );

        let window = offers::Running::Window {
            job: "j".to_string(),
            handle: "@7".to_string(),
            since: Some(5),
            says: String::new(),
        };
        assert_eq!(window.name(), "window");
        assert_eq!(window.says(), "running in window @7");
        assert_eq!(window.way_in().as_deref(), Some("@7"));

        // A window the opener never named is a window all the same, and has no
        // way in: nothing ephor holds could bring it forward
        // (§AR-002-summons.6).
        let unnamed = offers::Running::Window {
            job: "j".to_string(),
            handle: String::new(),
            since: Some(5),
            says: String::new(),
        };
        assert_eq!(
            unnamed.says(),
            "running in a window the opener did not name"
        );
        assert_eq!(unnamed.way_in(), None);

        // A parked ticket says §15's word, and its way in is the run still
        // standing at the gate — the plan where none is
        // (§FS-005-dispatch.9, §FS-005-dispatch.20).
        let waiting = |id: Option<&str>| offers::Running::Waiting {
            root: PathBuf::from("/w/demo/panta"),
            ticket: "widget-42.answer-2".to_string(),
            state: "needs-human".to_string(),
            plan: PathBuf::from("/w/demo/panta/widget-42.md"),
            id: id.map(str::to_string),
            attach: id.map(|id| format!("the-runner attach '{id}'")),
            since: Some(120),
        };
        let parked = waiting(Some("3f9a2c"));
        assert_eq!(parked.name(), "waiting");
        assert_eq!(
            parked.says(),
            "widget-42.answer-2 [needs-human] · waiting on you"
        );
        assert_eq!(
            parked.way_in().as_deref(),
            Some("the-runner attach '3f9a2c'")
        );
        assert_eq!(
            waiting(None).way_in().as_deref(),
            Some("/w/demo/panta/widget-42.md")
        );
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
        assert_eq!(
            offers::checkout_step(&menu.state, &menu.checkout).map(|(_, target)| target),
            Some(target)
        );

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
        assert_eq!(
            offers::checkout_step(&menu.state, &menu.checkout).map(|(_, path)| path),
            Some(target)
        );
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
            branch: None,
            autorun: false,
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
        let menu = ActionMenu::over(
            Subject::Item(Box::new(pr)),
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            None,
            WorkspaceState::Ready,
            None,
            offers::entries(
                &WorkspaceState::Ready,
                &None,
                &unplaced,
                // Spelled the old way on purpose: an older `requires` still
                // resolves to the *tasks* rung (§FS-006-project-interface.10).
                vec![requiring(&["ticketed"])],
                false,
                &mut offers::Unnamed,
            ),
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
            vec![rebase_action(
                "master",
                Trail {
                    behind: 3,
                    seen: None,
                },
            )],
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

    fn roster_hand(id: &str, efforts: &[&str], available: Option<&str>) -> Hand {
        Hand {
            id: id.to_string(),
            agent: Some("agent-x".to_string()),
            model: Some("m-x".to_string()),
            provider: None,
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            available: available.map(str::to_string),
        }
    }

    /// A menu holding one agent entry and a roster for `t` to offer.
    fn menu_with_roster(hands: Vec<Hand>) -> ActionMenu {
        menu(
            WorkspaceState::Ready,
            None,
            vec![agent_entry(&crate::work::recipe::shipped()[0])],
        )
        .with_roster(hands)
    }

    fn full_roster() -> Vec<Hand> {
        vec![
            roster_hand("luna", &["high", "yolo"], None),
            roster_hand("pi-alone", &[], None),
            roster_hand("away", &[], Some("nowhere is not on PATH")),
        ]
    }

    /// `t` on an entry that hands work over opens the picker, and Enter runs
    /// the entry with what is selected (§FS-005-dispatch.14). A hand that
    /// declares efforts is picked at the highlighted one — never the
    /// effort-less ask of a several-effort hand that resolution refuses —
    /// and a hand declaring none is asked plainly, with no column to enter.
    #[test]
    fn t_opens_the_picker_and_enter_runs_with_what_is_selected() {
        let mut menu = menu_with_roster(full_roster());
        assert!(
            menu.footer().contains("t pick the hand"),
            "{}",
            menu.footer()
        );
        assert!(matches!(
            menu.handle_key(KeyCode::Char('t')),
            MenuOutcome::Stay
        ));
        // The picker's footer teaches its own keys, with the choice named.
        assert!(
            menu.footer().contains("enter hand over to luna at high"),
            "{}",
            menu.footer()
        );
        match menu.handle_key(KeyCode::Enter) {
            MenuOutcome::Run(entry) => {
                assert!(entry.action.agent.is_some());
                assert_eq!(
                    entry.picked,
                    Some(HandPin::Named {
                        id: "luna".to_string(),
                        effort: Some("high".to_string()),
                    })
                );
            }
            _ => panic!("expected the entry to run with the pick"),
        }

        // Into the efforts column, and down: the second effort rides the pin.
        menu.handle_key(KeyCode::Char('t'));
        menu.handle_key(KeyCode::Right);
        assert!(menu.footer().contains("←/→ column"), "{}", menu.footer());
        menu.handle_key(KeyCode::Char('j'));
        match menu.handle_key(KeyCode::Enter) {
            MenuOutcome::Run(entry) => assert_eq!(
                entry.picked,
                Some(HandPin::Named {
                    id: "luna".to_string(),
                    effort: Some("yolo".to_string()),
                })
            ),
            _ => panic!("expected the picked effort to ride"),
        }

        // A hand declaring no efforts has no second column — Right does
        // nothing — and is asked plainly (§FS-005-dispatch.14).
        menu.handle_key(KeyCode::Char('t'));
        menu.handle_key(KeyCode::Char('j'));
        assert!(!menu.footer().contains("←/→"), "{}", menu.footer());
        menu.handle_key(KeyCode::Right);
        match menu.handle_key(KeyCode::Enter) {
            MenuOutcome::Run(entry) => assert_eq!(
                entry.picked,
                Some(HandPin::Named {
                    id: "pi-alone".to_string(),
                    effort: None,
                })
            ),
            _ => panic!("expected the plain ask"),
        }
    }

    /// The picker is not a way past the running mark (§FS-005-dispatch.21).
    ///
    /// `t` builds a dispatch with a hand pinned to it, which is a *start*: on a
    /// row that says *running* the footer stops teaching it, the key opens
    /// nothing, and a picker that was already open when the mark arrived
    /// chooses *open* rather than the second copy. Every path through this
    /// screen has to answer a running row the same way, or the one the footer
    /// advertises is the one that breaks the promise.
    #[test]
    fn the_hand_picker_never_starts_a_second_copy_of_what_is_running() {
        let going = || {
            Some(offers::Running::Run {
                root: PathBuf::from("/w/demo/panta"),
                id: Some("3f9a2c".to_string()),
                control_url: None,
                attach: Some("the-runner attach '3f9a2c'".to_string()),
                since: Some(600),
                doing: "widget-42.fix-gate-1 [fix]".to_string(),
            })
        };
        let mut menu = menu_with_roster(full_roster());
        assert!(menu.entries[0].action.agent.is_some());
        menu.entries[0].running = going();

        // The footer teaches the way in, and not the picker.
        let footer = menu.footer();
        assert!(
            footer.contains("enter open the-runner attach '3f9a2c'"),
            "{footer}"
        );
        assert!(!footer.contains("t pick the hand"), "{footer}");
        assert!(matches!(
            menu.handle_key(KeyCode::Char('t')),
            MenuOutcome::Stay
        ));
        assert!(menu.picker.is_none(), "the picker never opened");

        // And with the picker open — a menu built before the mark arrived —
        // Enter in it opens what is going, with nothing pinned to it.
        menu.entries[0].running = None;
        menu.handle_key(KeyCode::Char('t'));
        assert!(menu.picker.is_some(), "the picker is open");
        menu.entries[0].running = going();
        match menu.handle_key(KeyCode::Enter) {
            MenuOutcome::Open(entry) => assert_eq!(entry.picked, None),
            _ => panic!("expected Open, not a second dispatch"),
        }
        assert!(menu.picker.is_none(), "the picker closed behind it");
    }

    /// A row that is both refused and running is answered the same way by the
    /// footer, by Enter and by `l` (§FS-005-dispatch.21): opening what is going
    /// asks nothing of the capability its *start* needed, so the running arm
    /// wins in all three places rather than one of them saying *open* while
    /// another does nothing.
    #[test]
    fn a_blocked_row_that_is_running_still_opens_everywhere() {
        let mut menu = menu(
            WorkspaceState::Ready,
            None,
            vec![action("open the ide", &[])],
        );
        menu.entries[0].gate = Gate::Blocked("the workspace is not on this machine".to_string());
        menu.entries[0].running = Some(offers::Running::Job {
            id: "20260822T090000.000Z-ide".to_string(),
            since: Some(12),
            says: "still going".to_string(),
            log: PathBuf::from("/state/ephor/jobs/j/log"),
        });
        let footer = menu.footer();
        assert!(
            footer.contains("enter open /state/ephor/jobs/j/log"),
            "{footer}"
        );
        assert!(matches!(
            menu.handle_key(KeyCode::Enter),
            MenuOutcome::Open(_)
        ));
        assert!(matches!(
            menu.handle_key(KeyCode::Char('l')),
            MenuOutcome::Open(_)
        ));
    }

    /// An unavailable hand is shown with its reason and cannot be chosen —
    /// the refusal was computed when the roster was read, not discovered on
    /// the dispatch (§AR-002-summons.4) — and Esc returns to the menu with
    /// nothing picked.
    #[test]
    fn an_unavailable_hand_is_shown_and_not_chosen() {
        let mut menu = menu_with_roster(full_roster());
        menu.handle_key(KeyCode::Char('t'));
        menu.handle_key(KeyCode::Char('j'));
        menu.handle_key(KeyCode::Char('j'));
        // The footer stops teaching Enter on it (§FS-004-quick-actions.2).
        assert!(!menu.footer().contains("enter"), "{}", menu.footer());
        assert!(matches!(menu.handle_key(KeyCode::Enter), MenuOutcome::Stay));
        // Still in the picker: Esc leaves it, the next Esc leaves the menu.
        assert!(matches!(menu.handle_key(KeyCode::Esc), MenuOutcome::Stay));
        assert!(
            menu.footer().contains("t pick the hand"),
            "back on the menu: {}",
            menu.footer()
        );
        assert!(matches!(menu.handle_key(KeyCode::Esc), MenuOutcome::Close));
    }

    /// With an empty roster the picker is not offered at all and the entry
    /// still dispatches (§FS-005-dispatch.14) — the shape of every machine
    /// with no runtime bound. And `t` on an entry that runs a command, or on
    /// one whose choice was refused, opens nothing.
    #[test]
    fn with_an_empty_roster_the_picker_is_withheld_and_the_entry_still_dispatches() {
        let mut unpicked = menu(
            WorkspaceState::Ready,
            None,
            vec![agent_entry(&crate::work::recipe::shipped()[0])],
        );
        assert!(
            !unpicked.footer().contains("t pick"),
            "{}",
            unpicked.footer()
        );
        assert!(matches!(
            unpicked.handle_key(KeyCode::Char('t')),
            MenuOutcome::Stay
        ));
        match unpicked.handle_key(KeyCode::Enter) {
            MenuOutcome::Run(entry) => {
                assert!(entry.action.agent.is_some());
                assert_eq!(entry.picked, None);
            }
            _ => panic!("the entry dispatches as if nothing had been picked"),
        }

        // A command entry has nothing to hand over, so nothing to pick for.
        let mut commands = menu(WorkspaceState::Ready, None, vec![action("browser", &[])])
            .with_roster(full_roster());
        assert!(
            !commands.footer().contains("t pick"),
            "{}",
            commands.footer()
        );
        commands.handle_key(KeyCode::Char('t'));
        match commands.handle_key(KeyCode::Enter) {
            MenuOutcome::Run(entry) => assert_eq!(entry.picked, None),
            _ => panic!("expected the command to run"),
        }

        // A refused choice blocks the entry, and the picker with it: the
        // remedy is configuration, and the row carries the whole reason.
        let refused = ActionConfig {
            hand: Some(crate::feed::config::Handed {
                says: "permits only sonnet".to_string(),
                refusal: Some("permits only sonnet".to_string()),
            }),
            ..agent_entry(&crate::work::recipe::shipped()[0])
        };
        let mut blocked =
            menu(WorkspaceState::Ready, None, vec![refused]).with_roster(full_roster());
        assert!(!blocked.footer().contains("t pick"), "{}", blocked.footer());
        assert!(matches!(
            blocked.handle_key(KeyCode::Char('t')),
            MenuOutcome::Stay
        ));
        assert!(matches!(
            blocked.handle_key(KeyCode::Esc),
            MenuOutcome::Close
        ));
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
