//! The gate screen: why a pull request's gate is what it is.
//!
//! The row can only afford counts, and counts are not the whole verdict — a
//! gate whose jobs are all green may still refuse to merge
//! (§FS-001-forge-interface.1). This screen is where the rest of it lives: the
//! per-repository counts spelled out, and the forge's own reasons verbatim.
//! What *failed* is one step further out, behind the action menu, because
//! asking for it costs a round trip to the forge (§FS-004-quick-actions.4).

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::feed::gate::{Gate, BLOCKED};
use crate::feed::model::Item;

use super::Action;

pub(crate) struct GateScreen {
    item: Item,
    gate: Gate,
    scroll: u16,
    viewport: u16,
}

impl GateScreen {
    /// Opens only on an item that has a gate: there is no such thing as an
    /// empty gate screen, and one would be a keystroke that appears to do
    /// nothing.
    pub fn open(item: Item) -> Option<Self> {
        let gate = Gate::of(&item)?;
        Some(GateScreen {
            item,
            gate,
            scroll: 0,
            viewport: 0,
        })
    }

    pub fn title(&self) -> String {
        let title: String = self.item.title.chars().take(60).collect();
        format!(" ephor — gate — {title}")
    }

    pub fn footer(&self) -> &'static str {
        " j/k scroll  x actions  o open  ; ops  esc back"
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => {
                Action::Back
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
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
            KeyCode::Char('o') => Action::OpenUrl(self.item.url.clone()),
            KeyCode::Char('x') => Action::OpenActionMenu(self.item.clone()),
            _ => Action::None,
        }
    }

    /// The whole screen, rebuilt each draw: a gate is a handful of repositories
    /// and a handful of sentences, so there is nothing here worth caching.
    fn lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let mut lines = vec![Line::from(Span::styled(
            "  jobs".to_string(),
            dim.add_modifier(Modifier::BOLD),
        ))];

        let width = self
            .gate
            .repos
            .iter()
            .map(|repo| repo.repo.chars().count())
            .max()
            .unwrap_or(0);
        for repo in &self.gate.repos {
            let mut spans = vec![Span::raw(format!("    {:width$}  ", repo.repo))];
            spans.extend(count_spans(repo.passed, repo.failed, repo.running));
            lines.push(Line::from(spans));
        }
        if self.gate.repos.is_empty() {
            lines.push(Line::from(Span::styled(
                "    none reported".to_string(),
                dim,
            )));
        }

        // The reasons are the forge's own words, shown as it wrote them: they
        // are what the reader will match against the forge's own screen, and a
        // reworded reason is one they cannot find there.
        if self.gate.blocked || !self.gate.blockers.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {BLOCKED}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            for blocker in &self.gate.blockers {
                lines.push(Line::from(format!("    • {blocker}")));
            }
            if self.gate.blockers.is_empty() {
                lines.push(Line::from(Span::styled(
                    "    the forge gave no reason".to_string(),
                    dim,
                )));
            }
        }

        if self.gate.is_red() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  x  see the CI failures".to_string(),
                dim,
            )));
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

/// `✓N ✗N ⋯N` in the same colors the navigator row uses, so a reader moving
/// between the two is reading one vocabulary.
fn count_spans(passed: u64, failed: u64, running: u64) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut push = |text: String, style: Style| {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(text, style));
    };
    if passed > 0 {
        push(format!("✓{passed}"), Style::default().fg(Color::Green));
    }
    if failed > 0 {
        push(
            format!("✗{failed}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        );
    }
    if running > 0 {
        push(format!("⋯{running}"), Style::default().fg(Color::Yellow));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::gate::RepoGate;
    use crate::feed::model::ItemKind;
    use serde_json::json;

    fn item(gate: Option<Gate>) -> Item {
        let raw = match gate {
            Some(gate) => json!({ "gate": gate.to_value() }),
            None => json!({}),
        };
        Item {
            id: "forge:widget/23562".to_string(),
            project: "widget".to_string(),
            source: "forge".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "Add error messages".to_string(),
            url: Some("https://forge.example/pr/23562".to_string()),
            state: Some("open".to_string()),
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw,
        }
    }

    fn blocked_gate() -> Gate {
        Gate {
            repos: vec![
                RepoGate {
                    repo: "app".to_string(),
                    passed: 40,
                    failed: 6,
                    running: 0,
                },
                RepoGate {
                    repo: "plugins".to_string(),
                    passed: 118,
                    failed: 0,
                    running: 0,
                },
            ],
            blocked: true,
            blockers: vec![
                "Requires approvals".to_string(),
                "The gate plugins has 122 jobs not yet run.".to_string(),
            ],
        }
    }

    fn text(screen: &GateScreen) -> String {
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
    fn a_gateless_item_has_no_screen() {
        assert!(GateScreen::open(item(None)).is_none());
    }

    #[test]
    fn the_screen_spells_out_the_counts_and_the_forges_own_reasons() {
        let screen = GateScreen::open(item(Some(blocked_gate()))).expect("has a gate");
        let text = text(&screen);
        let row = |repo: &str, counts: &str| {
            text.lines()
                .any(|line| line.contains(repo) && line.contains(counts))
        };
        assert!(row("app", "✓40 ✗6"), "{text}");
        assert!(row("plugins", "✓118"), "{text}");
        assert!(text.contains(BLOCKED), "{text}");
        // Verbatim, including the count the row itself cannot show.
        assert!(text.contains("122 jobs not yet run"), "{text}");
        assert!(text.contains("see the CI failures"), "{text}");
    }

    #[test]
    fn a_green_gate_offers_no_failures_line() {
        let gate = Gate {
            repos: vec![RepoGate {
                repo: "app".to_string(),
                passed: 35,
                failed: 0,
                running: 0,
            }],
            ..Gate::default()
        };
        let screen = GateScreen::open(item(Some(gate))).expect("has a gate");
        let text = text(&screen);
        assert!(!text.contains("see the CI failures"), "{text}");
        assert!(!text.contains(BLOCKED), "{text}");
    }

    #[test]
    fn scrolling_never_runs_past_the_content() {
        let mut screen = GateScreen::open(item(Some(blocked_gate()))).expect("has a gate");
        screen.viewport = 4;
        for _ in 0..50 {
            screen.handle_key(KeyCode::Char('j'));
        }
        // draw() clamps; emulate its clamp with the same arithmetic.
        let max = screen.lines().len().saturating_sub(4) as u16;
        screen.scroll = screen.scroll.min(max);
        assert_eq!(screen.scroll, max);
        screen.handle_key(KeyCode::Char('g'));
        assert_eq!(screen.scroll, 0);
    }
}
