//! Answering a workflow's inputs, on one screen (§FS-005-dispatch.19).
//!
//! Every input the workflow declares is a row carrying the answer the five
//! steps reached and the step it came from — the defaults included, because
//! the answers ephor resolved are as likely to be the wrong ones for this run
//! as the missing ones, and a screen that showed only the holes would be
//! asking the reader to accept the rest unseen.
//!
//! Where the values an input can take are known, the row is chosen from them
//! rather than spelled: a flag has two, an input that names who does the work
//! has the roster (§FS-005-dispatch.14, §DA-006-hands-fill-a-workflows-targets),
//! and an input whose own check spells out a set has that set. Everything
//! else is one line typed on its row, and what no row can carry — a record,
//! or a list of them — is the reader's editor.
//!
//! What is decided here is presentation and nothing else. Which inputs there
//! are, what each resolved to, and whether the thing can be laid down at all
//! is the session's answer (§AR-009-surfaces.1): this screen shows it,
//! collects what the reader says, and hands each answer back as the `--set`
//! pair the command line takes, so that a workflow laid down from here and
//! one laid down from a command are the same call (§REQ-002-parity.2).

use std::collections::BTreeMap;

use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::feed::config::ActionConfig;
use crate::feed::model::Item;
use crate::work::recipe::HandList;
use crate::work::runtime::roster::Hand;
use crate::work::runtime::workflow::Kind;
use crate::work::workflow::From;
use crate::work::Laying;

use super::highlight_style;

/// What the reader did here, for the shell to carry out. The screen writes no
/// files and asks the runtime for nothing itself: laying a workflow down is
/// the session's move, from the one place that makes it (§AR-009-surfaces.1).
pub(crate) enum AnswerOutcome {
    Stay,
    /// Leave without laying anything down.
    Close,
    /// One input answered here, for this laying alone — the value spelled the
    /// way `--set <input>=<value>` spells it.
    Set {
        input: String,
        value: String,
    },
    /// Hand the whole set of answers to the reader's editor, which is where
    /// what no row can carry is answered (§FS-005-dispatch.19).
    Edit,
    /// What the binding would write, asked for before it writes it.
    Preview,
    /// Lay it down.
    Lay,
}

/// One input on the screen: what it wants, what stands in it, and where that
/// answer came from.
#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub input: String,
    pub says: String,
    pub kind: Kind,
    pub required: bool,
    /// The answer standing in it, on one line.
    pub shown: String,
    pub from: From,
    /// The values it is chosen from, where they are a known set — the
    /// element's set on a list, since that is what is picked.
    pub choices: Vec<String>,
    /// It names who does the work, so the roster is what it is chosen from
    /// (§DA-006-hands-fill-a-workflows-targets).
    pub hand: bool,
    /// Several are answered at once, and what is written is a list of them.
    pub several: bool,
}

impl Row {
    /// Whether a line of typing can answer this at all. A record cannot be
    /// typed on one, and neither can a list of them — those go to the editor.
    fn typeable(&self) -> bool {
        match self.kind {
            Kind::Record => false,
            Kind::List => !self.choices.is_empty() || self.hand,
            _ => true,
        }
    }

    /// What this row is chosen from, where it is chosen rather than typed.
    /// The roster is not here: a hand's row is filled from the roster the
    /// screen was given, which knows what each hand resolves to.
    fn set(&self) -> Vec<String> {
        match self.kind {
            Kind::Flag => vec!["true".to_string(), "false".to_string()],
            _ => self.choices.clone(),
        }
    }

    /// How the row says what it wants, beside the value.
    fn wants(&self) -> String {
        if self.hand {
            return match self.several {
                true => "several hands".to_string(),
                false => "hand".to_string(),
            };
        }
        if !self.choices.is_empty() {
            return match self.several {
                true => format!("several of {}", self.choices.len()),
                false => format!("one of {}", self.choices.len()),
            };
        }
        self.kind.label().to_string()
    }
}

/// One thing that may be picked on an open row: the value it writes, how it
/// is shown, the efforts it may be asked at where it declares any, and why it
/// cannot be picked where it cannot.
struct Choice {
    value: String,
    label: String,
    says: String,
    efforts: Vec<String>,
    refusal: Option<String>,
}

/// The set open over one row — the screen's second level, like the menu's
/// picker. Where several may be taken it remembers them in the order they
/// were taken, because a list's order is part of its answer.
struct Choosing {
    at: usize,
    choices: Vec<Choice>,
    selected: usize,
    taken: Option<Vec<usize>>,
    /// The reader is in the efforts column of the selected choice.
    on_efforts: bool,
    effort: usize,
}

/// A line being typed on one row: the whole of what a one-line ask is
/// (§FS-005-dispatch.10), with the input's name already on it.
struct Typing {
    at: usize,
    line: String,
}

pub(crate) struct AnswerScreen {
    pub item: Box<Item>,
    pub entry: Box<ActionConfig>,
    pub picked: Option<HandList>,
    /// What the reader has answered here, by input name — the first of the
    /// five steps, and what travels as the `--set` pairs of the same call a
    /// command makes (§REQ-002-parity.2).
    pub typed: BTreeMap<String, String>,
    /// The workflow's own id and words, for the reader who reached this from
    /// a menu entry named something else.
    workflow: String,
    says: String,
    rows: Vec<Row>,
    /// Required inputs nobody has answered: what holds the laying.
    missing: Vec<String>,
    /// What could not stand — a hand a narrowing does not permit, most often.
    refusals: Vec<String>,
    selected: usize,
    /// The hands an input naming who does the work is chosen from, already
    /// narrowed by the project and already saying who is unavailable and why.
    roster: Vec<Hand>,
    choosing: Option<Choosing>,
    typing: Option<Typing>,
    /// The binding's own account of what would be written, where it was asked
    /// for (§FS-005-dispatch.19). A reading over the screen, closed by Esc.
    account: Option<String>,
}

impl AnswerScreen {
    /// The screen over one laying the session resolved. Nothing here decides
    /// what the rows are: they are read off the answers, in the workflow's own
    /// order (§AR-009-surfaces.1).
    pub fn over(
        item: Item,
        entry: ActionConfig,
        picked: Option<HandList>,
        laying: &Laying,
        roster: Vec<Hand>,
    ) -> Self {
        AnswerScreen {
            item: Box::new(item),
            entry: Box::new(entry),
            picked,
            typed: BTreeMap::new(),
            workflow: laying.workflow.id.clone(),
            says: laying.workflow.description.clone(),
            rows: rows_of(laying),
            missing: laying.answered.missing.clone(),
            refusals: laying.answered.refusals.clone(),
            selected: 0,
            roster,
            choosing: None,
            typing: None,
            account: None,
        }
    }

    /// The same screen over the laying as it stands after an answer. The
    /// answering runs again below the screen, so that provenance, the hands a
    /// narrowing refuses, and what is still missing are the session's reading
    /// and never this screen's arithmetic.
    pub fn refresh(&mut self, laying: &Laying) {
        self.rows = rows_of(laying);
        self.missing = laying.answered.missing.clone();
        self.refusals = laying.answered.refusals.clone();
        self.selected = self.selected.min(self.rows.len());
    }

    /// What the binding said it would write, to show over the rows.
    pub fn account(&mut self, account: String) {
        self.account = Some(account);
    }

    pub fn title(&self) -> String {
        format!("{} — {}", self.workflow, self.item.title)
    }

    /// Whether the workflow can be laid down as it stands.
    fn ready(&self) -> bool {
        self.missing.is_empty() && self.refusals.is_empty()
    }

    /// The row the cursor is on, where it is on one — the last position is
    /// the laying itself, which is not an input.
    fn row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn footer(&self) -> String {
        if self.account.is_some() {
            return "esc closes what would be written".to_string();
        }
        if self.typing.is_some() {
            return "enter answers  ·  esc keeps what was there".to_string();
        }
        if let Some(choosing) = &self.choosing {
            return match choosing.taken.is_some() {
                true => {
                    "space takes one  ·  enter answers  ·  esc keeps what was there".to_string()
                }
                false => "enter answers  ·  → efforts  ·  esc keeps what was there".to_string(),
            };
        }
        match self.row() {
            Some(row) if !row.typeable() && row.set().is_empty() && !row.hand => {
                "e answers it in your editor  ·  p what would be written  ·  esc leaves".to_string()
            }
            Some(_) => "enter answers  ·  e editor  ·  p what would be written  ·  G lays it down"
                .to_string(),
            None => "enter lays it down  ·  p what would be written  ·  esc leaves".to_string(),
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> AnswerOutcome {
        if self.account.is_some() {
            // A reading over the screen: any key that leaves closes it, and
            // nothing beneath it moves while it is up.
            if matches!(code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.account = None;
            }
            return AnswerOutcome::Stay;
        }
        if self.typing.is_some() {
            return self.type_key(code, modifiers);
        }
        if self.choosing.is_some() {
            return self.choose_key(code);
        }
        match code {
            KeyCode::Esc => AnswerOutcome::Close,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected < self.rows.len() {
                    self.selected += 1;
                }
                AnswerOutcome::Stay
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                AnswerOutcome::Stay
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.selected = 0;
                AnswerOutcome::Stay
            }
            // The end of the rows is the laying, which is where `G` means to
            // go: everything is answered and the reader is done reading.
            KeyCode::Char('G') | KeyCode::End => {
                self.selected = self.rows.len();
                AnswerOutcome::Stay
            }
            KeyCode::Char('e') => AnswerOutcome::Edit,
            KeyCode::Char('p') => AnswerOutcome::Preview,
            KeyCode::Enter => self.open(),
            _ => AnswerOutcome::Stay,
        }
    }

    /// Enter, on whatever the cursor is on: the laying at the end, a set to
    /// pick from where the row has one, and a line to type where it does not.
    fn open(&mut self) -> AnswerOutcome {
        let Some(row) = self.row().cloned() else {
            return AnswerOutcome::Lay;
        };
        if row.hand {
            // Nobody to pick from is not an empty list to stare at: the row
            // falls back to the line, which is what the command line takes
            // for it too.
            if !self.roster.is_empty() {
                self.choosing = Some(self.hands(&row));
                return AnswerOutcome::Stay;
            }
        } else {
            let set = row.set();
            if !set.is_empty() {
                self.choosing = Some(self.set(&row, set));
                return AnswerOutcome::Stay;
            }
        }
        if !row.typeable() {
            // A record is not a line, and saying so beats a line that cannot
            // be right (§FS-005-dispatch.19).
            return AnswerOutcome::Edit;
        }
        self.typing = Some(Typing {
            at: self.selected,
            line: row.shown.clone(),
        });
        AnswerOutcome::Stay
    }

    /// The roster, as the set one hand-shaped row is chosen from. What each
    /// hand resolves to rides along, and a hand that cannot be asked carries
    /// its reason rather than being hidden (§FS-005-dispatch.14).
    fn hands(&self, row: &Row) -> Choosing {
        let choices: Vec<Choice> = self
            .roster
            .iter()
            .map(|hand| Choice {
                value: hand.id.clone(),
                label: hand.id.clone(),
                says: hand.resolves_to(),
                efforts: match hand.available.is_none() {
                    true => hand.efforts.clone(),
                    false => Vec::new(),
                },
                refusal: hand.available.clone(),
            })
            .collect();
        Choosing {
            at: self.selected,
            selected: 0,
            taken: row.several.then(Vec::new),
            choices,
            on_efforts: false,
            effort: 0,
        }
    }

    /// A known set, as the choices one row is picked from. The answer already
    /// standing in the row is where the cursor opens, so a reader who meant
    /// to look and not to change sees what they have.
    fn set(&self, row: &Row, set: Vec<String>) -> Choosing {
        let choices: Vec<Choice> = set
            .into_iter()
            .map(|value| Choice {
                label: value.clone(),
                says: String::new(),
                efforts: Vec::new(),
                refusal: None,
                value,
            })
            .collect();
        let selected = choices
            .iter()
            .position(|choice| choice.value == row.shown)
            .unwrap_or(0);
        Choosing {
            at: self.selected,
            selected,
            taken: row.several.then(Vec::new),
            choices,
            on_efforts: false,
            effort: 0,
        }
    }

    /// One key of the open set. The columns are the picker's
    /// (§FS-005-dispatch.14) so that choosing a hand is the same gesture
    /// wherever a hand is chosen.
    fn choose_key(&mut self, code: KeyCode) -> AnswerOutcome {
        let Some(choosing) = &mut self.choosing else {
            return AnswerOutcome::Stay;
        };
        let last = choosing.choices.len().saturating_sub(1);
        let efforts = choosing.choices[choosing.selected].efforts.len();
        match code {
            KeyCode::Esc => self.choosing = None,
            KeyCode::Char('j') | KeyCode::Down => {
                if choosing.on_efforts {
                    choosing.effort = (choosing.effort + 1).min(efforts.saturating_sub(1));
                } else if choosing.selected < last {
                    choosing.selected += 1;
                    choosing.effort = 0;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if choosing.on_efforts {
                    choosing.effort = choosing.effort.saturating_sub(1);
                } else if choosing.selected > 0 {
                    choosing.selected -= 1;
                    choosing.effort = 0;
                }
            }
            KeyCode::Right => {
                if choosing.choices[choosing.selected].refusal.is_none() && efforts > 0 {
                    choosing.on_efforts = true;
                }
            }
            KeyCode::Left => choosing.on_efforts = false,
            // Taking one of several. The order they are taken in is the order
            // they are written in, and taking one twice puts it back.
            KeyCode::Char(' ') => {
                let at = choosing.selected;
                if choosing.choices[at].refusal.is_some() {
                    return AnswerOutcome::Stay;
                }
                if let Some(taken) = &mut choosing.taken {
                    match taken.iter().position(|held| *held == at) {
                        Some(index) => {
                            taken.remove(index);
                        }
                        None => taken.push(at),
                    }
                }
            }
            KeyCode::Enter => return self.chosen(),
            _ => {}
        }
        AnswerOutcome::Stay
    }

    /// What the open set answers with: one value, or the several that were
    /// taken written as a list. A hand asked at an effort carries the effort,
    /// because a hand that declares efforts and is asked without one is a
    /// choice the resolution refuses (§FS-005-dispatch.14).
    fn chosen(&mut self) -> AnswerOutcome {
        let Some(choosing) = self.choosing.take() else {
            return AnswerOutcome::Stay;
        };
        let Some(row) = self.rows.get(choosing.at) else {
            return AnswerOutcome::Stay;
        };
        let spelling = |choice: &Choice, effort: usize| -> String {
            match choice.efforts.get(effort) {
                Some(effort) => format!("{}:{effort}", choice.value),
                None => choice.value.clone(),
            }
        };
        let value = match &choosing.taken {
            Some(taken) => {
                let held: Vec<String> = taken
                    .iter()
                    .filter_map(|at| choosing.choices.get(*at))
                    // Several are taken one by one, and an effort column
                    // belongs to the one the cursor is on — so what several
                    // are taken at is each one's first effort, or none.
                    .map(|choice| spelling(choice, 0))
                    .collect();
                if held.is_empty() {
                    // Nothing taken is not an answer of an empty list: it is
                    // the reader leaving the row as it was.
                    return AnswerOutcome::Stay;
                }
                serde_json::Value::Array(held.into_iter().map(serde_json::Value::String).collect())
                    .to_string()
            }
            None => {
                let choice = &choosing.choices[choosing.selected];
                if choice.refusal.is_some() {
                    // Chosen and nothing happens: the row already carries the
                    // whole reason (§AR-002-summons.4).
                    self.choosing = Some(choosing);
                    return AnswerOutcome::Stay;
                }
                spelling(choice, choosing.effort)
            }
        };
        AnswerOutcome::Set {
            input: row.input.clone(),
            value,
        }
    }

    /// One key of the line being typed. The editing keys are the prompt's, so
    /// a line typed here behaves as a line typed anywhere.
    fn type_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> AnswerOutcome {
        let Some(typing) = &mut self.typing else {
            return AnswerOutcome::Stay;
        };
        match code {
            KeyCode::Esc => self.typing = None,
            KeyCode::Enter => {
                let typed = self.typing.take().expect("a line being typed");
                let Some(row) = self.rows.get(typed.at) else {
                    return AnswerOutcome::Stay;
                };
                return AnswerOutcome::Set {
                    input: row.input.clone(),
                    value: typed.line.trim().to_string(),
                };
            }
            KeyCode::Backspace => {
                typing.line.pop();
            }
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => typing.line.clear(),
            KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => {
                let trimmed = typing.line.trim_end();
                let cut = trimmed.rfind(' ').map(|index| index + 1).unwrap_or(0);
                typing.line.truncate(cut);
            }
            KeyCode::Char(ch) => typing.line.push(ch),
            _ => {}
        }
        AnswerOutcome::Stay
    }

    /// The rows, as they are drawn: one per input, then a line of air, then
    /// the laying. The air is not a row — the cursor never lands on it, which
    /// is what [`AnswerScreen::at`] is for.
    pub fn lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let names = self
            .rows
            .iter()
            .map(|row| row.input.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(8, 28);
        // One column for the answers, so the provenance beside them lines up
        // and the eye reads down it rather than hunting along each row.
        let values = self
            .rows
            .iter()
            .map(|row| shown(&row.shown).chars().count())
            .max()
            .unwrap_or(0)
            .clamp(12, 44);
        let mut lines: Vec<Line<'static>> = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let unanswered = self.missing.iter().any(|name| *name == row.input);
                let typing = self
                    .typing
                    .as_ref()
                    .filter(|typing| typing.at == index)
                    .map(|typing| typing.line.clone());
                let mut spans = vec![
                    Span::styled(
                        match unanswered {
                            true => " ! ".to_string(),
                            false => "   ".to_string(),
                        },
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(format!("{:names$}  ", row.input)),
                    Span::styled(format!("{:14}", row.wants()), dim),
                ];
                match typing {
                    // The line being typed stands where the value stands, so
                    // the row never moves under the reader's hands.
                    Some(line) => spans.push(Span::styled(
                        format!("{line}▌"),
                        Style::default().add_modifier(Modifier::BOLD),
                    )),
                    None => {
                        spans.push(match unanswered {
                            true => Span::styled(
                                format!("{:values$}", "nothing answers this"),
                                Style::default().fg(Color::Yellow),
                            ),
                            false => Span::raw(format!("{:values$}", fit(&row.shown, values))),
                        });
                        // Where the answer came from, which is the half of the
                        // account that is ephor's own (§FS-005-dispatch.19).
                        spans.push(Span::styled(format!("  ← {}", row.from.label()), dim));
                    }
                }
                Line::from(spans)
            })
            .collect();
        lines.push(Line::from(Span::raw(String::new())));
        lines.push(Line::from(vec![Span::styled(
            match self.ready() {
                true => format!("   ▸ lay {} down", self.workflow),
                false => format!(
                    "   ▸ lay {} down — nothing answers {}",
                    self.workflow,
                    self.missing.join(", ")
                ),
            },
            match self.ready() {
                true => Style::default().add_modifier(Modifier::BOLD),
                false => dim,
            },
        )]));
        lines
    }

    /// Where the cursor is among those lines: past the line of air once it is
    /// on the laying.
    fn at(&self) -> usize {
        match self.selected < self.rows.len() {
            true => self.selected,
            false => self.selected + 1,
        }
    }

    /// What the selected row is for, under the rows: its own words, and the
    /// refusals that stand against the whole laying.
    fn detail(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let mut lines = Vec::new();
        match self.row() {
            Some(row) => {
                let mut says = row.says.clone();
                if row.required {
                    says = match says.is_empty() {
                        true => "required".to_string(),
                        false => format!("{says}  ·  required"),
                    };
                }
                lines.push(Line::from(Span::styled(format!("   {says}"), dim)));
            }
            None => lines.push(Line::from(Span::styled(format!("   {}", self.says), dim))),
        }
        for refusal in &self.refusals {
            lines.push(Line::from(Span::styled(
                format!("   {refusal}"),
                Style::default().fg(Color::Red),
            )));
        }
        lines
    }

    pub fn draw(&self, frame: &mut ratatui::Frame, area: Rect) {
        let detail = self.detail();
        let width = area.width.saturating_sub(4).min(110).max(24);
        let height = (self.rows.len() as u16 + detail.len() as u16 + 5).min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" answer {}'s inputs ", self.workflow));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let listed = inner.height.saturating_sub(detail.len() as u16 + 1);
        let rows_rect = Rect {
            height: listed,
            ..inner
        };
        let rows: Vec<ListItem> = self.lines().into_iter().map(ListItem::new).collect();
        let list = List::new(rows).highlight_style(match self.choosing.is_some() {
            true => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
            false => highlight_style(),
        });
        let mut state = ListState::default();
        state.select(Some(self.at()));
        frame.render_stateful_widget(list, rows_rect, &mut state);

        let detail_rect = Rect {
            y: inner.y + listed + 1,
            height: inner.height.saturating_sub(listed + 1),
            ..inner
        };
        frame.render_widget(
            Paragraph::new(detail).wrap(Wrap { trim: false }),
            detail_rect,
        );

        if let Some(choosing) = &self.choosing {
            self.draw_choosing(frame, area, choosing);
        }
        if let Some(account) = &self.account {
            draw_account(frame, area, account);
        }
    }

    /// The set open over the row, drawn where the picker is drawn and read
    /// the same way: what may be chosen, what each one resolves to, and the
    /// efforts column beside a choice that declares any.
    fn draw_choosing(&self, frame: &mut ratatui::Frame, area: Rect, choosing: &Choosing) {
        let width = area.width.saturating_sub(4).min(76).max(20);
        let height = (choosing.choices.len() as u16 + 2).min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, rect);
        let input = self
            .rows
            .get(choosing.at)
            .map(|row| row.input.clone())
            .unwrap_or_default();
        let block = Block::default()
            .borders(Borders::ALL)
            .title(match choosing.taken.is_some() {
                true => format!(" {input} — take as many as it takes "),
                false => format!(" {input} "),
            });
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let chosen = &choosing.choices[choosing.selected];
        let efforts_width = match chosen.refusal.is_none() && !chosen.efforts.is_empty() {
            true => chosen
                .efforts
                .iter()
                .map(|effort| effort.chars().count() as u16 + 3)
                .max()
                .unwrap_or(0)
                .min(inner.width / 2),
            false => 0,
        };
        let choices_rect = Rect {
            width: inner.width.saturating_sub(efforts_width),
            ..inner
        };
        let dim = Style::default().fg(Color::DarkGray);
        let rows: Vec<ListItem> = choosing
            .choices
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let held = choosing
                    .taken
                    .as_ref()
                    .map(|taken| match taken.iter().position(|at| *at == index) {
                        // The order they were taken in is the order they will
                        // be written in, so it is what the row shows.
                        Some(place) => format!(" {} ", place + 1),
                        None => " · ".to_string(),
                    })
                    .unwrap_or_else(|| " ".to_string());
                let mut spans = vec![
                    Span::styled(held, Style::default().fg(Color::Cyan)),
                    Span::raw(choice.label.clone()),
                ];
                if !choice.says.is_empty() {
                    spans.push(Span::styled(format!("  {}", choice.says), dim));
                }
                if let Some(why) = &choice.refusal {
                    spans.push(Span::styled(
                        format!("  (unavailable: {why})"),
                        dim.add_modifier(Modifier::ITALIC),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let choices = List::new(rows).highlight_style(match choosing.on_efforts {
            true => dim.add_modifier(Modifier::BOLD),
            false => highlight_style(),
        });
        let mut state = ListState::default();
        state.select(Some(choosing.selected));
        frame.render_stateful_widget(choices, choices_rect, &mut state);

        if efforts_width > 0 {
            let efforts_rect = Rect {
                x: inner.x + choices_rect.width,
                width: efforts_width,
                ..inner
            };
            let rows: Vec<ListItem> = chosen
                .efforts
                .iter()
                .map(|effort| ListItem::new(format!(" {effort}")))
                .collect();
            let efforts = List::new(rows).highlight_style(match choosing.on_efforts {
                true => highlight_style(),
                false => dim.add_modifier(Modifier::BOLD),
            });
            let mut state = ListState::default();
            state.select(Some(choosing.effort));
            frame.render_stateful_widget(efforts, efforts_rect, &mut state);
        }
    }
}

/// What the binding said it would write, over the screen that asked
/// (§FS-005-dispatch.19). Its own words, unedited: what a workflow lays down
/// is the binding's account to give.
fn draw_account(frame: &mut ratatui::Frame, area: Rect, account: &str) {
    let width = area.width.saturating_sub(4).min(110).max(24);
    let height = (account.lines().count() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(account.to_string()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" what would be written "),
        ),
        rect,
    );
}

/// One input's rows, read off the answers in the workflow's own order.
fn rows_of(laying: &Laying) -> Vec<Row> {
    laying
        .workflow
        .inputs
        .iter()
        .map(|input| {
            let answer = laying.answered.answer(&input.name);
            let element = input.of.as_ref();
            Row {
                input: input.name.clone(),
                says: input.description.clone(),
                kind: input.kind,
                required: input.required,
                shown: answer
                    .map(|answer| answer.shown.clone())
                    .unwrap_or_default(),
                from: answer.map(|answer| answer.from).unwrap_or(From::Nobody),
                // A list is picked one element at a time, so the set it is
                // picked from is the element's.
                choices: match element {
                    Some(element) => element.choices.clone(),
                    None => input.choices.clone(),
                },
                hand: input.hand,
                several: input.kind == Kind::List,
            }
        })
        .collect()
}

/// A value on one row: what it says, with nothing on it that a row cannot
/// hold. An answer that is empty says so, rather than leaving the column
/// looking unfilled by accident.
fn shown(value: &str) -> String {
    let one = value.replace('\n', " ");
    match one.trim().is_empty() {
        true => "—".to_string(),
        false => one,
    }
}

/// The same, cut to the column it stands in. A cut answer says it was cut:
/// the whole of it is one keystroke away, and a value silently shortened is a
/// value a reader would have corrected if they had seen it.
fn fit(value: &str, width: usize) -> String {
    let one = shown(value);
    match one.chars().count() > width {
        true => {
            one.chars()
                .take(width.saturating_sub(1))
                .collect::<String>()
                + "…"
        }
        false => one,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::runtime::roster::Hand;

    fn row(input: &str, kind: Kind) -> Row {
        Row {
            input: input.to_string(),
            says: format!("what {input} is for"),
            kind,
            required: false,
            shown: String::new(),
            from: From::Default,
            choices: Vec::new(),
            hand: false,
            several: kind == Kind::List,
        }
    }

    fn hand(id: &str, efforts: &[&str], available: Option<&str>) -> Hand {
        Hand {
            id: id.to_string(),
            agent: Some("pi".to_string()),
            model: Some("m-1".to_string()),
            provider: Some("acme".to_string()),
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            available: available.map(str::to_string),
        }
    }

    fn screen(rows: Vec<Row>, roster: Vec<Hand>) -> AnswerScreen {
        AnswerScreen {
            item: Box::new(item()),
            entry: Box::new(ActionConfig::default()),
            picked: None,
            typed: BTreeMap::new(),
            workflow: "changeset-review".to_string(),
            says: "Review a code change.".to_string(),
            missing: rows
                .iter()
                .filter(|row| row.required && row.shown.is_empty())
                .map(|row| row.input.clone())
                .collect(),
            refusals: Vec::new(),
            rows,
            selected: 0,
            roster,
            choosing: None,
            typing: None,
            account: None,
        }
    }

    fn item() -> Item {
        Item {
            id: "forge:demo/17".to_string(),
            project: "demo".to_string(),
            source: "forge".to_string(),
            kind: crate::feed::model::ItemKind::Pr,
            role: None,
            title: "Widen the retry window".to_string(),
            url: None,
            state: None,
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw: serde_json::Value::Null,
        }
    }

    /// A screen with one of everything on it: a hand, several hands, a set,
    /// a flag, a line, and a record.
    pub(super) fn furnished() -> AnswerScreen {
        let mut target = row("smart_target", Kind::Text);
        target.hand = true;
        target.shown = "careful".to_string();
        let mut targets = row("review_targets", Kind::List);
        targets.hand = true;
        let mut set = row("fix_prepare", Kind::Text);
        set.choices = ["none", "branch", "worktree", "fork"]
            .iter()
            .map(|word| word.to_string())
            .collect();
        let mut needed = row("change_ref", Kind::Text);
        needed.required = true;
        needed.from = From::Nobody;
        screen(
            vec![
                needed,
                target,
                targets,
                set,
                row("download_pdfs", Kind::Flag),
                row("personalities", Kind::Record),
            ],
            vec![
                hand("fast", &[], None),
                hand("careful", &["high", "low"], None),
                hand("gone", &[], Some("not installed")),
            ],
        )
    }

    fn text(screen: &AnswerScreen) -> String {
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

    fn press(screen: &mut AnswerScreen, code: KeyCode) -> AnswerOutcome {
        screen.handle_key(code, KeyModifiers::NONE)
    }

    /// Every input the workflow declares is on the screen with the answer
    /// standing in it and where that answer came from — the ones nobody had to
    /// ask about included, because those are as likely to be the wrong ones
    /// (§FS-005-dispatch.19).
    #[test]
    fn every_input_is_shown_with_its_answer_and_where_it_came_from() {
        let mut standing = row("smart_target", Kind::Text);
        standing.shown = "careful".to_string();
        standing.from = From::Hand;
        standing.hand = true;
        let mut defaulted = row("fix_prepare", Kind::Text);
        defaulted.shown = "none".to_string();
        defaulted.choices = ["none", "branch", "worktree", "fork"]
            .iter()
            .map(|word| word.to_string())
            .collect();
        let screen = screen(vec![standing, defaulted], vec![hand("careful", &[], None)]);
        let text = text(&screen);
        assert!(text.contains("smart_target"), "{text}");
        assert!(text.contains("careful"), "{text}");
        assert!(text.contains("← the hand"), "{text}");
        assert!(text.contains("fix_prepare"), "{text}");
        assert!(text.contains("← the workflow"), "{text}");
        // And what each one wants, in the same column: a set says how many it
        // holds, and a hand says it is a hand.
        assert!(text.contains("one of 4"), "{text}");
        assert!(text.contains("hand "), "{text}");
        // The laying is the last row, and it says it can go.
        assert!(text.contains("▸ lay changeset-review down"), "{text}");
    }

    /// A required input nobody answered holds the laying and is named there,
    /// rather than being laid down with a hole in it (§FS-005-dispatch.19).
    #[test]
    fn what_nobody_answered_holds_the_laying_and_is_named() {
        let mut needed = row("change_ref", Kind::Text);
        needed.required = true;
        needed.from = From::Nobody;
        let mut screen = screen(vec![needed], Vec::new());
        let text = text(&screen);
        assert!(text.contains("nothing answers this"), "{text}");
        assert!(text.contains("nothing answers change_ref"), "{text}");

        // Enter on the laying row still asks for it — what refuses is the
        // session, in one place, and not two rules that can come apart.
        press(&mut screen, KeyCode::Char('G'));
        assert!(matches!(
            press(&mut screen, KeyCode::Enter),
            AnswerOutcome::Lay
        ));
    }

    /// An input whose values are a known set is chosen from that set, and
    /// what is chosen comes back as the `--set` pair a command takes.
    #[test]
    fn a_known_set_is_chosen_from_rather_than_typed() {
        let mut choices = row("fix_prepare", Kind::Text);
        choices.shown = "none".to_string();
        choices.choices = ["none", "branch", "worktree", "fork"]
            .iter()
            .map(|word| word.to_string())
            .collect();
        let mut screen = screen(vec![choices], Vec::new());
        press(&mut screen, KeyCode::Enter);
        // It opens where the standing answer is, so looking is not changing.
        assert_eq!(screen.choosing.as_ref().expect("open").selected, 0);
        press(&mut screen, KeyCode::Char('j'));
        press(&mut screen, KeyCode::Char('j'));
        match press(&mut screen, KeyCode::Enter) {
            AnswerOutcome::Set { input, value } => {
                assert_eq!(input, "fix_prepare");
                assert_eq!(value, "worktree");
            }
            _ => panic!("expected the chosen value"),
        }
    }

    /// A flag has two values and is chosen between them, without anybody
    /// having to know how the binding spells true.
    #[test]
    fn a_flag_is_chosen_between_its_two_values() {
        let mut flag = row("download_pdfs", Kind::Flag);
        flag.shown = "true".to_string();
        let mut screen = screen(vec![flag], Vec::new());
        press(&mut screen, KeyCode::Enter);
        press(&mut screen, KeyCode::Char('j'));
        match press(&mut screen, KeyCode::Enter) {
            AnswerOutcome::Set { value, .. } => assert_eq!(value, "false"),
            _ => panic!("expected the flag's other value"),
        }
    }

    /// An input that names who does the work is chosen from the roster, at an
    /// effort where the hand declares any — never asked plainly when it
    /// declares several, which is the choice the resolution refuses
    /// (§FS-005-dispatch.14).
    #[test]
    fn a_hand_is_chosen_from_the_roster_at_an_effort() {
        let mut target = row("smart_target", Kind::Text);
        target.hand = true;
        let mut screen = screen(
            vec![target],
            vec![
                hand("fast", &[], None),
                hand("careful", &["high", "low"], None),
            ],
        );
        press(&mut screen, KeyCode::Enter);
        press(&mut screen, KeyCode::Char('j'));
        press(&mut screen, KeyCode::Right);
        press(&mut screen, KeyCode::Char('j'));
        match press(&mut screen, KeyCode::Enter) {
            AnswerOutcome::Set { input, value } => {
                assert_eq!(input, "smart_target");
                assert_eq!(value, "careful:low");
            }
            _ => panic!("expected the hand at its effort"),
        }
    }

    /// A hand that cannot be asked is shown with its reason and cannot be
    /// chosen — the refusal was computed when the roster was read
    /// (§AR-002-summons.4).
    #[test]
    fn a_hand_that_cannot_be_asked_is_not_chosen() {
        let mut target = row("smart_target", Kind::Text);
        target.hand = true;
        let mut screen = screen(vec![target], vec![hand("gone", &[], Some("not installed"))]);
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(
            press(&mut screen, KeyCode::Enter),
            AnswerOutcome::Stay
        ));
        assert!(screen.choosing.is_some(), "the set stays open");
    }

    /// An input wanting several hands is answered with several, in the order
    /// they were taken (§DA-006-hands-fill-a-workflows-targets).
    #[test]
    fn several_hands_are_taken_one_by_one_and_written_as_a_list() {
        let mut targets = row("review_targets", Kind::List);
        targets.hand = true;
        let mut screen = screen(
            vec![targets],
            vec![
                hand("fast", &[], None),
                hand("careful", &[], None),
                hand("thorough", &[], None),
            ],
        );
        press(&mut screen, KeyCode::Enter);
        press(&mut screen, KeyCode::Char('j'));
        press(&mut screen, KeyCode::Char(' '));
        press(&mut screen, KeyCode::Char('k'));
        press(&mut screen, KeyCode::Char(' '));
        match press(&mut screen, KeyCode::Enter) {
            AnswerOutcome::Set { input, value } => {
                assert_eq!(input, "review_targets");
                assert_eq!(value, r#"["careful","fast"]"#);
            }
            _ => panic!("expected the several that were taken"),
        }

        // Taking nothing leaves the row as it was rather than answering it
        // with an empty list.
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(
            press(&mut screen, KeyCode::Enter),
            AnswerOutcome::Stay
        ));
    }

    /// Everything else is one line, typed on the row, starting from what is
    /// already there.
    #[test]
    fn anything_without_a_set_is_typed_on_its_row() {
        let mut free = row("paper_id", Kind::Text);
        free.shown = "submission".to_string();
        let mut screen = screen(vec![free], Vec::new());
        press(&mut screen, KeyCode::Enter);
        // The line starts at the answer standing in the row, so a small
        // correction is a small correction.
        assert!(text(&screen).contains("submission▌"), "{}", text(&screen));
        press(&mut screen, KeyCode::Char('-'));
        press(&mut screen, KeyCode::Char('2'));
        match press(&mut screen, KeyCode::Enter) {
            AnswerOutcome::Set { input, value } => {
                assert_eq!(input, "paper_id");
                assert_eq!(value, "submission-2");
            }
            _ => panic!("expected the typed line"),
        }
    }

    /// What no row can carry goes to the editor, whether the reader asked for
    /// it or simply pressed Enter on such a row (§FS-005-dispatch.19).
    #[test]
    fn a_record_is_answered_in_the_editor() {
        let record = row("personalities", Kind::Record);
        let mut screen = screen(vec![record], Vec::new());
        assert!(matches!(
            press(&mut screen, KeyCode::Enter),
            AnswerOutcome::Edit
        ));
        assert!(matches!(
            press(&mut screen, KeyCode::Char('e')),
            AnswerOutcome::Edit
        ));
    }

    /// What the binding would write is asked for from the same screen, and
    /// read over it (§FS-005-dispatch.19).
    #[test]
    fn what_would_be_written_is_asked_for_here() {
        let mut screen = screen(vec![row("paper_id", Kind::Text)], Vec::new());
        assert!(matches!(
            press(&mut screen, KeyCode::Char('p')),
            AnswerOutcome::Preview
        ));
        screen.account("would write 3 files\nplan.rhei.md".to_string());
        assert!(screen.footer().contains("what would be written"));
        // While it is up, nothing beneath it moves.
        press(&mut screen, KeyCode::Char('j'));
        assert_eq!(screen.selected, 0);
        press(&mut screen, KeyCode::Esc);
        assert!(screen.account.is_none());
    }

    /// Esc leaves without laying anything down: what a key press does here is
    /// answer inputs, and writing is the move after (§FS-005-dispatch.7).
    #[test]
    fn leaving_lays_nothing_down() {
        let mut screen = screen(vec![row("paper_id", Kind::Text)], Vec::new());
        assert!(matches!(
            press(&mut screen, KeyCode::Esc),
            AnswerOutcome::Close
        ));
    }
}

#[cfg(test)]
mod drawing {
    use super::tests as fixtures;
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// The screen draws — at the size a terminal actually is, and at one far
    /// too small for it. A panel that only fits on a wide screen is a panel
    /// that takes the interface down on a narrow one.
    #[test]
    fn it_draws_at_any_size() {
        for (width, height) in [(120u16, 40u16), (80, 24), (40, 10), (24, 6)] {
            let mut terminal =
                Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
            let mut screen = fixtures::furnished();
            terminal
                .draw(|frame| screen.draw(frame, frame.area()))
                .expect("draws the rows");
            screen.handle_key(KeyCode::Enter, KeyModifiers::NONE);
            terminal
                .draw(|frame| screen.draw(frame, frame.area()))
                .expect("draws the open set");
            screen.handle_key(KeyCode::Esc, KeyModifiers::NONE);
            screen.account("would write 4 files\nplan.rhei.md\nstates.yaml".to_string());
            terminal
                .draw(|frame| screen.draw(frame, frame.area()))
                .expect("draws the account");
        }
    }
}
