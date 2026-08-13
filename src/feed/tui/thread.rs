//! The thread screen: full-text visualization of one item's conversation.
//!
//! Messages render as cards — a colored author gutter, wrapped body text,
//! and a reactions line (👍 2 (alice, bob)). A message the forge tracks as a
//! task wears its box, ☐ or ☑ (§FS-004-quick-actions.5). j/k select whole
//! messages, not lines; the viewport follows the selection.
//!
//! The write keys are offered per selected message rather than per screen
//! (§FS-004-quick-actions.2): `+` opens the reaction picker where the message
//! carries a `react` descriptor, `t` ticks where it carries an unresolved
//! `task`. A source that reports neither shows neither key, and the footer is
//! built from the selection for exactly that reason.

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
use crate::feed::task::{self, Task};

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
    task: Option<Task>,
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
                messages.push(parse_msg(thread_index, message, &item.source));
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

    /// Built from the selected message, not from the screen
    /// (§FS-004-quick-actions.2): a key is advertised where it would do
    /// something. A reader on a forge that posts neither reactions nor task
    /// transitions is never taught `+`, and so never spends a keystroke
    /// finding out from a one-line refusal at the bottom of the screen.
    pub fn footer(&self) -> String {
        if self.picker.is_some() {
            return " ←/→ choose  1-8 pick  enter react  esc cancel".to_string();
        }
        let selected = &self.messages[self.selected];
        let mut keys = String::from(" j/k message  f/b page");
        if selected.react.is_some() {
            keys.push_str("  + react");
        }
        if selected.task.as_ref().is_some_and(|task| !task.resolved) {
            keys.push_str("  t tick");
        }
        keys.push_str("  x actions  o open  m done  esc back");
        keys
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        if let Some(pick) = self.picker {
            return self.handle_picker_key(pick, code);
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => {
                Action::Back
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
            KeyCode::Char('t') => self.tick(),
            _ => Action::None,
        }
    }

    /// Tick the selected task (§FS-004-quick-actions.5). Both refusals name
    /// what the message is rather than what the key is: a reader who pressed
    /// `t` on an ordinary comment and a reader who pressed it on a box already
    /// ticked have different questions.
    fn tick(&mut self) -> Action {
        match &self.messages[self.selected].task {
            Some(task) if task.resolved => {
                Action::SetMessage("This task is already ticked".to_string())
            }
            Some(task) => Action::ResolveTask {
                task: task.clone(),
                project: self.item.project.clone(),
                message: self.selected,
            },
            None => Action::SetMessage("This message is not a task".to_string()),
        }
    }

    /// Record a ticked task without waiting for the next refresh
    /// (§FS-004-quick-actions.5), the way a posted reaction is recorded.
    pub fn tick_local(&mut self, message: usize) {
        if let Some(task) = self
            .messages
            .get_mut(message)
            .and_then(|msg| msg.task.as_mut())
        {
            task.resolved = true;
            self.wrap_width = 0;
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
            project: self.item.project.clone(),
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

            // A task wears its box on the first body line, and its wrapped
            // continuation lines are indented under it, so the sentence the
            // box refers to reads as one block (§FS-004-quick-actions.5).
            let box_style = msg.task.as_ref().map(|task| {
                Style::default().fg(if task.resolved {
                    Color::Green
                } else {
                    Color::Yellow
                })
            });
            let body_width = match msg.task {
                Some(_) => wrap_width.saturating_sub(2).max(8),
                None => wrap_width,
            };
            let mut body: Vec<&str> = msg.text.lines().collect();
            if body.is_empty() && msg.task.is_some() {
                body.push("");
            }
            let mut first = true;
            for text_line in body {
                for wrapped in wrap_line(text_line, body_width) {
                    let mut spans = vec![gutter()];
                    if let (Some(task), Some(style)) = (&msg.task, box_style) {
                        let glyph = if first { task.box_glyph() } else { " " };
                        spans.push(Span::styled(format!("{glyph} "), style));
                    }
                    spans.push(Span::raw(wrapped));
                    self.lines.push(Line::from(spans));
                    first = false;
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

fn parse_msg(thread: usize, value: &Value, source: &str) -> Msg {
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
        react: react::parse_target(value, source),
        task: task::parse(value, source),
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

    /// One bot thread as a forge that tracks tasks reports it.
    fn checklist() -> Value {
        json!([{ "messages": [
            { "author": "Bot", "text": "Please check this PR for the following:" },
            { "author": "Bot", "text": "Considered items in above checklist",
              "task": { "state": "open", "comment": 1432050 } },
        ] }])
    }

    #[test]
    fn a_task_renders_its_box() {
        let (_, text) = rendered(checklist(), 80);
        assert!(
            text.contains(&"▍ ☐ Considered items in above checklist".to_string()),
            "{text:?}"
        );
        // The prompt above it is an ordinary message and wears nothing.
        assert!(
            text.contains(&"▍ Please check this PR for the following:".to_string()),
            "{text:?}"
        );
    }

    /// Wrapped task text lines up under its box rather than under the gutter,
    /// so the box reads as belonging to the whole sentence.
    #[test]
    fn a_wrapped_task_indents_under_its_box() {
        let (_, text) = rendered(
            json!([{ "messages": [{
                "author": "Bot",
                "text": "I acknowledge that performance impact is known and causes no regression",
                "task": { "state": "open" },
            }] }]),
            40,
        );
        // [0] is the author header; the body follows it.
        assert!(text[1].starts_with("▍ ☐ I acknowledge"), "{text:?}");
        assert!(text[2].starts_with("▍   impact"), "{text:?}");
    }

    /// §FS-004-quick-actions.2: the footer is what the selected message can
    /// actually do. A forge that posts no reactions never advertises `+`.
    #[test]
    fn the_footer_offers_only_what_the_selection_supports() {
        let (mut screen, _) = rendered(checklist(), 80);
        assert!(!screen.footer().contains("+ react"), "{}", screen.footer());
        assert!(!screen.footer().contains("t tick"), "{}", screen.footer());

        screen.handle_key(KeyCode::Char('j'));
        assert!(screen.footer().contains("t tick"), "{}", screen.footer());
        assert!(!screen.footer().contains("+ react"), "{}", screen.footer());

        let (screen, _) = rendered(
            json!([{ "messages": [{ "author": "a", "text": "hi",
                                    "react": { "provider": "github", "subject_id": "MDEy" } }] }]),
            80,
        );
        assert!(screen.footer().contains("+ react"), "{}", screen.footer());
        assert!(!screen.footer().contains("t tick"), "{}", screen.footer());
    }

    #[test]
    fn ticking_asks_the_source_that_reported_the_task() {
        let (mut screen, _) = rendered(checklist(), 80);
        screen.handle_key(KeyCode::Char('j'));
        match screen.handle_key(KeyCode::Char('t')) {
            Action::ResolveTask {
                task,
                project,
                message,
            } => {
                assert_eq!(task.source, "github-prs");
                assert_eq!(task.target, json!({ "state": "open", "comment": 1432050 }));
                assert_eq!((project.as_str(), message), ("widget", 1));
            }
            _ => panic!("expected a ResolveTask action"),
        }
    }

    /// Ticking shows immediately, the way a posted reaction does
    /// (§FS-004-quick-actions.5) — the reader should not have to refresh to
    /// see the box they just ticked.
    #[test]
    fn a_ticked_box_fills_in_without_a_refresh() {
        let (mut screen, _) = rendered(checklist(), 80);
        screen.tick_local(1);
        screen.rebuild_lines(80);
        let text = plain_text(&screen.lines);
        assert!(
            text.contains(&"▍ ☑ Considered items in above checklist".to_string()),
            "{text:?}"
        );
        // Ticked once, the key stops being offered.
        screen.handle_key(KeyCode::Char('j'));
        assert!(!screen.footer().contains("t tick"), "{}", screen.footer());
        match screen.handle_key(KeyCode::Char('t')) {
            Action::SetMessage(message) => assert!(message.contains("already"), "{message}"),
            _ => panic!("expected a status message"),
        }
    }

    #[test]
    fn ticking_a_message_that_is_not_a_task_says_so() {
        let (mut screen, _) = rendered(checklist(), 80);
        match screen.handle_key(KeyCode::Char('t')) {
            Action::SetMessage(message) => assert!(message.contains("not a task"), "{message}"),
            _ => panic!("expected a status message"),
        }
    }
}
