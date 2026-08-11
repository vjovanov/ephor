//! The thread screen: full-text visualization of one item's conversation.
//!
//! Messages render as cards — a colored author gutter, wrapped body text,
//! and a reactions line (👍 2 (alice, bob)). j/k select whole messages, not
//! lines; the viewport follows the selection. `+` opens a reaction picker
//! for messages whose provider supports posting (a `react` descriptor in
//! the thread data).

use std::ops::Range;

use chrono::{DateTime, Utc};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use serde_json::Value;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::feed::model::Item;
use crate::feed::react::{self, ReactTarget, PALETTE};
use crate::feed::render::age;

use super::Action;

struct Reaction {
    emoji: String,
    users: Vec<String>,
}

struct Msg {
    /// Index of the thread this message belongs to (for separators).
    thread: usize,
    author: String,
    when: Option<DateTime<Utc>>,
    text: String,
    reactions: Vec<Reaction>,
    react: Option<ReactTarget>,
}

pub(crate) struct ThreadScreen {
    item: Item,
    messages: Vec<Msg>,
    /// Flat message index of the selected card.
    selected: usize,
    scroll: u16,
    /// Move the viewport to the selected card on the next draw.
    follow: bool,
    /// Open reaction picker: index into [`PALETTE`].
    picker: Option<usize>,

    // Render cache, rebuilt when the body width changes.
    wrap_width: u16,
    viewport: u16,
    lines: Vec<Line<'static>>,
    /// Flat message index -> its card's line range (excludes the blank
    /// separator line after the card).
    ranges: Vec<Range<usize>>,
}

impl ThreadScreen {
    /// None when the item has no recorded thread messages.
    pub fn open(item: Item) -> Option<Self> {
        let threads = item
            .raw
            .get("threads")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut messages = Vec::new();
        for (thread_index, thread) in threads.iter().enumerate() {
            for message in thread
                .get("messages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                messages.push(parse_msg(thread_index, message));
            }
        }
        if messages.is_empty() {
            return None;
        }
        Some(ThreadScreen {
            item,
            messages,
            selected: 0,
            scroll: 0,
            follow: true,
            picker: None,
            wrap_width: 0,
            viewport: 0,
            lines: Vec::new(),
            ranges: Vec::new(),
        })
    }

    pub fn title(&self) -> String {
        let title: String = self.item.title.chars().take(60).collect();
        format!(" ephor — thread — {title}")
    }

    pub fn footer(&self) -> &'static str {
        if self.picker.is_some() {
            " ←/→ choose  1-8 pick  enter react  esc cancel"
        } else {
            " j/k message  f/b page  + react  x actions  o open  m done  esc back"
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        if let Some(pick) = self.picker {
            return self.handle_picker_key(pick, code);
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => {
                Action::CloseThread
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.messages.len() {
                    self.selected += 1;
                }
                self.follow = true;
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.follow = true;
                Action::None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.selected = 0;
                self.follow = true;
                Action::None
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.selected = self.messages.len() - 1;
                self.follow = true;
                Action::None
            }
            KeyCode::Char('f') | KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(self.page());
                Action::None
            }
            KeyCode::Char('b') | KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.page());
                Action::None
            }
            KeyCode::Enter | KeyCode::Char('o') => Action::OpenUrl(self.item.url.clone()),
            KeyCode::Char('m') | KeyCode::Char('d') | KeyCode::Char(' ') => Action::MarkDone {
                marks: vec![(
                    self.item.id.clone(),
                    self.item.updated_at,
                    self.item.title.clone(),
                )],
                pop: true,
            },
            KeyCode::Char('x') => Action::OpenActionMenu(self.item.clone()),
            KeyCode::Char('+') => {
                if self.messages[self.selected].react.is_some() {
                    self.picker = Some(0);
                    Action::None
                } else {
                    Action::SetMessage(
                        "Posting reactions is not supported for this message".to_string(),
                    )
                }
            }
            _ => Action::None,
        }
    }

    fn handle_picker_key(&mut self, pick: usize, code: KeyCode) -> Action {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.picker = None;
                Action::None
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.picker = Some((pick + PALETTE.len() - 1) % PALETTE.len());
                Action::None
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => {
                self.picker = Some((pick + 1) % PALETTE.len());
                Action::None
            }
            KeyCode::Enter => self.post(pick),
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                let index = (digit as usize).wrapping_sub('1' as usize);
                if index < PALETTE.len() {
                    self.post(index)
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        }
    }

    fn post(&mut self, pick: usize) -> Action {
        self.picker = None;
        let Some(target) = self.messages[self.selected].react.clone() else {
            return Action::None;
        };
        let (emoji, content) = PALETTE[pick];
        Action::React {
            target,
            content,
            emoji,
            message: self.selected,
        }
    }

    /// Optimistically record a successfully posted reaction so it shows
    /// without waiting for the next refresh.
    pub fn add_local_reaction(&mut self, message: usize, emoji: &str) {
        let Some(msg) = self.messages.get_mut(message) else {
            return;
        };
        match msg
            .reactions
            .iter_mut()
            .find(|reaction| reaction.emoji == emoji)
        {
            Some(reaction) => {
                if !reaction.users.iter().any(|user| user == "you") {
                    reaction.users.push("you".to_string());
                }
            }
            None => msg.reactions.push(Reaction {
                emoji: emoji.to_string(),
                users: vec!["you".to_string()],
            }),
        }
        self.wrap_width = 0; // invalidate the render cache
    }

    fn page(&self) -> u16 {
        self.viewport.saturating_sub(2).max(1)
    }

    pub fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        let picker_height = if self.picker.is_some() { 1 } else { 0 };
        let [head_area, body_area, picker_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(picker_height),
        ])
        .areas(area);

        frame.render_widget(
            Paragraph::new(self.header_lines(head_area.width)),
            head_area,
        );

        // ranges is empty only before the first build (covers a zero-width
        // terminal, where the width check alone would never trigger).
        if self.wrap_width != body_area.width || self.ranges.is_empty() {
            self.rebuild_lines(body_area.width);
        }
        self.viewport = body_area.height;
        let height = body_area.height as usize;
        if self.follow {
            self.ensure_visible(height);
            self.follow = false;
        }
        let max_scroll = self.lines.len().saturating_sub(height);
        self.scroll = self.scroll.min(max_scroll as u16);

        let selected_range = self.ranges[self.selected].clone();
        let body: Vec<Line> = self
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let line = line.clone();
                if selected_range.contains(&index) {
                    line.patch_style(Style::default().bg(Color::Rgb(45, 45, 62)))
                } else {
                    line
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(body).scroll((self.scroll, 0)), body_area);

        if let Some(pick) = self.picker {
            let mut spans = vec![Span::styled(
                " react:",
                Style::default().fg(Color::DarkGray),
            )];
            for (index, (emoji, _)) in PALETTE.iter().enumerate() {
                let style = if index == pick {
                    Style::default()
                        .bg(Color::Rgb(60, 60, 80))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(format!(" {emoji} "), style));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), picker_area);
        }
    }

    fn header_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut title = vec![Span::styled(
            self.item.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )];
        if let Some(state) = &self.item.state {
            title.push(Span::styled(
                format!("  [{state}]"),
                Style::default().fg(Color::DarkGray),
            ));
        }
        title.push(Span::styled(
            format!("  {} · {}", self.item.project, self.item.kind.label()),
            Style::default().fg(Color::DarkGray),
        ));
        let url = self
            .item
            .url
            .clone()
            .unwrap_or_else(|| "(no url)".to_string());
        vec![
            Line::from(title),
            Line::from(Span::styled(url, Style::default().fg(Color::DarkGray))),
            Line::from(Span::styled(
                "─".repeat(width as usize),
                Style::default().fg(Color::DarkGray),
            )),
        ]
    }

    /// Scroll so the selected card is fully visible (its top wins when the
    /// card is taller than the viewport).
    fn ensure_visible(&mut self, height: usize) {
        let range = &self.ranges[self.selected];
        let mut scroll = self.scroll as usize;
        if range.end > scroll + height {
            scroll = range.end - height;
        }
        if range.start < scroll {
            scroll = range.start;
        }
        self.scroll = scroll as u16;
    }

    fn rebuild_lines(&mut self, width: u16) {
        let now = Utc::now();
        self.wrap_width = width;
        self.lines.clear();
        self.ranges.clear();
        let wrap_width = (width as usize).saturating_sub(2).max(8);
        let multi = self
            .messages
            .iter()
            .any(|msg| msg.thread != self.messages[0].thread);

        let mut prev_thread = None;
        for (index, msg) in self.messages.iter().enumerate() {
            if multi && prev_thread != Some(msg.thread) {
                self.lines.push(Line::from(Span::styled(
                    format!("── thread {} ", msg.thread + 1),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            prev_thread = Some(msg.thread);

            let color = author_color(&msg.author);
            let gutter = || Span::styled("▍ ", Style::default().fg(color));
            let start = self.lines.len();

            let mut header = vec![
                gutter(),
                Span::styled(
                    msg.author.clone(),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ];
            if let Some(when) = msg.when {
                header.push(Span::styled(
                    format!("  {} ago", age(now, when)),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            if self.item.needs_response && index == self.messages.len() - 1 {
                header.push(Span::styled(
                    "  ← needs response",
                    Style::default().fg(Color::Red),
                ));
            }
            self.lines.push(Line::from(header));

            for text_line in msg.text.lines() {
                for wrapped in wrap_line(text_line, wrap_width) {
                    self.lines
                        .push(Line::from(vec![gutter(), Span::raw(wrapped)]));
                }
            }

            if !msg.reactions.is_empty() {
                let mut spans = vec![gutter()];
                for reaction in &msg.reactions {
                    let count = reaction.users.len().max(1);
                    spans.push(Span::raw(format!("{} {count}", reaction.emoji)));
                    if !reaction.users.is_empty() {
                        spans.push(Span::styled(
                            format!(" ({})", reaction.users.join(", ")),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                    spans.push(Span::raw("  "));
                }
                self.lines.push(Line::from(spans));
            }

            self.ranges.push(start..self.lines.len());
            self.lines.push(Line::default());
        }
    }
}

fn parse_msg(thread: usize, value: &Value) -> Msg {
    let reactions = value
        .get("reactions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|reaction| {
            let emoji = reaction.get("emoji").and_then(Value::as_str)?.to_string();
            let users = reaction
                .get("users")
                .and_then(Value::as_array)
                .map(|users| {
                    users
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|user| !user.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            Some(Reaction { emoji, users })
        })
        .collect();
    Msg {
        thread,
        author: value
            .get("author")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        when: value
            .get("when")
            .and_then(Value::as_str)
            .and_then(|when| DateTime::parse_from_rfc3339(when).ok())
            .map(|when| when.with_timezone(&Utc)),
        text: value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        reactions,
        react: react::parse_target(value),
    }
}

/// Stable per-author color so each participant keeps theirs across messages.
fn author_color(author: &str) -> Color {
    const COLORS: [Color; 6] = [
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Magenta,
        Color::Blue,
        Color::LightRed,
    ];
    let hash = author.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    });
    COLORS[hash % COLORS.len()]
}

/// Greedy word wrap by display width; words longer than the line are
/// hard-broken. Lines that already fit pass through untouched (preserving
/// leading whitespace, e.g. code snippets).
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if UnicodeWidthStr::width(line) <= width {
        return vec![line.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for word in line.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        if !current.is_empty() {
            if current_width + 1 + word_width <= width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
                continue;
            }
            out.push(std::mem::take(&mut current));
            current_width = 0;
        }
        if word_width <= width {
            current.push_str(word);
            current_width = word_width;
        } else {
            for ch in word.chars() {
                let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width + ch_width > width && !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    current_width = 0;
                }
                current.push(ch);
                current_width += ch_width;
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::{ItemKind, ItemRole};
    use serde_json::json;

    fn item_with_threads(threads: Value) -> Item {
        Item {
            id: "github-prs:acme/widget#77".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: Some(ItemRole::Reviewer),
            title: "Add layered workflow".to_string(),
            url: Some("https://github.com/acme/widget/pull/77".to_string()),
            state: Some("open:mentioned".to_string()),
            needs_response: true,
            updated_at: Utc::now(),
            raw: json!({ "threads": threads }),
        }
    }

    fn plain_text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn rendered(threads: Value, width: u16) -> (ThreadScreen, Vec<String>) {
        let mut screen = ThreadScreen::open(item_with_threads(threads)).unwrap();
        screen.rebuild_lines(width);
        let text = plain_text(&screen.lines);
        (screen, text)
    }

    #[test]
    fn open_returns_none_without_messages() {
        assert!(ThreadScreen::open(item_with_threads(json!([]))).is_none());
        assert!(ThreadScreen::open(item_with_threads(json!([{ "messages": [] }]))).is_none());
    }

    #[test]
    fn renders_authors_ages_and_text() {
        let now = Utc::now();
        let when = (now - chrono::Duration::hours(3)).to_rfc3339();
        let (screen, text) = rendered(
            json!([{
                "messages": [
                    { "author": "reviewer", "text": "@tester can you confirm?\nsecond line", "when": when },
                    { "author": "tester", "text": "confirmed", "when": when },
                ]
            }]),
            80,
        );
        assert_eq!(screen.ranges.len(), 2);
        assert!(
            text.iter()
                .any(|line| line.starts_with("▍ reviewer  3h ago")),
            "{text:?}"
        );
        assert!(
            text.contains(&"▍ @tester can you confirm?".to_string()),
            "{text:?}"
        );
        assert!(text.contains(&"▍ second line".to_string()), "{text:?}");
        assert!(
            text.iter().any(|line| line.starts_with("▍ tester")),
            "{text:?}"
        );
        // The item needs a response: the last message is flagged.
        assert!(
            text.iter().any(|line| line.ends_with("← needs response")),
            "{text:?}"
        );
        // Ranges point at each card's header line.
        assert!(
            text[screen.ranges[1].start].starts_with("▍ tester"),
            "{text:?}"
        );
    }

    #[test]
    fn labels_multiple_threads() {
        let (_, text) = rendered(
            json!([
                { "messages": [{ "author": "a", "text": "first" }] },
                { "messages": [{ "author": "b", "text": "second" }] },
            ]),
            80,
        );
        assert!(
            text.iter().any(|line| line.starts_with("── thread 1")),
            "{text:?}"
        );
        assert!(
            text.iter().any(|line| line.starts_with("── thread 2")),
            "{text:?}"
        );
    }

    #[test]
    fn wraps_long_lines_to_width() {
        let long = "word ".repeat(40);
        let (screen, text) = rendered(
            json!([{ "messages": [{ "author": "a", "text": long }] }]),
            40,
        );
        assert!(
            screen.ranges[0].len() > 2,
            "expected a multi-line card: {text:?}"
        );
        for line in &text {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 40,
                "too wide: {line:?}"
            );
        }
    }

    #[test]
    fn renders_reactions_with_users() {
        let (_, text) = rendered(
            json!([{ "messages": [{
                "author": "a",
                "text": "hello",
                "reactions": [
                    { "emoji": "👍", "users": ["alice", "bob"] },
                    { "emoji": "🚀", "users": [] },
                ],
            }] }]),
            80,
        );
        assert!(
            text.iter().any(|line| line.contains("👍 2 (alice, bob)")),
            "{text:?}"
        );
        assert!(text.iter().any(|line| line.contains("🚀 1")), "{text:?}");
    }

    #[test]
    fn local_reaction_updates_once() {
        let (mut screen, _) = rendered(
            json!([{ "messages": [{
                "author": "a",
                "text": "hello",
                "react": { "provider": "github", "subject_id": "MDEy" },
            }] }]),
            80,
        );
        screen.add_local_reaction(0, "🚀");
        screen.add_local_reaction(0, "🚀");
        screen.rebuild_lines(80);
        let text = plain_text(&screen.lines);
        assert!(
            text.iter().any(|line| line.contains("🚀 1 (you)")),
            "{text:?}"
        );
    }

    #[test]
    fn selection_follows_into_view() {
        let messages: Vec<Value> = (0..30)
            .map(|index| json!({ "author": "a", "text": format!("message {index}") }))
            .collect();
        let (mut screen, _) = rendered(json!([{ "messages": messages }]), 80);
        screen.selected = 29;
        screen.ensure_visible(10);
        let range = screen.ranges[29].clone();
        assert!(range.start >= screen.scroll as usize);
        assert!(range.end <= screen.scroll as usize + 10);

        screen.selected = 0;
        screen.ensure_visible(10);
        assert_eq!(screen.scroll, 0);
    }

    #[test]
    fn picker_posts_selected_reaction() {
        let (mut screen, _) = rendered(
            json!([{ "messages": [{
                "author": "a",
                "text": "hello",
                "react": { "provider": "github", "subject_id": "MDEy" },
            }] }]),
            80,
        );
        screen.handle_key(KeyCode::Char('+'));
        assert_eq!(screen.picker, Some(0));
        screen.handle_key(KeyCode::Right);
        let action = screen.handle_key(KeyCode::Enter);
        match action {
            Action::React {
                content,
                emoji,
                message,
                ..
            } => {
                assert_eq!((emoji, content, message), ("👎", "THUMBS_DOWN", 0));
            }
            _ => panic!("expected a React action"),
        }
        assert_eq!(screen.picker, None);
    }

    #[test]
    fn picker_unavailable_without_target() {
        let (mut screen, _) = rendered(
            json!([{ "messages": [{ "author": "a", "text": "hello" }] }]),
            80,
        );
        match screen.handle_key(KeyCode::Char('+')) {
            Action::SetMessage(_) => {}
            _ => panic!("expected a status message"),
        }
        assert_eq!(screen.picker, None);
    }
}
