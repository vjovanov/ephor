//! Interactive two-screen browser over the status feed.
//!
//! - **Navigator** (`navigator.rs`): everything organized per organization,
//!   then per project, then per type (Status, Pull Requests, CI, Messages),
//!   then per branch. Three modes toggled with Tab / Enter: Stream (full
//!   tree), Projects (org-grouped summary), Detail (one project plus its
//!   registry branches).
//! - **Thread** (`thread.rs`): full-screen visualization of one item's
//!   conversation — per-message selection, reactions, and a reaction picker.
//!
//! Screens never mutate shared state directly: key handlers return an
//! [`Action`] and the shell here executes it. "Done" is mark-read: an item
//! resurfaces when it changes again.

mod actions;
mod answers;
mod gate;
mod navigator;
mod operations;
mod prompt;
mod thread;
mod work;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use chrono::{DateTime, Utc};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::DefaultTerminal;

use crate::error::{EphorError, Result};
use crate::feed::cache::{self, ProjectFeed};
use crate::feed::config::{load_config, ActionConfig, StatusConfig};
use crate::feed::model::Item;
use crate::seams::summons::{self, Place, Site};

pub(crate) use crate::api::session::display_root;
#[allow(unused_imports)]
pub(crate) use crate::api::OrgInfo;
pub(crate) use crate::api::Session as Ctx;
pub(crate) use crate::branches::BranchInfo;
pub(crate) use crate::branches::WorkspaceState;
use actions::{ActionMenu, MenuOutcome};
use answers::{AnswerOutcome, AnswerScreen};
use gate::GateScreen;
use navigator::NavigatorState;
use operations::OperationsScreen;
use prompt::{Asking, Prompt, PromptOutcome};
use thread::ThreadScreen;
use work::WorkScreen;

/// What a screen asks the shell to do in response to a key.
pub(crate) enum Action {
    None,
    Quit,
    /// Show the item's thread screen. With `or_url`, an item without any
    /// recorded messages falls back to opening its URL.
    OpenThread {
        item: Item,
        or_url: bool,
    },
    /// Show the item's gate screen: the counts spelled out and the
    /// forge's own reasons for refusing the merge.
    OpenGate(Item),
    /// Leave the current screen for the navigator.
    Back,
    OpenUrl(Option<String>),
    /// Mark items read: `(id, updated_at, title)`. `pop` returns to the
    /// navigator afterwards (used by the thread screen).
    MarkDone {
        marks: Vec<(String, DateTime<Utc>, String)>,
        pop: bool,
    },
    /// Post `content` (palette name) on a message. `message` is the flat
    /// message index — the address the move takes and the one the optimistic
    /// local update writes at, which are the same number because both surfaces
    /// walk the conversation once (§AR-009-surfaces.1). `emoji` rides along
    /// only for what the screen draws before the answer comes back.
    React {
        item: Item,
        message: usize,
        content: &'static str,
        emoji: &'static str,
    },
    /// Tick a task on a message (§FS-004-quick-actions.5).
    ResolveTask {
        item: Item,
        message: usize,
    },
    /// Send the reply a run drafted, as it now stands (§FS-005-dispatch.13).
    PostReply {
        item: Item,
    },
    /// Open a drafted reply in the reader's editor before it goes anywhere.
    EditReply {
        path: PathBuf,
        item: Item,
    },
    /// Summon the configured action menu for an item.
    OpenActionMenu(Item),
    /// The same menu on a branch row, which has no item behind it: what ephor
    /// offers about the branch itself (§FS-004-quick-actions.6).
    OpenBranchActions {
        project: String,
        branch: BranchInfo,
    },
    /// Make the workspace the row says is not there (§FS-004-quick-actions.7.2).
    Checkout(CheckoutOf),
    /// Show what is being done about an item, and what could be
    /// (§FS-005-dispatch).
    OpenWork(Item),
    /// Hand the item to the runtime under one entry of its work menu: a recipe
    /// that opens a ticket, or a workflow that gets laid down beside it
    /// (§FS-005-dispatch.19). Named by the entry's key, which is the name
    /// `ephor work offers` prints and `ephor actions run` takes.
    DispatchWork {
        item: Item,
        entry: String,
    },
    /// Reopen work whose item has moved under it (§FS-005-dispatch.5).
    SyncWork(Item),
    /// Leave the interface and let the runtime work one item's plan.
    RunWork {
        /// Whose plan this is: the ledger entry behind it answers which hand
        /// rides the run (§FS-005-dispatch.14).
        item: String,
        root: PathBuf,
        /// Where to run it from — the checkout the work is about.
        checkout: PathBuf,
        plan_id: String,
        label: String,
    },
    /// Open a plan in the reader's editor.
    ReadPlan(PathBuf),
    /// Read what a job wrote (§FS-005-dispatch.17). `following` asks the
    /// pager to keep up with a job that is still writing.
    ReadLog {
        path: PathBuf,
        following: bool,
    },
    /// Watch a live run by attaching to it (§FS-005-dispatch.20). Leaving the
    /// surface detaches and never stops the run.
    AttachRun {
        root: PathBuf,
        id: String,
    },
    /// Ask this item for something no recipe covers (§FS-005-dispatch.10).
    AskWork(Item),
    /// Read the plan behind a work row (§FS-005-dispatch.23). The path is the
    /// matter's own, resolved when the key is pressed rather than carried on
    /// the row, because the plan is the runtime's and moves with it.
    OpenWorkPlan(Item),
    /// Watch whatever run is holding this matter's work
    /// (§FS-005-dispatch.20, §FS-005-dispatch.23). Liveness is read from the
    /// lock at the keypress and never remembered (§FS-005-dispatch.15).
    AttachWork(Item),
    /// Take one of this item's tickets back (§FS-005-dispatch.16): the shell
    /// asks why, then asks the runtime for the move.
    CancelWork {
        item: Item,
        ticket: String,
    },
    ToggleUnread,
    Refresh,
    SetMessage(String),
}

/// Whose workspace a checkout is about (§FS-004-quick-actions.7.2). A matter
/// carries its own branch, and a branch row is the branch itself; either way
/// the move is the one entry that row's menu holds about a missing workspace.
#[derive(Clone)]
pub(crate) enum CheckoutOf {
    Item(Box<Item>),
    Branch {
        project: String,
        branch: Box<BranchInfo>,
    },
}

enum Screen {
    Navigator,
    Thread(ThreadScreen),
    Gate(GateScreen),
    Work(WorkScreen),
    /// The operations board, watch-only (§FS-005-dispatch.15).
    Operations(OperationsScreen),
}

/// How often the interface glances at the work artifacts between key reads
/// (§FS-005-dispatch.15.1): a clock gates stat calls, a changed timestamp
/// gates the re-read, and nothing is ever read while drawing.
const WORK_TICK: Duration = Duration::from_secs(2);

struct App {
    ctx: Ctx,
    navigator: NavigatorState,
    screen: Screen,
    /// The screen the reader was on when the operations board opened over
    /// it — one modal layer, restored by Esc. What the board's Enter opens
    /// replaces the pair rather than nesting (§FS-005-dispatch.15): only a
    /// Back from the board itself restores this.
    saved: Option<Screen>,
    /// Open action menu, drawn over the active screen.
    menu: Option<ActionMenu>,
    /// A line the reader is typing, drawn over everything else.
    prompt: Option<Prompt>,
    /// A workflow's inputs, being answered before it is laid down
    /// (§FS-005-dispatch.19). Over the screen it was reached from, and under
    /// the prompt like everything else.
    answers: Option<AnswerScreen>,
    /// A refresh running underneath this screen, where one is
    /// (§FS-001-forge-interface.7).
    refresh: Option<crate::feed::refresh::BackgroundRefresh>,
    /// The work configuration, kept for the board and the tick: both read
    /// the runtime's artifacts through the binding (§AR-007-runtime.1).
    work: crate::work::recipe::WorkConfig,
    /// Every execution root the last enumeration found, with its plans —
    /// what the ledger dispatched and what the work roots hold besides
    /// (§FS-005-dispatch.15). The walk runs when rows are built, never on
    /// the bare tick: between builds this cache is what the glance stats
    /// (§FS-005-dispatch.15.1).
    work_groups: Vec<crate::work::runtime::watch::RootPlans>,
    /// When the work artifacts were last glanced at, and the newest write
    /// seen then (§FS-005-dispatch.15.1).
    ticked_at: std::time::Instant,
    work_seen: Option<std::time::SystemTime>,
    /// What ephor is running beneath this screen (§FS-005-dispatch.17), read
    /// from where jobs are written rather than from anything that remembers
    /// starting one — a job another ephor started is in here too. Refreshed
    /// when rows are built and probed on the tick, exactly as a run is.
    jobs: Vec<crate::seams::jobs::Job>,
    message: String,
}

pub fn run() -> Result<ExitCode> {
    let config = load_config()?;
    let mut app = App::load(&config)?;
    let mut terminal = ratatui::init();
    let result = app.event_loop(&mut terminal, &config);
    ratatui::restore();
    result
}

pub(crate) fn highlight_style() -> Style {
    Style::default()
        .bg(Color::Rgb(60, 60, 80))
        .add_modifier(Modifier::BOLD)
}

impl App {
    fn load(config: &StatusConfig) -> Result<Self> {
        // The same session a command opens (§AR-009-surfaces.2): the screen
        // reads exactly the data `ephor actions` and `ephor branches` do.
        let mut app = App {
            ctx: Ctx::open(config)?,
            navigator: NavigatorState::new(),
            screen: Screen::Navigator,
            saved: None,
            menu: None,
            prompt: None,
            answers: None,
            refresh: None,
            work: config.work.clone(),
            work_groups: Vec::new(),
            ticked_at: std::time::Instant::now(),
            work_seen: None,
            jobs: Vec::new(),
            message: String::new(),
        };
        // Whatever is still going from an earlier session is the reader's
        // news on the first frame, and what is long over is swept here rather
        // than on a path a keystroke waits on (§FS-005-dispatch.17).
        crate::seams::jobs::sweep();
        app.jobs = crate::seams::jobs::all();
        app.reload_feeds()?;
        // What is on disk now is the baseline the tick moves from: the load
        // just read it, and re-reading it two seconds in would be a glance
        // at nothing (§FS-005-dispatch.15.1).
        app.work_seen = app.work_wrote();
        if !app.navigator.has_stream_entries()
            && app.ctx.feeds.iter().all(|feed| feed.fetched_at.is_none())
        {
            app.message = "No cached data — press r to refresh".to_string();
        }
        Ok(app)
    }

    fn reload_feeds(&mut self) -> Result<()> {
        self.ctx.reload_feeds()?;
        self.reload_work_groups();
        self.reload_work();
        self.rebuild_view();
        Ok(())
    }

    /// Re-enumerate the work roots (§FS-005-dispatch.15): the ledger's
    /// dispatches and every plan the roots hold besides. Runs when rows are
    /// built — a load, a refresh landing, the board opening, a glance that
    /// saw something move — never on the bare tick, which only stats what
    /// this last found (§FS-005-dispatch.15.1).
    fn reload_work_groups(&mut self) {
        self.work_groups = match &mut self.ctx.dispatcher {
            Some(dispatcher) => dispatcher.work_roots(),
            None => Vec::new(),
        };
    }

    /// Re-read every dispatched item's plan (§FS-005-dispatch.4). The
    /// session's own, because the ledger it reads is the session's own: this
    /// screen used to keep a second [`crate::work::Dispatcher`] and refresh
    /// the badges off that one, which left the session holding a ledger a
    /// dispatch had already moved past (§AR-009-surfaces.2).
    fn reload_work(&mut self) {
        self.ctx.reload_work();
    }

    fn event_loop(
        &mut self,
        terminal: &mut DefaultTerminal,
        config: &StatusConfig,
    ) -> Result<ExitCode> {
        loop {
            terminal
                .draw(|frame| self.draw(frame))
                .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;

            // Between key reads, never instead of them: whatever the refresh
            // beneath this screen has finished lands here, and the screen goes
            // back to being the reader's (§FS-001-forge-interface.7).
            if self.collect_refresh()? {
                self.reload_operations();
                continue;
            }

            // Also between key reads: the clock-gated glance at the work
            // artifacts, so what the runtime moved on disk surfaces without
            // waiting for a refresh (§FS-005-dispatch.15.1).
            if self.tick() {
                continue;
            }

            if !event::poll(Duration::from_millis(250))
                .map_err(|err| EphorError::Command(format!("event poll failed: {err}")))?
            {
                continue;
            }
            let Event::Key(key) = event::read()
                .map_err(|err| EphorError::Command(format!("event read failed: {err}")))?
            else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(ExitCode::SUCCESS);
            }

            // The prompt is over everything, including the menu that opened
            // it: what is typed there is meant for it and nothing else.
            if let Some(prompt) = &mut self.prompt {
                match prompt.handle_key(key.code, key.modifiers) {
                    PromptOutcome::Stay => {}
                    PromptOutcome::Cancel => {
                        let prompt = self.prompt.take().expect("prompt is open");
                        self.message = match prompt.asking {
                            Asking::Cancel { ticket, .. } => {
                                format!("{ticket} kept — nothing cancelled")
                            }
                            _ => "Nothing asked for".to_string(),
                        };
                    }
                    PromptOutcome::Submit(line) => {
                        let prompt = self.prompt.take().expect("prompt is open");
                        self.submit(terminal, prompt.asking, &line)?;
                    }
                }
                continue;
            }
            // A workflow's inputs, answered over whatever opened them. Above
            // the menu — the menu is closed by the time this is up — and below
            // the prompt, like every other second level (§FS-005-dispatch.19).
            if self.answers.is_some() {
                let outcome = self
                    .answers
                    .as_mut()
                    .expect("the answers are open")
                    .handle_key(key.code, key.modifiers);
                self.answered(terminal, outcome)?;
                continue;
            }
            if let Some(menu) = &mut self.menu {
                match menu.handle_key(key.code) {
                    MenuOutcome::Stay => {}
                    MenuOutcome::Close => self.menu = None,
                    MenuOutcome::Run(entry) => {
                        let menu = self.menu.take().expect("menu is open");
                        // The one entry that has no command yet: the reader
                        // types it (§FS-005-dispatch.10).
                        if entry.is_freehand {
                            self.prompt = Some(Prompt::new(
                                Asking::Command(Box::new(menu)),
                                "run a command here",
                                "runs in the item's checkout, with its EPHOR_* environment  ·  enter runs  ·  esc cancels",
                            ));
                        // An entry that carries a brief is handed over rather
                        // than run: the terminal stays where it is, because
                        // nothing runs in front of the reader
                        // (§FS-005-dispatch.1).
                        } else if entry.action.agent.is_some() {
                            self.dispatch_entry(&menu, &entry);
                        // An entry that lays down a workflow writes files and
                        // takes no terminal (§FS-005-dispatch.19).
                        } else if entry.is_workflows {
                            self.open_workflows(&menu);
                        } else if entry.action.workflow.is_some() {
                            self.lay_entry(&menu, &entry)?;
                        } else {
                            self.run_menu_entry(terminal, &menu, &entry)?;
                        }
                    }
                    // The key on a row that says *running* goes to the thing
                    // that is running (§FS-005-dispatch.21).
                    MenuOutcome::Open(entry) => {
                        self.menu = None;
                        if let Some(running) = entry.running.clone() {
                            self.open_running(terminal, &running)?;
                        }
                    }
                }
                continue;
            }
            // The operations board opens from anywhere over whatever is on
            // screen, and closes back to it (§FS-005-dispatch.15). Below the
            // prompt and the menu: a `;` typed into either is theirs — and
            // below a screen's own modal for the same reason, since a board
            // opened over an armed reaction picker leaves it armed beneath.
            let inside = match &self.screen {
                Screen::Thread(thread) => thread.is_picking(),
                _ => false,
            };
            if key.code == KeyCode::Char(';') && !inside {
                self.toggle_operations();
                continue;
            }
            let action = match &mut self.screen {
                Screen::Navigator => self.navigator.handle_key(&self.ctx, key.code),
                Screen::Thread(thread) => thread.handle_key(key.code),
                Screen::Gate(gate) => gate.handle_key(key.code),
                Screen::Work(work) => work.handle_key(key.code),
                Screen::Operations(board) => board.handle_key(key.code),
            };
            // A finished job's line stays under its subject until the reader
            // opens that row, and opening it is what reads it off
            // (§FS-005-dispatch.17).
            let read = self.navigator.take_read_news();
            if !read.is_empty() {
                for subject in read {
                    self.ctx.job_news.remove(&subject);
                }
                self.rebuild_view();
            }
            if self.apply(action, terminal, config)? {
                return Ok(ExitCode::SUCCESS);
            }
        }
    }

    /// Execute a screen's action. Returns true to quit.
    fn apply(
        &mut self,
        action: Action,
        terminal: &mut DefaultTerminal,
        config: &StatusConfig,
    ) -> Result<bool> {
        match action {
            Action::None => {}
            Action::Quit => return Ok(true),
            Action::SetMessage(message) => self.message = message,
            Action::OpenUrl(url) => self.open_url(url),
            Action::OpenThread { item, or_url } => {
                // What a run drafted about this matter, read from the work
                // root every time it is shown (§FS-005-dispatch.13).
                let proposal = self.proposal(&item);
                match ThreadScreen::open(item.clone(), proposal) {
                    Some(screen) => self.screen = Screen::Thread(screen),
                    None if or_url => self.open_url(item.url),
                    None => self.message = "No messages recorded for this item".to_string(),
                }
            }
            Action::OpenGate(item) => match GateScreen::open(item) {
                Some(screen) => self.screen = Screen::Gate(screen),
                None => self.message = "No gate recorded for this item".to_string(),
            },
            Action::Back => {
                // The board is one modal layer: leaving it restores the
                // screen it opened over, and leaving anything else drops any
                // stale slot on the way to the navigator
                // (§FS-005-dispatch.15).
                self.screen = match self.saved.take() {
                    Some(previous) if matches!(self.screen, Screen::Operations(_)) => previous,
                    _ => Screen::Navigator,
                }
            }
            Action::ToggleUnread => {
                self.ctx.unread_only = !self.ctx.unread_only;
                self.rebuild_view();
            }
            Action::MarkDone { marks, pop } => {
                self.mark_done(marks)?;
                if pop {
                    self.screen = Screen::Navigator;
                }
            }
            // The three moves inside a conversation are the API's, not this
            // screen's (§AR-009-surfaces.1): `ephor react`, `ephor tick` and
            // `ephor reply` call exactly these, so the sentence a status line
            // shows and the `says` a program reads are one sentence, and the
            // half a move has to remember — retiring a posted draft — cannot be
            // remembered by one surface and forgotten by the other. What stays
            // here is presentation: the note drawn while the far side is being
            // asked, and the optimistic local update afterwards.
            Action::React {
                item,
                message,
                content,
                emoji,
            } => {
                self.message = format!("Reacting {emoji}…");
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                let outcome = self.ctx.react(&item, message, content);
                self.message = outcome.says;
                if outcome.ok {
                    if let Screen::Thread(thread) = &mut self.screen {
                        thread.add_local_reaction(message, emoji);
                    }
                }
            }
            Action::ResolveTask { item, message } => {
                self.message = "Ticking…".to_string();
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                let outcome = self.ctx.tick(&item, message);
                self.message = outcome.says;
                if outcome.ok {
                    if let Screen::Thread(thread) = &mut self.screen {
                        thread.tick_local(message);
                    }
                }
            }
            Action::PostReply { item } => {
                self.message = "Posting the reply…".to_string();
                terminal
                    .draw(|frame| self.draw(frame))
                    .map_err(|err| EphorError::Command(format!("draw failed: {err}")))?;
                // With no words of its own this sends the draft as it stands
                // and retires it, which is the move's own second half: a
                // proposal still offered after it was posted invites posting it
                // twice (§FS-005-dispatch.13).
                let outcome = self.ctx.reply(&item, None, crate::api::act::Sending::Now);
                self.message = outcome.says;
                if outcome.ok {
                    if let Screen::Thread(thread) = &mut self.screen {
                        thread.reply_posted();
                    }
                }
            }
            Action::EditReply { path, item } => {
                self.edit_file(terminal, &path)?;
                // The reader may have rewritten it, emptied it, or left it
                // alone: what is on disk now is what would be posted.
                let proposal = self.proposal(&item);
                if let Screen::Thread(thread) = &mut self.screen {
                    thread.reread(proposal);
                }
            }
            // One assembly, below the screen (§AR-009-surfaces.1): this is
            // the list `ephor actions` prints, gated by the same table
            // (§AR-005-capabilities.2). The menu opens where the project is
            // placed; where it is not, the ladder's own sentence says why.
            Action::OpenActionMenu(item) => match self.item_menu(item) {
                Ok(menu) => self.menu = Some(menu),
                Err(refusal) => self.message = refusal,
            },
            // The same menu, opened from the row the fact is shown on
            // (§FS-004-quick-actions.6). It carries ephor's own offers only:
            // there is no item here for a source's, a project's or a person's
            // entries to be selected against, and none for a recipe either —
            // work is asked for about a matter, and there is none here
            // (§FS-005-dispatch.2).
            Action::OpenBranchActions { project, branch } => {
                match self.branch_menu(&project, branch) {
                    Ok(menu) => self.menu = Some(menu),
                    Err(refusal) => self.message = refusal,
                }
            }
            // The key on the row that says the workspace is missing
            // (§FS-004-quick-actions.7.2). It assembles the row's own menu and
            // runs the one entry in it about the missing workspace, so the key
            // and that entry cannot come to be two different operations
            // (§AR-009-surfaces.1) — including the project's own checkout
            // command where it configured one (§FS-006-project-interface.8).
            Action::Checkout(about) => {
                let menu = match about {
                    CheckoutOf::Item(item) => self.item_menu(*item),
                    CheckoutOf::Branch { project, branch } => self.branch_menu(&project, *branch),
                };
                match menu {
                    Ok(menu) => match menu.checkout_entry().cloned() {
                        Some(entry) => self.run_menu_entry(terminal, &menu, &entry)?,
                        // Said rather than swallowed: the reader pressed a key
                        // on a row, and a key that does nothing without saying
                        // why reads as a key that is broken
                        // (§FS-004-quick-actions.2).
                        None => {
                            self.message =
                                format!("{} is checked out already", menu.subject.title())
                        }
                    },
                    Err(refusal) => self.message = refusal,
                }
            }
            Action::OpenWork(item) => self.open_work(item),
            Action::DispatchWork { item, entry } => {
                self.dispatch_work(&item, &entry)?;
                self.open_work(item);
            }
            Action::SyncWork(item) => {
                self.sync_work(&item);
                self.open_work(item);
            }
            Action::RunWork {
                item,
                root,
                checkout,
                plan_id,
                label,
            } => {
                // The runtime is a rung: refused here in the same words the
                // command line uses, instead of handing the terminal over to a
                // command that cannot start (§AR-005-capabilities.2). Before
                // the hand is resolved and before anything is ceded, because a
                // refusal answered after the terminal is gone is answered too
                // late (§FS-005-dispatch.14).
                //
                // False, not true: this arm refuses the run, it does not end the
                // session. Returning quit here shut the inbox down on a machine
                // with no runtime bound — the one machine where the refusal is
                // the whole point of the message. The screen ahead of it does
                // not offer the key at all when nothing can run, so this is the
                // second line of defence: a screen built before the runtime
                // left `PATH` can still send the action.
                if let Some(refusal) = crate::work::runtime::refusal(&config.work) {
                    self.message = refusal;
                    return Ok(false);
                }
                // Who gets this run, resolved the way `work run` resolves it —
                // a hand the plan language could not spell rides here as the
                // runtime's own agent flags (§FS-005-dispatch.14). This run
                // names one plan and advances no other, so that plan's tickets
                // settle its flags alone and there is nothing to group.
                let (hand, notes) = match &mut self.ctx.dispatcher {
                    Some(dispatcher) => dispatcher.run_hand_for(&item),
                    None => (None, Vec::new()),
                };
                // The checkout, not the plan directory: it is where the work
                // is, and where the runtime falls back to when a workspace has
                // no one repository to be found by looking.
                let said = format!(
                    "{} — {label}{}",
                    crate::work::runtime::label(&config.work),
                    match &hand {
                        Some(hand) => format!(" · {}", hand.describe()),
                        None => String::new(),
                    }
                );
                // A run starts beneath the screen (§FS-005-dispatch.20): the
                // work was handed over precisely so that nobody had to stay,
                // and a screen given away to one run cannot watch the other
                // items. What the reader gets is one line saying the run began
                // and what it is called; the root turns live on the board from
                // the lock, as every run does.
                if crate::work::runtime::can_detach(&config.work) {
                    self.message = match crate::work::runtime::start_detached(
                        &config.work,
                        &root,
                        &checkout,
                        std::slice::from_ref(&plan_id),
                        hand.as_ref(),
                        &[],
                    ) {
                        Ok(crate::work::runtime::Started {
                            id: Some(id),
                            finished: false,
                        }) => {
                            format!("▶ run {id} started · press ; for the board")
                        }
                        // A run that named itself nothing is still a run: the
                        // row is live from the lock alone (§AR-007-runtime.3).
                        Ok(crate::work::runtime::Started {
                            id: None,
                            finished: false,
                        }) => "▶ run started · press ; for the board".to_string(),
                        // The launcher's own descriptor said the run was over
                        // before it returned: there was nothing to do. Saying
                        // *started* would send the reader to an empty board
                        // (§FS-005-dispatch.20).
                        Ok(crate::work::runtime::Started {
                            id: Some(id),
                            finished: true,
                        }) => {
                            format!("✓ run {id} finished already")
                        }
                        Ok(crate::work::runtime::Started { finished: true, .. }) => {
                            "✓ the run finished already".to_string()
                        }
                        Err(err) => err.to_string(),
                    };
                } else {
                    // Where the binding cannot detach the run is watched as it
                    // was — the terminal handed over — and the line says so
                    // rather than pretending (§AR-007-runtime.3).
                    //
                    // *Before* the handover, not after it. The handover blocks
                    // for the whole run, so a note appended to the message
                    // afterwards told the reader why they had lost their
                    // terminal only once they had it back. It rides the banner
                    // the handover prints, which is the last thing on the
                    // screen before the run takes it.
                    let said = format!(
                        "{said} · {} cannot start a run detached here, so it runs in this \
                         terminal",
                        crate::work::runtime::runner(&config.work)
                    );
                    self.handover(
                        terminal,
                        "▶",
                        &said,
                        &Site::root(&checkout),
                        &crate::work::runtime::summons_with(
                            &config.work,
                            &root,
                            std::slice::from_ref(&plan_id),
                            hand.as_ref(),
                            &[],
                        ),
                    )?;
                }
                // The runtime just advanced the plans this reads — and, where
                // it detached, took the lock the board watches.
                self.reload_work();
                self.reload_operations();
                self.rebuild_view();
                if let Screen::Work(screen) = &self.screen {
                    let item = screen.item.clone();
                    self.open_work(item);
                }
                // What the resolution had to say, kept for after the run: the
                // terminal was the runtime's while it worked, and a note
                // printed into it would have scrolled away with the run
                // (§FS-005-dispatch.14).
                if !notes.is_empty() {
                    self.message = format!("{} · {}", self.message, notes.join(" · "));
                }
            }
            Action::AskWork(item) => {
                self.prompt = Some(Prompt::new(
                    Asking::Work(item),
                    "ask for something",
                    "becomes a ticket with the dossier  ·  enter opens it  ·  esc cancels",
                ))
            }
            // The reason first, in one line: it becomes the ticket's result,
            // which is what the record keeps (§FS-005-dispatch.16). Blank is
            // allowed and recorded as blank; Esc keeps the ticket.
            Action::CancelWork { item, ticket } => self.prompt = Some(
                Prompt::new(
                    Asking::Cancel {
                        item,
                        ticket: ticket.clone(),
                    },
                    format!("cancel {ticket} — why?"),
                    "the reason becomes the ticket's result  ·  enter cancels it  ·  esc keeps it",
                )
                .empty_submits(),
            ),
            Action::ReadPlan(path) => {
                self.edit_file(terminal, &path)?;
                // The reader may have edited what the screens read.
                self.reload_work();
                self.reload_operations();
            }
            Action::OpenWorkPlan(item) => match self.work_status(&item) {
                Some(status) => {
                    self.edit_file(terminal, &status.plan)?;
                    self.reload_work();
                    self.reload_operations();
                }
                None => self.message = "There is no plan here yet".to_string(),
            },
            // Read from the lock now, never remembered: a run that died is not
            // running, and a row saying so a tick ago does not make it so
            // (§FS-005-dispatch.15).
            Action::AttachWork(item) => {
                match self.work_status(&item) {
                    Some(status) if crate::work::runtime::watch::live(&self.work, &status.root) => {
                        let id = crate::work::runtime::watch::identity(&self.work, &status.root)
                            .and_then(|identity| identity.id);
                        self.attach(
                            terminal,
                            &crate::api::offers::Running::Run {
                                root: status.root.clone(),
                                id,
                                control_url: None,
                                attach: None,
                                since: None,
                                doing: String::new(),
                            },
                        )?
                    }
                    _ => self.message =
                        "Nothing is running on this work's root — R on the work screen starts it"
                            .to_string(),
                }
            }
            Action::ReadLog { path, following } => self.read_log(terminal, &path, following)?,
            // A window of the reader's own where one is bound, and the terminal
            // otherwise (§FS-005-dispatch.22).
            Action::AttachRun { root, id } => self.attach(
                terminal,
                &crate::api::offers::Running::Run {
                    root,
                    id: Some(id),
                    control_url: None,
                    attach: None,
                    since: None,
                    doing: String::new(),
                },
            )?,
            Action::Refresh => self.start_refresh(config),
        }
        Ok(false)
    }

    /// What the reader typed, done: a ticket in their own words, or a command
    /// run exactly as a configured one is (§FS-005-dispatch.10).
    fn submit(&mut self, terminal: &mut DefaultTerminal, asking: Asking, line: &str) -> Result<()> {
        match asking {
            Asking::Work(item) => {
                let Some(dispatcher) = &mut self.ctx.dispatcher else {
                    self.message = "Work needs the registry, which could not be read".to_string();
                    return Ok(());
                };
                // The screen below shows the plan; the header says what landed
                // in it.
                self.message = match dispatcher.ask(&item, line, None, false) {
                    Ok(crate::work::Outcome::Opened { ticket, .. })
                    | Ok(crate::work::Outcome::Reopened { ticket, .. }) => {
                        match dispatcher.save() {
                            Ok(()) => format!("✎ asked — {ticket}"),
                            Err(err) => err.to_string(),
                        }
                    }
                    Ok(outcome) => outcome.describe(),
                    Err(err) => err.to_string(),
                };
                self.reload_work();
                self.rebuild_view();
                self.open_work(item);
            }
            // The runtime's move, in its own words, and its answer on the
            // message line (§FS-005-dispatch.16); the screen below is rebuilt
            // from the plan, which is where the truth of it now is.
            Asking::Cancel { item, ticket } => {
                let Some(dispatcher) = &self.ctx.dispatcher else {
                    self.message = "Work needs the registry, which could not be read".to_string();
                    return Ok(());
                };
                self.message = match dispatcher.cancel(&item.id, &ticket, line, false) {
                    Ok(cancelled) => cancelled.describe(),
                    Err(err) => err.to_string(),
                };
                self.reload_work();
                self.rebuild_view();
                self.open_work(item);
            }
            Asking::Command(menu) => {
                let entry = actions::MenuEntry {
                    action: ActionConfig {
                        icon: "⌨".to_string(),
                        description: line.to_string(),
                        command: line.to_string(),
                        ..ActionConfig::default()
                    },
                    is_checkout: false,
                    is_freehand: false,
                    is_workflows: false,
                    picked: None,
                    gate: actions::Gate::Ready,
                    running: None,
                };
                self.run_menu_entry(terminal, &menu, &entry)?;
            }
        }
        Ok(())
    }

    /// The work screen for an item, rebuilt from the plan each time it opens.
    /// Hand a file to the reader's editor, the terminal theirs while they have
    /// it — the same handover the runtime gets (§AR-002-summons).
    fn edit_file(&mut self, terminal: &mut DefaultTerminal, path: &Path) -> Result<()> {
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        // One spelling of what opening a file means, shared with the command
        // that opens a parked question (§AR-009-surfaces.1).
        let editing = crate::api::act::editing(path);
        self.handover(
            terminal,
            "📖",
            &editing.binding,
            &Site::root(&dir),
            &editing,
        )?;
        Ok(())
    }

    /// The reply a run drafted about a matter, where there is a dispatcher to
    /// ask and a run that drafted one (§FS-005-dispatch.13).
    fn proposal(&self, item: &Item) -> Option<crate::work::runtime::results::Proposal> {
        self.ctx
            .dispatcher
            .as_ref()
            .and_then(|dispatcher| dispatcher.proposal(item))
    }

    fn open_work(&mut self, item: Item) {
        if self.ctx.dispatcher.is_none() {
            self.message =
                "Work needs the registry, which could not be read at startup".to_string();
            return;
        }
        // What could be handed over here, off the API's own derivation
        // ([`Ctx::work_offers`]) rather than a second one of this screen's:
        // both answer the question `ephor work offers` answers, so both apply
        // the gating that decides whether the dispatch would be refused at all
        // — work is not offered about a finished matter (§FS-005-dispatch.6),
        // and work that edits the change is not offered where the change is
        // not on this machine (§FS-004-quick-actions.7). This screen used to
        // ask the recipes directly and so offered rows `ephor work offers`
        // said were not on the table, which is the drift §REQ-002-parity.2
        // forbids. Who each one would go to rides on the entry, resolved when
        // the menu was built (§FS-005-dispatch.14), so the row and the ticket
        // cannot come apart.
        //
        // *Every* row it derives, workflows included: a screen that kept only
        // the entries with a recipe behind them was the same drift the other
        // way round — `ephor work offers` listed a workflow this screen did
        // not (§FS-005-dispatch.19). And where the menu could not be assembled
        // at all the reason travels with the empty list, because an absence
        // reads as an oversight (§REQ-001-boundary.1).
        let (offers, unavailable) = match self.ctx.work_offers(&item) {
            Ok(offers) => (offers, None),
            Err(refusal) => (Vec::new(), Some(refusal)),
        };
        let status = self
            .ctx
            .dispatcher
            .as_ref()
            .and_then(|dispatcher| dispatcher.status(&item));
        // Whether anything here can run a plan, answered before the screen
        // advertises the key rather than when it is pressed
        // (§FS-004-quick-actions.2).
        let refusal = crate::work::runtime::refusal(&self.work);
        // What ephor has run about this item itself, newest first
        // (§FS-005-dispatch.17) — the record outlives the row on the board.
        let jobs: Vec<crate::seams::jobs::Job> = self
            .jobs
            .iter()
            .filter(|job| job.record.item.as_deref() == Some(item.id.as_str()))
            .cloned()
            .collect();
        // Which run is live on this matter's execution root, and how it is
        // stopped in the runner's own words — shown, never run
        // (§FS-005-dispatch.20).
        let run = status.as_ref().and_then(|status| {
            if !crate::work::runtime::watch::live(&self.work, &status.root) {
                return None;
            }
            let id = crate::work::runtime::watch::identity(&self.work, &status.root)?.id?;
            Some(format!(
                "run {id} · stop it: {}",
                crate::work::runtime::stop_command(&self.work, &id)
            ))
        });
        self.screen = Screen::Work(WorkScreen::new(
            item,
            status,
            offers,
            unavailable,
            refusal,
            jobs,
            run,
        ));
    }

    /// Enter on a work-screen row. The entry is looked up in the same list the
    /// screen was built from ([`Ctx::work_entries`]), so a row that has
    /// stopped applying since the screen opened says so rather than
    /// dispatching something the reading no longer offers.
    ///
    /// Which move that is belongs to the entry, not to the screen: a recipe
    /// opens a ticket, and a workflow opens the screen its inputs are answered
    /// on (§FS-005-dispatch.19) — the same two moves the action menu makes,
    /// through the same two calls.
    fn dispatch_work(&mut self, item: &Item, key: &str) -> Result<()> {
        let found = match self.ctx.work_entries(item) {
            Ok(entries) => entries.into_iter().find(|entry| entry.key() == key),
            // The menu could not be assembled at all — its sentence, not a
            // guess about the row (§AR-005-capabilities.2).
            Err(refusal) => {
                self.message = refusal;
                return Ok(());
            }
        };
        let Some(entry) = found else {
            self.message = format!("'{key}' does not apply to this item any more");
            return Ok(());
        };
        if entry.action.workflow.is_some() {
            // The gate the row already carried, answered before anything is
            // asked for: a reader who typed a workflow's inputs and was then
            // refused answered questions for nothing
            // (§FS-004-quick-actions.2).
            if let actions::Gate::Blocked(reason) = &entry.gate {
                self.message = reason.clone();
                return Ok(());
            }
            let action = entry.action.clone();
            let picked = entry.picked.clone();
            return self.lay_workflow(item, &action, std::collections::BTreeMap::new(), picked);
        }
        self.hand_over(item, &entry);
        Ok(())
    }

    /// An agent entry of the action menu, handed over
    /// (§FS-005-dispatch.1). The recipe rides on the entry, so this dispatches
    /// what the row was built from rather than looking something up by name
    /// and hoping it is the same thing — and it goes through
    /// [`App::hand_over`], the one implementation, so the ledger sees a menu
    /// dispatch and a work-screen dispatch alike (§FS-005-dispatch.4).
    fn dispatch_entry(&mut self, menu: &ActionMenu, entry: &actions::MenuEntry) {
        if let actions::Gate::Blocked(reason) = &entry.gate {
            self.message = reason.clone();
            return;
        }
        let Some(item) = menu.subject.item().cloned() else {
            self.message = "There is no matter here to open work about".to_string();
            return;
        };
        self.hand_over(&item, entry);
        // Where the reader pressed is not a fact about the work: they land on
        // the same screen the work key would have shown them.
        self.open_work(item);
    }

    /// The runtime's workflows, opened over the menu they were reached from
    /// (§FS-005-dispatch.19). Every workflow it offers here becomes an entry —
    /// the ones an entry already names keep that entry's own wording, and the
    /// rest stand under their own id — so what is picked is picked once,
    /// answered, and laid down, and nothing had to be written in a
    /// configuration file first (§FS-005-dispatch.10).
    fn open_workflows(&mut self, menu: &ActionMenu) {
        let project = menu.subject.project().to_string();
        let Some(dispatcher) = &mut self.ctx.dispatcher else {
            self.message = "Work needs the registry, which could not be read".to_string();
            return;
        };
        let offered = dispatcher.workflows(&project);
        if let Some(refusal) = &offered.refusal {
            self.message = refusal.clone();
            return;
        }
        if offered.workflows.is_empty() {
            self.message = "The runtime offers no workflows here".to_string();
            return;
        }
        let named = dispatcher.workflow_entries(&project);
        let entries: Vec<ActionConfig> = offered
            .workflows
            .iter()
            .map(|workflow| {
                named
                    .iter()
                    .find(|(_, entry)| {
                        entry
                            .workflow
                            .as_ref()
                            .is_some_and(|ask| ask.name == workflow.id)
                    })
                    .map(|(_, entry)| entry.clone())
                    .unwrap_or_else(|| crate::work::workflow::Beside::default().action(workflow))
            })
            .collect();
        let can = self.ctx.can(&project);
        // The same matter as the menu behind this screen, so the same naming:
        // an entry that says which branch its work belongs on is offered here
        // in the shape it is offered there (§REQ-002-parity.3,
        // §FS-005-dispatch.25).
        let item = menu.subject.item().cloned();
        self.menu = Some(menu.rebuilt(entries, &can, &mut self.ctx.naming(item.as_ref())));
    }

    /// A workflow entry of the action menu, laid down (§FS-005-dispatch.19).
    fn lay_entry(&mut self, menu: &ActionMenu, entry: &actions::MenuEntry) -> Result<()> {
        if let actions::Gate::Blocked(reason) = &entry.gate {
            self.message = reason.clone();
            return Ok(());
        }
        let Some(item) = menu.subject.item().cloned() else {
            self.message = "There is no matter here to lay a workflow down about".to_string();
            return Ok(());
        };
        let action = entry.action.clone();
        let picked = entry.picked.clone();
        self.lay_workflow(&item, &action, std::collections::BTreeMap::new(), picked)
    }

    /// Lay one workflow down about one item (§FS-005-dispatch.19). What a
    /// key press does here is open the screen its inputs are answered on:
    /// every input the workflow declares, with the answer the five steps
    /// reached and where it came from, and none of it written until the
    /// reader says so ([§FS-005-dispatch.7](crate)).
    fn lay_workflow(
        &mut self,
        item: &Item,
        entry: &ActionConfig,
        typed: std::collections::BTreeMap<String, String>,
        picked: Option<crate::work::recipe::HandPin>,
    ) -> Result<()> {
        let Some(dispatcher) = &mut self.ctx.dispatcher else {
            self.message = "Work needs the registry, which could not be read".to_string();
            return Ok(());
        };
        let laying = match dispatcher.laying(item, entry, &typed, picked.as_ref()) {
            Ok(laying) => laying,
            Err(err) => {
                self.message = err.to_string();
                return Ok(());
            }
        };
        // Who could be chosen for an input that names who does the work, read
        // against the work root the laying will use, exactly as the menu's
        // picker reads it (§FS-005-dispatch.14) — asked of the laying itself,
        // which already knows the workspace a `branch` template named
        // (§FS-005-dispatch.25).
        let roster = dispatcher.pickable(&item.project, laying.root());
        let mut screen = AnswerScreen::over(item.clone(), entry.clone(), picked, &laying, roster);
        screen.typed = typed;
        self.answers = Some(screen);
        Ok(())
    }

    /// What the reader did on that screen. Every answer sends the answering
    /// back through the session (§AR-009-surfaces.1), so provenance, the
    /// hands a narrowing refuses, and what is still missing are one reading
    /// and never the screen's own arithmetic.
    fn answered(&mut self, terminal: &mut DefaultTerminal, outcome: AnswerOutcome) -> Result<()> {
        match outcome {
            AnswerOutcome::Stay => {}
            AnswerOutcome::Close => {
                self.answers = None;
                self.message = "Nothing laid down".to_string();
            }
            AnswerOutcome::Set { input, value } => {
                if let Some(screen) = &mut self.answers {
                    screen.typed.insert(input, value);
                }
                self.resolve_answers();
            }
            // The file, with everything already resolved in it and every
            // unanswered input named — where what no row can carry is
            // answered, and where a reader who would rather type them all is
            // at home (§FS-005-dispatch.19).
            AnswerOutcome::Edit => self.edit_answers(terminal)?,
            AnswerOutcome::Preview => self.preview_answers(),
            AnswerOutcome::Lay => self.lay_it_down(),
        }
        Ok(())
    }

    /// The laying as it stands, for the screen to show. The answers travel as
    /// the `--set` pairs a command takes, which is what makes it the same
    /// call (§REQ-002-parity.2).
    fn laying_now(&mut self) -> Option<crate::work::Laying> {
        let screen = self.answers.as_ref()?;
        let item = screen.item.clone();
        let entry = screen.entry.clone();
        let typed = screen.typed.clone();
        let picked = screen.picked.clone();
        let dispatcher = self.ctx.dispatcher.as_mut()?;
        match dispatcher.laying(&item, &entry, &typed, picked.as_ref()) {
            Ok(laying) => Some(laying),
            Err(err) => {
                self.message = err.to_string();
                None
            }
        }
    }

    fn resolve_answers(&mut self) {
        let Some(laying) = self.laying_now() else {
            return;
        };
        if let Some(screen) = &mut self.answers {
            screen.refresh(&laying);
        }
    }

    /// What the binding would write, before it writes it (§FS-005-dispatch.19)
    /// — its own account, in its own words, which is the half of the account
    /// that is not ephor's to give.
    fn preview_answers(&mut self) {
        let Some(laying) = self.laying_now() else {
            return;
        };
        let Some(screen) = &self.answers else {
            return;
        };
        let item = screen.item.clone();
        let Some(dispatcher) = &mut self.ctx.dispatcher else {
            return;
        };
        match dispatcher.lay(&item, &laying, true) {
            Ok(laid) => {
                let account = match laid.report.trim().is_empty() {
                    true => laid.outcome.describe(),
                    false => laid.report,
                };
                if let Some(screen) = &mut self.answers {
                    screen.account(account);
                }
            }
            Err(err) => self.message = err.to_string(),
        }
    }

    /// The laying itself, which is the API's move (§AR-009-surfaces.1):
    /// `ephor work lay --set …` and this row lay the same plan down through
    /// the same code. A refusal — a required input nobody answered, a hand a
    /// narrowing does not permit — leaves the screen open on what is wrong.
    fn lay_it_down(&mut self) {
        let Some(screen) = &self.answers else {
            return;
        };
        let item = screen.item.clone();
        let answers: Vec<String> = screen
            .typed
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        let laid = actions::MenuEntry {
            action: (*screen.entry).clone(),
            is_checkout: false,
            is_freehand: false,
            is_workflows: false,
            picked: screen.picked.clone(),
            gate: actions::Gate::Ready,
            running: None,
        };
        let outcome = self.ctx.hand_over(&item, &laid, &answers, false);
        self.message = outcome.says;
        if outcome.ok {
            self.answers = None;
            self.rebuild_view();
        }
    }

    /// Everything resolved, in a file, opened in the reader's editor
    /// (§FS-005-dispatch.19). What comes back is read as answers of the
    /// reader's own, which is exactly what a row's answer is.
    fn edit_answers(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(laying) = self.laying_now() else {
            return Ok(());
        };
        let path = match self.write_answers(&laying) {
            Ok(path) => path,
            Err(err) => {
                self.message = err.to_string();
                return Ok(());
            }
        };
        self.edit_file(terminal, &path)?;
        let answered = read_answers(&path);
        if let Some(screen) = &mut self.answers {
            screen.typed.extend(answered);
        }
        self.resolve_answers();
        Ok(())
    }

    /// The file a reader answers a workflow's inputs in: everything ephor
    /// resolved, and every input nobody answered named with what it wants
    /// (§FS-005-dispatch.19).
    fn write_answers(&self, laying: &crate::work::Laying) -> Result<PathBuf> {
        let dir = laying.root().join(".ephor").join(&laying.plan_id);
        std::fs::create_dir_all(&dir)
            .map_err(|err| EphorError::Command(format!("Cannot make {}: {err}", dir.display())))?;
        let path = dir.join("answers.json");
        let mut says = serde_json::Map::new();
        let mut values = laying.answered.values.clone();
        for input in &laying.workflow.inputs {
            let unanswered = laying
                .answered
                .missing
                .iter()
                .any(|name| name == &input.name);
            if !unanswered {
                continue;
            }
            says.insert(
                input.name.clone(),
                serde_json::Value::String(match input.description.is_empty() {
                    true => format!("{} · required", input.kind.label()),
                    false => format!("{} · required · {}", input.kind.label(), input.description),
                }),
            );
            values.insert(input.name.clone(), serde_json::Value::Null);
        }
        let mut document = serde_json::Map::new();
        document.insert("what each input wants".to_string(), says.into());
        for (name, value) in values {
            document.insert(name, value);
        }
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".to_string()),
        )
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", path.display())))?;
        Ok(path)
    }

    /// Handing one entry's work over about one item, and saying what landed.
    /// Both keys that dispatch come through here (§FS-005-dispatch.4), and
    /// here goes straight to the API's move. The whole entry travels rather
    /// than the recipe alone: what the reader picked from the picker rides on
    /// it, spent by this one dispatch and recorded nowhere
    /// (§FS-005-dispatch.14), and so does the gate the row was drawn with.
    fn hand_over(&mut self, item: &Item, entry: &actions::MenuEntry) {
        // The API's move, not a second one of this screen's
        // (§AR-009-surfaces.1): `ephor actions run` and this key open the same
        // ticket through the same code, and the ledger it writes is the
        // session's own — a dispatch saved through a dispatcher of this
        // screen's left the session reading a ledger the dispatch had already
        // moved past. The screen below already shows the plan and its tickets,
        // so the header says what was asked for rather than repeating a long
        // path, which is what the outcome's own sentence is.
        self.message = self.ctx.hand_over(item, entry, &[], false).says;
        self.rebuild_view();
    }

    /// Who would get each piece of work in this menu, before the key is
    /// pressed (§FS-005-dispatch.14). Resolved through the one implementation
    /// that answers it at dispatch, and against the work root the dispatch
    /// will use, so what the row says and what the ticket gets cannot come
    /// apart. A choice that cannot stand rides along as its whole reason, and
    /// the entry is shown unable to run (§FS-006-project-interface.9).
    fn sync_work(&mut self, item: &Item) {
        let Some(dispatcher) = &mut self.ctx.dispatcher else {
            return;
        };
        self.message = match dispatcher.sync(item, false) {
            Ok(crate::work::Outcome::Reopened {
                ticket, changes, ..
            }) => match dispatcher.save() {
                Ok(()) => format!("reopened as {ticket} — {}", changes.join("; ")),
                Err(err) => err.to_string(),
            },
            Ok(outcome) => outcome.describe(),
            Err(err) => err.to_string(),
        };
        self.reload_work();
        self.rebuild_view();
    }

    /// Leave the interface, run something the reader watches, and come back.
    /// The runtime writes for minutes and asks questions; putting it behind a
    /// spinner would hide the only thing worth seeing.
    /// Leave the TUI, run one summons attached to the real terminal, and come
    /// back. Handing the terminal over is this call site's property, not the
    /// binding's (§AR-002-summons.2).
    fn handover(
        &mut self,
        terminal: &mut DefaultTerminal,
        icon: &str,
        description: &str,
        site: &Site,
        summons: &summons::Summons,
    ) -> Result<()> {
        ratatui::restore();
        match site.resolve(&Place::Workspace) {
            Ok(place) => println!("\n{icon} {description}   ({})\n", place.display()),
            Err(err) => println!("\n{icon} {description}   ({err})\n"),
        }
        self.message = match summons::run(summons, site, summons::Mode::Interactive) {
            Ok(answer) if answer.is_done() => format!("{description}: ok"),
            Ok(answer) => answer.refusal(description),
            Err(err) => format!("{description}: {err}"),
        };
        println!("\n{}", self.message);
        print!("Press Enter to return to ephor… ");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().lock().read_line(&mut String::new());
        *terminal = ratatui::init();
        terminal
            .clear()
            .map_err(|err| EphorError::Command(format!("terminal clear failed: {err}")))
    }

    /// Run a menu entry attached to the real terminal: leave the TUI, run
    /// the checkout dependency first when the entry needs one, then the
    /// action itself in the item's checkout, wait for a keypress, and come
    /// back. Blocked entries only set a status message.
    ///
    /// An entry that says it needs no reader never gets here: it is started
    /// beneath the screen instead, and the interface stays where it was
    /// (§FS-005-dispatch.17).
    fn run_menu_entry(
        &mut self,
        terminal: &mut DefaultTerminal,
        menu: &ActionMenu,
        entry: &actions::MenuEntry,
    ) -> Result<()> {
        // An entry that needs no reader never takes the terminal, and neither
        // does one whose program runs in a window of the reader's own: both are
        // started beneath the screen and the interface stays where it was
        // (§FS-005-dispatch.17, §FS-005-dispatch.22). Where each entry runs is
        // the session's answer, so the key and `ephor actions run` cannot come
        // to disagree about it (§AR-009-surfaces.1).
        if !matches!(self.ctx.how(entry), crate::api::act::Runs::Here) {
            self.start_job(menu, entry);
            return Ok(());
        }
        let request = self.request(menu, entry);
        let needs_checkout = matches!(
            (entry.is_checkout, &entry.gate),
            (true, _) | (_, actions::Gate::Blocked(_) | actions::Gate::NeedsCheckout)
        ) && entry.gate.refusal().is_none();
        // The terminal is this call site's property, not the move's
        // (§AR-002-summons.2): the interface gives it up around the call and
        // takes it back afterwards. What runs, where, and what it is told is
        // the one implementation both surfaces use (§AR-009-surfaces.1).
        if entry.gate.refusal().is_none() {
            ratatui::restore();
        }
        let outcome = self
            .ctx
            .run_entry(&request, crate::api::act::Watching::Terminal);
        self.message = outcome.says;
        if entry.gate.refusal().is_some() {
            return Ok(());
        }
        println!("\n{}", self.message);
        print!("Press Enter to return to ephor… ");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().lock().read_line(&mut String::new());
        *terminal = ratatui::init();
        terminal
            .clear()
            .map_err(|err| EphorError::Command(format!("terminal clear failed: {err}")))?;
        // A checkout changes what the branch rows show, and buys the rungs
        // that were waiting on it (§AR-005-capabilities.1).
        if needs_checkout {
            self.ctx.recompute_behind();
            self.ctx.recompute_capabilities();
        }
        self.rebuild_view();
        Ok(())
    }

    /// One matter's menu, assembled below the screen (§AR-009-surfaces.1):
    /// this is the list `ephor actions` prints, gated by the same table
    /// (§AR-005-capabilities.2). Built here rather than in the arm that opens
    /// it, because a key that runs one of its entries without opening it has
    /// to be looking at the same list (§FS-004-quick-actions.7.2). Err is the
    /// ladder's own sentence for a project that is not placed.
    fn item_menu(&mut self, item: Item) -> std::result::Result<ActionMenu, String> {
        let subject = crate::api::read::Subject::Item(&item);
        let placed = self.ctx.place(&subject)?;
        let entries = self.ctx.menu(&subject)?;
        // The hands `t` may offer, read once at menu open against the work
        // root the dispatch will use (§FS-005-dispatch.14) — empty where there
        // is no agent entry to pick for or nobody to pick, which is what
        // withholds the picker entirely.
        let roster = match (
            self.ctx.roster_root(&item, &entries),
            &mut self.ctx.dispatcher,
        ) {
            (Some(root), Some(dispatcher)) => dispatcher.pickable(&item.project, &root),
            _ => Vec::new(),
        };
        let checkout = self.ctx.checkouts.get(&item.project).cloned();
        Ok(ActionMenu::over(
            actions::Subject::Item(Box::new(item)),
            placed.root,
            placed.workspace,
            placed.branch,
            placed.state,
            checkout,
            entries,
        )
        .with_roster(roster))
    }

    /// One branch's menu, the same way (§FS-004-quick-actions.6). The row's own
    /// `BranchInfo` rides it rather than the registry's: a branch nothing
    /// declared still has a row, and it is the row the reader is on.
    fn branch_menu(
        &mut self,
        project: &str,
        branch: BranchInfo,
    ) -> std::result::Result<ActionMenu, String> {
        let subject = crate::api::read::Subject::Branch {
            project,
            branch: &branch.branch,
        };
        let placed = self.ctx.place(&subject)?;
        let entries = self.ctx.menu(&subject)?;
        let checkout = self.ctx.checkouts.get(project).cloned();
        Ok(ActionMenu::over(
            actions::Subject::Branch {
                project: project.to_string(),
                branch: branch.branch.clone(),
            },
            placed.root,
            placed.workspace,
            Some(branch),
            placed.state,
            checkout,
            entries,
        ))
    }

    /// The entry, ready to run, in the shape the move takes
    /// (§AR-009-surfaces.1). Both keys that run something build it here, so
    /// the foreground run and the job cannot come to differ in where they
    /// think the entry belongs.
    fn request(&self, menu: &ActionMenu, entry: &actions::MenuEntry) -> crate::api::act::Run {
        crate::api::act::Run {
            about: match menu.subject.item() {
                Some(item) => crate::api::act::About::Item(Box::new(item.clone())),
                None => crate::api::act::About::Branch {
                    project: menu.subject.project().to_string(),
                    branch: menu
                        .branch
                        .as_ref()
                        .map(|branch| branch.branch.clone())
                        .unwrap_or_default(),
                },
            },
            root: menu.root.clone(),
            workspace: menu.workspace.clone(),
            state: menu.state.clone(),
            checkout: menu.checkout.clone(),
            branch: menu.branch.clone(),
            entry: entry.clone(),
        }
    }

    /// Start a menu entry beneath the screen (§FS-005-dispatch.17). The move
    /// is the session's; what is left here is the row it takes on the board
    /// and the sentence the reader sees.
    fn start_job(&mut self, menu: &ActionMenu, entry: &actions::MenuEntry) {
        let request = self.request(menu, entry);
        // Where it runs is the session's answer, the same one the key that
        // routed it here asked for (§AR-009-surfaces.1).
        let place = self.ctx.how(entry);
        let outcome = self.ctx.start_job(&request, place);
        self.message = match &outcome.job {
            // Nothing is waited on — the row it takes among the operations is
            // how it is watched from here.
            Some(_) => format!("{} · press ; for the board", outcome.says),
            None => outcome.says.clone(),
        };
        if let Some(id) = outcome.job {
            if let Some(job) = crate::seams::jobs::find(&id) {
                self.jobs.insert(0, job);
            }
            self.reload_operations();
        }
    }

    /// Go to the thing that is going (§FS-005-dispatch.21): a job's log,
    /// followed as it writes (§FS-005-dispatch.17); a run of the runtime,
    /// attached (§FS-005-dispatch.20); a program in its own window, that window
    /// brought forward (§FS-005-dispatch.22). Never a second copy of it.
    fn open_running(
        &mut self,
        terminal: &mut DefaultTerminal,
        running: &crate::api::offers::Running,
    ) -> Result<()> {
        use crate::api::offers::Running;
        match running {
            Running::Job { log, .. } => self.read_log(terminal, &log.clone(), true),
            // A run still standing at the gate is where the answer goes
            // (§FS-005-dispatch.20).
            Running::Run { id: Some(_), .. }
            | Running::Queued { id: Some(_), .. }
            | Running::Waiting { id: Some(_), .. } => self.attach(terminal, running),
            // And where nothing is standing there, the question is in the plan
            // and so is the answer (§FS-005-dispatch.9) — the same move `e`
            // makes on the work screen.
            Running::Waiting { plan, .. } => self.edit_file(terminal, &plan.clone()),
            // A run that named itself nothing has no surface to put on it, and
            // bringing a window forward costs no terminal — both are the
            // session's move, and the interface stays where it is
            // (§AR-007-runtime.3, §FS-005-dispatch.22).
            Running::Run { .. } | Running::Queued { .. } | Running::Window { .. } => {
                self.message = self
                    .ctx
                    .open_running(running, crate::api::act::Watching::Window)
                    .says;
                Ok(())
            }
        }
    }

    /// The runner's own surface on a run, opened where the reader can type into
    /// it (§FS-005-dispatch.20). Leaving it detaches and never stops the run —
    /// the binding's own contract, which ephor neither adds to nor takes from.
    fn attach(
        &mut self,
        terminal: &mut DefaultTerminal,
        running: &crate::api::offers::Running,
    ) -> Result<()> {
        // A window of the reader's own where one is bound, and the terminal
        // otherwise (§FS-005-dispatch.22). Not one call, because the terminal
        // is this call site's property: the interface gives it up around a
        // surface that takes it, and must not around one that does not
        // (§AR-002-summons.2).
        if self.ctx.opener().is_some() {
            self.message = self
                .ctx
                .open_running(running, crate::api::act::Watching::Window)
                .says;
            return Ok(());
        }
        let (root, id) = match running {
            crate::api::offers::Running::Run { root, id, .. }
            | crate::api::offers::Running::Queued { root, id, .. }
            | crate::api::offers::Running::Waiting { root, id, .. } => {
                (root.clone(), id.clone().unwrap_or_default())
            }
            _ => return Ok(()),
        };
        self.handover(
            terminal,
            "▶",
            &format!("watching run {id} — q detaches, the run goes on"),
            &Site::root(&root),
            &crate::work::runtime::attach_summons(&self.work, &id),
        )
    }

    /// Everything one job wrote, in the reader's pager (§FS-005-dispatch.17).
    /// A pager rather than an editor: a log is read, and `less` asked to
    /// follow keeps up with a job that is still writing — asked only where
    /// the pager is known to understand it, since `$PAGER` may be anything.
    fn read_log(
        &mut self,
        terminal: &mut DefaultTerminal,
        path: &Path,
        following: bool,
    ) -> Result<()> {
        let (pager, known) = match std::env::var("PAGER") {
            Ok(pager) if !pager.trim().is_empty() => (pager, false),
            _ => ("less".to_string(), true),
        };
        let follow = match following && known {
            true => " +F",
            false => "",
        };
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let command = format!(
            "{pager}{follow} {}",
            crate::feed::providers::shell_quote(&path.to_string_lossy())
        );
        self.handover(
            terminal,
            "📖",
            &pager,
            &Site::root(&dir),
            &summons::Summons::new("log", command),
        )
    }

    fn open_url(&mut self, url: Option<String>) {
        match url {
            Some(url) => {
                let result = std::process::Command::new("xdg-open")
                    .arg(&url)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                self.message = match result {
                    Ok(_) => format!("Opened {url}"),
                    Err(err) => format!("xdg-open failed: {err}"),
                };
            }
            None => self.message = "Nothing to open here".to_string(),
        }
    }

    /// Rebuild the view, having first settled everything a row shows that
    /// would otherwise be worked out again on every frame. A draw reads what
    /// is already decided and does no matching and no counting: the cursor
    /// moves without rebuilding, so anything left in the draw path is paid
    /// once per keystroke.
    fn rebuild_view(&mut self) {
        self.ctx.recompute_stats();
        self.navigator.rebuild(&self.ctx);
    }

    fn mark_done(&mut self, marks: Vec<(String, DateTime<Utc>, String)>) -> Result<()> {
        self.message = match marks.as_slice() {
            [(_, _, title)] => format!("Done: {title}"),
            _ => format!("Done: {} items", marks.len()),
        };
        for (id, updated_at, _) in marks {
            // Remember what it looked like, so the row can say what moved when
            // it comes back (§FS-007-matters.5).
            let mark = self
                .ctx
                .matter(&id)
                .map(|matter| cache::Mark::of(&matter))
                .unwrap_or_else(|| cache::Mark::at(updated_at));
            self.ctx.resurfacing.remove(&id);
            self.ctx.seen.insert(id, mark);
        }
        cache::store_seen(&self.ctx.seen)?;
        self.rebuild_view();
        Ok(())
    }

    /// Start a refresh and give the screen straight back
    /// (§FS-001-forge-interface.7). What it asks for is what the view shows:
    /// one project in Detail, every configured project otherwise.
    fn start_refresh(&mut self, config: &StatusConfig) {
        if self.refresh.is_some() {
            self.message = "Already refreshing".to_string();
            return;
        }
        let only = self.navigator.refresh_filter(&self.ctx);
        match crate::feed::refresh::BackgroundRefresh::start(config, only.as_deref()) {
            Ok(refresh) => {
                // The header carries the run from here; what it says about the
                // last thing that happened is finished with.
                self.message.clear();
                self.refresh = Some(refresh);
            }
            Err(err) => self.message = err.to_string(),
        }
    }

    /// Take in whatever the running refresh has finished. Returns true when
    /// something changed on screen, so the caller redraws rather than waiting
    /// out its poll.
    fn collect_refresh(&mut self) -> Result<bool> {
        let Some(refresh) = &mut self.refresh else {
            return Ok(false);
        };
        let arrived = refresh.collect();
        let done = refresh.done();
        if arrived.is_empty() && !done {
            return Ok(false);
        }
        // Each project takes its place as it answers, rather than the whole
        // run landing at the pace of its slowest forge
        // (§FS-001-forge-interface.7).
        for landed in &arrived {
            self.absorb(&landed.project)?;
        }
        if done {
            let mut refresh = self.refresh.take().expect("a refresh is in flight");
            refresh.finish();
            // Named, not counted. "6 provider warnings" is the same sentence
            // whether a forge has been uninstalled for months or a laptop is
            // off the VPN for a minute, and in both cases the sections those
            // providers fill just look empty.
            self.message = refresh.summary();
            // Once, at the end: what a checkout trails and what a project can
            // do are questions for git and the disk, not answers a forge just
            // sent, and asking them per arrival would put the cost the run
            // avoided back on the screen a project at a time.
            self.reload_feeds()?;
        }
        Ok(true)
    }

    /// Take one project's newly written feed into the interface, without the
    /// passes that ask the world. The reader is mid-scan: this is the cheap
    /// half of [`App::reload_feeds`], and the rest waits for the end of the
    /// run (§FS-001-forge-interface.7).
    fn absorb(&mut self, project: &str) -> Result<()> {
        let landed = cache::load_feed(project)?.unwrap_or_else(|| ProjectFeed {
            project: project.to_string(),
            ..ProjectFeed::default()
        });
        let Some(slot) = self
            .ctx
            .feeds
            .iter_mut()
            .find(|feed| feed.project == project)
        else {
            // Configured, but not among the feeds this screen was built over.
            // There is no row to put it next to.
            return Ok(());
        };
        *slot = landed;
        self.ctx.recompute_resurfacing();
        // Where each new item sits, in the same pass. This is the cheap half —
        // an in-memory fold over what just landed, asking the world nothing —
        // and the tree reads the answer rather than placing rows itself, so
        // skipping it files every item that arrives mid-run under *not linked
        // to a branch* and undercounts the branch above it until the whole run
        // finishes (§FS-001-forge-interface.7, §FS-008-attribution.2). Over
        // this project alone: it is the only one whose feed moved, and the
        // whole-site pass would re-match every other project's items again on
        // every arrival.
        self.ctx.recompute_placements_for(project);
        self.reload_work();
        self.rebuild_view();
        Ok(())
    }

    /// Open the operations board over whatever is on screen, or close it
    /// back to where the reader was (§FS-005-dispatch.15). Opening
    /// re-enumerates the roots first: a plan written by hand, or a run
    /// started in another terminal on a root ephor never dispatched into,
    /// is found by looking, never remembered.
    fn toggle_operations(&mut self) {
        if matches!(self.screen, Screen::Operations(_)) {
            self.screen = self.saved.take().unwrap_or(Screen::Navigator);
            return;
        }
        self.reload_work_groups();
        let (rows, refusal) = self.board_rows();
        let board = Screen::Operations(OperationsScreen::new(rows, refusal));
        self.saved = Some(std::mem::replace(&mut self.screen, board));
    }

    /// The board's rows, built off the draw path (§FS-005-dispatch.15):
    /// every execution root the last enumeration found, grouped — rhei locks
    /// per root and ephor's work root is per branch workspace, so two items
    /// in one workspace are one operation — with the runtime's artifacts
    /// answering what is live there. The ledger's plans carry their matter;
    /// an enumerated plan ephor never dispatched has none, and its row leads
    /// to the plan itself.
    fn board_rows(&self) -> (Vec<operations::Row>, Option<String>) {
        // What ephor is running itself needs no runtime and no registry: it is
        // ephor running a command (§FS-005-dispatch.17), so its rows stand
        // even where the runtime's half of the board says why it is empty.
        // The reader's own move first: a job is something they pressed a key
        // for moments ago, and a board that filed it under the runtime's work
        // would answer "did that start?" with a scroll.
        let mut rows: Vec<operations::Row> = self
            .jobs
            .iter()
            .filter(|job| job.live)
            .cloned()
            .map(operations::Row::Job)
            .collect();
        // The runtime's half, off the API's own derivation
        // ([`Ctx::running`]) — the same one `ephor operations` prints, so the
        // board a key opens and the board a command prints cannot come apart
        // (§AR-009-surfaces.1). The enumeration handed in is this screen's,
        // which is the one thing the two surfaces differ in: a command walks
        // the roots on the spot, and the interface keeps the last walk between
        // ticks and stats it instead (§FS-005-dispatch.15.1).
        let (running, refusal) = self.ctx.running(&self.work_groups);
        rows.extend(running.into_iter().map(|row| {
            operations::Row::Op(Box::new(operations::OpRow {
                op: row.op,
                item: row.item,
                plan: row.plan,
            }))
        }));
        (rows, refusal)
    }

    /// This matter's work as it stands, read from the plan rather than
    /// remembered (§FS-005-dispatch.4).
    fn work_status(&self, item: &Item) -> Option<crate::work::WorkStatus> {
        self.ctx
            .dispatcher
            .as_ref()
            .and_then(|dispatcher| dispatcher.status(item))
    }

    /// Rebuild the open board's rows; a closed board costs nothing.
    fn reload_operations(&mut self) {
        if !matches!(self.screen, Screen::Operations(_)) {
            return;
        }
        let (rows, refusal) = self.board_rows();
        if let Screen::Operations(board) = &mut self.screen {
            board.replace(rows, refusal);
        }
    }

    /// The tick (§FS-005-dispatch.15.1): every couple of seconds between key
    /// reads — never in the draw path — glance at what the ledger points at.
    /// A moved timestamp re-reads the plans, so a ticket the runtime parked
    /// resurfaces when it parks (§FS-005-dispatch.9) instead of at the next
    /// refresh; an unmoved one costs stat calls and nothing else. The open
    /// board also probes liveness: neither a run dying nor a run starting
    /// moves a file — the OS takes and releases the lock — so the lock is
    /// probed rather than watched, on the roots the board shows and on the
    /// ones it does not.
    ///
    /// Returns true when something on screen changed, and only then: a board
    /// asking for a frame every couple of seconds regardless is paying to
    /// show the reader what they are already looking at.
    fn tick(&mut self) -> bool {
        if self.ticked_at.elapsed() < WORK_TICK {
            return false;
        }
        self.ticked_at = std::time::Instant::now();
        // Jobs first, and whatever screen the reader is on: a job ending is
        // news exactly as a ticket parking is (§FS-005-dispatch.17), and the
        // re-listing below must not take an ended job away before it is said.
        let jobs_moved = self.pulse_jobs();
        let newest = self.work_wrote();
        let moved = match (newest, self.work_seen) {
            (Some(now), Some(seen)) => now > seen,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if moved {
            self.work_seen = newest;
            // A job appearing or ending moves what the gate above stats, and
            // a job somebody else started is found by looking, here
            // (§FS-005-dispatch.17).
            self.jobs = crate::seams::jobs::all();
            // Something moved, so the walk is paid once here: a plan that
            // appeared in a known root — the directory's own timestamp is in
            // the gate — joins the universe now (§FS-005-dispatch.15.1).
            self.reload_work_groups();
            self.reload_work();
            self.rebuild_view();
            self.reload_operations();
            return true;
        }
        let shown = match &self.screen {
            Screen::Operations(board) => board.roots(),
            // The board is closed, so nothing below it can change the screen —
            // but a job that ended already has (§FS-005-dispatch.17).
            _ => return jobs_moved,
        };
        // A run *starting* on a root the board has no row for writes nothing
        // the timestamp above watches — the OS takes its lock, and that is the
        // whole event. So the roots the enumeration knows and the board is not
        // showing are probed for exactly that, and one that came alive asks
        // for the rebuild that would give it a row (§FS-005-dispatch.15.1).
        let appeared = self.work_groups.iter().any(|group| {
            !shown.contains(&group.root)
                && crate::work::runtime::watch::live(&self.work, &group.root)
        });
        let work = &self.work;
        let found = match &mut self.screen {
            Screen::Operations(board) => {
                board.repulse(|root| crate::work::runtime::watch::pulse(work, root))
            }
            _ => return false,
        };
        if found.flipped || appeared || jobs_moved {
            self.reload_operations();
            return true;
        }
        // Only where something actually moved: a board that redrew itself
        // every couple of seconds regardless would be paying for a frame to
        // show the reader what they are already looking at.
        found.changed
    }

    /// Probe every job ephor knows of, and answer whether the screen moved
    /// (§FS-005-dispatch.17).
    ///
    /// Liveness is the lock and nothing else: a job that took its lock becomes
    /// live here, and one that let it go is read once more — the outcome is
    /// written before the lock is released, so what is read then is complete.
    /// That release is the whole event, and it moves no timestamp, which is
    /// why it is probed rather than watched. The transition is also what makes
    /// the news honest: only a job seen running can be said to have finished
    /// or died, so a job whose supervisor has not started yet says nothing
    /// rather than being reported as dead a millisecond after it was asked
    /// for.
    fn pulse_jobs(&mut self) -> bool {
        let mut ended = false;
        let mut changed = false;
        for job in &mut self.jobs {
            if job.ended.is_some() {
                continue;
            }
            let live = crate::seams::jobs::live(&job.dir);
            if live == job.live {
                continue;
            }
            changed = true;
            if live {
                job.live = true;
                continue;
            }
            if let Some(fresh) = crate::seams::jobs::read(&job.dir) {
                *job = fresh;
            } else {
                job.live = false;
            }
            // The line goes under the subject the job ran on, not at the top
            // of the screen: a header saying a replay went through names no
            // branch, and the reader with three going has to guess which row
            // moved (§FS-005-dispatch.17). A later job about the same subject
            // replaces it, because the row says what happened last.
            self.ctx
                .job_news
                .insert(crate::api::JobSubject::of(&job.record), job.says());
            ended = true;
        }
        if ended {
            // What the move changed is what the rows say: a replay moves a
            // branch's distance from its base, a checkout buys the rungs that
            // were waiting on it (§AR-005-capabilities.1), and a conflict
            // handed over is a ticket that was not there before
            // (§FS-005-dispatch.12).
            self.ctx.recompute_behind();
            self.ctx.recompute_capabilities();
            self.reload_work_groups();
            self.reload_work();
            self.rebuild_view();
        }
        if changed {
            self.reload_operations();
        }
        changed
    }

    /// The newest write across everything the last enumeration found: each
    /// plan file, each execution root's own directory — a plan appearing or
    /// vanishing is a directory event — and each root's run artifacts. Stat
    /// calls only, a fixed handful per root — the gate in front of every
    /// re-read and of every re-walk (§FS-005-dispatch.15.1).
    fn work_wrote(&self) -> Option<std::time::SystemTime> {
        let mut newest: Option<std::time::SystemTime> = None;
        let mut fold = |at: Option<std::time::SystemTime>| {
            if let Some(at) = at {
                if newest.map(|seen| at > seen).unwrap_or(true) {
                    newest = Some(at);
                }
            }
        };
        let modified = |path: &Path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
        };
        for group in &self.work_groups {
            for plan in &group.plans {
                fold(modified(&plan.path));
            }
            fold(modified(&group.root));
            fold(crate::work::runtime::watch::wrote_at(
                &self.work,
                &group.root,
            ));
        }
        // A job appearing is a directory event, and a job ending writes its
        // outcome into its own (§FS-005-dispatch.17). Two stats and one per
        // known job, in the same fixed handful this gate is built out of
        // (§FS-005-dispatch.15.1) — never a walk.
        fold(modified(&crate::seams::jobs::jobs_dir()));
        for job in &self.jobs {
            fold(modified(&job.dir));
        }
        newest
    }

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let [header_area, body_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        let title = match &self.answers {
            // What is being answered is what the reader is doing, so it is
            // what the header says while the answers are up.
            Some(answers) => answers.title(),
            None => match &self.screen {
                Screen::Navigator => self.navigator.title(&self.ctx),
                Screen::Thread(thread) => thread.title(),
                Screen::Gate(gate) => gate.title(),
                Screen::Work(work) => work.title(),
                Screen::Operations(board) => board.title(),
            },
        };
        // A screen that stays live during a fetch is also a screen that looks
        // finished, so a run in flight says so and says where it has got to —
        // a half-filled feed read as the whole answer is the same failure as
        // an empty section that only means "not asked yet"
        // (§FS-001-forge-interface.7).
        let progress = match &self.refresh {
            Some(refresh) => format!("{}   ", refresh.progress()),
            None => String::new(),
        };
        frame.render_widget(
            Paragraph::new(format!("{title}   {progress}{}", self.message))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            header_area,
        );

        match &mut self.screen {
            Screen::Navigator => self.navigator.draw(&self.ctx, frame, body_area),
            Screen::Thread(thread) => thread.draw(frame, body_area),
            Screen::Gate(gate) => gate.draw(frame, body_area),
            Screen::Work(work) => work.draw(frame, body_area),
            // The refresh reports on the board additionally — the header
            // above keeps its line (§FS-001-forge-interface.7).
            Screen::Operations(board) => {
                let line = self
                    .refresh
                    .as_ref()
                    .map(crate::feed::refresh::BackgroundRefresh::progress);
                board.draw(frame, body_area, line)
            }
        }
        if let Some(menu) = &self.menu {
            menu.draw(frame, body_area);
        }
        // Over the menu it was reached from, under the prompt like everything
        // else (§FS-005-dispatch.19).
        if let Some(answers) = &self.answers {
            answers.draw(frame, body_area);
        }
        if let Some(prompt) = &self.prompt {
            prompt.draw(frame, body_area);
        }

        let footer = if self.prompt.is_some() {
            " type  ·  enter sends  ·  esc cancels  ·  ^w word back  ·  ^u clear".to_string()
        // Built from what is open, not fixed for the screen: what Enter does
        // on a row of answers depends on the row (§FS-004-quick-actions.2).
        } else if let Some(answers) = &self.answers {
            answers.footer()
        } else if let Some(menu) = &self.menu {
            menu.footer()
        } else {
            match &self.screen {
                Screen::Navigator => self.navigator.footer(),
                // Built from what is selected, not fixed per screen
                // (§FS-004-quick-actions.2).
                Screen::Thread(thread) => thread.footer(),
                Screen::Gate(gate) => gate.footer().to_string(),
                Screen::Work(work) => work.footer(),
                Screen::Operations(board) => board.footer().to_string(),
            }
        };
        frame.render_widget(
            Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
            footer_area,
        );
    }
}

/// What a reader wrote into the answers file, as answers. The line that says
/// what each input wants is not one of them, and an input left at null is
/// still unanswered — which is how leaving the file alone cancels.
fn read_answers(path: &Path) -> std::collections::BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Default::default();
    };
    let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(&text) else {
        return Default::default();
    };
    fields
        .into_iter()
        .filter(|(name, value)| !value.is_null() && name != "what each input wants")
        .map(|(name, value)| {
            let word = match value {
                serde_json::Value::String(text) => text,
                other => other.to_string(),
            };
            (name, word)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// What a menu entry's summons is told it is about, asked of the one
    /// implementation both surfaces use (§AR-009-surfaces.1).
    fn dossier_of(menu: &ActionMenu, workspace: &Path) -> Vec<(String, String)> {
        let about = match menu.subject.item() {
            Some(item) => crate::api::act::About::Item(Box::new(item.clone())),
            None => crate::api::act::About::Branch {
                project: menu.subject.project().to_string(),
                branch: menu
                    .branch
                    .as_ref()
                    .map(|branch| branch.branch.clone())
                    .unwrap_or_default(),
            },
        };
        crate::api::act::dossier_of(&about, &menu.root, workspace, menu.branch.as_ref(), None)
    }

    use crate::branches::Placement;
    use crate::capabilities::Rung;
    use crate::feed::cache::Seen;
    use crate::feed::model::ItemKind;
    use crate::forest::{Staleness, Standing, Upstream};
    use serde_json::json;

    pub(super) fn ctx_with_branch(root: &Path, template: Option<&str>) -> Ctx {
        let branch = BranchInfo {
            branch: "you/ABC-42-retry-window".to_string(),
            ticket: Some("ABC-42".to_string()),
            active: true,
            is_release: false,
            declared: true,
        };
        let placement = Placement {
            project: "widget".to_string(),
            root: root.to_path_buf(),
            template: template.map(String::from),
            branches: vec![branch],
            main_branch: Some("master".to_string()),
            repos: Vec::new(),
            aliases: Vec::new(),
            territory: Vec::new(),
            trust: crate::manifest::Trust::Full,
        };
        Ctx {
            feeds: Vec::new(),
            seen: Seen::new(),
            projects: vec!["widget".to_string()],
            orgs: Vec::new(),
            project_org: BTreeMap::new(),
            placements: BTreeMap::from([("widget".to_string(), placement)]),
            behind: BTreeMap::new(),
            standing: BTreeMap::new(),
            on_branch: BTreeMap::new(),
            linked: BTreeMap::new(),
            stats: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            resurfacing: BTreeMap::new(),
            unattributed: Vec::new(),
            actions: Vec::new(),
            project_actions: BTreeMap::new(),
            provider_blocks: BTreeMap::new(),
            checkouts: BTreeMap::new(),
            recent_days: 7,
            unread_only: true,
            ..Ctx::default()
        }
    }

    /// Give the fixture project a declared forest.
    fn declare(ctx: &mut Ctx, repos: &[&str]) {
        let placement = ctx
            .placements
            .get_mut("widget")
            .expect("the fixture project");
        placement.repos = repos
            .iter()
            .map(|name| crate::forest::Declaration::at(*name))
            .collect();
    }

    fn ticket_item() -> Item {
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "[ABC-42] Fix condition errors".to_string(),
            url: None,
            state: None,
            needs_response: false,
            updated_at: Utc::now(),
            raw: json!({}),
        }
    }

    /// An issue: no branch, because an issue has none until somebody cuts one.
    fn issue_item() -> Item {
        Item {
            id: "github-issues:acme/widget#95".to_string(),
            kind: ItemKind::Issue,
            source: "github-issues".to_string(),
            title: "Durations read as seconds".to_string(),
            ..ticket_item()
        }
    }

    /// The root a surface asks about is the root the dispatch will use
    /// (§FS-005-dispatch.14). An entry that says which branch its work belongs
    /// on is dispatched inside the workspace that template names, so the hand
    /// shown on its row and the roster its picker offers are read there — not
    /// at the project root, which for a branch-addressable project holds no
    /// change at all (§FS-005-dispatch.25).
    #[test]
    fn the_root_a_surface_asks_about_is_the_one_the_entry_would_be_dispatched_into() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
        let issue = issue_item();

        // Nothing said which branch: the matter is on none, so the answer is
        // the project's own root — which is what the dispatch refuses over
        // where the work edits the change.
        assert_eq!(
            ctx.work_root(&issue, None),
            Some(tmp.path().join("panta")),
            "the matter's own"
        );
        // And with the entry's template, the root inside the workspace that
        // template names — where its dispatch writes the plan.
        assert_eq!(
            ctx.work_root(&issue, Some("fix/issue-{number}")),
            Some(tmp.path().join("fix/issue-95/panta"))
        );
        // A matter with a branch of its own is never displaced by a template.
        let pr = ticket_item();
        assert_eq!(
            ctx.work_root(&pr, Some("fix/issue-{number}")),
            ctx.work_root(&pr, None)
        );
    }

    /// One project's cached feed, holding one matter the forge put on `branch`.
    fn feed_on(project: &str, key: &str, branch: &str) -> ProjectFeed {
        let matter = crate::matter::Matter {
            key: crate::matter::SubjectKey::stated(key),
            kind: ItemKind::Pr,
            placement: crate::matter::Placement::on(project),
            source: "github-prs".to_string(),
            title: "Retry window".to_string(),
            role: None,
            url: None,
            state: None,
            needs_response: false,
            updated_at: Utc::now(),
            links: Vec::new(),
            discussions: Vec::new(),
            events: Vec::new(),
            fingerprint: Default::default(),
            raw: json!({ "branch": branch }),
        };
        ProjectFeed {
            project: project.to_string(),
            providers: BTreeMap::from([(
                "github-prs".to_string(),
                crate::feed::cache::ProviderSlot {
                    ok: true,
                    matters: vec![matter],
                    ..Default::default()
                },
            )]),
            ..ProjectFeed::default()
        }
    }

    /// A second project beside the fixture's, with a branch and a feed of its
    /// own, so a pass scoped to one has something to leave alone.
    fn with_second_project(ctx: &mut Ctx) {
        let placement = Placement {
            project: "gadget".to_string(),
            branches: vec![BranchInfo {
                branch: "you/XYZ-7-widen".to_string(),
                ticket: Some("XYZ-7".to_string()),
                active: true,
                is_release: false,
                declared: true,
            }],
            ..ctx.placements["widget"].clone()
        };
        ctx.projects.push("gadget".to_string());
        ctx.placements.insert("gadget".to_string(), placement);
        ctx.feeds = vec![
            feed_on(
                "widget",
                "github-prs:acme/widget#42",
                "you/ABC-42-retry-window",
            ),
            feed_on("gadget", "github-prs:acme/gadget#7", "you/XYZ-7-widen"),
        ];
    }

    /// A refresh lands one project at a time, and the placement pass it runs
    /// per landing answers for that project alone: the rest of the site keeps
    /// the rows it had, and what the scoped pass leaves behind is what the
    /// whole-site pass would have left there — one implementation, so the
    /// mid-scan answer and the end-of-run answer cannot disagree
    /// (§FS-001-forge-interface.7, §FS-008-attribution.2).
    #[test]
    fn a_landing_places_its_own_project_and_leaves_the_rest_standing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = ctx_with_branch(tmp.path(), None);
        with_second_project(&mut ctx);
        ctx.recompute_placements();

        let widget = ctx.branches("widget")[0].clone();
        let gadget = ctx.branches("gadget")[0].clone();
        assert_eq!(ctx.branch_linked("widget", &widget), 1);
        assert_eq!(ctx.branch_linked("gadget", &gadget), 1);

        // Widget's feed lands again, this time with the item on no branch the
        // project knows. Only widget is re-placed.
        ctx.feeds[0] = feed_on("widget", "github-prs:acme/widget#42", "you/ABC-99-other");
        ctx.recompute_placements_for("widget");
        assert_eq!(ctx.branch_linked("widget", &widget), 0);
        assert_eq!(ctx.branch_linked("gadget", &gadget), 1);
        // The row that left the branch left the map with it — a stale entry
        // would keep filing it under a branch it is no longer on.
        assert!(ctx
            .on_branch
            .keys()
            .all(|(project, _)| project.as_str() != "widget"));

        // And the two scopes agree about the whole site.
        let scoped = (ctx.on_branch.clone(), ctx.linked.clone());
        ctx.recompute_placements();
        assert_eq!(scoped, (ctx.on_branch.clone(), ctx.linked.clone()));
    }

    #[test]
    fn a_sources_own_action_leads_the_menu() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = ctx_with_branch(tmp.path(), None);
        ctx.actions = vec![serde_json::from_value(json!({
            "icon": "🧪", "description": "run the gate", "command": "just gate"
        }))
        .unwrap()];
        ctx.provider_blocks = BTreeMap::from([(
            "widget".to_string(),
            vec![json!({ "provider": "github-ci", "repos": ["acme/widget"] })],
        )]);

        let mut ci = ticket_item();
        ci.id = "github-ci:acme/widget#42".to_string();
        ci.source = "github-ci".to_string();
        ci.kind = ItemKind::Pr;
        ci.state = None;
        // The gate rides on the pull request now, and the source's own action
        // is offered off the gate rather than off a state word.
        ci.raw = json!({
            "repo": "acme/widget",
            "gate": { "repos": [{
                "repo": "acme/widget", "passed": 1, "failed": 2, "running": 0
            }] }
        });

        // The configured action keeps its place and the source's own goes
        // ahead of it (§FS-004-quick-actions.3) — where `gh` is installed for
        // it to be offered at all.
        let menu = ctx.actions_with(&ci, &[], &[]);
        // The failures entry and both restarts (§FS-004-quick-actions.9),
        // where `gh` is installed for any of them to be offered.
        let quick = if crate::feed::provider::command_exists("gh") {
            3
        } else {
            0
        };
        assert_eq!(menu.len(), quick + 1);
        assert_eq!(menu.last().unwrap().description, "run the gate");
        if quick > 0 {
            assert_eq!(menu[0].description, "see the CI failures");
        }
    }

    /// Provenance orders the menu and a repeated id wins in place: what ephor
    /// recognized, then what the project offers of itself, then the person's
    /// own (§FS-006-project-interface.9). The project's offers arrive under
    /// the trust the row extends to them (§FS-006-project-interface.2).
    #[test]
    fn the_menu_is_shipped_then_the_projects_then_the_persons() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(crate::manifest::FILE),
            r#"{"actions": [
                 {"id": "bench", "description": "the project's benchmark",
                  "command": "./bench.sh", "when": {"kinds": ["pr"]}},
                 {"id": "nightly", "description": "only on a green gate",
                  "command": "./nightly.sh", "when": {"gate": "green"}},
                 {"id": "rebase", "description": "the project's own rebase",
                  "command": "./rebase.sh"}
               ]}"#,
        )
        .unwrap();
        let mut ctx = ctx_with_branch(tmp.path(), None);
        ctx.actions = vec![serde_json::from_value(json!({
            "id": "bench", "icon": "🧪", "description": "my benchmark", "command": "just bench"
        }))
        .unwrap()];

        let menu = ctx.actions_with(&ticket_item(), &[], &[]);
        let described: Vec<&str> = menu
            .iter()
            .map(|action| action.description.as_str())
            .collect();
        // The item has no gate, so the offer asking for a green one is not
        // there at all; the person's `bench` replaced the project's, in the
        // place the project's held.
        assert_eq!(
            described,
            ["my benchmark", "the project's own rebase"],
            "{described:?}"
        );
        assert_eq!(menu[0].command, "just bench");

        // A row that trusts the checkout for descriptions only runs none of
        // what it offers.
        ctx.placements
            .get_mut("widget")
            .expect("the fixture project")
            .trust = crate::manifest::Trust::Descriptions;
        let menu = ctx.actions_with(&ticket_item(), &[], &[]);
        assert_eq!(menu.len(), 1, "only the person's own is left");
        assert_eq!(menu[0].description, "my benchmark");
    }

    #[test]
    fn checkout_resolves_existing_branch_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace_dir = root.join("you/ABC-42-retry-window");
        std::fs::create_dir_all(&workspace_dir).unwrap();

        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        let placed = ctx.checkout(&ticket_item()).unwrap();
        assert_eq!(placed.workspace, workspace_dir);
        assert_eq!(placed.ticket.as_deref(), Some("ABC-42"));
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    /// A fixture writes its refs the moment the test runs, so a distance
    /// measured against one of them is dated today
    /// (§FS-004-quick-actions.6). The repositories `repo_behind` builds have
    /// no remote at all, so their base is a local branch nothing fetched and
    /// their distances carry no day.
    fn today() -> String {
        chrono::Local::now().format("%b %-d").to_string()
    }

    /// How far the item's checkout trails the project's main branch, out of
    /// the one fold the offers read.
    fn behind(ctx: &Ctx, item: &Item) -> Option<u64> {
        ctx.item_trailing(item)
            .and_then(|trailing| trailing.behind)
            .map(|trail| trail.behind)
    }

    /// A repo whose `feature` branch is `commits` commits behind `master`.
    fn repo_behind(dir: &Path, commits: usize) {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q", "-b", "master"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
        git(dir, &["branch", "feature"]);
        for index in 0..commits {
            git(
                dir,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("ahead {index}"),
                ],
            );
        }
        git(dir, &["checkout", "-q", "feature"]);
    }

    #[test]
    fn item_checkout_state_uses_recorded_branch_without_registry_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));

        // A PR whose branch has no registry entry, resolved via raw.branch.
        let mut pr = ticket_item();
        pr.title = "Unrelated title".to_string();
        pr.raw = json!({ "branch": "someone/feature" });
        assert_eq!(ctx.item_checked_out(&pr), Some(false));
        std::fs::create_dir_all(root.join("someone/feature")).unwrap();
        assert_eq!(ctx.item_checked_out(&pr), Some(true));
        assert_eq!(
            ctx.checkout(&pr).unwrap().workspace,
            root.join("someone/feature")
        );

        // No branch information at all: state is unknown.
        pr.raw = json!({});
        assert_eq!(ctx.item_checked_out(&pr), None);
        assert!(matches!(
            ctx.checkout(&pr).unwrap().state,
            WorkspaceState::Unmatched
        ));
    }

    #[test]
    fn behind_sums_across_workspace_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);
        repo_behind(&workspace.join("ee"), 3);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        ctx.recompute_behind();
        let staleness = ctx
            .branch_behind("widget", "you/ABC-42-retry-window")
            .expect("both repositories were measured");
        assert_eq!(staleness.total(), Some(5));
        // The sum is reported, and which repository it came from survives it
        // (§AR-004-forest.1).
        assert_eq!(
            staleness.summary().as_deref(),
            Some("5 behind (ce 2, ee 3)")
        );
    }

    /// The standing rides beside the behind count, from the same fold: two
    /// distances, two facts — one against the project's main branch, one
    /// against the branch's own published copy, and the branch is read off
    /// each repository's `HEAD`, never the workspace directory's name
    /// (§DA-003-upstream-is-the-published-copy).
    #[test]
    fn the_standing_is_measured_beside_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        let repo = workspace.join("ce");
        repo_behind(&repo, 3);
        // The branch was pushed, then its copy grew two commits this
        // checkout has not pulled — no tracking config, the worktree shape.
        for step in 0..2 {
            git(
                &repo,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("pushed {step}"),
                ],
            );
        }
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/feature", "HEAD"],
        );
        git(&repo, &["reset", "-q", "--hard", "HEAD~2"]);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        ctx.recompute_behind();
        assert_eq!(
            ctx.branch_behind("widget", "you/ABC-42-retry-window")
                .and_then(Staleness::total),
            Some(3)
        );
        let standing = ctx
            .branch_standing("widget", "you/ABC-42-retry-window")
            .expect("the copy was read");
        assert_eq!(standing.behind_upstream(), Some(2));
        assert_eq!(standing.repos[0].branch.as_deref(), Some("feature"));
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Published {
                remote: "origin".to_string(),
                branch: "feature".to_string(),
            }
        );
    }

    /// The rebase is in the menu because of what is on disk, and only then
    /// (§FS-004-quick-actions.6).
    #[test]
    fn the_rebase_is_offered_on_a_checkout_that_trails_main() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);
        repo_behind(&workspace.join("ee"), 3);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        let pr = ticket_item();
        assert_eq!(behind(&ctx, &pr), Some(5));
        let menu = ctx.actions_with(&pr, &[], &[]);
        assert_eq!(menu[0].description, "rebase onto master (5 behind)");
        assert!(menu[0].command.contains("rebase --project"));
        assert!(menu[0].requires_checkout);

        // Level with master: still offered, because the reading that says
        // level is only as fresh as the last fetch and the replay is what
        // would refresh it — and the entry says *level* rather than a count
        // (§FS-004-quick-actions.6).
        for repo in ["ce", "ee"] {
            git(&workspace.join(repo), &["checkout", "-q", "master"]);
        }
        assert_eq!(behind(&ctx, &pr), Some(0));
        let level = ctx.actions_with(&pr, &[], &[]);
        assert_eq!(level.len(), 1, "{level:?}");
        assert_eq!(level[0].id, "rebase");
        assert_eq!(level[0].description, "rebase onto master (level)");
    }

    #[test]
    fn the_rebase_is_not_offered_where_there_is_nothing_to_measure() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));

        // The branch workspace was never checked out.
        assert_eq!(behind(&ctx, &ticket_item()), None);
        assert!(ctx.actions_with(&ticket_item(), &[], &[]).is_empty());

        // An item that resolves to no branch at all has nowhere to rebase,
        // whatever kind it is (§FS-004-quick-actions.2).
        let mut nowhere = ticket_item();
        nowhere.title = "Nothing about any branch".to_string();
        assert_eq!(behind(&ctx, &nowhere), None);
        assert!(ctx.actions_with(&nowhere, &[], &[]).is_empty());
    }

    /// The offer follows the branch on disk, not the kind of the row that
    /// mentions it: an issue and a message about the same change are offered
    /// exactly what the pull request is (§FS-004-quick-actions.6).
    #[test]
    fn any_item_that_resolves_to_a_workspace_is_offered_the_rebase() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 4);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let offered = "rebase onto master (4 behind)";
        for kind in [
            ItemKind::Pr,
            ItemKind::Issue,
            ItemKind::Task,
            ItemKind::Message,
            ItemKind::Ci,
            ItemKind::Status,
        ] {
            let mut item = ticket_item();
            item.kind = kind;
            let menu = ctx.actions_with(&item, &[], &[]);
            assert_eq!(menu.len(), 1, "{kind:?}: {menu:?}");
            assert_eq!(menu[0].description, offered, "{kind:?}");
            // And the entry says nothing about kinds any more, so nothing
            // downstream can narrow it back to pull requests.
            assert!(menu[0].kinds.is_empty(), "{kind:?}");
        }
    }

    /// The two offers are gated apart: replaying onto the published copy
    /// resolves its ref inside each repository, so a project that declares no
    /// main branch is still offered it — and is offered nothing to replay onto
    /// a base nothing names (§FS-004-quick-actions.6).
    #[test]
    fn a_project_with_no_main_branch_is_still_offered_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = root.join("you/ABC-42-retry-window/ce");
        repo_behind(&repo, 3);
        published_ahead(&repo, "feature", 2);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        ctx.placements
            .get_mut("widget")
            .expect("the fixture project")
            .main_branch = None;

        let menu = ctx.actions_with(&ticket_item(), &[], &[]);
        assert_eq!(menu.len(), 1, "{menu:?}");
        assert_eq!(menu[0].id, "rebase-upstream");
        assert_eq!(
            menu[0].description,
            format!("rebase onto origin/feature (2 behind as of {})", today())
        );

        // The row is gated the same way, so what it shows and what the menu
        // offers cannot disagree: the copy's distance, and no distance to a
        // main branch the project never named.
        ctx.recompute_behind();
        assert!(ctx
            .branch_behind("widget", "you/ABC-42-retry-window")
            .is_none());
        assert_eq!(
            ctx.branch_standing("widget", "you/ABC-42-retry-window")
                .and_then(Standing::behind_upstream),
            Some(2)
        );
    }

    /// The branch row carries the same offers, built by the same code: this is
    /// where a reader looking at a stale branch is standing
    /// (§FS-004-quick-actions.6).
    #[test]
    fn a_branch_row_carries_the_same_offers_as_the_items_on_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);
        published_ahead(&workspace.join("ce"), "feature", 1);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let offered = ctx.branch_actions("widget", "you/ABC-42-retry-window");
        assert_eq!(offered.len(), 2, "{offered:?}");
        assert_eq!(offered[0].description, "rebase onto master (2 behind)");
        assert_eq!(
            offered[1].description,
            format!("rebase onto origin/feature (1 behind as of {})", today())
        );

        // The same entries the item's menu carries — one implementation, so a
        // reader cannot be told two different things about one checkout.
        let menu = ctx.actions_with(&ticket_item(), &[], &[]);
        let described = |actions: &[ActionConfig]| -> Vec<(String, String)> {
            actions
                .iter()
                .map(|action| (action.id.clone(), action.command.clone()))
                .collect()
        };
        assert_eq!(described(&offered), described(&menu));

        // A branch whose workspace is not on disk is a checkout question
        // (§FS-004-quick-actions.7), so the rebase is withheld rather than
        // offered and left to fail.
        assert!(ctx
            .branch_actions("widget", "you/never-checked-out")
            .is_empty());
    }

    /// Publish the branch this repository is on and move that copy `commits`
    /// ahead of the checkout — somebody else pushed to it, and no tracking
    /// config was ever written (§DA-003-upstream-is-the-published-copy).
    fn published_ahead(dir: &Path, branch: &str, commits: usize) {
        for index in 0..commits {
            git(
                dir,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("pushed {index}"),
                ],
            );
        }
        git(
            dir,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                "HEAD",
            ],
        );
        if commits > 0 {
            git(dir, &["reset", "-q", "--hard", &format!("HEAD~{commits}")]);
        }
    }

    /// A repository parked on the base itself and tracking it, whose copy is
    /// `commits` ahead: the workspace repository a change does not touch. Its
    /// published copy *is* its base, so both distances are the same distance.
    fn repo_on_the_base(dir: &Path, commits: usize) {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q", "-b", "master"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
        published_ahead(dir, "master", commits);
        git(dir, &["remote", "add", "origin", "."]);
        git(
            dir,
            &["branch", "--set-upstream-to=origin/master", "master"],
        );
    }

    /// The second offer: onto the branch's own published copy, naming the ref
    /// so the two entries differ in the word that matters
    /// (§FS-004-quick-actions.8).
    #[test]
    fn the_rebase_onto_the_published_copy_is_offered_and_names_the_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = root.join("you/ABC-42-retry-window/ce");
        // Level with main, so only the published copy has anything to replay.
        repo_behind(&repo, 0);
        published_ahead(&repo, "feature", 2);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let pr = ticket_item();
        let menu = ctx.actions_with(&pr, &[], &[]);
        assert_eq!(menu.len(), 2, "{menu:?}");
        assert_eq!(menu[1].id, "rebase-upstream");
        assert_eq!(
            menu[1].description,
            format!("rebase onto origin/feature (2 behind as of {})", today())
        );
        assert!(menu[1].command.contains("rebase --upstream --project"));
        assert!(menu[1].requires_checkout);

        // Level with the copy: still offered, and labelled *level* — the
        // distance to a copy is measured against what was last fetched too
        // (§FS-004-quick-actions.8).
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/feature", "HEAD"],
        );
        let level = ctx.actions_with(&pr, &[], &[]);
        assert_eq!(level.len(), 2, "{level:?}");
        assert_eq!(
            level[1].description,
            format!("rebase onto origin/feature (level as of {})", today())
        );

        // A branch published nowhere has no copy to name and no reading a
        // fetch would correct, so this entry alone goes.
        git(&repo, &["update-ref", "-d", "refs/remotes/origin/feature"]);
        let unpushed = ctx.actions_with(&pr, &[], &[]);
        assert_eq!(unpushed.len(), 1, "{unpushed:?}");
        assert_eq!(unpushed[0].id, "rebase");
    }

    /// A forest where the repositories disagree — one on the change's branch,
    /// one parked on the base — is offered both, because a forest is not one
    /// branch (§FS-004-quick-actions.8). The copy entry counts, and names,
    /// only the repository that trails a copy of its own: the parked one's
    /// distance is the first entry's, not this one's twice.
    #[test]
    fn both_rebases_are_offered_where_the_forest_disagrees() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 0);
        published_ahead(&workspace.join("ce"), "feature", 2);
        repo_on_the_base(&workspace.join("ee"), 1);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        let menu = ctx.actions_with(&ticket_item(), &[], &[]);
        assert_eq!(menu.len(), 2);
        assert_eq!(menu[0].description, "rebase onto master (1 behind)");
        // `ee`'s copy is its base, so it neither counts here nor keeps the
        // entry from naming the one ref the counted repositories share.
        assert_eq!(menu[1].id, "rebase-upstream");
        assert_eq!(
            menu[1].description,
            format!("rebase onto origin/feature (2 behind as of {})", today())
        );
    }

    /// And where every repository's published copy *is* its base, the copy
    /// entry has nothing of its own to count: only the first is offered
    /// (§FS-004-quick-actions.8).
    #[test]
    fn the_rebase_onto_the_copy_is_not_offered_where_the_copy_is_the_base() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_on_the_base(&workspace.join("ce"), 1);
        repo_on_the_base(&workspace.join("ee"), 1);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce", "ee"]);
        // The distance is real, and the base count carries it; the copy sum
        // leaves it out entirely, so no gate anywhere reads one distance
        // under two names.
        let trailing = ctx
            .item_trailing(&ticket_item())
            .expect("the checkout was measured");
        assert_eq!(trailing.behind.map(|trail| trail.behind), Some(2));
        assert_eq!(trailing.behind_upstream, None);
        let menu = ctx.actions_with(&ticket_item(), &[], &[]);
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].id, "rebase");
        assert_eq!(
            menu[0].description,
            format!("rebase onto master (2 behind as of {})", today())
        );
    }

    /// A red gate on my own change, on a checkout that trails: the commands
    /// and the work stand in one menu (§FS-005-dispatch.1), each carrying its
    /// own icon, and the replay appears once — the recipe named `rebase` is
    /// what that entry hands its conflict to, not a second row saying the same
    /// thing.
    #[test]
    fn the_menu_carries_the_work_that_can_be_handed_over() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let workspace = root.join("you/ABC-42-retry-window");
        repo_behind(&workspace.join("ce"), 2);

        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        declare(&mut ctx, &["ce"]);
        let mut mine = ticket_item();
        mine.role = Some(crate::feed::model::ItemRole::Author);
        mine.raw = json!({ "gate": { "repos": [
            { "repo": "ce", "passed": 1, "failed": 2, "running": 0 }
        ] } });

        let recipes = crate::work::recipe::shipped();
        let menu = ctx.actions_with(&mine, &recipes, &[]);
        let described: Vec<(&str, &str, bool)> = menu
            .iter()
            .map(|entry| {
                (
                    entry.icon.as_str(),
                    entry.description.as_str(),
                    entry.agent.is_some(),
                )
            })
            .collect();
        assert_eq!(
            described,
            [
                ("⤴", "rebase onto master (2 behind)", false),
                ("🛠", "fix the red gate", true),
            ],
            "{described:?}"
        );
        // The work rides on the entry whole, so what is dispatched from the
        // menu is the recipe itself (§FS-005-dispatch.4).
        let work = menu[1].agent.as_ref().expect("the recipe rides along");
        assert_eq!(work.id, "fix-gate");
        assert!(work.brief.starts_with("The gate on {title} is red."));
        // And the replay is one entry, the deterministic one.
        assert_eq!(menu.iter().filter(|entry| entry.id == "rebase").count(), 1);
    }

    /// Offered only where it would work (§FS-004-quick-actions.2): work that
    /// edits the change waits on the change being here, work that reads one
    /// does not, and nothing is asked about an item that is finished
    /// (§FS-005-dispatch.6).
    #[test]
    fn work_is_offered_where_it_would_work_and_nowhere_else() {
        let tmp = tempfile::tempdir().unwrap();
        // Nothing checked out: the branch workspace the template names is not
        // on disk.
        let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
        let recipes = crate::work::recipe::shipped();
        let ids = |ctx: &Ctx, item: &Item| -> Vec<String> {
            ctx.actions_with(item, &recipes, &[])
                .into_iter()
                .filter(|entry| entry.agent.is_some())
                .map(|entry| entry.id)
                .collect()
        };

        // Fixing a gate edits the change, so it is the checkout's question
        // first (§FS-004-quick-actions.7).
        let mut mine = ticket_item();
        mine.role = Some(crate::feed::model::ItemRole::Author);
        mine.raw = json!({ "gate": { "repos": [
            { "repo": "ce", "passed": 1, "failed": 2, "running": 0 }
        ] } });
        assert!(ids(&ctx, &mine).is_empty());

        // Reviewing one reads it, and fetches what it needs: offered with
        // nothing on disk at all.
        let mut theirs = ticket_item();
        theirs.role = Some(crate::feed::model::ItemRole::Reviewer);
        assert_eq!(ids(&ctx, &theirs), ["review"]);

        // Merged: there is nothing to ask for about it any more.
        let mut done = theirs.clone();
        done.state = Some("merged".to_string());
        assert!(ids(&ctx, &done).is_empty());
    }

    /// With no runner bound the work is still offered — a ticket is written
    /// whether or not anything can run it — and where the entry would say who
    /// gets it, it says instead that nobody can be asked, in the *workable*
    /// rung's own words (§FS-005-dispatch.14).
    #[test]
    fn with_no_runner_bound_the_work_is_still_offered_and_says_nobody_can_be_asked() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
        let mut theirs = ticket_item();
        theirs.role = Some(crate::feed::model::ItemRole::Reviewer);
        let offered = ctx.actions_with(&theirs, &crate::work::recipe::shipped(), &[]);
        assert_eq!(
            offered.iter().filter(|entry| entry.agent.is_some()).count(),
            1
        );

        // The rung's own sentence about a runner that is not there.
        let unbound = crate::work::runtime::refusal(&crate::work::recipe::WorkConfig {
            runner: Some("no-such-runner-here".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        })
        .expect("a runner that is not on PATH is refused");
        assert!(unbound.contains("no-such-runner-here"), "{unbound}");

        use crate::work::runtime::roster::{Choice, Hand};
        let nobody =
            crate::api::session::who_gets_it(&Choice::Unasked { note: None }, Some(&unbound));
        assert_eq!(nobody.says, unbound);
        // Said, not refused: the ticket is written all the same.
        assert!(nobody.refusal.is_none());

        // With a runner there and nobody named, the runtime picks unasked.
        let unasked = crate::api::session::who_gets_it(&Choice::Unasked { note: None }, None);
        assert_eq!(unasked.says, "whoever the runtime picks");

        // A chosen hand names itself, and carries why it cannot be asked right
        // now rather than vanishing (§FS-005-dispatch.14).
        let chosen = crate::api::session::who_gets_it(
            &Choice::Chosen {
                hand: Hand {
                    id: "luna".to_string(),
                    agent: Some("claude-code".to_string()),
                    model: None,
                    provider: None,
                    efforts: vec!["high".to_string()],
                    available: Some("'claude-code' is not on PATH".to_string()),
                },
                effort: Some("high".to_string()),
                whence: "the site's default hand".to_string(),
                note: None,
            },
            None,
        );
        assert_eq!(
            chosen.says,
            "luna at high (unavailable: 'claude-code' is not on PATH)"
        );
        assert!(chosen.refusal.is_none());

        // And a choice that cannot stand is the whole reason, and refuses.
        let refused = crate::api::session::who_gets_it(
            &Choice::Refused("permits only sonnet".to_string()),
            None,
        );
        assert_eq!(refused.refusal.as_deref(), Some("permits only sonnet"));
    }

    #[test]
    fn behind_skips_unchecked_branches_and_non_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Workspace missing entirely: no entry.
        let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        ctx.recompute_behind();
        assert!(ctx.behind.is_empty());

        // Workspace exists but is not a git repo: no entry either.
        std::fs::create_dir_all(root.join("you/ABC-42-retry-window")).unwrap();
        ctx.recompute_behind();
        assert!(ctx.behind.is_empty());
    }

    /// The table is what the surfaces read, and it is honest about time: a
    /// checkout that appears buys the rungs that were waiting on it
    /// (§AR-005-capabilities.1, §AR-005-capabilities.3).
    #[test]
    fn the_capability_table_is_resolved_per_project_and_again_when_the_world_moves() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("widget");
        let mut ctx = ctx_with_branch(&root, Some("{project_root}/{branch}"));
        ctx.recompute_capabilities();

        // Nothing on disk: placed fails, and what cannot be looked in says so.
        let can = ctx.can("widget");
        assert!(!can.holds(Rung::Placed));
        assert!(!can.holds(Rung::Checkable));
        assert!(!can.holds(Rung::Tasks));
        assert!(can.holds(Rung::BranchAddressable));
        assert!(can
            .refusal(&[Rung::Placed])
            .unwrap()
            .contains("is not on disk"));

        // The project arrives, with a check verb and a task store in it.
        std::fs::create_dir_all(root.join("panta")).unwrap();
        std::fs::write(root.join("check.sh"), "#!/bin/sh\n").unwrap();
        ctx.recompute_capabilities();
        let can = ctx.can("widget");
        assert!(can.holds(Rung::Placed));
        assert!(can.holds(Rung::Checkable));
        assert!(can.holds(Rung::Tasks));

        // A project the registry says nothing about holds nothing, and the
        // table answers rather than being absent.
        assert!(ctx.can("ghost").held().is_empty());
    }

    /// The checkout offered on a branch row can actually run. The entry runs
    /// `ephor checkout`, which needs to be told a branch or a matter it can
    /// read one off (§FS-004-quick-actions.7); a branch row has no matter
    /// (§FS-004-quick-actions.6), so the dossier says the branch and says the
    /// item id empty rather than leaving a stale inherited one to bind the
    /// command to somebody else's change. An offer refused on the keystroke is
    /// worse than no offer (§FS-004-quick-actions.2).
    #[test]
    fn a_branch_rows_checkout_is_told_the_branch_and_no_matter() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
        let branch = ctx.branches("widget")[0].clone();
        let target = tmp.path().join(&branch.branch);
        let entry = crate::api::offers::checkout_action(&target);
        // Both are named, so the one command serves an item row and a branch
        // row alike.
        assert!(entry.command.contains("--item \"$EPHOR_ITEM_ID\""));
        assert!(entry.command.contains("--branch \"$EPHOR_BRANCH\""));

        let menu = ActionMenu::new(
            actions::Subject::Branch {
                project: "widget".to_string(),
                branch: branch.branch.clone(),
            },
            tmp.path().to_path_buf(),
            tmp.path().to_path_buf(),
            Some(branch.clone()),
            WorkspaceState::Missing(target),
            None,
            &ctx.can("widget"),
            Vec::new(),
        );
        let carried = dossier_of(&menu, tmp.path());
        let value = |key: &str| {
            carried
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value("EPHOR_BRANCH"), Some(branch.branch.as_str()));
        assert_eq!(value("EPHOR_TICKET"), Some("ABC-42"));
        assert_eq!(value("EPHOR_PROJECT"), Some("widget"));
        // Said, and said empty: an unset variable is whatever the shell that
        // launched ephor held.
        assert_eq!(value("EPHOR_ITEM_ID"), Some(""));

        // An item row is unchanged: its own id, and its own branch.
        let item_menu = ActionMenu::new(
            actions::Subject::Item(Box::new(ticket_item())),
            tmp.path().to_path_buf(),
            tmp.path().to_path_buf(),
            Some(branch.clone()),
            WorkspaceState::Ready,
            None,
            &ctx.can("widget"),
            Vec::new(),
        );
        let carried = dossier_of(&item_menu, tmp.path());
        let value = |key: &str| {
            carried
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value("EPHOR_ITEM_ID"), Some(ticket_item().id.as_str()));
        assert_eq!(value("EPHOR_BRANCH"), Some(branch.branch.as_str()));
    }

    #[test]
    fn checkout_falls_back_to_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Branch matched but its workspace directory does not exist.
        let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
        let placed = ctx.checkout(&ticket_item()).unwrap();
        assert_eq!(placed.workspace, root);
        assert!(placed.branch.is_some());

        // No branch template at all (plain single-checkout project).
        let ctx = ctx_with_branch(root, None);
        assert_eq!(ctx.checkout(&ticket_item()).unwrap().workspace, root);
    }
}
