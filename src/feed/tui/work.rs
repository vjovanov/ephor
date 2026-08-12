//! The work screen: what is being done about this item, and what could be
//! (§FS-005-dispatch).
//!
//! One screen for the whole of it — the tickets already open and what they
//! reached, whether the item has moved under them, and the recipes that apply
//! to it now. Dispatching from here is one keystroke on a row that says what
//! it will ask for, which is the difference between handing work over and
//! hoping.

use std::path::PathBuf;

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::feed::model::Item;
use crate::work::recipe::Recipe;
use crate::work::WorkStatus;

use super::Action;

/// A recipe as this screen shows it: what it is, and the words it would
/// actually send about this item (§FS-005-dispatch.7).
pub(crate) struct Offer {
    pub recipe: Recipe,
    pub brief: String,
}

pub(crate) struct WorkScreen {
    pub item: Item,
    status: Option<WorkStatus>,
    offers: Vec<Offer>,
    selected: usize,
    scroll: u16,
    viewport: u16,
}

impl WorkScreen {
    pub fn new(item: Item, status: Option<WorkStatus>, offers: Vec<Offer>) -> Self {
        WorkScreen {
            item,
            status,
            offers,
            selected: 0,
            scroll: 0,
            viewport: 0,
        }
    }

    pub fn title(&self) -> String {
        let title: String = self.item.title.chars().take(60).collect();
        format!(" ephor — work — {title}")
    }

    pub fn footer(&self) -> &'static str {
        " j/k move  enter/1-9 open work  s reopen  R run the runtime  e read the plan  o browser  esc back"
    }

    fn plan(&self) -> Option<PathBuf> {
        self.status.as_ref().map(|status| status.plan.clone())
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => {
                Action::Back
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.offers.len() {
                    self.selected += 1;
                }
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                Action::None
            }
            KeyCode::Char('f') | KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(self.viewport.max(1));
                Action::None
            }
            KeyCode::Char('b') | KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.viewport.max(1));
                Action::None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.scroll = 0;
                Action::None
            }
            KeyCode::Enter => self.dispatch(self.selected),
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                self.dispatch((digit as usize).wrapping_sub('1' as usize))
            }
            KeyCode::Char('s') => match &self.status {
                Some(status) if status.stale() => Action::SyncWork(self.item.clone()),
                Some(_) => Action::SetMessage(
                    "Nothing has happened to this item since its work was asked for".to_string(),
                ),
                None => Action::SetMessage("No work to reopen — open some first".to_string()),
            },
            // This item's plan, not everything the root holds: the reader is
            // on one item, and the root may carry another item's work about
            // the same checkout.
            KeyCode::Char('R') => match &self.status {
                Some(status) => Action::RunWork {
                    root: status.root.clone(),
                    checkout: status.checkout.clone(),
                    rhei: status.rhei.clone(),
                    label: self.item.title.clone(),
                },
                None => Action::SetMessage("No work to run yet".to_string()),
            },
            KeyCode::Char('e') => match self.plan() {
                Some(plan) => Action::ReadPlan(plan),
                None => Action::SetMessage("No plan yet".to_string()),
            },
            // Anything the recipes do not cover (§FS-005-dispatch.10).
            KeyCode::Char('a') => Action::AskWork(self.item.clone()),
            KeyCode::Char('o') => Action::OpenUrl(self.item.url.clone()),
            _ => Action::None,
        }
    }

    fn dispatch(&self, index: usize) -> Action {
        match self.offers.get(index) {
            Some(offer) => Action::DispatchWork {
                item: self.item.clone(),
                recipe: offer.recipe.id.clone(),
            },
            None => Action::None,
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let heading = dim.add_modifier(Modifier::BOLD);
        let mut lines = Vec::new();

        match &self.status {
            None => {
                lines.push(Line::from(Span::styled(
                    "  nothing has been handed over for this item yet".to_string(),
                    dim,
                )));
            }
            Some(status) => {
                lines.push(Line::from(Span::styled("  the plan".to_string(), heading)));
                lines.push(Line::from(Span::styled(
                    format!("    {}", status.plan.display()),
                    dim,
                )));
                if status.missing {
                    lines.push(Line::from(Span::styled(
                        "    the plan this points at is gone".to_string(),
                        Style::default().fg(Color::Red),
                    )));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  what has been asked for".to_string(),
                    heading,
                )));
                for ticket in &status.tickets {
                    let (marker, style) = if ticket.waiting {
                        (
                            "⚠",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        )
                    } else if ticket.finished {
                        ("✓", Style::default().fg(Color::Green))
                    } else {
                        ("⚙", Style::default().fg(Color::Yellow))
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("    {marker} "), style),
                        Span::raw(format!("{:<14}", ticket.id)),
                        Span::raw(shorten(&ticket.title, &self.item.title)),
                        Span::styled(
                            format!("  [{}]", ticket.state.as_deref().unwrap_or("?")),
                            dim,
                        ),
                    ]));
                    if let Some(verdict) = &ticket.verdict {
                        lines.push(Line::from(Span::styled(
                            format!("        {verdict}"),
                            Style::default().fg(Color::Cyan),
                        )));
                    }
                }
                if status.tickets.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "    the plan holds no tickets".to_string(),
                        dim,
                    )));
                }
                if let Some(waiting) = status.waiting() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  ⚠ {} is waiting on you — the question is in the plan",
                            waiting.id
                        ),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        "    answer it in the ticket (e), then move it on:".to_string(),
                        dim,
                    )));
                    lines.push(Line::from(Span::styled(
                        format!(
                            "    rhei transition {} --from {} --to <state>",
                            waiting.id,
                            waiting.state.as_deref().unwrap_or("?")
                        ),
                        dim,
                    )));
                }
                if status.stale() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("  ⟳ since that was asked: {}", status.changes.join("; ")),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        "    s reopens it with what changed".to_string(),
                        dim,
                    )));
                }
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  what can be asked for".to_string(),
            heading,
        )));
        if self.offers.is_empty() {
            lines.push(Line::from(Span::styled(
                "    no recipe applies to this item".to_string(),
                dim,
            )));
        }
        for (index, offer) in self.offers.iter().enumerate() {
            let recipe = &offer.recipe;
            let selected = index == self.selected;
            let mut spans = vec![
                Span::styled(
                    format!("  {} {} ", if selected { "▸" } else { " " }, index + 1),
                    dim,
                ),
                Span::styled(
                    format!("{} {}", recipe.icon, recipe.description),
                    if selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
            ];
            if recipe.needs_checkout {
                spans.push(Span::styled("  (needs the branch here)".to_string(), dim));
            }
            lines.push(Line::from(spans));
        }
        // What the selected recipe would actually ask for: dispatching is
        // cheap to press and expensive to run, so the words go on screen
        // before the keystroke rather than into a file afterwards.
        if let Some(offer) = self.offers.get(self.selected) {
            lines.push(Line::from(""));
            for line in offer.brief.lines().take(6) {
                lines.push(Line::from(Span::styled(format!("      {line}"), dim)));
            }
        }
        lines
    }

    pub fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        self.viewport = area.height;
        let lines = self.lines();
        let max_scroll = lines.len().saturating_sub(area.height as usize);
        self.scroll = self.scroll.min(max_scroll as u16);
        frame.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), area);
    }
}

/// A ticket's title without the item's own, which this screen's header
/// already carries: "fix the red gate — #17 Humanize durations in the log
/// reader" is "fix the red gate" once the reader knows which item they are on.
fn shorten(title: &str, item: &str) -> String {
    title
        .strip_suffix(item)
        .map(|kept| kept.trim_end().trim_end_matches('—').trim_end())
        .filter(|kept| !kept.is_empty())
        .unwrap_or(title)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::ItemKind;
    use crate::work::TicketStatus;

    fn item() -> Item {
        Item {
            id: "forge:demo/17".to_string(),
            project: "demo".to_string(),
            source: "forge".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "Humanize durations".to_string(),
            url: Some("https://forge.example/17".to_string()),
            state: Some("open".to_string()),
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw: serde_json::json!({}),
        }
    }

    fn status(stale: bool) -> WorkStatus {
        WorkStatus {
            project: "demo".to_string(),
            root: PathBuf::from("/w/demo/panta"),
            rhei: "forge-demo-17".to_string(),
            checkout: PathBuf::from("/w/demo"),
            plan: PathBuf::from("/w/demo/panta/forge-demo-17.rhei.md"),
            missing: false,
            tickets: vec![TicketStatus {
                id: "fix-gate-1".to_string(),
                recipe: "fix-gate".to_string(),
                title: "fix the red gate".to_string(),
                state: Some("done".to_string()),
                finished: true,
                waiting: false,
                verdict: Some("done — the change is right".to_string()),
            }],
            changes: if stale {
                vec!["1 new message".to_string()]
            } else {
                Vec::new()
            },
        }
    }

    /// Offers as the shell builds them: each recipe with its brief already
    /// rendered against the item.
    fn offers() -> Vec<Offer> {
        crate::work::recipe::shipped()
            .into_iter()
            .map(|recipe| Offer {
                brief: recipe.brief.replace("{title}", "Humanize durations"),
                recipe,
            })
            .collect()
    }

    fn text(screen: &WorkScreen) -> String {
        screen
            .lines()
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_screen_says_what_was_asked_what_it_reached_and_what_else_could_be() {
        let shown = offers();
        let screen = WorkScreen::new(item(), Some(status(true)), offers());
        let text = text(&screen);
        assert!(text.contains("forge-demo-17.rhei.md"), "{text}");
        assert!(text.contains("fix-gate-1"), "{text}");
        assert!(text.contains("done — the change is right"), "{text}");
        assert!(
            text.contains("⟳ since that was asked: 1 new message"),
            "{text}"
        );
        // The ticket's title without the item's, which the header carries.
        assert!(text.contains("fix the red gate  "), "{text}");
        // Every offer, and the words the first one would actually send —
        // rendered, not the template they came from (§FS-005-dispatch.7).
        assert!(text.contains(&shown[0].recipe.description), "{text}");
        assert!(
            text.contains(shown[0].brief.lines().next().unwrap()),
            "{text}"
        );
        assert!(!text.contains("{title}"), "{text}");
    }

    #[test]
    fn an_item_with_no_work_still_shows_what_could_be_asked_for() {
        let screen = WorkScreen::new(item(), None, offers());
        let text = text(&screen);
        assert!(text.contains("nothing has been handed over"), "{text}");
        assert!(text.contains("what can be asked for"), "{text}");
    }

    #[test]
    fn keys_dispatch_by_number_and_refuse_what_there_is_nothing_to_do() {
        let ids: Vec<String> = offers().into_iter().map(|o| o.recipe.id).collect();
        let mut screen = WorkScreen::new(item(), None, offers());
        match screen.handle_key(KeyCode::Char('2')) {
            Action::DispatchWork { recipe, .. } => assert_eq!(recipe, ids[1]),
            _ => panic!("expected a dispatch"),
        }
        screen.handle_key(KeyCode::Char('j'));
        match screen.handle_key(KeyCode::Enter) {
            Action::DispatchWork { recipe, .. } => assert_eq!(recipe, ids[1]),
            _ => panic!("expected a dispatch"),
        }
        // Nothing dispatched yet: nothing to run, reopen, or read.
        assert!(matches!(
            screen.handle_key(KeyCode::Char('R')),
            Action::SetMessage(_)
        ));
        assert!(matches!(
            screen.handle_key(KeyCode::Char('s')),
            Action::SetMessage(_)
        ));

        // With work that is current, reopening says so rather than doing it.
        let mut screen = WorkScreen::new(item(), Some(status(false)), offers());
        assert!(matches!(
            screen.handle_key(KeyCode::Char('s')),
            Action::SetMessage(_)
        ));
        assert!(matches!(
            screen.handle_key(KeyCode::Char('R')),
            Action::RunWork { .. }
        ));
    }
}
