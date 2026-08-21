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

use chrono::{DateTime, Utc};

use crate::feed::model::Item;
use crate::feed::render::{age, span};
use crate::work::recipe::Recipe;
use crate::work::WorkStatus;

use super::Action;

/// A recipe as this screen shows it: what it is, and the words it would
/// actually send about this item (§FS-005-dispatch.7).
pub(crate) struct Offer {
    pub recipe: Recipe,
    pub brief: String,
    /// Who would get it, resolved when the screen opened — the same sentence
    /// the menu's entry carries (§FS-005-dispatch.14). None where nothing
    /// can say: the item cannot be placed, or there is no dispatcher.
    pub hand: Option<String>,
}

pub(crate) struct WorkScreen {
    pub item: Item,
    status: Option<WorkStatus>,
    offers: Vec<Offer>,
    /// Why nothing here can run a plan, where nothing can — the runtime rung's
    /// own sentence (§AR-005-capabilities.2), handed in because the screen has
    /// to know before it advertises the key, not after it is pressed. It
    /// withholds `c` too: cancelling is the runtime's move
    /// (§FS-005-dispatch.16), and nobody is there to make it.
    refusal: Option<String>,
    selected: usize,
    /// The cursor over the open tickets while the reader is choosing one to
    /// take back (§FS-005-dispatch.16); None otherwise. An index into
    /// [`WorkScreen::cancellable`], not into the tickets.
    picking: Option<usize>,
    /// What ephor has run about this item itself, newest first
    /// (§FS-005-dispatch.17). A job's record stays with the item it was about
    /// after it has left the board, which is what makes "what happened when I
    /// pressed that" answerable an hour later.
    jobs: Vec<crate::seams::jobs::Job>,
    /// Tickets that are over are collected behind one line until the reader
    /// asks for them (§FS-005-dispatch.18). A reading, never a change to the
    /// plan: `z` shows every one of them, in their place in the order.
    folded: bool,
    scroll: u16,
    viewport: u16,
}

impl WorkScreen {
    pub fn new(
        item: Item,
        status: Option<WorkStatus>,
        offers: Vec<Offer>,
        refusal: Option<String>,
        jobs: Vec<crate::seams::jobs::Job>,
    ) -> Self {
        WorkScreen {
            item,
            status,
            offers,
            refusal,
            selected: 0,
            picking: None,
            jobs,
            folded: true,
            scroll: 0,
            viewport: 0,
        }
    }

    /// The tickets `c` may take back: every one that is not over
    /// (§FS-005-dispatch.16). What a live run holds is refused when it is
    /// asked for, in the dispatcher's words — the screen does not read the
    /// journal to hide a row.
    fn cancellable(&self) -> Vec<&crate::work::TicketStatus> {
        self.status
            .iter()
            .flat_map(|status| status.tickets.iter())
            .filter(|ticket| !ticket.finished)
            .collect()
    }

    /// Tickets that are over — finished, or taken back, which is finished too
    /// (§FS-005-dispatch.16). What the fold collects.
    fn over(&self) -> usize {
        self.status
            .iter()
            .flat_map(|status| status.tickets.iter())
            .filter(|ticket| ticket.finished)
            .count()
    }

    /// Whether the fold is actually folding anything right now. A plan with
    /// nothing open is shown whole: the fold is for what is over *beside*
    /// what is not, and folding every row away would leave a heading over an
    /// empty space (§FS-005-dispatch.18).
    fn folding(&self) -> bool {
        let over = self.over();
        let all = self
            .status
            .as_ref()
            .map(|status| status.tickets.len())
            .unwrap_or(0);
        self.folded && over > 0 && over < all
    }

    pub fn title(&self) -> String {
        let title: String = self.item.title.chars().take(60).collect();
        format!(" ephor — work — {title}")
    }

    /// The keys this screen can act on. `R` is dropped where no runtime is
    /// bound: writing the ticket and running it are different capabilities,
    /// and a footer that teaches a key nothing can answer spends the reader's
    /// keystroke to refuse them (§FS-004-quick-actions.2). Everything else on
    /// this screen goes on working with nothing bound — the plan is written,
    /// read, reopened and edited either way (§FS-005-dispatch lead).
    pub fn footer(&self) -> String {
        if self.picking.is_some() {
            return " j/k choose  enter/1-9 cancel that ticket  esc keep them".to_string();
        }
        let keys = match (self.refusal.is_some(), self.jobs.is_empty()) {
            (true, true) => " j/k move  enter/1-9 open work  a ask  s reopen  e read the plan",
            (true, false) => " j/k move  enter/1-9 open work  a ask  s reopen  e plan  L job log",
            (false, true) => " j/k move  enter/1-9 open work  a ask  s reopen  c cancel  R run the runtime  e read the plan",
            (false, false) => " j/k move  enter/1-9 open work  a ask  s reopen  c cancel  R run  e plan  L job log",
        };
        // `z` is taught only where there is a fold to work, since a key that
        // does nothing visible spends the reader's keystroke to teach them
        // nothing (§FS-004-quick-actions.2).
        let fold = match (self.folding(), self.folded, self.over()) {
            (true, _, _) => "  z the ones that are over",
            (false, false, over) if over > 0 => "  z folds them back",
            _ => "",
        };
        format!("{keys}{fold}  o browser  ; ops  esc back")
    }

    fn plan(&self) -> Option<PathBuf> {
        self.status.as_ref().map(|status| status.plan.clone())
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        // Choosing a ticket to take back is the screen's second level: while
        // it is open, the keys are its (§FS-005-dispatch.16).
        if self.picking.is_some() {
            return self.pick_key(code);
        }
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
            // Show the tickets that are over, or put them back
            // (§FS-005-dispatch.18). The plan is untouched either way — this
            // key changes what is read, never what is recorded.
            KeyCode::Char('z') => {
                self.folded = !self.folded;
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
            // the same checkout. Refused here rather than by handing the
            // terminal over to a command that cannot start: the footer already
            // stopped teaching the key, and this is what answers a reader who
            // knew it anyway (§AR-005-capabilities.2).
            KeyCode::Char('R') => match (&self.refusal, &self.status) {
                (Some(refusal), _) => Action::SetMessage(refusal.clone()),
                (None, Some(status)) => Action::RunWork {
                    item: self.item.id.clone(),
                    root: status.root.clone(),
                    checkout: status.checkout.clone(),
                    plan_id: status.plan_id.clone(),
                    label: self.item.title.clone(),
                },
                (None, None) => Action::SetMessage("No work to run yet".to_string()),
            },
            KeyCode::Char('e') => match self.plan() {
                Some(plan) => Action::ReadPlan(plan),
                None => Action::SetMessage("No plan yet".to_string()),
            },
            // What ephor last ran here itself (§FS-005-dispatch.17). The
            // newest, because that is the one a reader who just pressed a key
            // is asking about; the rest are listed above with what they came
            // to, and `ephor job log` reads any of them by name.
            KeyCode::Char('L') => match self.jobs.first() {
                Some(job) => Action::ReadLog {
                    path: job.log_path(),
                    following: job.live,
                },
                None => Action::SetMessage("ephor has run nothing here itself".to_string()),
            },
            // Anything the recipes do not cover (§FS-005-dispatch.10).
            KeyCode::Char('a') => Action::AskWork(self.item.clone()),
            // Take a ticket back (§FS-005-dispatch.16). The move is the
            // runtime's, so with nothing bound the key answers with the rung's
            // sentence, as `R` does; with one open ticket the choice is still
            // shown, since a cancel is a keystroke worth seeing land.
            KeyCode::Char('c') => match &self.refusal {
                Some(refusal) => Action::SetMessage(refusal.clone()),
                None if self.cancellable().is_empty() => {
                    Action::SetMessage("Nothing to cancel — no ticket here is open".to_string())
                }
                None => {
                    self.picking = Some(0);
                    Action::None
                }
            },
            KeyCode::Char('o') => Action::OpenUrl(self.item.url.clone()),
            _ => Action::None,
        }
    }

    /// One key while a ticket is being chosen to take back: `j`/`k` and the
    /// digits move and pick, Enter takes the chosen one, Esc keeps them all.
    fn pick_key(&mut self, code: KeyCode) -> Action {
        let count = self.cancellable().len();
        let Some(at) = self.picking.as_mut() else {
            return Action::None;
        };
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') | KeyCode::Backspace => {
                self.picking = None;
                Action::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if *at + 1 < count {
                    *at += 1;
                }
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                *at = at.saturating_sub(1);
                Action::None
            }
            KeyCode::Enter => self.cancel(self.picking.unwrap_or(0)),
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                self.cancel((digit as usize).wrapping_sub('1' as usize))
            }
            _ => Action::None,
        }
    }

    /// The chosen ticket goes to the shell for its reason; the choice is
    /// spent whether or not the reader follows through.
    fn cancel(&mut self, index: usize) -> Action {
        let chosen = self
            .cancellable()
            .get(index)
            .map(|ticket| ticket.id.clone());
        match chosen {
            Some(ticket) => {
                self.picking = None;
                Action::CancelWork {
                    item: self.item.clone(),
                    ticket,
                }
            }
            None => Action::None,
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
        // One reading of the clock for the whole screen, so two rows a
        // millisecond apart do not disagree about what time it is.
        let now = Utc::now();
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
                lines.push(Line::from(vec![
                    Span::styled("  what has been asked for".to_string(), heading),
                    Span::styled(
                        match self.picking {
                            Some(_) => "   — which one to take back?".to_string(),
                            None => String::new(),
                        },
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
                // While a ticket is being chosen, the open ones wear the
                // cursor and a number, in the order the digits pick them.
                let mut open_index = 0usize;
                let folding = self.folding();
                for ticket in &status.tickets {
                    if folding && ticket.finished {
                        continue;
                    }
                    let (marker, style) = if ticket.cancelled {
                        ("⊘", dim)
                    } else if ticket.waiting {
                        (
                            "⚠",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        )
                    } else if ticket.finished {
                        ("✓", Style::default().fg(Color::Green))
                    } else {
                        ("⚙", Style::default().fg(Color::Yellow))
                    };
                    let cursor = match (self.picking, ticket.finished) {
                        (Some(at), false) => {
                            open_index += 1;
                            format!(
                                " {} {} ",
                                if at + 1 == open_index { "▸" } else { " " },
                                open_index
                            )
                        }
                        (Some(_), true) => "     ".to_string(),
                        (None, _) => "    ".to_string(),
                    };
                    let title_style = match (self.picking, ticket.finished) {
                        (Some(at), false) if at + 1 == open_index => {
                            Style::default().add_modifier(Modifier::BOLD)
                        }
                        _ => Style::default(),
                    };
                    let mut spans = vec![
                        Span::styled(cursor, dim),
                        Span::styled(format!("{marker} "), style),
                        Span::styled(format!("{:<14}", ticket.id), title_style),
                        Span::styled(shorten(&ticket.title, &self.item.title), title_style),
                        Span::styled(
                            format!("  [{}]", ticket.state.as_deref().unwrap_or("?")),
                            dim,
                        ),
                    ];
                    // When it was asked for, where anything knows — a ticket
                    // ephor did not dispatch carries no age rather than a
                    // guessed one (§FS-005-dispatch.18).
                    if let Some(asked) = ticket.asked {
                        spans.push(Span::styled(format!("   asked {}", ago(now, asked)), dim));
                    }
                    lines.push(Line::from(spans));
                    if let Some(verdict) = &ticket.verdict {
                        lines.push(Line::from(Span::styled(
                            format!("        {verdict}"),
                            Style::default().fg(Color::Cyan),
                        )));
                    }
                }
                if folding {
                    let over = self.over();
                    lines.push(Line::from(Span::styled(
                        format!(
                            "    … {over} more that {} over — z shows {}",
                            if over == 1 { "is" } else { "are" },
                            if over == 1 { "it" } else { "them" }
                        ),
                        dim,
                    )));
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
                    if let Some(advance) = &status.advance {
                        lines.push(Line::from(Span::styled(format!("    {advance}"), dim)));
                    }
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

        // What ephor ran here itself (§FS-005-dispatch.17). Above the recipes
        // and below the tickets, because it sits between the two in kind: a
        // move already made, by nobody who had to be asked.
        if !self.jobs.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  what ephor ran here".to_string(),
                heading,
            )));
            for job in &self.jobs {
                let (marker, style) = match (&job.ended, job.live) {
                    (_, true) => ("⚙", Style::default().fg(Color::Yellow)),
                    (Some(ended), false) if ended.outcome == "done" => {
                        ("✓", Style::default().fg(Color::Green))
                    }
                    (Some(_), false) => ("✗", Style::default().fg(Color::Red)),
                    // No lock and no outcome: the supervisor died, which is
                    // neither success nor failure and is not reported as either.
                    (None, false) => ("✗", Style::default().fg(Color::Red)),
                };
                let mut spans = vec![
                    Span::styled(format!("    {marker} "), style),
                    Span::raw(format!("{} {}", job.record.icon, job.record.description)),
                ];
                // When it ran, and for how long (§FS-005-dispatch.18): the
                // difference between the job the reader started a minute ago
                // and the one from yesterday that says the same words.
                if let Some(when) = ran(now, job) {
                    spans.push(Span::styled(format!("   {when}"), dim));
                }
                lines.push(Line::from(spans));
                lines.push(Line::from(Span::styled(
                    format!("      {}", job.says()),
                    dim,
                )));
            }
            lines.push(Line::from(Span::styled(
                "    L reads the newest one".to_string(),
                dim,
            )));
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
            // Who would get this work, before the key is pressed — the same
            // sentence the menu shows (§FS-005-dispatch.14).
            if let Some(hand) = &offer.hand {
                spans.push(Span::styled(
                    format!("  → {hand}"),
                    Style::default().fg(Color::Cyan),
                ));
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

/// An age as a row says it (§FS-005-dispatch.18). The watch spells the first
/// minute "now", and "now ago" is not a time.
fn ago(now: DateTime<Utc>, then: DateTime<Utc>) -> String {
    match age(now, then).as_str() {
        "now" => "just now".to_string(),
        age => format!("{age} ago"),
    }
}

/// When a job ran, and for how long — or how long it has been going, which is
/// the same question asked of a thing that has not finished
/// (§FS-005-dispatch.18). None where the record carries no readable stamp: a
/// row that says nothing is better than one that dates a job by a guess.
fn ran(now: DateTime<Utc>, job: &crate::seams::jobs::Job) -> Option<String> {
    let started = job.started_at()?;
    if job.live {
        return Some(format!("going {}", span(job.took(now).unwrap_or(0))));
    }
    // A job whose supervisor died has an end nothing recorded, so it is said
    // by its beginning: how long it lasted is exactly the unknown.
    if job.died() {
        return Some(format!("started {}", ago(now, started)));
    }
    let ended = job.ended_at().unwrap_or(started);
    Some(match job.took(now) {
        Some(took) => format!("ran {}, took {}", ago(now, ended), span(took)),
        None => format!("ran {}", ago(now, ended)),
    })
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
            plan_id: "forge-demo-17".to_string(),
            checkout: PathBuf::from("/w/demo"),
            plan: PathBuf::from("/w/demo/panta/forge-demo-17.rhei.md"),
            missing: false,
            tickets: vec![TicketStatus {
                id: "fix-gate-1".to_string(),
                recipe: "fix-gate".to_string(),
                title: "fix the red gate".to_string(),
                state: Some("done".to_string()),
                finished: true,
                cancelled: false,
                waiting: false,
                assignee: None,
                pinned: None,
                verdict: Some("done — the change is right".to_string()),
                asked: Some(Utc::now() - chrono::Duration::hours(3)),
            }],
            changes: if stale {
                vec!["1 new message".to_string()]
            } else {
                Vec::new()
            },
            advance: None,
        }
    }

    /// Offers as the shell builds them: each recipe with its brief already
    /// rendered against the item, and the hand it would go to resolved.
    fn offers() -> Vec<Offer> {
        crate::work::recipe::shipped()
            .into_iter()
            .map(|recipe| Offer {
                brief: recipe.brief.replace("{title}", "Humanize durations"),
                hand: Some("luna at high".to_string()),
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
        let screen = WorkScreen::new(item(), Some(status(true)), offers(), None, Vec::new());
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
        // And who each offer would go to, beside it — the same sentence the
        // menu's entry carries (§FS-005-dispatch.14).
        assert!(text.contains("→ luna at high"), "{text}");
    }

    #[test]
    fn an_item_with_no_work_still_shows_what_could_be_asked_for() {
        let screen = WorkScreen::new(item(), None, offers(), None, Vec::new());
        let text = text(&screen);
        assert!(text.contains("nothing has been handed over"), "{text}");
        assert!(text.contains("what can be asked for"), "{text}");
    }

    #[test]
    fn keys_dispatch_by_number_and_refuse_what_there_is_nothing_to_do() {
        let ids: Vec<String> = offers().into_iter().map(|o| o.recipe.id).collect();
        let mut screen = WorkScreen::new(item(), None, offers(), None, Vec::new());
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
        let mut screen = WorkScreen::new(item(), Some(status(false)), offers(), None, Vec::new());
        assert!(matches!(
            screen.handle_key(KeyCode::Char('s')),
            Action::SetMessage(_)
        ));
        // The run names this item's own plan, and names the item too: the
        // hand riding the run is resolved from that item's ledger entry
        // (§FS-005-dispatch.14).
        match screen.handle_key(KeyCode::Char('R')) {
            Action::RunWork { item, plan_id, .. } => {
                assert_eq!(item, "forge:demo/17");
                assert_eq!(plan_id, "forge-demo-17");
            }
            _ => panic!("expected a run"),
        }
    }

    /// A status with two open tickets and one taken back, as the plan would
    /// read after a cancel (§FS-005-dispatch.16).
    fn status_with_open_tickets() -> WorkStatus {
        let mut status = status(false);
        let open = |id: &str, state: &str| TicketStatus {
            id: id.to_string(),
            recipe: "fix-gate".to_string(),
            title: "fix the red gate".to_string(),
            state: Some(state.to_string()),
            finished: false,
            cancelled: false,
            waiting: false,
            assignee: None,
            pinned: None,
            verdict: None,
            asked: Some(Utc::now() - chrono::Duration::minutes(12)),
        };
        status.tickets = vec![
            open("fix-gate-1", "collect"),
            TicketStatus {
                id: "fix-gate-2".to_string(),
                state: Some("cancelled".to_string()),
                finished: true,
                cancelled: true,
                verdict: Some("asked twice by mistake".to_string()),
                ..open("fix-gate-2", "cancelled")
            },
            open("fix-gate-3", "collect"),
        ];
        status
    }

    /// `c` chooses among the open tickets — never the finished or the taken
    /// back — by cursor or digit, and hands the choice to the shell for its
    /// reason; Esc keeps them all. A ticket taken back reads as such, with
    /// its reason beneath it (§FS-005-dispatch.16).
    #[test]
    fn c_picks_an_open_ticket_to_take_back_and_a_cancelled_one_reads_as_such() {
        let mut screen = WorkScreen::new(
            item(),
            Some(status_with_open_tickets()),
            offers(),
            None,
            Vec::new(),
        );
        // It is over, so it is folded away until asked for
        // (§FS-005-dispatch.18) — and then reads as taken back, in its place
        // in the order, with its reason beneath it.
        let shown = text(&screen);
        assert!(!shown.contains("⊘ fix-gate-2"), "{shown}");
        assert!(shown.contains("1 more that is over"), "{shown}");
        assert!(matches!(
            screen.handle_key(KeyCode::Char('z')),
            Action::None
        ));
        let shown = text(&screen);
        assert!(shown.contains("⊘ fix-gate-2"), "{shown}");
        assert!(shown.contains("[cancelled]"), "{shown}");
        assert!(shown.contains("asked twice by mistake"), "{shown}");
        assert!(screen.footer().contains("c cancel"), "{}", screen.footer());

        assert!(matches!(
            screen.handle_key(KeyCode::Char('c')),
            Action::None
        ));
        assert!(
            screen.footer().contains("cancel that ticket"),
            "{}",
            screen.footer()
        );
        let shown = text(&screen);
        assert!(shown.contains("which one to take back?"), "{shown}");
        // The open tickets are numbered in order; the cancelled one is not.
        assert!(shown.contains("▸ 1 ⚙ fix-gate-1"), "{shown}");
        assert!(shown.contains("  2 ⚙ fix-gate-3"), "{shown}");
        assert!(!shown.contains(" 3 ⊘"), "{shown}");

        // j moves the cursor over the open ones only, and Enter takes it.
        screen.handle_key(KeyCode::Char('j'));
        match screen.handle_key(KeyCode::Enter) {
            Action::CancelWork { ticket, item } => {
                assert_eq!(ticket, "fix-gate-3");
                assert_eq!(item.id, "forge:demo/17");
            }
            _ => panic!("expected the chosen ticket to go to the shell"),
        }
        // The choice is spent: the keys are the screen's again.
        assert!(!screen.footer().contains("cancel that ticket"));

        // A digit picks directly; Esc keeps them all.
        screen.handle_key(KeyCode::Char('c'));
        match screen.handle_key(KeyCode::Char('1')) {
            Action::CancelWork { ticket, .. } => assert_eq!(ticket, "fix-gate-1"),
            _ => panic!("expected the first open ticket"),
        }
        screen.handle_key(KeyCode::Char('c'));
        assert!(matches!(screen.handle_key(KeyCode::Esc), Action::None));
        assert!(!screen.footer().contains("cancel that ticket"));
        // A digit naming no open ticket picks nothing and stays.
        screen.handle_key(KeyCode::Char('c'));
        assert!(matches!(
            screen.handle_key(KeyCode::Char('9')),
            Action::None
        ));
        assert!(screen.footer().contains("cancel that ticket"));
    }

    /// Nothing open is nothing to take back, said rather than picked from an
    /// empty list; and with no runtime bound the key answers with the rung's
    /// sentence and is not taught, exactly as `R` is (§FS-005-dispatch.16).
    #[test]
    fn c_refuses_where_nothing_is_open_and_where_nothing_can_move_it() {
        let mut screen = WorkScreen::new(item(), Some(status(false)), offers(), None, Vec::new());
        match screen.handle_key(KeyCode::Char('c')) {
            Action::SetMessage(said) => assert!(said.contains("Nothing to cancel"), "{said}"),
            _ => panic!("expected a refusal"),
        }
        let mut screen = WorkScreen::new(item(), None, offers(), None, Vec::new());
        assert!(matches!(
            screen.handle_key(KeyCode::Char('c')),
            Action::SetMessage(_)
        ));

        let unbound =
            "nothing-here is not on PATH; ephor writes the tickets but the runtime runs them."
                .to_string();
        let mut screen = WorkScreen::new(
            item(),
            Some(status_with_open_tickets()),
            offers(),
            Some(unbound.clone()),
            Vec::new(),
        );
        assert!(!screen.footer().contains("c cancel"), "{}", screen.footer());
        match screen.handle_key(KeyCode::Char('c')) {
            Action::SetMessage(said) => assert_eq!(said, unbound),
            _ => panic!("nothing can move it, so nothing is picked"),
        }
    }

    /// With no runtime bound there is nothing `R` could start, so the screen
    /// stops teaching it and answers a reader who knew it anyway with the
    /// rung's own sentence rather than handing the terminal to a command that
    /// cannot run (§FS-004-quick-actions.2, §AR-005-capabilities.2). The rest
    /// of the screen is unchanged: the plan is still written, read and
    /// reopened (§FS-005-dispatch lead).
    #[test]
    fn with_no_runtime_bound_the_run_key_is_neither_offered_nor_pretended() {
        let unbound = crate::work::runtime::refusal(&crate::work::recipe::WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        })
        .expect("a runner that is not on PATH is refused");

        let mut screen = WorkScreen::new(
            item(),
            Some(status(false)),
            offers(),
            Some(unbound.clone()),
            Vec::new(),
        );
        assert!(!screen.footer().contains("R run"), "{}", screen.footer());
        match screen.handle_key(KeyCode::Char('R')) {
            Action::SetMessage(said) => assert_eq!(said, unbound),
            _ => panic!("nothing can run it, so nothing is handed the terminal"),
        }
        // Everything else the screen does is unaffected.
        assert!(matches!(
            screen.handle_key(KeyCode::Char('e')),
            Action::ReadPlan(_)
        ));
        assert!(matches!(
            screen.handle_key(KeyCode::Char('1')),
            Action::DispatchWork { .. }
        ));

        // Bound: the key is advertised and acts.
        let mut bound = WorkScreen::new(item(), Some(status(false)), offers(), None, Vec::new());
        assert!(bound.footer().contains("R run"), "{}", bound.footer());
        assert!(matches!(
            bound.handle_key(KeyCode::Char('R')),
            Action::RunWork { .. }
        ));
    }

    /// A job ephor ran about this item keeps its record here after it has
    /// left the board, with what it came to — which is what makes "what
    /// happened when I pressed that" answerable an hour later
    /// (§FS-005-dispatch.17).
    #[test]
    fn what_ephor_ran_here_stays_with_the_item() {
        let job = crate::seams::jobs::Job {
            id: "20260818T090000.000Z-rebase".to_string(),
            dir: std::path::PathBuf::from("/state/ephor/jobs/20260818T090000.000Z-rebase"),
            record: crate::seams::jobs::Record {
                version: 1,
                project: "demo".to_string(),
                item: Some("forge:demo/17".to_string()),
                icon: "⤴".to_string(),
                description: "rebase onto master (3 behind as of Jul 28)".to_string(),
                root: std::path::PathBuf::from("/w/demo"),
                workspace: None,
                steps: Vec::new(),
                dossier: Vec::new(),
                started: "2026-08-18T09:00:00Z".to_string(),
            },
            live: false,
            ended: Some(crate::seams::jobs::Ended {
                outcome: "done".to_string(),
                exit_code: Some(0),
                step: None,
                says: "⤴ rebase onto master (3 behind as of Jul 28): ok".to_string(),
                ended: "2026-08-18T09:04:00Z".to_string(),
            }),
        };
        let mut screen = WorkScreen::new(item(), None, offers(), None, vec![job]);
        let text = text(&screen);
        assert!(text.contains("what ephor ran here"), "{text}");
        assert!(text.contains("rebase onto master"), "{text}");
        assert!(text.contains(": ok"), "the outcome is on the row: {text}");
        assert!(screen.footer().contains("L job log"), "{}", screen.footer());
        match screen.handle_key(KeyCode::Char('L')) {
            Action::ReadLog { path, following } => {
                assert!(path.ends_with("log"), "{path:?}");
                assert!(!following, "nothing to follow: it is over");
            }
            _ => panic!("L reads the newest job"),
        }
    }

    /// With nothing run here the key is not taught and says so plainly rather
    /// than opening an empty pager.
    #[test]
    fn an_item_ephor_ran_nothing_on_says_so() {
        let mut screen = WorkScreen::new(item(), None, offers(), None, Vec::new());
        assert!(
            !screen.footer().contains("L job log"),
            "{}",
            screen.footer()
        );
        match screen.handle_key(KeyCode::Char('L')) {
            Action::SetMessage(said) => assert!(said.contains("nothing"), "{said}"),
            _ => panic!("nothing to read"),
        }
    }

    /// A job as the jobs directory would have it, dated relative to now so
    /// the assertions read the same on any day.
    fn job_ago(started: i64, ended: Option<i64>, live: bool) -> crate::seams::jobs::Job {
        let at = |minutes: i64| (Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339();
        crate::seams::jobs::Job {
            id: "20260820T090000.000Z-restart".to_string(),
            dir: std::path::PathBuf::from("/state/ephor/jobs/20260820T090000.000Z-restart"),
            record: crate::seams::jobs::Record {
                version: 1,
                project: "demo".to_string(),
                item: Some("forge:demo/17".to_string()),
                icon: "⟳".to_string(),
                description: "restart the whole gate".to_string(),
                root: std::path::PathBuf::from("/w/demo"),
                workspace: None,
                steps: Vec::new(),
                dossier: Vec::new(),
                started: at(started),
            },
            live,
            ended: ended.map(|ended| crate::seams::jobs::Ended {
                outcome: "done".to_string(),
                exit_code: Some(0),
                step: None,
                says: "⟳ restart the whole gate: ok".to_string(),
                ended: at(ended),
            }),
        }
    }

    /// Every row that already happened says when (§FS-005-dispatch.18). The
    /// ticket says when it was **asked for**, which only the ledger knows; the
    /// job says when it **ran** and how long it took, and one still going says
    /// how long it has been going.
    #[test]
    fn the_rows_say_when_they_happened() {
        let screen = WorkScreen::new(
            item(),
            Some(status_with_open_tickets()),
            offers(),
            None,
            vec![job_ago(65, Some(60), false)],
        );
        let shown = text(&screen);
        assert!(shown.contains("fix-gate-1"), "{shown}");
        assert!(shown.contains("asked 12m ago"), "{shown}");
        assert!(shown.contains("ran 1h ago, took 5m"), "{shown}");

        // Going, not gone: the same question asked of a thing that has not
        // finished, and the only answer there is.
        let screen = WorkScreen::new(
            item(),
            Some(status_with_open_tickets()),
            offers(),
            None,
            vec![job_ago(3, None, true)],
        );
        assert!(text(&screen).contains("going 3m"), "{}", text(&screen));

        // A supervisor that died recorded no end, so the row is said by its
        // beginning: how long it lasted is exactly the unknown.
        let screen = WorkScreen::new(
            item(),
            Some(status_with_open_tickets()),
            offers(),
            None,
            vec![job_ago(90, None, false)],
        );
        let shown = text(&screen);
        assert!(shown.contains("started 1h ago"), "{shown}");
        assert!(!shown.contains("took"), "nothing timed it: {shown}");
    }

    /// A ticket ephor did not dispatch — written into the plan by hand, or by
    /// the machine for itself — has no ask for anything to date, so its row
    /// carries no age rather than one the plan's mtime invented
    /// (§FS-005-dispatch.18).
    #[test]
    fn a_ticket_nothing_dispatched_carries_no_age() {
        let mut status = status(false);
        status.tickets[0].asked = None;
        let screen = WorkScreen::new(item(), Some(status), offers(), None, Vec::new());
        let row = text(&screen)
            .lines()
            .find(|line| line.contains("fix-gate-1"))
            .expect("the ticket is on the screen")
            .to_string();
        assert!(!row.contains("asked"), "{row}");
    }

    /// Tickets that are over are collected behind one line until `z` asks for
    /// them, and then read in their place in the plan's order. The plan is
    /// untouched either way: this is a reading of the record
    /// (§FS-005-dispatch.16, §FS-005-dispatch.18).
    #[test]
    fn what_is_over_folds_away_and_z_shows_it() {
        let mut screen = WorkScreen::new(
            item(),
            Some(status_with_open_tickets()),
            offers(),
            None,
            Vec::new(),
        );
        let shown = text(&screen);
        assert!(shown.contains("fix-gate-1"), "{shown}");
        assert!(shown.contains("fix-gate-3"), "{shown}");
        assert!(
            !shown.contains("fix-gate-2"),
            "the cancelled one is over: {shown}"
        );
        assert!(
            shown.contains("… 1 more that is over — z shows it"),
            "{shown}"
        );
        assert!(
            screen.footer().contains("z the ones that are over"),
            "{}",
            screen.footer()
        );

        screen.handle_key(KeyCode::Char('z'));
        let shown = text(&screen);
        let at = |id: &str| {
            shown
                .find(id)
                .unwrap_or_else(|| panic!("{id} is not shown: {shown}"))
        };
        assert!(
            at("fix-gate-1") < at("fix-gate-2"),
            "in place, in the plan's order: {shown}"
        );
        assert!(
            at("fix-gate-2") < at("fix-gate-3"),
            "in place, in the plan's order: {shown}"
        );
        assert!(
            screen.footer().contains("z folds them back"),
            "{}",
            screen.footer()
        );

        // And back.
        screen.handle_key(KeyCode::Char('z'));
        assert!(!text(&screen).contains("fix-gate-2"), "{}", text(&screen));
    }

    /// Where every ticket is over there is nothing for the fold to be beside,
    /// so they are all shown: folding the whole list away would leave a
    /// heading above an empty space (§FS-005-dispatch.18).
    #[test]
    fn a_plan_with_nothing_open_is_not_folded_to_nothing() {
        let screen = WorkScreen::new(item(), Some(status(false)), offers(), None, Vec::new());
        let shown = text(&screen);
        assert!(shown.contains("✓ fix-gate-1"), "{shown}");
        assert!(!shown.contains("more that is over"), "{shown}");
        assert!(!screen.footer().contains(" z "), "{}", screen.footer());
    }
}
