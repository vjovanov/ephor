//! The operations board: every operation beneath the reading, in one place
//! (§FS-005-dispatch.15).
//!
//! Watch-only. Rows are execution roots read from the runtime's artifacts —
//! a live run, or a claim with no run behind it — plus the refresh, which
//! reports here *additionally* to the header line it already owns
//! (§FS-001-forge-interface.7). Nothing here starts, stops, or touches a
//! run; the rows are built by the shell off the draw path, and a draw only
//! renders what was already decided.

use std::path::PathBuf;

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::feed::model::Item;
use crate::work::runtime::watch::{Doing, Operation};

use super::Action;

/// One row of the board. Two kinds of operation stand here: what the runtime
/// is at (§FS-005-dispatch.15), and what ephor is running itself
/// (§FS-005-dispatch.17) — one cursor over both, because "what is going on"
/// has one answer and the reader should not have to know which half of the
/// machine is doing it.
pub(crate) enum Row {
    /// A job of ephor's own, while it runs. A job that ended is no longer an
    /// operation: its outcome went to the reader and its record stays with the
    /// item (§FS-005-dispatch.17).
    Job(crate::seams::jobs::Job),
    Op(Box<OpRow>),
}

impl Row {
    /// What this row *is*, for keeping the cursor on it across a rebuild: the
    /// job it is, or the execution root it is (§FS-005-dispatch.15.1).
    fn key(&self) -> String {
        match self {
            Row::Job(job) => format!("job:{}", job.id),
            Row::Op(row) => format!("root:{}", row.op.root.display()),
        }
    }

    fn op(&self) -> Option<&OpRow> {
        match self {
            Row::Op(row) => Some(row),
            Row::Job(_) => None,
        }
    }
}

/// What opening a job row gets the reader: everything it wrote, followed as it
/// writes (§FS-005-dispatch.17).
///
/// A windowed job has no log — what its program wrote is on a screen the reader
/// was looking at and is not duplicated (§AR-002-summons.6) — so the row says
/// where the program went rather than opening a file that is not there. The
/// board brings no window forward: it watches, and every ability it holds is a
/// command (§FS-005-dispatch.15, §REQ-002-parity.2).
fn way_into(job: &crate::seams::jobs::Job) -> Action {
    match job.log() {
        Some(path) => Action::ReadLog {
            path,
            following: job.live,
        },
        None => Action::SetMessage(job.says()),
    }
}

/// One operation with the matter behind it, where the feed still carries
/// one — resolved by the shell when the board is built, never while drawing.
pub(crate) struct OpRow {
    pub op: Operation,
    pub item: Option<Item>,
    /// The plan this row reads and edits: the operation's own where one of
    /// its tickets names it, the ledger's for this execution root otherwise.
    /// Resolved by the shell for the same reason the matter is.
    pub plan: Option<PathBuf>,
}

/// What one re-probe found: whether anything the reader can see moved, and
/// whether a row's liveness flipped — which is the caller's cue to rebuild
/// the whole board, because a run that died leaves tickets whose flavour
/// depends on the lock it just released.
#[derive(Default)]
pub(crate) struct Repulsed {
    pub changed: bool,
    pub flipped: bool,
}

pub(crate) struct OperationsScreen {
    rows: Vec<Row>,
    /// Why there are no operations, where a runtime cannot run any — the
    /// workable rung's own sentence, shown rather than an empty screen that
    /// looks broken (§FS-005-dispatch.15).
    refusal: Option<String>,
    selected: usize,
    scroll: u16,
    viewport: u16,
}

impl OperationsScreen {
    pub fn new(rows: Vec<Row>, refusal: Option<String>) -> Self {
        OperationsScreen {
            rows,
            refusal,
            selected: 0,
            scroll: 0,
            viewport: 0,
        }
    }

    pub fn title(&self) -> String {
        " ephor — operations".to_string()
    }

    /// `;` is the key that opened this, and it is the key that closes it
    /// again — said here rather than left for the reader to guess, the way
    /// every other screen now says `; ops` (§FS-005-dispatch.15).
    pub fn footer(&self) -> &'static str {
        " j/k move  enter matter/plan/log  e plan/log  a watch the run  o dashboard  r refresh  \
         esc/; back"
    }

    /// Fresh rows from a rebuild, with the cursor still on the operation it
    /// was on. A rebuild fires from the tick and from every refresh landing
    /// (§FS-001-forge-interface.7), so a row appearing above the cursor would
    /// otherwise silently change what the next `enter` or `o` acts on — the
    /// same rule the tree keeps, keyed on the execution root, which is what a
    /// row of this board *is* (§FS-005-dispatch.15).
    pub fn replace(&mut self, rows: Vec<Row>, refusal: Option<String>) {
        let was = self.rows.get(self.selected).map(Row::key);
        self.rows = rows;
        self.refusal = refusal;
        self.selected = was
            .and_then(|key| self.rows.iter().position(|row| row.key() == key))
            .unwrap_or_else(|| self.selected.min(self.rows.len().saturating_sub(1)));
        self.follow_selection();
    }

    /// Re-probe every row's liveness and the badges riding on it, in place —
    /// the cheap half of the tick (§FS-005-dispatch.15.1).
    pub fn repulse(
        &mut self,
        probe: impl Fn(&std::path::Path) -> crate::work::runtime::watch::Pulse,
    ) -> Repulsed {
        let mut found = Repulsed::default();
        for row in &mut self.rows {
            // A job's liveness is its own lock, probed by the shell that owns
            // the job list (§FS-005-dispatch.17); this is the runtime's half.
            let Row::Op(row) = row else {
                continue;
            };
            let pulse = probe(&row.op.root);
            if pulse.live != row.op.live {
                found.flipped = true;
            }
            if pulse.live != row.op.live
                || pulse.dashboard != row.op.dashboard
                || pulse.quiet != row.op.quiet
                || pulse.identity != row.op.identity
            {
                found.changed = true;
            }
            row.op.live = pulse.live;
            row.op.dashboard = pulse.dashboard;
            row.op.quiet = pulse.quiet;
            // A run that ended and one that started in its place are two runs,
            // and the row says which it is looking at (§FS-005-dispatch.20).
            // The stop command follows the id it is about, and is re-composed
            // beside it: clearing it here left the row naming its run and no
            // longer saying how to end it, from the first tick onwards.
            row.op.stop = pulse.stop;
            row.op.identity = pulse.identity;
        }
        found
    }

    /// The execution roots the board has a row for. What the tick needs to
    /// tell "a run whose badges moved" from "a run that started somewhere
    /// this board is not showing" (§FS-005-dispatch.15.1).
    pub fn roots(&self) -> Vec<PathBuf> {
        self.rows
            .iter()
            .filter_map(Row::op)
            .map(|row| row.op.root.clone())
            .collect()
    }

    fn selected_row(&self) -> Option<&OpRow> {
        self.rows.get(self.selected).and_then(Row::op)
    }

    fn selected_job(&self) -> Option<&crate::seams::jobs::Job> {
        match self.rows.get(self.selected) {
            Some(Row::Job(job)) => Some(job),
            _ => None,
        }
    }

    fn selected_plan(&self) -> Option<PathBuf> {
        self.selected_row().and_then(|row| row.plan.clone())
    }

    /// Lines above the first operation, in the order [`OperationsScreen::lines`]
    /// pushes them: the heading, the refresh line, the refusal where there is
    /// one, a blank, and the operations heading.
    fn head_lines(&self) -> usize {
        4 + usize::from(self.refusal.is_some())
    }

    /// Lines one row takes: for a run, its own, one per ticket, and the
    /// finished count where there is one; for a job, its own and the line
    /// saying what it is doing right now (§FS-005-dispatch.17).
    fn row_lines(row: &Row) -> usize {
        match row {
            Row::Job(_) => 2,
            Row::Op(row) => {
                1 + usize::from(row.op.stop.is_some())
                    + row.op.tickets.len()
                    + usize::from(row.op.done > 0 || row.op.cancelled > 0)
            }
        }
    }

    /// Where an operation's first line sits in what [`OperationsScreen::lines`]
    /// builds.
    fn row_offset(&self, index: usize) -> usize {
        self.head_lines()
            + self.rows[..index]
                .iter()
                .map(Self::row_lines)
                .sum::<usize>()
    }

    /// Bring the selected operation into the viewport. An operation is several
    /// lines, so a selection moved without the scroll following it puts the
    /// cursor off screen and leaves the reader pressing `enter` on something
    /// they cannot see (§FS-004-quick-actions.2).
    fn follow_selection(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        // Nothing has been drawn yet, so there is no viewport to be inside.
        if self.viewport == 0 {
            return;
        }
        let top = self.row_offset(self.selected) as u16;
        let height = Self::row_lines(row) as u16;
        let viewport = self.viewport;
        if top < self.scroll {
            self.scroll = top;
        } else if top.saturating_add(height) > self.scroll.saturating_add(viewport) {
            self.scroll = top.saturating_add(height).saturating_sub(viewport);
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => {
                Action::Back
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.rows.len() {
                    self.selected += 1;
                }
                self.follow_selection();
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.follow_selection();
                Action::None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.selected = 0;
                self.scroll = 0;
                Action::None
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.selected = self.rows.len().saturating_sub(1);
                self.follow_selection();
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
            // The matter, where the feed still carries it; the plan where the
            // operation has none (§FS-005-dispatch.15). On a job it is the
            // log — what the reader would have watched, kept
            // (§FS-005-dispatch.17).
            KeyCode::Enter | KeyCode::Char('l') => match self.selected_job() {
                Some(job) => way_into(job),
                None => match self.selected_row() {
                    Some(row) => match &row.item {
                        Some(item) => Action::OpenThread {
                            item: item.clone(),
                            or_url: true,
                        },
                        None => match self.selected_plan() {
                            Some(plan) => Action::ReadPlan(plan),
                            None => Action::None,
                        },
                    },
                    None => Action::None,
                },
            },
            KeyCode::Char('e') => match self.selected_job() {
                Some(job) => way_into(job),
                None => match self.selected_plan() {
                    Some(plan) => Action::ReadPlan(plan),
                    None => Action::SetMessage("No plan behind this row".to_string()),
                },
            },
            // Watching is attaching (§FS-005-dispatch.20). It watches: the
            // board starts nothing and stops nothing, and leaving the surface
            // detaches and never ends the run. A row with no identity behind it
            // says so rather than doing nothing (§REQ-001-boundary.1).
            KeyCode::Char('a') => match self.selected_row() {
                Some(row) if !row.op.live => {
                    Action::SetMessage("Nothing is running on this root".to_string())
                }
                Some(row) => match row.op.run_id() {
                    Some(id) => Action::AttachRun {
                        root: row.op.root.clone(),
                        id: id.to_string(),
                    },
                    None => Action::SetMessage(
                        "This run left no id beside its lock, so there is nothing to attach to"
                            .to_string(),
                    ),
                },
                None => Action::SetMessage("Nothing is running".to_string()),
            },
            KeyCode::Char('o') => match self.selected_row() {
                // A job serves nothing to open: it writes a log, and that is
                // what `e` is for (§FS-005-dispatch.17).
                Some(row) => match &row.op.dashboard {
                    Some(url) => Action::OpenUrl(Some(url.clone())),
                    None => Action::SetMessage(
                        "No dashboard — a live run publishes one only while it serves one"
                            .to_string(),
                    ),
                },
                None => Action::SetMessage("Nothing is running".to_string()),
            },
            KeyCode::Char('r') => Action::Refresh,
            _ => Action::None,
        }
    }

    /// Everything on the board, phrased. `progress` is the running refresh's
    /// own line, where one is in flight — the run appears here additionally
    /// to the header (§FS-001-forge-interface.7).
    fn lines(&self, progress: Option<&str>) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let heading = dim.add_modifier(Modifier::BOLD);
        let mut lines = Vec::new();

        lines.push(Line::from(Span::styled(
            "  beneath the reading".to_string(),
            heading,
        )));
        lines.push(Line::from(Span::styled(
            match progress {
                Some(line) => format!("    ⟳ {line}"),
                None => "    ⟳ refresh — idle · r starts one".to_string(),
            },
            Style::default(),
        )));
        if let Some(refusal) = &self.refusal {
            lines.push(Line::from(Span::styled(format!("    {refusal}"), dim)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  operations".to_string(),
            heading,
        )));

        if self.rows.is_empty() {
            lines.push(Line::from(Span::styled(
                "    nothing is running — a live run or job appears here on its own".to_string(),
                dim,
            )));
        }
        for (index, row) in self.rows.iter().enumerate() {
            let selected = index == self.selected;
            // What ephor is running itself (§FS-005-dispatch.17): one row and
            // the line under it saying what the log last said, because
            // "running" and "stuck" look identical without it.
            let row = match row {
                Row::Job(job) => {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", if selected { "▸" } else { " " }), dim),
                        Span::styled(
                            format!("▶ {} · {}", job.record.project, job.record.description),
                            if selected {
                                Style::default().add_modifier(Modifier::BOLD)
                            } else {
                                Style::default()
                            },
                        ),
                        Span::styled("   job · e reads it".to_string(), dim),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("        ⚙ ".to_string(), Style::default().fg(Color::Yellow)),
                        Span::styled(job.says(), dim),
                    ]));
                    continue;
                }
                Row::Op(row) => row,
            };
            let op = &row.op;
            let mut badges: Vec<String> = Vec::new();
            // What the row is, read off what its tickets say rather than off
            // liveness alone: a root is an operation while work waits on the
            // reader, and the run that parked it has usually exited
            // (§FS-005-dispatch.15). Calling that "claimed" would name a
            // person who never claimed it — and calling a dead run's leavings
            // either would hide that a run wants starting again.
            badges.push(if op.live {
                "running".to_string()
            } else if op
                .tickets
                .iter()
                .any(|ticket| matches!(ticket.doing, Doing::Waiting))
            {
                "waiting on you".to_string()
            } else if op
                .tickets
                .iter()
                .any(|ticket| matches!(ticket.doing, Doing::Dropped))
            {
                "a run died here".to_string()
            } else {
                "claimed, not scheduled".to_string()
            });
            // The machine could not be read: queued and finished are
            // withheld in the data, and the row says so rather than letting
            // the zero read as nothing done (§FS-005-dispatch.15).
            if let Some(unread) = &op.machine_unread {
                badges.push(unread.clone());
            }
            // An id is how the reader and the runtime agree on which run they
            // mean, so the row says it (§FS-005-dispatch.20).
            if let Some(id) = op.run_id() {
                badges.push(format!("run {id} (a)"));
            }
            if let Some(minutes) = op.quiet {
                badges.push(format!("quiet {minutes}m"));
            }
            if op.dashboard.is_some() {
                badges.push("dashboard (o)".to_string());
            }
            let marker = if op.live { "▶" } else { "✋" };
            let root = super::display_root(&op.root.to_string_lossy());
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", if selected { "▸" } else { " " }), dim),
                Span::styled(
                    format!("{marker} {} · {root}", op.project),
                    if selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(format!("   {}", badges.join(" · ")), dim),
            ]));
            // The runner's own command for stopping this run, shown and never
            // run: a key that stopped a run would be a channel to the run ephor
            // promised never to hold (§FS-005-dispatch.20).
            if let Some(stop) = &op.stop {
                lines.push(Line::from(Span::styled(
                    format!("        stop it: {stop}"),
                    dim,
                )));
            }
            for ticket in &op.tickets {
                let state = ticket.state.as_deref().unwrap_or("?");
                let name = format!("{}.{}", ticket.plan_id, ticket.ticket);
                let (mark, style, saying) = match &ticket.doing {
                    Doing::Running => (
                        "⚙",
                        Style::default().fg(Color::Yellow),
                        "running".to_string(),
                    ),
                    Doing::Queued => ("‖", dim, "queued".to_string()),
                    Doing::Waiting => (
                        "⚠",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        "waiting on you".to_string(),
                    ),
                    // Not a question about the work, a run that wants
                    // starting again — never conflated with waiting
                    // (§FS-005-dispatch.15).
                    Doing::Dropped => (
                        "✗",
                        Style::default().fg(Color::Red),
                        "dropped by a run that died — a new run takes it up".to_string(),
                    ),
                    Doing::Claimed { assignee, free } => (
                        "✋",
                        Style::default().fg(Color::Yellow),
                        format!("claimed by {assignee} — free it: {free}"),
                    ),
                };
                // The matter's own words beside the ids: a reader should not
                // need to decode a plan id to know what the work is about.
                let title = if ticket.title.is_empty() {
                    String::new()
                } else {
                    format!("· {}  ", ticket.title)
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("        {mark} "), style),
                    Span::raw(format!("{name}  ")),
                    Span::styled(title, dim),
                    Span::styled(format!("[{state}]  "), dim),
                    Span::raw(saying),
                ]));
            }
            if op.done > 0 || op.cancelled > 0 {
                let mut over = Vec::new();
                if op.done > 0 {
                    over.push(Span::styled(
                        format!("✓ {} finished", op.done),
                        Style::default().fg(Color::Green),
                    ));
                }
                if op.cancelled > 0 {
                    if !over.is_empty() {
                        over.push(Span::styled(" · ".to_string(), dim));
                    }
                    over.push(Span::styled(format!("⊘ {} cancelled", op.cancelled), dim));
                }
                let mut spans = vec![Span::raw("        ".to_string())];
                spans.extend(over);
                lines.push(Line::from(spans));
            }
        }
        lines
    }

    pub fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect, progress: Option<String>) {
        self.viewport = area.height;
        let lines = self.lines(progress.as_deref());
        let max_scroll = lines.len().saturating_sub(area.height as usize);
        self.scroll = self.scroll.min(max_scroll as u16);
        frame.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::ItemKind;
    use crate::work::runtime::watch::BoardTicket;

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

    fn ticket(id: &str, doing: Doing) -> BoardTicket {
        BoardTicket {
            plan_id: "forge-demo-17".to_string(),
            ticket: id.to_string(),
            state: Some("fix".to_string()),
            doing,
            item: Some("forge:demo/17".to_string()),
            title: "Humanize durations".to_string(),
            plan: PathBuf::from("/w/demo/panta/forge-demo-17.rhei.md"),
        }
    }

    fn plan() -> PathBuf {
        PathBuf::from("/w/demo/panta/forge-demo-17.rhei.md")
    }

    /// Runs as board rows. The board is one cursor over two kinds of
    /// operation (§FS-005-dispatch.17), and these tests are about the
    /// runtime's kind.
    fn rows(ops: Vec<OpRow>) -> Vec<Row> {
        ops.into_iter().map(|op| Row::Op(Box::new(op))).collect()
    }

    /// A job of ephor's own, mid-flight (§FS-005-dispatch.17).
    fn job_row() -> Row {
        Row::Job(crate::seams::jobs::Job {
            id: "20260818T090000.000Z-rebase".to_string(),
            dir: PathBuf::from("/state/ephor/jobs/20260818T090000.000Z-rebase"),
            record: crate::seams::jobs::Record {
                version: 1,
                project: "demo".to_string(),
                item: Some("forge:demo/17".to_string()),
                icon: "⤴".to_string(),
                description: "rebase onto master (1582 behind as of Nov 21)".to_string(),
                root: PathBuf::from("/w/demo"),
                workspace: Some(PathBuf::from("/w/demo/you/ABC-42")),
                action: None,
                branch: None,
                window: None,
                windowed: false,
                steps: Vec::new(),
                dossier: Vec::new(),
                started: "2026-08-18T09:00:00Z".to_string(),
            },
            live: true,
            ended: None,
        })
    }

    fn running_row() -> OpRow {
        OpRow {
            op: Operation {
                project: "demo".to_string(),
                root: PathBuf::from("/w/demo/panta"),
                live: true,
                dashboard: Some("http://127.0.0.1:39114".to_string()),
                quiet: Some(12),
                tickets: vec![
                    ticket("fix-gate-1", Doing::Running),
                    ticket("answer-1", Doing::Queued),
                ],
                done: 2,
                cancelled: 1,
                machine_unread: None,
                identity: None,
                stop: None,
                plans: vec![plan()],
            },
            item: Some(item()),
            plan: Some(plan()),
        }
    }

    /// A live run that named itself, with the way in and the way out beside it.
    fn identified_row() -> OpRow {
        let mut row = running_row();
        row.op.identity = Some(crate::work::runtime::watch::RunIdentity {
            id: Some("3f9a2c".to_string()),
            pid: Some(48213),
            control_url: Some("http://127.0.0.1:54321".to_string()),
            started_at: None,
            headless: true,
        });
        row.op.stop = Some("the-runner stop 3f9a2c".to_string());
        row
    }

    /// A live run says which run it is, and carries the runner's own command
    /// for stopping it — shown, never run (§FS-005-dispatch.20). `a` puts the
    /// runner's own surface on it; a row with no identity says so rather than
    /// doing nothing, and an idle root has nothing to attach to at all.
    #[test]
    fn the_row_names_its_run_shows_the_stop_and_attaches_on_a() {
        let mut screen = OperationsScreen::new(rows(vec![identified_row()]), None);
        let text = text(&screen, None);
        assert!(text.contains("run 3f9a2c (a)"), "{text}");
        assert!(text.contains("stop it: the-runner stop 3f9a2c"), "{text}");

        match screen.handle_key(KeyCode::Char('a')) {
            Action::AttachRun { id, root } => {
                assert_eq!(id, "3f9a2c");
                assert_eq!(root, PathBuf::from("/w/demo/panta"));
            }
            _ => panic!("expected AttachRun"),
        }

        // A live run that left no descriptor is live from the lock alone
        // (§AR-007-runtime.3): there is nothing to attach to, and the row says
        // that rather than doing nothing (§REQ-001-boundary.1).
        let mut screen = OperationsScreen::new(rows(vec![running_row()]), None);
        match screen.handle_key(KeyCode::Char('a')) {
            Action::SetMessage(said) => assert!(said.contains("no id"), "{said}"),
            _ => panic!("expected a sentence"),
        }
        // And a root nothing is running on is not a run to watch.
        let mut screen = OperationsScreen::new(rows(vec![claimed_row()]), None);
        match screen.handle_key(KeyCode::Char('a')) {
            Action::SetMessage(said) => assert!(said.contains("Nothing is running"), "{said}"),
            _ => panic!("expected a sentence"),
        }
    }

    /// A second execution root — the board is one row per root, so two rows
    /// are two roots.
    fn claimed_row() -> OpRow {
        OpRow {
            op: Operation {
                project: "demo".to_string(),
                root: PathBuf::from("/w/other/panta"),
                live: false,
                dashboard: None,
                quiet: None,
                tickets: vec![ticket(
                    "fix-gate-1",
                    Doing::Claimed {
                        assignee: "luna".to_string(),
                        free: "the-runner release forge-demo-17.fix-gate-1".to_string(),
                    },
                )],
                done: 0,
                cancelled: 0,
                machine_unread: None,
                identity: None,
                stop: None,
                plans: vec![plan()],
            },
            item: None,
            plan: Some(plan()),
        }
    }

    /// A run that parked a ticket has usually exited, so the row is not live
    /// and nobody claimed it (§FS-005-dispatch.15). Naming that "claimed"
    /// would put a person's name on work no person took.
    #[test]
    fn a_parked_row_waits_on_the_reader_rather_than_reading_as_claimed() {
        let mut row = claimed_row();
        row.op.tickets = vec![ticket("fix-gate-1", Doing::Waiting)];
        let text = text(&OperationsScreen::new(rows(vec![row]), None), None);
        assert!(text.contains("waiting on you"), "{text}");
        assert!(!text.contains("claimed, not scheduled"), "{text}");
    }

    fn text(screen: &OperationsScreen, progress: Option<&str>) -> String {
        screen
            .lines(progress)
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

    /// The board says what is running, what waits its turn, how long a live
    /// run has been silent, and what finished — and the refresh reports here
    /// additionally (§FS-001-forge-interface.7, §FS-005-dispatch.15).
    #[test]
    fn a_running_operation_reads_whole() {
        let screen = OperationsScreen::new(rows(vec![running_row()]), None);
        let text = text(&screen, Some("Refreshing demo (1/3)…"));
        assert!(text.contains("⟳ Refreshing demo (1/3)…"), "{text}");
        assert!(text.contains("▶ demo · /w/demo/panta"), "{text}");
        assert!(
            text.contains("running · quiet 12m · dashboard (o)"),
            "{text}"
        );
        assert!(
            text.contains("⚙ forge-demo-17.fix-gate-1  · Humanize durations  [fix]  running"),
            "{text}"
        );
        assert!(
            text.contains("‖ forge-demo-17.answer-1  · Humanize durations  [fix]  queued"),
            "{text}"
        );
        assert!(text.contains("✓ 2 finished · ⊘ 1 cancelled"), "{text}");
    }

    /// A dead run's leavings are their own flavour on the screen, never
    /// worded as a question about the work: the row says a run died here,
    /// and the ticket says what happens next (§FS-005-dispatch.15).
    #[test]
    fn a_dropped_ticket_says_a_run_died_not_waiting() {
        let mut row = claimed_row();
        row.op.tickets = vec![ticket("fix-gate-1", Doing::Dropped)];
        let text = text(&OperationsScreen::new(rows(vec![row]), None), None);
        assert!(text.contains("a run died here"), "{text}");
        assert!(
            text.contains(
                "✗ forge-demo-17.fix-gate-1  · Humanize durations  [fix]  \
                           dropped by a run that died — a new run takes it up"
            ),
            "{text}"
        );
        assert!(!text.contains("waiting on you"), "{text}");
        assert!(!text.contains("claimed, not scheduled"), "{text}");
    }

    /// A root whose machine could not be read says so on its row: queued and
    /// finished are withheld in the data, and a silent zero would read as
    /// nothing done (§FS-005-dispatch.15).
    #[test]
    fn an_unreadable_machine_is_said_on_the_row() {
        let mut row = running_row();
        row.op.machine_unread =
            Some("no states.yaml — nothing judged queued or finished".to_string());
        let text = text(&OperationsScreen::new(rows(vec![row]), None), None);
        assert!(
            text.contains("running · no states.yaml — nothing judged queued or finished"),
            "{text}"
        );
    }

    /// A claim with no run behind it is its own flavour, with the runner's
    /// own words for freeing it — reported, never offered as a key.
    #[test]
    fn a_claim_is_reported_with_the_remedy() {
        let screen = OperationsScreen::new(rows(vec![claimed_row()]), None);
        let text = text(&screen, None);
        assert!(text.contains("✋ demo · /w/other/panta"), "{text}");
        assert!(text.contains("claimed, not scheduled"), "{text}");
        assert!(
            text.contains("claimed by luna — free it: the-runner release forge-demo-17.fix-gate-1"),
            "{text}"
        );
    }

    /// The empty board is the shape most installations see, and it is
    /// correct rather than broken: the refresh row, and the workable rung's
    /// sentence where a runtime is missing (§FS-005-dispatch.15).
    #[test]
    fn an_empty_board_is_the_refresh_row_and_the_reason() {
        let screen = OperationsScreen::new(
            Vec::new(),
            Some(
                "acme-runtime is not on PATH; ephor writes the tickets but the runtime runs them."
                    .to_string(),
            ),
        );
        let text = text(&screen, None);
        assert!(text.contains("⟳ refresh — idle · r starts one"), "{text}");
        assert!(text.contains("acme-runtime is not on PATH"), "{text}");
        assert!(text.contains("nothing is running"), "{text}");
    }

    /// Enter goes to the matter, or to the plan where the operation has
    /// none; o opens the dashboard only where a live run published one.
    #[test]
    fn keys_go_to_the_matter_the_plan_and_the_dashboard() {
        let mut screen = OperationsScreen::new(rows(vec![running_row(), claimed_row()]), None);
        match screen.handle_key(KeyCode::Enter) {
            Action::OpenThread { item, or_url } => {
                assert_eq!(item.id, "forge:demo/17");
                assert!(or_url);
            }
            _ => panic!("expected the matter"),
        }
        match screen.handle_key(KeyCode::Char('o')) {
            Action::OpenUrl(Some(url)) => assert_eq!(url, "http://127.0.0.1:39114"),
            _ => panic!("expected the dashboard"),
        }
        screen.handle_key(KeyCode::Char('j'));
        match screen.handle_key(KeyCode::Enter) {
            Action::ReadPlan(plan) => {
                assert!(plan.ends_with("forge-demo-17.rhei.md"), "{plan:?}")
            }
            _ => panic!("an operation with no matter opens the plan"),
        }
        assert!(matches!(
            screen.handle_key(KeyCode::Char('o')),
            Action::SetMessage(_)
        ));
        assert!(matches!(screen.handle_key(KeyCode::Esc), Action::Back));
        assert!(matches!(
            screen.handle_key(KeyCode::Char('r')),
            Action::Refresh
        ));
    }

    /// The tick's cheap half patches liveness in place, and a flip is the
    /// cue to rebuild (§FS-005-dispatch.15.1). What it reports is what moved:
    /// a probe that found nothing new asks for no frame, because a board
    /// redrawing itself every couple of seconds regardless is paying to show
    /// the reader what they are already looking at.
    #[test]
    fn the_pulse_patches_in_place_and_says_what_moved() {
        use crate::work::runtime::watch::Pulse;
        let mut screen = OperationsScreen::new(rows(vec![running_row()]), None);
        // Same liveness, moved badges: something changed, nothing flipped.
        let found = screen.repulse(|_| Pulse {
            live: true,
            dashboard: None,
            quiet: Some(13),
            identity: None,
            stop: None,
        });
        assert!(found.changed);
        assert!(!found.flipped);
        let text = text(&screen, None);
        assert!(text.contains("quiet 13m"), "{text}");
        assert!(!text.contains("dashboard (o)"), "{text}");

        // The very same answer again: nothing to show, nothing to redraw.
        let found = screen.repulse(|_| Pulse {
            live: true,
            dashboard: None,
            quiet: Some(13),
            identity: None,
            stop: None,
        });
        assert!(!found.changed);
        assert!(!found.flipped);

        // The run died: the OS released the lock, the probe sees it, and the
        // caller is told to rebuild the rows whose flavour depended on it.
        let found = screen.repulse(|_| Pulse {
            live: false,
            dashboard: None,
            quiet: None,
            identity: None,
            stop: None,
        });
        assert!(found.changed);
        assert!(found.flipped);
    }

    /// The tick re-reads who the run is, so it re-reads how the run is stopped
    /// (§FS-005-dispatch.20). The stop line used to be cleared on every pulse
    /// and re-composed only by a full rebuild, so a board opened on a steadily
    /// live run said `run 3f9a2c (a)` and stopped saying `stop it:` two seconds
    /// later — the one line §20 puts on the row precisely because the board
    /// starts nothing and stops nothing itself.
    #[test]
    fn the_stop_line_survives_a_tick_on_a_run_that_is_still_going() {
        use crate::work::runtime::watch::{Pulse, RunIdentity};
        let identity = || {
            Some(RunIdentity {
                id: Some("3f9a2c".to_string()),
                pid: Some(48213),
                control_url: Some("http://127.0.0.1:54321".to_string()),
                started_at: None,
                headless: true,
            })
        };
        let mut screen = OperationsScreen::new(rows(vec![identified_row()]), None);
        let found = screen.repulse(|_| Pulse {
            live: true,
            dashboard: None,
            quiet: None,
            identity: identity(),
            stop: Some("the-runner stop 3f9a2c".to_string()),
        });
        assert!(!found.flipped);
        let still = text(&screen, None);
        assert!(still.contains("run 3f9a2c (a)"), "{still}");
        assert!(still.contains("stop it: the-runner stop 3f9a2c"), "{still}");

        // And a run that ended with another started in its place is two runs:
        // the row says which it is looking at, and how to end *that* one.
        let found = screen.repulse(|_| Pulse {
            live: true,
            dashboard: None,
            quiet: None,
            identity: Some(RunIdentity {
                id: Some("b41d70".to_string()),
                ..identity().expect("an identity")
            }),
            stop: Some("the-runner stop b41d70".to_string()),
        });
        assert!(found.changed);
        let again = text(&screen, None);
        assert!(again.contains("run b41d70 (a)"), "{again}");
        assert!(again.contains("stop it: the-runner stop b41d70"), "{again}");
    }

    /// A rebuild fires from the tick and from every refresh landing
    /// (§FS-001-forge-interface.7), and rows can arrive above the cursor. The
    /// cursor belongs to the operation, not to the index it had — otherwise
    /// `enter` acts on something the reader never selected
    /// (§FS-004-quick-actions.2).
    #[test]
    fn the_cursor_follows_the_operation_when_a_row_arrives_above_it() {
        let mut screen = OperationsScreen::new(rows(vec![claimed_row()]), None);
        match screen.handle_key(KeyCode::Enter) {
            Action::ReadPlan(_) => {}
            _ => panic!("the claim has no matter, so Enter opens its plan"),
        }
        screen.replace(rows(vec![running_row(), claimed_row()]), None);
        // Still the claim, now the second row.
        assert_eq!(screen.selected, 1);
        match screen.handle_key(KeyCode::Enter) {
            Action::ReadPlan(_) => {}
            _ => panic!("the cursor stayed on the operation it was on"),
        }

        // The operation is gone: the index stands, and the cursor lands on
        // whatever took its place.
        screen.replace(rows(vec![running_row()]), None);
        assert_eq!(screen.selected, 0);
    }

    /// An operation is several lines, so a selection that moves without the
    /// scroll following it puts the cursor off screen and leaves the reader
    /// pressing `enter` on something they cannot see
    /// (§FS-004-quick-actions.2).
    #[test]
    fn the_scroll_follows_the_selected_operation() {
        let mut screen = OperationsScreen::new(rows(vec![running_row(), claimed_row()]), None);
        // Four header lines, then the running row's four (its own, two
        // tickets, the finished count), then the claim's two.
        assert_eq!(screen.row_offset(0), 4);
        assert_eq!(screen.row_offset(1), 8);

        // A viewport too short to hold both: moving down scrolls just far
        // enough to show the claim's last line, moving back up scrolls to the
        // running row's first.
        screen.viewport = 4;
        screen.handle_key(KeyCode::Char('j'));
        assert_eq!(screen.selected, 1);
        assert_eq!(screen.scroll, 8 + 2 - 4);
        screen.handle_key(KeyCode::Char('k'));
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.scroll, 4);
    }

    /// The offsets `follow_selection` computes are the offsets `lines` builds
    /// — measured against the lines themselves, so the two cannot drift.
    #[test]
    fn a_rows_offset_is_where_that_row_actually_is() {
        for refusal in [None, Some("no runtime is bound".to_string())] {
            let screen = OperationsScreen::new(rows(vec![running_row(), claimed_row()]), refusal);
            let rendered: Vec<String> = screen
                .lines(None)
                .iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect::<String>()
                })
                .collect();
            assert!(
                rendered[screen.row_offset(0)].contains("▶ demo"),
                "{rendered:?}"
            );
            assert!(
                rendered[screen.row_offset(1)].contains("✋ demo"),
                "{rendered:?}"
            );
        }
    }

    /// A job stands among the operations and stands first: it is what the
    /// reader pressed a key for a moment ago, and a board that filed it under
    /// the runtime's work would answer "did that start?" with a scroll
    /// (§FS-005-dispatch.17).
    #[test]
    fn a_job_is_an_operation_and_reads_ahead_of_the_runtimes() {
        let mut all = vec![job_row()];
        all.extend(rows(vec![running_row()]));
        let text = text(&OperationsScreen::new(all, None), None);
        let job_at = text.find("rebase onto master").expect("the job");
        let run_at = text.find("/w/demo/panta").expect("the run");
        assert!(job_at < run_at, "{text}");
        assert!(text.contains("job · e reads it"), "{text}");
    }

    /// A job's inspection is its log, wherever the run's would be its plan —
    /// and following is asked for only while it is still writing.
    #[test]
    fn a_job_row_reads_its_log_and_follows_a_live_one() {
        let mut screen = OperationsScreen::new(vec![job_row()], None);
        match screen.handle_key(KeyCode::Char('e')) {
            Action::ReadLog { path, following } => {
                assert!(path.ends_with("log"), "{path:?}");
                assert!(following, "a live job is followed");
            }
            _ => panic!("a job is read from its log, not its plan"),
        }
        match screen.handle_key(KeyCode::Enter) {
            Action::ReadLog { .. } => {}
            _ => panic!("Enter on a job is the same reading"),
        }
    }

    /// The cursor stays on the operation it was on, whichever kind that is:
    /// a run appearing above a job must not silently change what the next key
    /// acts on (§FS-005-dispatch.15.1).
    #[test]
    fn the_cursor_keeps_the_job_it_was_on_when_a_run_appears_above_it() {
        let mut screen = OperationsScreen::new(vec![job_row()], None);
        assert_eq!(screen.selected, 0);
        let mut all = rows(vec![running_row()]);
        all.push(job_row());
        screen.replace(all, None);
        assert_eq!(screen.selected, 1, "still the job");
        match screen.handle_key(KeyCode::Char('e')) {
            Action::ReadLog { .. } => {}
            _ => panic!("the cursor stayed on the job"),
        }
    }
}
