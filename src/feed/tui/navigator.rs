//! The navigation screen: org/project/type/branch trees.
//!
//! Three modes: Stream (full tree across organizations, unread-only by
//! default), Projects (org-grouped summary rows), and Detail (one project
//! plus all its branches — the row's and the ones found checked out,
//! §FS-008-attribution.2). Tab toggles Stream/Projects; Enter on a project row
//! drills into Detail.

use chrono::Utc;
use ratatui::crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::api::JobSubject;
use crate::feed::cache::{self, Seen};
use crate::feed::gate::Gate;
use crate::feed::model::{Item, ItemKind, ItemRole};
use crate::feed::render::age;
use crate::forest::{Staleness, Standing, Trail};

use super::{highlight_style, Action, BranchInfo, Ctx, WorkBadge};

/// One category's filter (§FS-003-feed-categories.1).
type SectionFilter = fn(&Item) -> bool;

/// The categories of §FS-003-feed-categories.1, in the order they are worked.
/// An item lands in exactly one: every filter but Recent's excludes finished
/// work, and the kinds do not overlap.
const SECTIONS: [(&str, SectionFilter); 9] = [
    ("Status", |item| {
        item.kind == ItemKind::Status && !item.is_finished()
    }),
    ("My Pull Requests", |item| {
        item.kind == ItemKind::Pr && item.role != Some(ItemRole::Reviewer) && !item.is_finished()
    }),
    ("Reviewing", |item| {
        item.kind == ItemKind::Pr && item.role == Some(ItemRole::Reviewer) && !item.is_finished()
    }),
    // Gate results ride on the matter they are about (§FS-007-matters.5);
    // what is left for this section is the periodic build that belongs to no
    // change of its own.
    ("CI", |item| {
        item.kind == ItemKind::Ci && !item.is_finished()
    }),
    ("My Issues", |item| {
        item.kind == ItemKind::Issue && item.role != Some(ItemRole::Reviewer) && !item.is_finished()
    }),
    ("Participating", |item| {
        item.kind == ItemKind::Issue && item.role == Some(ItemRole::Reviewer) && !item.is_finished()
    }),
    // The project's own tasks, out of a store in its checkout
    // (§FS-006-project-interface.7). Nobody is a reviewer on one, so the role
    // does not divide them the way it divides issues.
    ("Tasks", |item| {
        item.kind == ItemKind::Task && !item.is_finished()
    }),
    ("Messages", |item| {
        item.kind == ItemKind::Message && !item.is_finished()
    }),
    ("Recent", |item| item.is_finished()),
];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Stream,
    Projects,
    Detail,
}

#[derive(Clone)]
struct Row {
    item: Item,
    stale: bool,
    /// PR items: whether the PR's branch workspace is on disk (None when
    /// unknowable — no branch recorded, or no branch workspaces).
    checked_out: Option<bool>,
    /// What has been handed to the runtime about this item, if anything
    /// (§FS-005-dispatch.4).
    work: Option<WorkBadge>,
    /// Why it is back in front of the reader, where the store remembers what
    /// it looked like when it was read (§FS-007-matters.5).
    resurfacing: Option<String>,
    /// The one line a job that ran about this matter left when it ended
    /// (§FS-005-dispatch.17).
    news: Option<String>,
}

/// One line in a tree view.
enum Entry {
    /// Organization header, not selectable.
    Org(String),
    /// Project row; Enter opens the detail view. Carries the finished-job
    /// lines filed under this project whose own row is nowhere on this
    /// screen, because news with nowhere to land is news that is lost
    /// (§FS-005-dispatch.17) — each with the subject it was filed under, so
    /// that opening the row reads exactly those lines off.
    Project(String, Vec<(JobSubject, String)>),
    /// Type-level section header, not selectable.
    Header(&'static str),
    /// Branch row: group header inside a type section, or the branch
    /// overview in the detail view. Carries its project id, whether its
    /// workspace is checked out on disk, how far it trails main and how far
    /// it trails its own published copy (each summed over the workspace's
    /// repos, and each carrying the day it was measured as of —
    /// §FS-004-quick-actions.6) — two distances, two facts
    /// (§DA-003-upstream-is-the-published-copy) — how many items are
    /// filed under it, and the one line a job that ran on it left when it
    /// ended (§FS-005-dispatch.17). Every fact on the row is settled here, at
    /// rebuild: the last of them was being looked up inside `draw`, which
    /// allocated two keys per branch row per frame to answer something that
    /// cannot change between two keystrokes.
    Branch(
        String,
        BranchInfo,
        bool,
        Option<Trail>,
        Option<Trail>,
        usize,
        Option<String>,
    ),
    /// Group header for items linked to none of the project's branches —
    /// neither one the row names nor one with a workspace on disk.
    Unassigned,
    Item(Row),
}

fn selectable(entry: &Entry) -> bool {
    !matches!(entry, Entry::Org(_) | Entry::Header(_) | Entry::Unassigned)
}

/// What the cursor is on: one of the three selectable rows.
enum Selected {
    Item(Item),
    Branch(String, BranchInfo),
    Project(String),
}

pub(crate) struct NavigatorState {
    mode: Mode,
    stream_entries: Vec<Entry>,
    stream_state: ListState,
    project_entries: Vec<Entry>,
    project_state: ListState,
    detail_project: usize,
    detail_entries: Vec<Entry>,
    detail_state: ListState,
    /// The subjects whose finished-job line the reader has just opened, taken
    /// by the shell after each key (§FS-005-dispatch.17). The line is news,
    /// and news that has been opened has been read.
    read_news: Vec<JobSubject>,
}

impl NavigatorState {
    pub fn new() -> Self {
        NavigatorState {
            mode: Mode::Stream,
            stream_entries: Vec::new(),
            stream_state: ListState::default(),
            project_entries: Vec::new(),
            project_state: ListState::default(),
            detail_project: 0,
            detail_entries: Vec::new(),
            detail_state: ListState::default(),
            read_news: Vec::new(),
        }
    }

    /// What the reader has opened since this was last asked, and so has read.
    pub fn take_read_news(&mut self) -> Vec<JobSubject> {
        std::mem::take(&mut self.read_news)
    }

    /// The subjects filed under the row the cursor is on, and only where that
    /// row is carrying a line: an ordinary keypress on an ordinary row must
    /// not churn the tree.
    fn news_under_cursor(&mut self) -> Vec<JobSubject> {
        let (entries, state) = self.tree();
        match state.selected().and_then(|index| entries.get(index)) {
            Some(Entry::Item(row)) if row.news.is_some() => vec![JobSubject::Matter(
                row.item.project.clone(),
                row.item.id.clone(),
            )],
            Some(Entry::Branch(project, branch, _, _, _, _, news)) if news.is_some() => {
                vec![JobSubject::Branch(project.clone(), branch.branch.clone())]
            }
            // The project row carries what had no row of its own, so opening
            // it reads exactly those off and nothing else.
            Some(Entry::Project(_, news)) => {
                news.iter().map(|(subject, _)| subject.clone()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Mark the row under the cursor read, where it is carrying news.
    fn read_selection(&mut self) {
        let read = self.news_under_cursor();
        self.read_news.extend(read);
    }

    pub fn has_stream_entries(&self) -> bool {
        !self.stream_entries.is_empty()
    }

    /// In Detail mode, refresh only the shown project.
    pub fn refresh_filter(&self, ctx: &Ctx) -> Option<String> {
        match self.mode {
            Mode::Detail => Some(ctx.projects[self.detail_project].clone()),
            _ => None,
        }
    }

    pub fn title(&self, ctx: &Ctx) -> String {
        let mode = if ctx.unread_only {
            "unread"
        } else {
            "everything"
        };
        match self.mode {
            Mode::Stream => format!(" ephor stream ({mode})"),
            Mode::Projects => " ephor projects".to_string(),
            Mode::Detail => format!(" ephor — {} ({mode})", ctx.projects[self.detail_project]),
        }
    }

    pub fn footer(&self) -> &'static str {
        match self.mode {
            Mode::Stream => " j/k move  enter thread  c gate  w work  o browser  x actions  m done  a all done  u unread  ; ops  tab projects  r refresh  q quit",
            Mode::Projects => " j/k move  enter view project  ; ops  tab stream  r refresh  q quit",
            Mode::Detail => " j/k move  enter thread  c gate  w work  o browser  x actions  m done  [/] project  esc back  u unread  ; ops  r refresh  q quit",
        }
    }

    pub fn rebuild(&mut self, ctx: &Ctx) {
        self.rebuild_stream(ctx);
        self.rebuild_projects(ctx);
        if self.mode == Mode::Detail {
            self.rebuild_detail(ctx);
        }
    }

    /// Type sections for one project: per kind a header, then per-branch
    /// groups — the row's branches first, then the ones found checked out —
    /// then items linked to none of them.
    fn type_section_entries(&self, ctx: &Ctx, project: &str) -> Vec<Entry> {
        let mut entries = Vec::new();
        let Some(feed) = ctx.feed(project) else {
            return entries;
        };
        let branches = ctx.branches(project).to_vec();

        let now = Utc::now();
        for (header, section_filter) in SECTIONS {
            let mut rows: Vec<Row> = feed
                .items()
                .filter(|item| item.is_visible(now, ctx.recent_days))
                .filter(|item| section_filter(item))
                .filter(|item| !ctx.unread_only || cache::is_unread(&ctx.seen, item))
                .map(|item| Row {
                    stale: feed.is_stale(&item.source),
                    checked_out: ctx.item_checked_out(&item),
                    work: ctx.work.get(&item.id).cloned(),
                    resurfacing: ctx.resurfacing.get(&item.id).cloned(),
                    news: ctx
                        .job_news
                        .get(&JobSubject::Matter(project.to_string(), item.id.clone()))
                        .cloned(),
                    item,
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            rows.sort_by(|a, b| {
                b.item
                    .needs_response
                    .cmp(&a.item.needs_response)
                    .then(b.item.updated_at.cmp(&a.item.updated_at))
            });
            entries.push(Entry::Header(header));

            // Where each row was placed when the feeds were read: against the
            // whole branch list, so a row is filed under the branch it is on
            // rather than the first that resembles it (§FS-008-attribution.2).
            let placed: Vec<Option<usize>> = rows
                .iter()
                .map(|row| ctx.item_branch(project, &row.item))
                .collect();
            for (position, branch) in branches.iter().enumerate() {
                let matching: Vec<usize> = (0..rows.len())
                    .filter(|index| placed[*index] == Some(position))
                    .collect();
                if matching.is_empty() {
                    continue;
                }
                entries.push(Entry::Branch(
                    project.to_string(),
                    branch.clone(),
                    ctx.branch_checked_out(project, branch),
                    ctx.branch_behind(project, &branch.branch)
                        .and_then(Staleness::trail),
                    ctx.branch_standing(project, &branch.branch)
                        .and_then(Standing::upstream_trail),
                    ctx.branch_linked(project, branch),
                    ctx.job_news
                        .get(&JobSubject::Branch(
                            project.to_string(),
                            branch.branch.clone(),
                        ))
                        .cloned(),
                ));
                for index in matching {
                    entries.push(Entry::Item(rows[index].clone()));
                }
            }
            let unassigned: Vec<usize> = (0..rows.len())
                .filter(|index| placed[*index].is_none())
                .collect();
            if !unassigned.is_empty() {
                entries.push(Entry::Unassigned);
                for index in unassigned {
                    entries.push(Entry::Item(rows[index].clone()));
                }
            }
        }
        entries
    }

    fn rebuild_stream(&mut self, ctx: &Ctx) {
        let was = selected_identity(&self.stream_entries, &self.stream_state);
        self.stream_entries.clear();
        for org in &ctx.orgs {
            let mut org_entries = Vec::new();
            for project in ctx.org_projects(&org.id) {
                let sections = self.type_section_entries(ctx, &project);
                let unplaced = unplaced_news(ctx, &project, &sections);
                // A project with nothing to show still gets its row where a
                // job that ran on it has something to say: news with nowhere
                // to land is news that is lost (§FS-005-dispatch.17).
                if sections.is_empty() && unplaced.is_empty() {
                    continue;
                }
                org_entries.push(Entry::Project(project, unplaced));
                org_entries.extend(sections);
            }
            if org_entries.is_empty() {
                continue;
            }
            self.stream_entries.push(Entry::Org(ctx.org_label(org)));
            self.stream_entries.append(&mut org_entries);
        }
        // Last, and only where there is something in it: what nothing claimed,
        // and what two projects claimed equally. Visible rather than dropped —
        // a guess that lands wrong amends someone's matter silently
        // (§FS-008-attribution.4).
        let now = Utc::now();
        let mut orphans: Vec<Row> = ctx
            .unattributed
            .iter()
            .filter(|item| item.is_visible(now, ctx.recent_days))
            .filter(|item| !ctx.unread_only || cache::is_unread(&ctx.seen, item))
            .map(|item| Row {
                stale: false,
                checked_out: None,
                work: None,
                resurfacing: ctx.resurfacing.get(&item.id).cloned(),
                news: ctx
                    .job_news
                    .get(&JobSubject::Matter(item.project.clone(), item.id.clone()))
                    .cloned(),
                item: item.clone(),
            })
            .collect();
        if !orphans.is_empty() {
            orphans.sort_by(|a, b| {
                b.item
                    .needs_response
                    .cmp(&a.item.needs_response)
                    .then(b.item.updated_at.cmp(&a.item.updated_at))
            });
            self.stream_entries.push(Entry::Org(
                "Unattributed — no project claimed these".to_string(),
            ));
            self.stream_entries
                .extend(orphans.into_iter().map(Entry::Item));
        }
        fix_selection(&self.stream_entries, &mut self.stream_state, was);
    }

    fn rebuild_projects(&mut self, ctx: &Ctx) {
        let was = selected_identity(&self.project_entries, &self.project_state);
        self.project_entries.clear();
        for org in &ctx.orgs {
            let projects = ctx.org_projects(&org.id);
            if projects.is_empty() {
                continue;
            }
            self.project_entries.push(Entry::Org(ctx.org_label(org)));
            // The summary has no item or branch rows, so everything filed
            // under the project lands on the project's own row
            // (§FS-005-dispatch.17).
            self.project_entries
                .extend(projects.into_iter().map(|project| {
                    let unplaced = unplaced_news(ctx, &project, &[]);
                    Entry::Project(project, unplaced)
                }));
        }
        fix_selection(&self.project_entries, &mut self.project_state, was);
    }

    fn rebuild_detail(&mut self, ctx: &Ctx) {
        let was = selected_identity(&self.detail_entries, &self.detail_state);
        let project = ctx.projects[self.detail_project].clone();
        self.detail_entries.clear();

        let branches = ctx.branches(&project).to_vec();
        if !branches.is_empty() {
            self.detail_entries.push(Entry::Header("Branches"));
            for branch in &branches {
                self.detail_entries.push(Entry::Branch(
                    project.clone(),
                    branch.clone(),
                    ctx.branch_checked_out(&project, branch),
                    ctx.branch_behind(&project, &branch.branch)
                        .and_then(Staleness::trail),
                    ctx.branch_standing(&project, &branch.branch)
                        .and_then(Standing::upstream_trail),
                    ctx.branch_linked(&project, branch),
                    ctx.job_news
                        .get(&JobSubject::Branch(project.clone(), branch.branch.clone()))
                        .cloned(),
                ));
            }
        }
        let sections = self.type_section_entries(ctx, &project);
        self.detail_entries.extend(sections);
        fix_selection(&self.detail_entries, &mut self.detail_state, was);
    }

    pub fn handle_key(&mut self, ctx: &Ctx, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Tab => {
                self.mode = match self.mode {
                    Mode::Stream => Mode::Projects,
                    _ => Mode::Stream,
                };
                self.rebuild(ctx);
                Action::None
            }
            KeyCode::Char('u') => Action::ToggleUnread,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('j') | KeyCode::Down => {
                self.tree_move(1);
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tree_move(-1);
                Action::None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.tree_select_edge(true);
                Action::None
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.tree_select_edge(false);
                Action::None
            }
            // Every key that opens the row the cursor is on reads its
            // finished-job line off with it (§FS-005-dispatch.17).
            KeyCode::Enter | KeyCode::Char('l') => {
                self.read_selection();
                self.activate(ctx, true)
            }
            KeyCode::Char('o') => {
                self.read_selection();
                self.activate(ctx, false)
            }
            KeyCode::Char('v') => match self.selected_item() {
                Some(item) => Action::OpenThread {
                    item,
                    or_url: false,
                },
                None => Action::None,
            },
            // Where the fact is shown is where the move is offered: a branch
            // row carries ephor's own offers about the branch, with no matter
            // behind it (§FS-004-quick-actions.6).
            KeyCode::Char('x') => {
                self.read_selection();
                match self.selected_row() {
                    Some(Selected::Item(item)) => Action::OpenActionMenu(item),
                    Some(Selected::Branch(project, branch)) => {
                        Action::OpenBranchActions { project, branch }
                    }
                    _ => Action::None,
                }
            }
            // What is being done about this, and what could be
            // (§FS-005-dispatch).
            KeyCode::Char('w') => {
                self.read_selection();
                match self.selected_item() {
                    Some(item) => Action::OpenWork(item),
                    None => Action::None,
                }
            }
            // The counts on the row are a summary of a verdict; `c` is where
            // the verdict itself is (§FS-001-forge-interface.1).
            KeyCode::Char('c') => {
                self.read_selection();
                match self.selected_item() {
                    Some(item) => Action::OpenGate(item),
                    None => Action::None,
                }
            }
            KeyCode::Char('m') | KeyCode::Char('d') | KeyCode::Char(' ') => {
                match self.selected_item() {
                    Some(item) => Action::MarkDone {
                        marks: vec![(item.id, item.updated_at, item.title)],
                        pop: false,
                    },
                    None => Action::None,
                }
            }
            KeyCode::Char('a') => {
                let (entries, _) = self.tree();
                let marks: Vec<_> = entries
                    .iter()
                    .filter_map(|entry| match entry {
                        Entry::Item(row) => Some((
                            row.item.id.clone(),
                            row.item.updated_at,
                            row.item.title.clone(),
                        )),
                        _ => None,
                    })
                    .collect();
                Action::MarkDone { marks, pop: false }
            }
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace if self.mode == Mode::Detail => {
                self.mode = Mode::Projects;
                Action::None
            }
            KeyCode::Char(']') | KeyCode::Char('n') if self.mode == Mode::Detail => {
                self.detail_project = (self.detail_project + 1) % ctx.projects.len();
                self.detail_state.select(None);
                self.rebuild_detail(ctx);
                Action::None
            }
            KeyCode::Char('[') | KeyCode::Char('P') if self.mode == Mode::Detail => {
                self.detail_project =
                    (self.detail_project + ctx.projects.len() - 1) % ctx.projects.len();
                self.detail_state.select(None);
                self.rebuild_detail(ctx);
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Enter/l (`thread_first`) or o (browser) on the selected row.
    fn activate(&mut self, ctx: &Ctx, thread_first: bool) -> Action {
        match self.selected_row() {
            Some(Selected::Item(item)) if thread_first => Action::OpenThread { item, or_url: true },
            Some(Selected::Item(item)) => Action::OpenUrl(item.url),
            Some(Selected::Branch(project, branch)) => {
                Action::OpenUrl(ctx.branch_url(&project, &branch))
            }
            Some(Selected::Project(project)) => {
                if let Some(index) = ctx.projects.iter().position(|p| p == &project) {
                    self.detail_project = index;
                    self.mode = Mode::Detail;
                    self.detail_state.select(None);
                    self.rebuild_detail(ctx);
                }
                Action::None
            }
            None => Action::None,
        }
    }

    fn selected_item(&mut self) -> Option<Item> {
        let (entries, state) = self.tree();
        state.selected().and_then(|index| match entries.get(index) {
            Some(Entry::Item(row)) => Some(row.item.clone()),
            _ => None,
        })
    }

    /// What the cursor is on, in the three shapes a key can act on. Keys that
    /// only work on one of them read this and refuse on the rest, rather than
    /// the row deciding what a key means (§FS-004-quick-actions.2).
    fn selected_row(&mut self) -> Option<Selected> {
        let (entries, state) = self.tree();
        match state.selected().and_then(|index| entries.get(index)) {
            Some(Entry::Item(row)) => Some(Selected::Item(row.item.clone())),
            Some(Entry::Branch(project, branch, ..)) => {
                Some(Selected::Branch(project.clone(), branch.clone()))
            }
            Some(Entry::Project(project, _)) => Some(Selected::Project(project.clone())),
            _ => None,
        }
    }

    fn tree(&mut self) -> (&Vec<Entry>, &mut ListState) {
        match self.mode {
            Mode::Stream => (&self.stream_entries, &mut self.stream_state),
            Mode::Projects => (&self.project_entries, &mut self.project_state),
            Mode::Detail => (&self.detail_entries, &mut self.detail_state),
        }
    }

    fn tree_move(&mut self, delta: isize) {
        let (entries, state) = self.tree();
        if entries.is_empty() {
            return;
        }
        let mut index = state.selected().unwrap_or(0) as isize;
        loop {
            index += delta;
            if index < 0 || index as usize >= entries.len() {
                return;
            }
            if selectable(&entries[index as usize]) {
                state.select(Some(index as usize));
                return;
            }
        }
    }

    fn tree_select_edge(&mut self, top: bool) {
        let (entries, state) = self.tree();
        let range: Box<dyn Iterator<Item = usize>> = if top {
            Box::new(0..entries.len())
        } else {
            Box::new((0..entries.len()).rev())
        };
        for index in range {
            if selectable(&entries[index]) {
                state.select(Some(index));
                return;
            }
        }
    }

    pub fn draw(&mut self, ctx: &Ctx, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let now = Utc::now();
        let seen = &ctx.seen;
        let summary_projects = self.mode == Mode::Projects;
        let entries = match self.mode {
            Mode::Stream => &self.stream_entries,
            Mode::Projects => &self.project_entries,
            Mode::Detail => &self.detail_entries,
        };

        let items: Vec<ListItem> = entries
            .iter()
            .map(|entry| match entry {
                Entry::Org(label) => ListItem::new(Line::from(Span::styled(
                    format!("█ {label}"),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ))),
                Entry::Project(project, news) => {
                    if summary_projects {
                        let (total, unread, respond) = ctx.unread_stats(project);
                        let branches = ctx
                            .branches(project)
                            .iter()
                            .filter(|branch| branch.active)
                            .count();
                        let respond_span = if respond > 0 {
                            Span::styled(
                                format!("{respond} need response"),
                                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                            )
                        } else {
                            Span::styled("nothing pending", Style::default().fg(Color::DarkGray))
                        };
                        let mut spans = vec![
                            Span::styled(
                                format!("  {project:<30}"),
                                Style::default().fg(Color::Cyan),
                            ),
                            Span::raw(format!(
                                "{branches:>2} branches  {total:>3} items  {unread:>3} unread  "
                            )),
                            respond_span,
                        ];
                        if !news.is_empty() {
                            spans.push(news_span(&joined(news)));
                        }
                        ListItem::new(Line::from(spans))
                    } else {
                        let (_, unread, respond) = ctx.unread_stats(project);
                        let mut spans = vec![Span::styled(
                            format!("  ▍{project}"),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )];
                        if respond > 0 {
                            spans.push(Span::styled(
                                format!("   {respond} need response"),
                                Style::default().fg(Color::Red),
                            ));
                        } else if unread > 0 {
                            spans.push(Span::styled(
                                format!("   {unread} unread"),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                        if !news.is_empty() {
                            spans.push(news_span(&joined(news)));
                        }
                        ListItem::new(Line::from(spans))
                    }
                }
                Entry::Header(header) => ListItem::new(Line::from(Span::styled(
                    format!("    ── {header} "),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ))),
                Entry::Unassigned => ListItem::new(Line::from(Span::styled(
                    "      (not linked to a branch)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ))),
                Entry::Branch(_, branch, checked_out, behind, behind_upstream, linked, news) => {
                    ListItem::new(branch_line(
                        branch,
                        *checked_out,
                        *behind,
                        *behind_upstream,
                        *linked,
                        news.as_deref(),
                    ))
                }
                Entry::Item(row) => {
                    let mut line = item_line(row, seen, now);
                    line.spans.insert(0, Span::raw("        "));
                    ListItem::new(line)
                }
            })
            .collect();

        let list = List::new(items)
            .highlight_style(highlight_style())
            .highlight_symbol("▸ ");
        let state = match self.mode {
            Mode::Stream => &mut self.stream_state,
            Mode::Projects => &mut self.project_state,
            Mode::Detail => &mut self.detail_state,
        };
        frame.render_stateful_widget(list, area, state);
    }
}

/// The lines this project's own row has to carry: every finished job filed
/// under it whose own row is not among `shown` (§FS-005-dispatch.17). News
/// with nowhere to land is news that is lost, and a project with no row for
/// the branch a replay ran on is exactly that case.
fn unplaced_news(ctx: &Ctx, project: &str, shown: &[Entry]) -> Vec<(JobSubject, String)> {
    let placed: Vec<JobSubject> = shown
        .iter()
        .filter_map(|entry| match entry {
            Entry::Item(row) => Some(JobSubject::Matter(
                row.item.project.clone(),
                row.item.id.clone(),
            )),
            Entry::Branch(project, branch, ..) => {
                Some(JobSubject::Branch(project.clone(), branch.branch.clone()))
            }
            _ => None,
        })
        .collect();
    ctx.job_news
        .iter()
        .filter(|(subject, _)| subject.project() == project)
        .filter(|(subject, _)| !placed.contains(subject))
        .map(|(subject, line)| (subject.clone(), line.clone()))
        .collect()
}

/// Several lines on one project row, in the order they are filed.
fn joined(news: &[(JobSubject, String)]) -> String {
    news.iter()
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// How a finished job's line is set apart wherever it lands: one colour, on
/// every row that carries one (§FS-005-dispatch.17).
fn news_span(line: &str) -> Span<'static> {
    Span::styled(
        format!("   {line}"),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    )
}

/// One branch row. Both distances a checkout can trail by are on it, kept
/// distinguishable — `N behind` is against the project's main branch, `↓N`
/// against the branch's own published copy
/// (§DA-003-upstream-is-the-published-copy) — because a reader who confuses
/// them replays onto the wrong thing. A copy that is level, or a branch
/// published nowhere, adds nothing: the arrow is news, not a state.
fn branch_line(
    branch: &BranchInfo,
    checked_out: bool,
    behind: Option<Trail>,
    behind_upstream: Option<Trail>,
    linked: usize,
    news: Option<&str>,
) -> Line<'static> {
    let marker = if branch.active {
        Span::styled("      ● ", Style::default().fg(Color::Green))
    } else {
        Span::styled("      ○ ", Style::default().fg(Color::DarkGray))
    };
    let mut spans = vec![
        marker,
        Span::raw(format!(
            "{:<44}",
            branch.branch.chars().take(44).collect::<String>()
        )),
    ];
    if let Some(ticket) = &branch.ticket {
        spans.push(Span::styled(
            format!("{ticket:<10}"),
            Style::default().fg(Color::Yellow),
        ));
    } else {
        spans.push(Span::raw(" ".repeat(10)));
    }
    if checked_out {
        spans.push(Span::styled(
            "✓ checked out",
            Style::default().fg(Color::Green),
        ));
        // The distance and the day it is a distance as of, together: a bare
        // count is a claim about now that nothing here measured
        // (§FS-004-quick-actions.6). Level is still stated — a reader wants to
        // know the comparison was made — but in the colour of a state rather
        // than of news.
        if let Some(trail) = behind {
            let colour = if trail.behind == 0 {
                Color::DarkGray
            } else {
                Color::Yellow
            };
            spans.push(Span::styled(
                format!(" · {}", trail.label()),
                Style::default().fg(colour),
            ));
        }
        // The copy's distance stays the bare arrow, undated: it is news that
        // somebody pushed, and a stale reading of it can only under-report —
        // it never tells the reader their branch is current when it is not.
        if let Some(trail) = behind_upstream.filter(|trail| trail.behind > 0) {
            spans.push(Span::styled(
                format!(" · ↓{}", trail.behind),
                Style::default().fg(Color::Cyan),
            ));
        }
        spans.push(Span::raw("  "));
    } else {
        spans.push(Span::styled(
            "∅ not checked out  ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    if branch.is_release {
        spans.push(Span::styled(
            "release  ",
            Style::default().fg(Color::DarkGray),
        ));
    }
    if linked > 0 {
        spans.push(Span::styled(
            format!("{linked} linked"),
            Style::default().fg(Color::Magenta),
        ));
    }
    // What the last job to run on this branch said as it ended, beside the
    // distance it just moved (§FS-005-dispatch.17).
    if let Some(news) = news {
        spans.push(news_span(news));
    }
    Line::from(spans)
}

fn item_line(row: &Row, seen: &Seen, now: chrono::DateTime<Utc>) -> Line<'static> {
    let Row {
        item,
        stale,
        checked_out,
        work,
        resurfacing: _,
        news: _,
    } = row;
    let (stale, checked_out) = (*stale, *checked_out);
    let marker = if item.needs_response {
        Span::styled(
            "! ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else if cache::is_unread(seen, item) {
        Span::styled("* ", Style::default().fg(Color::Yellow))
    } else {
        Span::raw("  ")
    };
    let mut spans = vec![marker];
    spans.push(Span::styled(
        format!("{:<5}", age(now, item.updated_at)),
        Style::default().fg(Color::DarkGray),
    ));
    match checked_out {
        Some(true) => spans.push(Span::styled("✓ ", Style::default().fg(Color::Green))),
        Some(false) => spans.push(Span::styled("∅ ", Style::default().fg(Color::DarkGray))),
        // Keep titles aligned within a section even where one item's branch
        // is unknown — a section is one kind, and the marker now follows the
        // branch rather than the kind (§FS-004-quick-actions.6).
        None => spans.push(Span::raw("  ")),
    }
    spans.push(Span::raw(item.title.clone()));
    // Why it is back, where ephor can say (§FS-007-matters.5): a row that
    // reappears without a reason sends the reader to re-read everything.
    if let Some(reason) = &row.resurfacing {
        spans.push(Span::styled(
            format!("  {reason}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    if let Some(state) = &item.state {
        spans.push(Span::styled(
            format!("  [{state}]"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(gate) = Gate::of(item) {
        spans.extend(gate_spans(&gate));
    }
    if stale {
        spans.push(Span::styled(
            "  (stale)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    // What is being done about it, after what it is: the row's subject is the
    // item, and the work is the answer to it (§FS-005-dispatch.4).
    if let Some(work) = work {
        let style = if work.stale {
            Style::default().fg(Color::Yellow)
        } else if work.open {
            Style::default().fg(Color::Blue)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!("  {}", work.text), style));
    }
    // What a job ephor ran about this matter said as it ended, on the row it
    // was about rather than at the top of the screen (§FS-005-dispatch.17).
    if let Some(news) = &row.news {
        spans.push(news_span(news));
    }
    Line::from(spans)
}

/// The gate on a PR row: totals across the whole gate, then — when it spans
/// more than one repository — the per-repo breakdown in parentheses.
fn gate_spans(gate: &Gate) -> Vec<Span<'static>> {
    let mut spans = vec![Span::raw("  ")];
    spans.extend(count_spans(gate.passed(), gate.failed(), gate.running()));
    // The forge's refusal, where it has one. It goes next to the counts
    // because it contradicts them: an all-green gate that will not merge is
    // exactly the row a reader would otherwise skip (§FS-001-forge-interface.1).
    if gate.blocked {
        if spans.len() > 1 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            crate::feed::gate::BLOCKED,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }
    if gate.repos.len() < 2 {
        return spans;
    }
    let dim = Style::default().fg(Color::DarkGray);
    spans.push(Span::styled("  (", dim));
    for (index, repo) in gate.repos.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", dim));
        }
        spans.push(Span::styled(format!("{} ", repo.repo), dim));
        spans.extend(count_spans(repo.passed, repo.failed, repo.running));
    }
    spans.push(Span::styled(")", dim));
    spans
}

/// `✓N ✗N ⋯N`, dropping the counts that are zero.
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

/// What a selectable entry is, across a rebuild: this matter, this project,
/// this branch — not the position it happens to occupy.
fn identity(entry: &Entry) -> Option<String> {
    match entry {
        Entry::Item(row) => Some(format!("item:{}", row.item.id)),
        Entry::Project(project, _) => Some(format!("project:{project}")),
        Entry::Branch(project, branch, ..) => Some(format!("branch:{project}:{}", branch.branch)),
        Entry::Org(_) | Entry::Header(_) | Entry::Unassigned => None,
    }
}

/// What the cursor was on before a rebuild, where it was on anything.
fn selected_identity(entries: &[Entry], state: &ListState) -> Option<String> {
    entries.get(state.selected()?).and_then(identity)
}

/// Put the cursor back on the row it was on. Rows arrive under a reader who is
/// still moving — a refresh runs beneath the screen (§FS-001-forge-interface.7)
/// — and a selection kept by index over a list that grew above it silently
/// changes what the next key acts on: the reader presses `x` on the row they
/// were reading and gets the menu for another one.
///
/// The row it was on may also be gone, which is what marking an item done does
/// to it. Then the index stands, and the cursor lands on whatever took its
/// place.
fn fix_selection(entries: &[Entry], state: &mut ListState, was: Option<String>) {
    if let Some(was) = was {
        if let Some(index) = entries
            .iter()
            .position(|entry| identity(entry).as_deref() == Some(was.as_str()))
        {
            state.select(Some(index));
            return;
        }
    }
    match state.selected() {
        Some(index) if index < entries.len() && selectable(&entries[index]) => {}
        _ => state.select((0..entries.len()).find(|index| selectable(&entries[*index]))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: &str) -> Entry {
        Entry::Item(Row {
            item: Item {
                id: id.to_string(),
                project: "widget".to_string(),
                source: "github-prs".to_string(),
                kind: ItemKind::Pr,
                role: None,
                title: id.to_string(),
                url: None,
                state: None,
                needs_response: false,
                updated_at: Utc::now(),
                raw: json!({}),
            },
            stale: false,
            checked_out: None,
            work: None,
            resurfacing: None,
            news: None,
        })
    }

    /// An item belongs to exactly one category, so that the size of a
    /// category is the size of that pile of work and not a double count
    /// (§FS-003-feed-categories.1) — and the project's own task belongs to
    /// **Tasks**, never to My Issues, because an issue is what a forge files
    /// (§FS-006-project-interface.7).
    #[test]
    fn every_kind_lands_in_exactly_one_category_and_a_task_lands_in_tasks() {
        let mut item = match row("rhei:work.1") {
            Entry::Item(row) => row.item,
            _ => unreachable!("the fixture is a row"),
        };
        for (kind, expected) in [
            (ItemKind::Status, "Status"),
            (ItemKind::Pr, "My Pull Requests"),
            (ItemKind::Ci, "CI"),
            (ItemKind::Issue, "My Issues"),
            (ItemKind::Task, "Tasks"),
            (ItemKind::Message, "Messages"),
        ] {
            item.kind = kind;
            let landed: Vec<&str> = SECTIONS
                .iter()
                .filter(|(_, filter)| filter(&item))
                .map(|(header, _)| *header)
                .collect();
            assert_eq!(landed, [expected], "{kind:?}");
        }

        // Finished work leaves its category for Recent, whatever its kind.
        item.kind = ItemKind::Task;
        item.state = Some("closed".to_string());
        let landed: Vec<&str> = SECTIONS
            .iter()
            .filter(|(_, filter)| filter(&item))
            .map(|(header, _)| *header)
            .collect();
        assert_eq!(landed, ["Recent"]);
    }

    fn on(entries: &[Entry], index: usize) -> ListState {
        let mut state = ListState::default();
        state.select(Some(index));
        assert!(selectable(&entries[index]), "the fixture selects a row");
        state
    }

    /// A refresh lands a project's answers while the reader is still moving
    /// through the tree (§FS-001-forge-interface.7), and what arrives can
    /// sort above the cursor. The cursor belongs to the matter, not to the
    /// line number it had (§FS-001-forge-interface.7).
    #[test]
    fn the_cursor_follows_the_matter_when_rows_arrive_above_it() {
        let before = vec![row("pr:1"), row("pr:2")];
        let mut state = on(&before, 1);
        let was = selected_identity(&before, &state);

        let after = vec![row("pr:9"), row("pr:8"), row("pr:1"), row("pr:2")];
        fix_selection(&after, &mut state, was);

        assert_eq!(state.selected(), Some(3));
    }

    /// The same rule across a header that is not selectable: it is the matter
    /// that is looked for, and headers are never it.
    #[test]
    fn a_header_arriving_above_the_cursor_moves_it_too() {
        let before = vec![row("pr:1")];
        let mut state = on(&before, 0);
        let was = selected_identity(&before, &state);

        let after = vec![Entry::Header("My Pull Requests"), row("pr:1")];
        fix_selection(&after, &mut state, was);

        assert_eq!(state.selected(), Some(1));
    }

    /// Marking an item done takes it off an unread-only screen. Nothing to
    /// follow, so the index stands and the row that took its place is
    /// selected — which is what makes `m` `m` `m` work down a pile.
    #[test]
    fn a_row_that_is_gone_leaves_the_cursor_where_it_stood() {
        let before = vec![row("pr:1"), row("pr:2"), row("pr:3")];
        let mut state = on(&before, 1);
        let was = selected_identity(&before, &state);

        let after = vec![row("pr:1"), row("pr:3")];
        fix_selection(&after, &mut state, was);

        assert_eq!(state.selected(), Some(1));
    }

    /// The last row, marked done, has nothing under it: the cursor falls back
    /// to something selectable rather than off the end.
    #[test]
    fn a_cursor_past_the_end_lands_on_a_selectable_row() {
        let before = vec![row("pr:1"), row("pr:2"), row("pr:3")];
        let mut state = on(&before, 2);
        let was = selected_identity(&before, &state);

        let after = vec![Entry::Header("My Pull Requests"), row("pr:1")];
        fix_selection(&after, &mut state, was);

        assert_eq!(state.selected(), Some(1));
    }

    fn branch_info(name: &str) -> BranchInfo {
        BranchInfo {
            branch: name.to_string(),
            ticket: None,
            active: true,
            is_release: false,
            declared: true,
        }
    }

    /// A distance nothing dated — the shape of a repository whose base was
    /// never fetched here (§FS-004-quick-actions.6).
    fn trail(behind: u64) -> Trail {
        Trail { behind, seen: None }
    }

    fn text_of(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// The row says both distances and keeps them apart — `N behind` is
    /// against the project's main branch, `↓N` against the branch's own
    /// published copy (§DA-003-upstream-is-the-published-copy) — down to the
    /// color, so a reader cannot take one number for the other.
    #[test]
    fn the_branch_row_says_both_distances_and_keeps_them_apart() {
        let line = branch_line(
            &branch_info("you/ABC-42"),
            true,
            Some(trail(13)),
            Some(trail(2)),
            0,
            None,
        );
        let text = text_of(&line);
        assert!(text.contains(" · 13 behind"), "{text:?}");
        assert!(text.contains(" · ↓2"), "{text:?}");
        let style_of = |needle: &str| {
            line.spans
                .iter()
                .find(|span| span.content.contains(needle))
                .expect("the span is on the row")
                .style
        };
        assert_ne!(style_of("behind").fg, style_of("↓").fg);
    }

    /// A copy that is level, or a branch published nowhere, adds nothing:
    /// the arrow is news, not a state
    /// (§DA-003-upstream-is-the-published-copy).
    #[test]
    fn a_copy_level_or_unpublished_puts_no_arrow_on_the_row() {
        let level = branch_line(
            &branch_info("you/ABC-42"),
            true,
            Some(trail(0)),
            Some(trail(0)),
            0,
            None,
        );
        assert!(
            text_of(&level).contains(" · level"),
            "{:?}",
            text_of(&level)
        );
        assert!(!text_of(&level).contains('↓'));

        let unpushed = branch_line(
            &branch_info("you/ABC-42"),
            true,
            Some(trail(3)),
            None,
            0,
            None,
        );
        assert!(text_of(&unpushed).contains(" · 3 behind"));
        assert!(!text_of(&unpushed).contains('↓'));
    }

    /// The row says how fresh its comparison is, because the count is only
    /// ever as fresh as the last fetch — and it says nothing where no day was
    /// recorded rather than inventing one. "Up to date" is gone: it claimed
    /// the branch was current when all that was measured was a match against
    /// a copy of unstated age (§FS-004-quick-actions.6).
    #[test]
    fn the_row_says_how_fresh_its_comparison_is() {
        let dated = |behind| Trail {
            behind,
            seen: Some(chrono::Utc::now()),
        };
        let trailing = branch_line(
            &branch_info("you/ABC-42"),
            true,
            Some(dated(13)),
            None,
            0,
            None,
        );
        assert!(
            text_of(&trailing).contains(" · 13 behind as of "),
            "{:?}",
            text_of(&trailing)
        );

        let level = branch_line(
            &branch_info("you/ABC-42"),
            true,
            Some(dated(0)),
            None,
            0,
            None,
        );
        assert!(
            text_of(&level).contains(" · level as of "),
            "{:?}",
            text_of(&level)
        );
        for line in [&trailing, &level] {
            assert!(!text_of(line).contains("up to date"));
        }

        // Never fetched here: the qualifier is left off, not filled in.
        let undated = branch_line(
            &branch_info("you/ABC-42"),
            true,
            Some(trail(0)),
            None,
            0,
            None,
        );
        assert!(
            !text_of(&undated).contains("as of"),
            "{:?}",
            text_of(&undated)
        );
    }

    /// The copy's distance stands on its own where main's could not be
    /// measured, and nothing is measured at all on a branch not on disk.
    #[test]
    fn each_distance_stands_without_the_other() {
        let copy_only = branch_line(
            &branch_info("you/ABC-42"),
            true,
            None,
            Some(trail(4)),
            0,
            None,
        );
        assert!(!text_of(&copy_only).contains("behind"));
        assert!(text_of(&copy_only).contains(" · ↓4"));

        let absent = branch_line(&branch_info("you/ABC-42"), false, None, None, 0, None);
        assert!(text_of(&absent).contains("∅ not checked out"));
        assert!(!text_of(&absent).contains('↓'));
    }

    /// A job that ended says so under the branch it ran on, not at the top of
    /// the screen: a header line naming no branch is news about nothing in
    /// particular, and the reader with three replays going has to guess which
    /// row moved (§FS-005-dispatch.17).
    #[test]
    fn a_finished_job_says_so_under_the_branch_it_ran_on() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = super::super::tests::ctx_with_branch(tmp.path(), None);
        ctx.job_news.insert(
            JobSubject::Branch("widget".to_string(), "you/ABC-42-retry-window".to_string()),
            "⤴ rebase onto master (level as of Aug 23): ok".to_string(),
        );
        let mut navigator = NavigatorState::new();
        navigator.mode = Mode::Detail;
        navigator.rebuild_detail(&ctx);

        let branch = navigator
            .detail_entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Branch(_, branch, checked_out, behind, upstream, linked, news) => {
                    Some(branch_line(
                        branch,
                        *checked_out,
                        *behind,
                        *upstream,
                        *linked,
                        news.as_deref(),
                    ))
                }
                _ => None,
            })
            .expect("the branch row");
        assert!(
            text_of(&branch).contains("rebase onto master (level as of Aug 23): ok"),
            "{:?}",
            text_of(&branch)
        );
        // And it is set apart from the facts that were already on the row.
        let news = branch
            .spans
            .iter()
            .find(|span| span.content.contains(": ok"))
            .expect("the line is a span of its own");
        assert!(news.style.add_modifier.contains(Modifier::BOLD));
    }

    /// News with nowhere to land is news that is lost, so a line whose own row
    /// is not on this screen is carried by the project's row
    /// (§FS-005-dispatch.17).
    #[test]
    fn a_line_with_no_row_of_its_own_is_carried_by_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = super::super::tests::ctx_with_branch(tmp.path(), None);
        let subject =
            JobSubject::Branch("widget".to_string(), "you/ABC-42-retry-window".to_string());
        ctx.job_news
            .insert(subject.clone(), "⤴ rebase onto master: ok".to_string());
        // Nothing of this project is on the screen: the project row takes it.
        let alone = unplaced_news(&ctx, "widget", &[]);
        assert_eq!(alone.len(), 1, "{alone:?}");
        assert_eq!(alone[0].0, subject);

        // The branch's own row is on the screen: the project row takes
        // nothing, because the line is already where it belongs.
        let shown = [Entry::Branch(
            "widget".to_string(),
            branch_info("you/ABC-42-retry-window"),
            true,
            None,
            None,
            0,
            Some("⤴ rebase onto master: ok".to_string()),
        )];
        assert!(unplaced_news(&ctx, "widget", &shown).is_empty());

        // Another project's row is not a place for it either.
        assert!(unplaced_news(&ctx, "gadget", &[]).is_empty());
    }

    /// The line stays under its subject until the reader opens that row, and
    /// opening it is what reads it off (§FS-005-dispatch.17). An ordinary key
    /// on an ordinary row reads nothing.
    #[test]
    fn opening_the_row_reads_its_line_off_and_moving_the_cursor_does_not() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = super::super::tests::ctx_with_branch(tmp.path(), None);
        let subject =
            JobSubject::Branch("widget".to_string(), "you/ABC-42-retry-window".to_string());
        ctx.job_news
            .insert(subject.clone(), "⤴ rebase onto master: ok".to_string());
        let mut navigator = NavigatorState::new();
        navigator.mode = Mode::Detail;
        navigator.rebuild_detail(&ctx);

        // Moving onto the row is not opening it: the line is still there to
        // be read.
        navigator.handle_key(&ctx, KeyCode::Char('j'));
        assert!(navigator.take_read_news().is_empty());

        navigator.handle_key(&ctx, KeyCode::Char('x'));
        assert_eq!(navigator.take_read_news(), vec![subject.clone()]);

        // And only once: the shell drops what was read and rebuilds, and the
        // row has nothing left to read off.
        ctx.job_news.remove(&subject);
        navigator.rebuild_detail(&ctx);
        navigator.handle_key(&ctx, KeyCode::Char('x'));
        assert!(navigator.take_read_news().is_empty());
    }

    /// `x` on a branch row opens the menu about the branch. The action menu
    /// was written for items and a branch is not one, so the key had nothing
    /// to act on exactly where the counts it is about are shown
    /// (§FS-004-quick-actions.6).
    #[test]
    fn the_action_key_acts_on_a_branch_row_too() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = super::super::tests::ctx_with_branch(tmp.path(), None);
        let mut navigator = NavigatorState::new();
        navigator.mode = Mode::Detail;
        navigator.detail_entries = vec![
            Entry::Branch(
                "widget".to_string(),
                branch_info("you/ABC-42"),
                true,
                Some(trail(13)),
                Some(trail(2)),
                0,
                None,
            ),
            row("pr:1"),
        ];
        navigator.detail_state = on(&navigator.detail_entries, 0);

        match navigator.handle_key(&ctx, KeyCode::Char('x')) {
            Action::OpenBranchActions { project, branch } => {
                assert_eq!(project, "widget");
                assert_eq!(branch.branch, "you/ABC-42");
            }
            _ => panic!("the branch row opens its own menu"),
        }

        // And the item rows go on opening theirs.
        navigator.detail_state = on(&navigator.detail_entries, 1);
        assert!(matches!(
            navigator.handle_key(&ctx, KeyCode::Char('x')),
            Action::OpenActionMenu(_)
        ));
    }

    /// A project row is a matter of its own for this purpose: the projects
    /// view keeps its place across a rebuild too.
    #[test]
    fn a_project_row_keeps_its_place() {
        let before = vec![
            Entry::Org("Acme".to_string()),
            Entry::Project("widget".to_string(), Vec::new()),
        ];
        let mut state = on(&before, 1);
        let was = selected_identity(&before, &state);

        let after = vec![
            Entry::Org("Acme".to_string()),
            Entry::Project("gadget".to_string(), Vec::new()),
            Entry::Project("widget".to_string(), Vec::new()),
        ];
        fix_selection(&after, &mut state, was);

        assert_eq!(state.selected(), Some(2));
    }
}
