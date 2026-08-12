//! A single line of the reader's own words, typed where they are standing
//! (§FS-005-dispatch.8).
//!
//! One line, deliberately. What is typed here is a sentence — an ask, or a
//! command — and anything longer belongs in the plan, which `e` opens in an
//! editor, or on the command line, which has a shell. A text editor inside an
//! inbox is a text editor nobody asked for.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// What the prompt is collecting, so the shell knows what to do with it.
pub(crate) enum Asking {
    /// Words for a ticket about this item.
    Work(crate::feed::model::Item),
    /// A shell command to run on the item the action menu is open on.
    Command(Box<super::ActionMenu>),
}

pub(crate) struct Prompt {
    pub asking: Asking,
    title: String,
    hint: String,
    input: String,
}

pub(crate) enum PromptOutcome {
    Stay,
    Cancel,
    Submit(String),
}

impl Prompt {
    pub fn new(asking: Asking, title: impl Into<String>, hint: impl Into<String>) -> Self {
        Prompt {
            asking,
            title: title.into(),
            hint: hint.into(),
            input: String::new(),
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> PromptOutcome {
        match code {
            KeyCode::Esc => PromptOutcome::Cancel,
            KeyCode::Enter => {
                if self.input.trim().is_empty() {
                    PromptOutcome::Cancel
                } else {
                    PromptOutcome::Submit(self.input.trim().to_string())
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                PromptOutcome::Stay
            }
            // The one editing key worth having: a typo eight words back is
            // faster to retype than to walk back to.
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                PromptOutcome::Stay
            }
            KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => {
                let trimmed = self.input.trim_end();
                let cut = trimmed.rfind(' ').map(|index| index + 1).unwrap_or(0);
                self.input.truncate(cut);
                PromptOutcome::Stay
            }
            KeyCode::Char(ch) => {
                self.input.push(ch);
                PromptOutcome::Stay
            }
            _ => PromptOutcome::Stay,
        }
    }

    /// A band across the middle, over whatever screen summoned it.
    pub fn draw(&self, frame: &mut ratatui::Frame, area: Rect) {
        let width = area.width.saturating_sub(4).max(20);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + area.height.saturating_sub(4) / 2,
            width,
            height: 4.min(area.height),
        };
        frame.render_widget(Clear, rect);

        let dim = Style::default().fg(Color::DarkGray);
        // The tail of what was typed, so a long line keeps its cursor in view.
        let room = rect.width.saturating_sub(4) as usize;
        let shown: String = if self.input.chars().count() > room {
            self.input
                .chars()
                .skip(self.input.chars().count() - room)
                .collect()
        } else {
            self.input.clone()
        };
        let body = vec![
            Line::from(vec![
                Span::raw(shown),
                Span::styled("▌", Style::default().add_modifier(Modifier::RAPID_BLINK)),
            ]),
            Line::from(Span::styled(format!(" {}", self.hint), dim)),
        ];
        frame.render_widget(
            Paragraph::new(body).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", self.title)),
            ),
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::{Item, ItemKind};

    fn asking() -> Prompt {
        let item = Item {
            id: "forge:demo/17".to_string(),
            project: "demo".to_string(),
            source: "forge".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "t".to_string(),
            url: None,
            state: None,
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw: serde_json::Value::Null,
        };
        Prompt::new(Asking::Work(item), "ask", "enter sends")
    }

    fn type_in(prompt: &mut Prompt, text: &str) {
        for ch in text.chars() {
            prompt.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        }
    }

    #[test]
    fn a_line_is_typed_corrected_and_sent() {
        let mut prompt = asking();
        type_in(&mut prompt, "bump the timeoutx");
        prompt.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
        type_in(&mut prompt, " to 30s");
        match prompt.handle_key(KeyCode::Enter, KeyModifiers::NONE) {
            PromptOutcome::Submit(words) => assert_eq!(words, "bump the timeout to 30s"),
            _ => panic!("expected the words"),
        }

        // Word and line kill, as a shell has them.
        let mut prompt = asking();
        type_in(&mut prompt, "run the flaky test");
        prompt.handle_key(KeyCode::Char('w'), KeyModifiers::CONTROL);
        match prompt.handle_key(KeyCode::Enter, KeyModifiers::NONE) {
            PromptOutcome::Submit(words) => assert_eq!(words, "run the flaky"),
            _ => panic!("expected the words"),
        }
        prompt.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert!(matches!(
            prompt.handle_key(KeyCode::Enter, KeyModifiers::NONE),
            PromptOutcome::Cancel
        ));
    }

    /// Nothing typed is nothing asked for: enter on an empty line closes the
    /// prompt rather than opening a ticket that says nothing.
    #[test]
    fn an_empty_line_asks_for_nothing() {
        let mut prompt = asking();
        assert!(matches!(
            prompt.handle_key(KeyCode::Enter, KeyModifiers::NONE),
            PromptOutcome::Cancel
        ));
        type_in(&mut prompt, "   ");
        assert!(matches!(
            prompt.handle_key(KeyCode::Enter, KeyModifiers::NONE),
            PromptOutcome::Cancel
        ));
        assert!(matches!(
            prompt.handle_key(KeyCode::Esc, KeyModifiers::NONE),
            PromptOutcome::Cancel
        ));
    }
}
