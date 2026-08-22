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
//!
//! A reply a run drafted is shown here too, under the conversation it answers
//! and marked as unsent (§FS-005-dispatch.13): `p` posts it where the channel
//! declares reply (§FS-007-matters.4), `e` opens it for editing first, and
//! where nothing can post it the card is what the reader copies.

use std::ops::Range;
use std::path::PathBuf;

use chrono::Utc;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::feed::model::Item;
use crate::feed::react::PALETTE;
use crate::feed::render::age;
use crate::feed::reply::ReplyTarget;
use crate::work::runtime::results::Proposal;

use super::Action;

/// The walk is the session's (§AR-009-surfaces.1), so the index this screen
/// selects is the index `ephor react` and `ephor tick` take.
use crate::api::conversation::{Conversation, Message as Msg};
use crate::api::views::Reaction;

/// A reply a run drafted, waiting under the conversation it answers
/// (§FS-005-dispatch.13). It is a file until a person sends it, which is why
/// `path` is shown wherever the channel cannot carry it.
struct Draft {
    text: String,
    path: PathBuf,
    /// The thread it belongs under: the last one that can carry a reply, or
    /// the last one there is.
    thread: usize,
    /// How to send it, where the channel declared that it can be sent
    /// (§FS-007-matters.4).
    target: Option<ReplyTarget>,
    posted: bool,
}

pub(crate) struct ThreadScreen {
    item: Item,
    messages: Vec<Msg>,
    /// The reply a run proposed about this matter, where one is waiting.
    draft: Option<Draft>,
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
    /// None when the item has no recorded thread messages. `proposal` is the
    /// reply a run drafted about this matter, where one is waiting
    /// (§FS-005-dispatch.13).
    pub fn open(item: Item, proposal: Option<Proposal>) -> Option<Self> {
        let Conversation { messages, draft } = Conversation::of(&item, proposal);
        if messages.is_empty() {
            return None;
        }
        let draft = draft.map(|draft| Draft {
            text: draft.text,
            path: draft.path,
            thread: draft.thread,
            target: draft.target,
            posted: false,
        });
        Some(ThreadScreen {
            item,
            messages,
            draft,
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

    /// Take the proposal again after the reader edited it, so the card shows
    /// what would actually be posted.
    pub fn reread(&mut self, proposal: Option<Proposal>) {
        match (proposal, &mut self.draft) {
            (Some(proposal), Some(draft)) => {
                draft.text = proposal.text;
                draft.path = proposal.path;
            }
            // Edited to nothing, or already posted from elsewhere: a proposal
            // that is no longer there is no longer offered.
            (None, _) => self.draft = None,
            (Some(_), None) => {}
        }
        self.wrap_width = 0;
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
        // Offered where the channel declared it can carry a reply, and
        // nowhere else (§FS-007-matters.4): on a channel that cannot, the
        // draft is copy material and teaching a key for it would be a
        // keystroke spent to be refused.
        if let Some(draft) = self.draft.as_ref().filter(|draft| !draft.posted) {
            keys.push_str("  e edit reply");
            if draft.target.is_some() {
                keys.push_str("  p post reply");
            }
        }
        keys.push_str("  x actions  o open  m done  ; ops  esc back");
        keys
    }

    /// Whether this screen has something of its own open over it. The shell
    /// intercepts `;` for every screen (§FS-005-dispatch.15), and the picker
    /// is a modal the reader is inside: opening the board over it would leave
    /// it armed underneath, so the next `enter` after coming back posts a
    /// reaction the reader stopped meaning to.
    pub fn is_picking(&self) -> bool {
        self.picker.is_some()
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
            KeyCode::Char('p') => self.post_reply(),
            KeyCode::Char('e') => self.edit_reply(),
            _ => Action::None,
        }
    }

    /// Post the drafted reply as it stands (§FS-005-dispatch.13). One
    /// deliberate move: nothing here posts on its own, and the refusals name
    /// the channel rather than the key.
    fn post_reply(&mut self) -> Action {
        match &self.draft {
            None => Action::SetMessage("No reply has been drafted for this".to_string()),
            Some(draft) if draft.posted => {
                Action::SetMessage("This reply has already been posted".to_string())
            }
            // The move re-reads the draft as it now stands and sends *that*
            // (§FS-005-dispatch.13), so nothing is carried from here but the
            // matter: a screen that handed over the text it was showing would
            // send an edit the reader had since made to a file, or one they
            // had since undone.
            Some(draft) => match &draft.target {
                Some(_) => Action::PostReply {
                    item: self.item.clone(),
                },
                None => Action::SetMessage(format!(
                    "This conversation cannot be replied to from here — the draft is at {}",
                    draft.path.display()
                )),
            },
        }
    }

    /// Open the draft in the reader's editor before it goes anywhere: posted
    /// edited or as it stands is the reader's call (§FS-005-dispatch.13).
    fn edit_reply(&mut self) -> Action {
        match &self.draft {
            Some(draft) if !draft.posted => Action::EditReply {
                path: draft.path.clone(),
                item: self.item.clone(),
            },
            Some(_) => Action::SetMessage("This reply has already been posted".to_string()),
            None => Action::SetMessage("No reply has been drafted for this".to_string()),
        }
    }

    /// Record a posted reply without waiting for a refresh, the way a posted
    /// reaction is recorded — and stop offering to post it again.
    pub fn reply_posted(&mut self) {
        if let Some(draft) = &mut self.draft {
            draft.posted = true;
        }
        self.wrap_width = 0;
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
            Some(_) => Action::ResolveTask {
                item: self.item.clone(),
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
        // The screen still asks whether the message carries a way to react, so
        // that closing the picker on one that does not is silent rather than a
        // refusal the reader did not ask for; the move re-derives the target
        // itself from the same walk (§AR-009-surfaces.1).
        if self.messages[self.selected].react.is_none() {
            return Action::None;
        }
        let (emoji, content) = PALETTE[pick];
        Action::React {
            item: self.item.clone(),
            message: self.selected,
            content,
            emoji,
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

            // Under the last message of the conversation it answers, which is
            // where a reader looks for what to say next (§FS-005-dispatch.13).
            let last_of_thread = self
                .messages
                .get(index + 1)
                .map(|next| next.thread != msg.thread)
                .unwrap_or(true);
            if last_of_thread {
                if let Some(draft) = self
                    .draft
                    .as_ref()
                    .filter(|draft| draft.thread == msg.thread)
                {
                    draft_lines(draft, wrap_width, &mut self.lines);
                }
            }
        }
    }
}

/// The draft as a card of its own: marked as unsent, with the one thing the
/// reader can do about it here — post it, or copy it from where it sits
/// (§FS-005-dispatch.13, §REQ-001-boundary.1).
fn draft_lines(draft: &Draft, wrap_width: usize, lines: &mut Vec<Line<'static>>) {
    let color = if draft.posted {
        Color::Green
    } else {
        Color::Magenta
    };
    let gutter = || Span::styled("▍ ", Style::default().fg(color));
    let banner = match (draft.posted, draft.target.is_some()) {
        (true, _) => "posted".to_string(),
        (false, true) => "proposed reply — not posted".to_string(),
        // A channel that declared no reply gets the honest half-offer: the
        // words are here, sending them is somewhere else.
        (false, false) => "proposed reply — this channel takes none from here".to_string(),
    };
    lines.push(Line::from(vec![
        gutter(),
        Span::styled(
            banner,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]));
    for text_line in draft.text.lines() {
        for wrapped in wrap_line(text_line, wrap_width) {
            lines.push(Line::from(vec![gutter(), Span::raw(wrapped)]));
        }
    }
    if !draft.posted {
        let hint = match draft.target.is_some() {
            true => format!("p posts it · e edits it first · {}", draft.path.display()),
            false => format!("e edits it · copy it from {}", draft.path.display()),
        };
        lines.push(Line::from(vec![
            gutter(),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]));
    }
    lines.push(Line::default());
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
    use serde_json::Value;

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
        with_proposal(threads, None, width)
    }

    /// The same screen, with a reply a run drafted waiting under it.
    fn with_proposal(
        threads: Value,
        proposal: Option<Proposal>,
        width: u16,
    ) -> (ThreadScreen, Vec<String>) {
        let mut screen = ThreadScreen::open(item_with_threads(threads), proposal).unwrap();
        screen.rebuild_lines(width);
        let text = plain_text(&screen.lines);
        (screen, text)
    }

    fn proposal(text: &str) -> Proposal {
        Proposal {
            text: text.to_string(),
            path: PathBuf::from("/w/widget/panta/runtime/ephor/widget-77.reply.md"),
        }
    }

    #[test]
    fn open_returns_none_without_messages() {
        assert!(ThreadScreen::open(item_with_threads(json!([])), None).is_none());
        assert!(ThreadScreen::open(item_with_threads(json!([{ "messages": [] }])), None).is_none());
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
            // Which task that is, and how the source is asked to tick it, is
            // the move's to resolve from the same walk this screen selected in
            // (§AR-009-surfaces.1); the key owes the matter and the index.
            Action::ResolveTask { item, message } => {
                assert_eq!(item.id, "github-prs:acme/widget#77");
                assert_eq!(message, 1);
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

    /// A conversation the venue can carry a reply into, as a source that
    /// declares it reports one (§FS-007-matters.4).
    fn answerable() -> Value {
        json!([{
            "messages": [{ "author": "ada", "text": "does the retry window reset?" }],
            "reply": { "provider": "github", "subject_id": "PR_1" },
        }])
    }

    /// The proposal is shown under the conversation it answers, marked as
    /// unsent, and the key to post it is offered because the channel declared
    /// it can carry one (§FS-005-dispatch.13).
    #[test]
    fn a_drafted_reply_waits_under_the_conversation_and_offers_the_post_key() {
        let (mut screen, text) = with_proposal(
            answerable(),
            Some(proposal("Yes — it resets per attempt.")),
            80,
        );
        assert!(
            text.iter()
                .any(|line| line.contains("proposed reply — not posted")),
            "{text:?}"
        );
        assert!(
            text.contains(&"▍ Yes — it resets per attempt.".to_string()),
            "{text:?}"
        );
        // After the message it answers, not before it.
        let message = text.iter().position(|line| line.contains("retry window"));
        let card = text.iter().position(|line| line.contains("proposed reply"));
        assert!(message < card, "{text:?}");
        assert!(
            screen.footer().contains("p post reply"),
            "{}",
            screen.footer()
        );
        assert!(
            screen.footer().contains("e edit reply"),
            "{}",
            screen.footer()
        );

        // The key asks the API to send this matter's draft; what it resolves
        // the words and the target to is the move's, tested where the move is
        // (§AR-009-surfaces.1). What this screen owes is that the key reaches
        // the move at all, and about the right matter.
        match screen.handle_key(KeyCode::Char('p')) {
            Action::PostReply { item } => assert_eq!(item.id, "github-prs:acme/widget#77"),
            _ => panic!("expected a reply to be posted"),
        }

        // Posted once: the card says so and the key stops being offered, so
        // the same words cannot go out twice (§FS-005-dispatch.13).
        screen.reply_posted();
        screen.rebuild_lines(80);
        let text = plain_text(&screen.lines);
        assert!(
            text.iter().any(|line| line.contains("▍ posted")),
            "{text:?}"
        );
        assert!(
            !screen.footer().contains("p post reply"),
            "{}",
            screen.footer()
        );
        assert!(matches!(
            screen.handle_key(KeyCode::Char('p')),
            Action::SetMessage(_)
        ));
    }

    /// A channel that declared no reply is a stated degrade, not a failure
    /// (§REQ-001-boundary.1): the words are still here, and where they sit is
    /// what the reader copies from.
    #[test]
    fn a_channel_that_cannot_carry_a_reply_offers_the_draft_as_copy_material() {
        let threads = json!([{ "messages": [{ "author": "ada", "text": "why?" }] }]);
        let (mut screen, text) =
            with_proposal(threads, Some(proposal("Because of the retry.")), 80);
        assert!(
            text.iter()
                .any(|line| line.contains("this channel takes none from here")),
            "{text:?}"
        );
        assert!(
            text.iter()
                .any(|line| line.contains("copy it from /w/widget")),
            "{text:?}"
        );
        assert!(
            !screen.footer().contains("p post reply"),
            "{}",
            screen.footer()
        );
        // Pressing it anyway says where the words are rather than what key
        // this is.
        match screen.handle_key(KeyCode::Char('p')) {
            Action::SetMessage(message) => {
                assert!(message.contains("widget-77.reply.md"), "{message}")
            }
            _ => panic!("expected a status message"),
        }
        // Editing is still offered: the draft is a file wherever it can go.
        assert!(
            screen.footer().contains("e edit reply"),
            "{}",
            screen.footer()
        );
        assert!(matches!(
            screen.handle_key(KeyCode::Char('e')),
            Action::EditReply { .. }
        ));
    }

    /// With several conversations, the draft belongs under the one that can
    /// carry it (§FS-007-matters.4).
    #[test]
    fn the_draft_lands_under_the_conversation_that_can_carry_it() {
        let threads = json!([
            { "messages": [{ "author": "ada", "text": "on the diff" }] },
            {
                "messages": [{ "author": "bo", "text": "in the conversation" }],
                "reply": { "provider": "github", "subject_id": "PR_1" },
            },
        ]);
        let (_, text) = with_proposal(threads, Some(proposal("answered")), 80);
        let diff = text.iter().position(|l| l.contains("on the diff")).unwrap();
        let card = text
            .iter()
            .position(|l| l.contains("proposed reply"))
            .unwrap();
        let convo = text
            .iter()
            .position(|l| l.contains("in the conversation"))
            .unwrap();
        assert!(diff < convo && convo < card, "{text:?}");
    }

    /// Edited to nothing is withdrawn: what is on disk is what would be
    /// posted, and there is nothing there.
    #[test]
    fn a_draft_edited_away_stops_being_offered() {
        let (mut screen, _) = with_proposal(answerable(), Some(proposal("draft")), 80);
        screen.reread(None);
        screen.rebuild_lines(80);
        let text = plain_text(&screen.lines);
        assert!(
            !text.iter().any(|line| line.contains("proposed reply")),
            "{text:?}"
        );
        assert!(
            !screen.footer().contains("p post reply"),
            "{}",
            screen.footer()
        );

        // Edited to something else: that is what goes out.
        let (mut screen, _) = with_proposal(answerable(), Some(proposal("draft")), 80);
        screen.reread(Some(proposal("what I actually want to say")));
        screen.rebuild_lines(80);
        match screen.handle_key(KeyCode::Char('p')) {
            Action::PostReply { item } => assert_eq!(item.id, "github-prs:acme/widget#77"),
            _ => panic!("expected a reply to be posted"),
        }
    }
}
