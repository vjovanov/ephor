//! The navigation screen: org/project/type/branch trees.
//!
//! Three modes: Stream (full tree across organizations, unread-only by
//! default), Projects (org-grouped summary rows), and Detail (one project
//! plus all its registry branches). Tab toggles Stream/Projects; Enter on a
//! project row drills into Detail.

use chrono::Utc;
use ratatui::crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::feed::cache::{self, Seen};
use crate::feed::gate::Gate;
use crate::feed::model::{Item, ItemKind, ItemRole};
use crate::feed::render::age;

use super::{highlight_style, matches_branch, Action, BranchInfo, Ctx, WorkBadge};

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
}

/// One line in a tree view.
enum Entry {
    /// Organization header, not selectable.
    Org(String),
    /// Project row; Enter opens the detail view.
    Project(String),
    /// Type-level section header, not selectable.
    Header(&'static str),
    /// Branch row: group header inside a type section, or the branch
    /// overview in the detail view. Carries its project id, whether its
    /// workspace is checked out on disk, and how many commits it trails
    /// main (summed over the workspace's repos).
    Branch(String, BranchInfo, bool, Option<u64>),
    /// Group header for items not linked to any registry branch.
    Unassigned,
    Item(Row),
}

fn selectable(entry: &Entry) -> bool {
    !matches!(entry, Entry::Org(_) | Entry::Header(_) | Entry::Unassigned)
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
        }
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
            Mode::Stream => " j/k move  enter thread  c gate  w work  o browser  x actions  m done  a all done  u unread  tab projects  r refresh  q quit",
            Mode::Projects => " j/k move  enter view project  tab stream  r refresh  q quit",
            Mode::Detail => " j/k move  enter thread  c gate  w work  o browser  x actions  m done  [/] project  esc back  u unread  r refresh  q quit",
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
    /// groups in registry order, then items not linked to any branch.
    fn type_section_entries(&self, ctx: &Ctx, project: &str) -> Vec<Entry> {
        let mut entries = Vec::new();
        let Some(feed) = ctx.feed(project) else {
            return entries;
        };
        let branches = ctx.branches.get(project).cloned().unwrap_or_default();

        // §FS-003-feed-categories.1. Every filter but Recent's excludes
        // finished work, so an item lands in exactly one category.
        type SectionFilter = fn(&Item) -> bool;
        let sections: [(&'static str, SectionFilter); 8] = [
            ("Status", |item| item.kind == ItemKind::Status),
            ("My Pull Requests", |item| {
                item.kind == ItemKind::Pr
                    && item.role != Some(ItemRole::Reviewer)
                    && !item.is_finished()
            }),
            ("Reviewing", |item| {
                item.kind == ItemKind::Pr
                    && item.role == Some(ItemRole::Reviewer)
                    && !item.is_finished()
            }),
            ("CI", |item| {
                item.kind == ItemKind::Ci && !item.is_finished()
            }),
            ("My Issues", |item| {
                item.kind == ItemKind::Issue
                    && item.role != Some(ItemRole::Reviewer)
                    && !item.is_finished()
            }),
            ("Participating", |item| {
                item.kind == ItemKind::Issue
                    && item.role == Some(ItemRole::Reviewer)
                    && !item.is_finished()
            }),
            ("Messages", |item| {
                item.kind == ItemKind::Message && !item.is_finished()
            }),
            ("Recent", |item| item.is_finished()),
        ];
        let now = Utc::now();
        for (header, section_filter) in sections {
            let mut rows: Vec<Row> = feed
                .items()
                .filter(|item| item.is_visible(now, ctx.recent_days))
                .filter(|item| section_filter(item))
                .filter(|item| !ctx.unread_only || cache::is_unread(&ctx.seen, item))
                .map(|item| Row {
                    stale: feed.is_stale(&item.source),
                    checked_out: ctx.item_checked_out(&item),
                    work: ctx.work.get(&item.id).cloned(),
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

            let mut consumed = vec![false; rows.len()];
            for branch in &branches {
                let matching: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter(|(index, row)| !consumed[*index] && matches_branch(&row.item, branch))
                    .map(|(index, _)| index)
                    .collect();
                if matching.is_empty() {
                    continue;
                }
                entries.push(Entry::Branch(
                    project.to_string(),
                    branch.clone(),
                    ctx.branch_checked_out(project, branch),
                    ctx.behind
                        .get(&(project.to_string(), branch.branch.clone()))
                        .copied(),
                ));
                for index in matching {
                    consumed[index] = true;
                    entries.push(Entry::Item(rows[index].clone()));
                }
            }
            let unassigned: Vec<usize> =
                (0..rows.len()).filter(|index| !consumed[*index]).collect();
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
        self.stream_entries.clear();
        for org in &ctx.orgs {
            let mut org_entries = Vec::new();
            for project in ctx.org_projects(&org.id) {
                let sections = self.type_section_entries(ctx, &project);
                if sections.is_empty() {
                    continue;
                }
                org_entries.push(Entry::Project(project));
                org_entries.extend(sections);
            }
            if org_entries.is_empty() {
                continue;
            }
            self.stream_entries.push(Entry::Org(ctx.org_label(org)));
            self.stream_entries.append(&mut org_entries);
        }
        fix_selection(&self.stream_entries, &mut self.stream_state);
    }

    fn rebuild_projects(&mut self, ctx: &Ctx) {
        self.project_entries.clear();
        for org in &ctx.orgs {
            let projects = ctx.org_projects(&org.id);
            if projects.is_empty() {
                continue;
            }
            self.project_entries.push(Entry::Org(ctx.org_label(org)));
            self.project_entries
                .extend(projects.into_iter().map(Entry::Project));
        }
        fix_selection(&self.project_entries, &mut self.project_state);
    }

    fn rebuild_detail(&mut self, ctx: &Ctx) {
        let project = ctx.projects[self.detail_project].clone();
        self.detail_entries.clear();

        let branches = ctx.branches.get(&project).cloned().unwrap_or_default();
        if !branches.is_empty() {
            self.detail_entries.push(Entry::Header("Branches"));
            for branch in &branches {
                self.detail_entries.push(Entry::Branch(
                    project.clone(),
                    branch.clone(),
                    ctx.branch_checked_out(&project, branch),
                    ctx.behind
                        .get(&(project.clone(), branch.branch.clone()))
                        .copied(),
                ));
            }
        }
        let sections = self.type_section_entries(ctx, &project);
        self.detail_entries.extend(sections);
        fix_selection(&self.detail_entries, &mut self.detail_state);
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
            KeyCode::Enter | KeyCode::Char('l') => self.activate(ctx, true),
            KeyCode::Char('o') => self.activate(ctx, false),
            KeyCode::Char('v') => match self.selected_item() {
                Some(item) => Action::OpenThread {
                    item,
                    or_url: false,
                },
                None => Action::None,
            },
            KeyCode::Char('x') => match self.selected_item() {
                Some(item) => Action::OpenActionMenu(item),
                None => Action::None,
            },
            // What is being done about this, and what could be
            // (§FS-005-dispatch).
            KeyCode::Char('w') => match self.selected_item() {
                Some(item) => Action::OpenWork(item),
                None => Action::None,
            },
            // The counts on the row are a summary of a verdict; `c` is where
            // the verdict itself is (§FS-001-forge-interface.1).
            KeyCode::Char('c') => match self.selected_item() {
                Some(item) => Action::OpenGate(item),
                None => Action::None,
            },
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
        enum Selected {
            Item(Item),
            Branch(String, BranchInfo),
            Project(String),
        }
        let selected = {
            let (entries, state) = self.tree();
            match state.selected().and_then(|index| entries.get(index)) {
                Some(Entry::Item(row)) => Some(Selected::Item(row.item.clone())),
                Some(Entry::Branch(project, branch, _, _)) => {
                    Some(Selected::Branch(project.clone(), branch.clone()))
                }
                Some(Entry::Project(project)) => Some(Selected::Project(project.clone())),
                _ => None,
            }
        };
        match selected {
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
                Entry::Project(project) => {
                    if summary_projects {
                        let (total, unread, respond) = ctx.unread_stats(project);
                        let branches = ctx
                            .branches
                            .get(project)
                            .map(|branches| branches.iter().filter(|b| b.active).count())
                            .unwrap_or(0);
                        let respond_span = if respond > 0 {
                            Span::styled(
                                format!("{respond} need response"),
                                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                            )
                        } else {
                            Span::styled("nothing pending", Style::default().fg(Color::DarkGray))
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("  {project:<30}"),
                                Style::default().fg(Color::Cyan),
                            ),
                            Span::raw(format!(
                                "{branches:>2} branches  {total:>3} items  {unread:>3} unread  "
                            )),
                            respond_span,
                        ]))
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
                Entry::Branch(project, branch, checked_out, behind) => {
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
                    if *checked_out {
                        spans.push(Span::styled(
                            "✓ checked out",
                            Style::default().fg(Color::Green),
                        ));
                        match behind {
                            Some(0) => spans.push(Span::styled(
                                " · up to date  ",
                                Style::default().fg(Color::DarkGray),
                            )),
                            Some(count) => spans.push(Span::styled(
                                format!(" · {count} behind  "),
                                Style::default().fg(Color::Yellow),
                            )),
                            None => spans.push(Span::raw("  ")),
                        }
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
                    let linked = ctx
                        .feed(project)
                        .map(|feed| {
                            feed.items()
                                .filter(|item| matches_branch(item, branch))
                                .count()
                        })
                        .unwrap_or(0);
                    if linked > 0 {
                        spans.push(Span::styled(
                            format!("{linked} linked"),
                            Style::default().fg(Color::Magenta),
                        ));
                    }
                    ListItem::new(Line::from(spans))
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

fn item_line(row: &Row, seen: &Seen, now: chrono::DateTime<Utc>) -> Line<'static> {
    let Row {
        item,
        stale,
        checked_out,
        work,
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
        // Keep PR titles aligned within a section even when one PR's
        // branch is unknown.
        None if item.kind == ItemKind::Pr => spans.push(Span::raw("  ")),
        None => {}
    }
    spans.push(Span::raw(item.title.clone()));
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

fn fix_selection(entries: &[Entry], state: &mut ListState) {
    match state.selected() {
        Some(index) if index < entries.len() && selectable(&entries[index]) => {}
        _ => state.select((0..entries.len()).find(|index| selectable(&entries[*index]))),
    }
}
