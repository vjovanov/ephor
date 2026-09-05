//! The moves: what changes the world, run the same way from either surface
//! (§AR-009-surfaces.1).
//!
//! A move takes a request and returns an outcome. It is total about its
//! refusals — it returns the sentence rather than printing one — because the
//! same refusal has to reach a greyed menu row and a JSON field
//! (§AR-005-capabilities.2, §REQ-002-parity.3).

use std::path::{Path, PathBuf};

use crate::branches::{BranchInfo, WorkspaceState};
use crate::feed::config::{ActionConfig, CheckoutConfig};
use crate::feed::model::Item;
use crate::seams::dossier;
use crate::seams::summons::{self, Outcome as Answered, Place, Site};

use super::offers::{Gate, MenuEntry};
use super::{views, Session};

/// What a move is about: a matter, or a branch row that has none behind it
/// (§FS-004-quick-actions.6).
#[derive(Clone)]
pub enum About {
    Item(Box<Item>),
    Branch { project: String, branch: String },
}

impl About {
    pub fn project(&self) -> &str {
        match self {
            About::Item(item) => &item.project,
            About::Branch { project, .. } => project,
        }
    }

    /// The matter this is about, where there is one. A branch row is not one,
    /// and saying so is what keeps a stand-in item out of the dossier.
    pub fn item(&self) -> Option<&Item> {
        match self {
            About::Item(item) => Some(item),
            About::Branch { .. } => None,
        }
    }
}

/// One entry, ready to run: what it is about, where it runs, and what has to
/// happen first. Assembled by a surface from a reading, so that what runs is
/// the entry the reader saw rather than a second lookup of the same name.
pub struct Run {
    pub about: About,
    pub root: PathBuf,
    pub workspace: PathBuf,
    pub state: WorkspaceState,
    pub checkout: Option<CheckoutConfig>,
    pub branch: Option<BranchInfo>,
    pub entry: MenuEntry,
}

/// One link of the chain an entry runs as: what runs, where it is placed, and
/// — for the checkout — the directory it has to end up creating
/// (§FS-004-quick-actions.7).
///
/// One description of the chain for the three things done with it: run here,
/// started as a job beneath the screen, and reported by `--dry-run`. Built
/// once so those three cannot come to disagree about what would happen — a
/// dry run that under-reported, or a job that ran a step the foreground run
/// skips, is the drift §AR-009-surfaces.1 exists to stop.
#[derive(Debug)]
pub struct Link {
    pub action: ActionConfig,
    /// Where it runs, as a binding spells it (§AR-002-summons.1). `None` is
    /// the workspace, which is what a command that says nothing asks for.
    pub cwd_spec: Option<String>,
    pub place: Place,
    /// The site the place is resolved against: the root for a checkout, which
    /// has no workspace to run in yet, and the workspace the checkout is about
    /// to make for everything after it.
    pub site: Site,
    /// Where the step lands, worked out without asking the disk — the
    /// directory a checkout is about to create is the right answer about where
    /// the step after it runs, and it is not there yet.
    pub cwd: PathBuf,
    /// The workspace the step's dossier names as `EPHOR_WORKSPACE`
    /// (§FS-005-dispatch.8). For the checkout that is the directory it has to
    /// *make*, which is the whole of its contract
    /// (§FS-006-project-interface.8) and is not where it runs; for everything
    /// after it, the directory that checkout made.
    pub workspace: PathBuf,
    /// The directory this step has to create, and which becomes the workspace
    /// every later link runs in.
    pub creates: Option<PathBuf>,
}

impl Run {
    fn needs_checkout(&self) -> bool {
        self.entry.is_checkout || matches!(self.entry.gate, Gate::NeedsCheckout)
    }

    /// The checkout to run before an action that needs the workspace, and the
    /// directory it has to create (§FS-004-quick-actions.7).
    fn checkout_step(&self) -> Option<(ActionConfig, PathBuf)> {
        super::offers::checkout_step(&self.state, &self.checkout)
    }

    /// The whole chain, in order: the checkout where the branch workspace is
    /// missing, then the entry itself — in the workspace the checkout has just
    /// made, never the one the reader is standing in.
    ///
    /// The checkout row is the one entry with nothing after it: its `action`
    /// *is* the command already placed first, so a second link would run the
    /// make-the-workspace command twice, the second time inside the workspace
    /// it had just created.
    pub fn chain(&self) -> Result<Vec<Link>, String> {
        let mut chain = Vec::new();
        let mut workspace = self.workspace.clone();
        if self.needs_checkout() {
            let (checkout, target) = self
                .checkout_step()
                .ok_or("the workspace is missing and nothing checks it out")?;
            let site = Site::root(&self.root);
            chain.push(Link {
                action: checkout,
                // The checkout's job is to make the workspace, so it runs in
                // the root — the workspace is not there to run in yet.
                cwd_spec: Some("root".to_string()),
                cwd: site.place_of(&Place::Root),
                place: Place::Root,
                site,
                // What it is asked to make, and what its environment names —
                // the contract is `$EPHOR_WORKSPACE` existing afterwards.
                workspace: target.clone(),
                creates: Some(target.clone()),
            });
            workspace = target;
        }
        if self.entry.is_checkout {
            return Ok(chain);
        }
        let action = self.entry.action.clone();
        // Where the entry said it runs — a project's offer may name one
        // repository of the forest (§AR-002-summons.1); an action that says
        // nothing runs where it always has. Read here rather than at the
        // spawn, so a `cwd` that is not a place refuses before anything runs,
        // whichever surface asked (§AR-005-capabilities.2).
        let place = match &action.cwd {
            Some(spec) => Place::parse(spec).map_err(|err| err.to_string())?,
            None => Place::Workspace,
        };
        let site = Site::workspace(&self.root, &workspace);
        chain.push(Link {
            cwd_spec: action.cwd.clone(),
            cwd: site.place_of(&place),
            place,
            site,
            workspace,
            creates: None,
            action,
        });
        Ok(chain)
    }
}

/// What a menu entry's summons is told it is about: a matter where there is
/// one, and otherwise the project and the branch, because a branch row has no
/// matter and a stand-in one would put a kind, a source and an id into the
/// contract that nothing filed (§AR-002-summons.1).
///
/// The branch subject says the item id too, and says it empty. A summons
/// inherits the environment it was started in (§AR-002-summons), so a variable
/// left unset is whatever the shell that launched ephor happened to hold — and
/// an entry reading `$EPHOR_ITEM_ID` would bind this branch's rebase to
/// somebody else's matter.
pub fn dossier_of(
    about: &About,
    root: &Path,
    workspace: &Path,
    branch: Option<&BranchInfo>,
    forest: Option<&crate::forest::Forest>,
) -> Vec<(String, String)> {
    match about.item() {
        Some(item) => dossier::of_item(item, root, workspace, branch, forest),
        None => dossier::of_branch(about.project(), root, workspace, branch, forest),
    }
}

/// Whether a reply goes out or is only asked about (§FS-011-command-line.7).
///
/// Not two code paths: [`Session::reply`] resolves the draft, the words and the
/// target the same way for both and stops one statement short of the post. A
/// dry run is a *reading of the move*, so it may not know anything the move
/// does not — one that reported success where the move refuses is worse than
/// no dry run, because a program that checks it before acting learns the
/// opposite of the truth.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sending {
    Now,
    Dry,
}

/// The reader's own editor on one file, as a summons (§AR-002-summons.2).
///
/// One spelling of it, so `e` on the work screen and a command that opens a
/// parked question hand the reader the same program (§AR-009-surfaces.1).
/// `$EDITOR` where they named one, and a pager otherwise: reading the question
/// is worth doing even on a machine where nothing is configured to write it.
pub fn editing(path: &Path) -> summons::Summons {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "less".to_string());
    summons::Summons::new(
        "edit",
        format!(
            "{editor} {}",
            crate::feed::providers::shell_quote(&path.to_string_lossy())
        ),
    )
}

/// How the reader is watching, which is the one thing the two surfaces differ
/// in here (§AR-002-summons.2): the interface hands over its terminal, and a
/// command already owns one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Watching {
    /// The output goes to the terminal as it is produced.
    Terminal,
    /// The terminal, but with the entry's own output beside the reading rather
    /// than in it: a surface whose standard output is already spoken for
    /// (§FS-011-command-line.7). The entry can still be typed into.
    Aside,
    /// A window of the reader's own, where one is bound — ephor stays where it
    /// was and the reader gets a second terminal beside it
    /// (§FS-005-dispatch.22). Where nothing is bound this is
    /// [`Watching::Terminal`] and the outcome line says so: the terminal is the
    /// floor, and the floor is never removed (§AR-002-summons.6).
    Window,
}

/// Where a menu entry runs (§FS-005-dispatch.17, §FS-005-dispatch.22).
///
/// Answered below both surfaces (§AR-009-surfaces.1), so the key and the
/// command cannot come to disagree about which of the three places an entry
/// takes — which is exactly how one surface comes to background what the other
/// hands its terminal to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Runs {
    /// Here, with the terminal handed over — what a menu entry has always been
    /// allowed to be (§FS-006-project-interface.9).
    Here,
    /// Beneath the screen, as a job (§FS-005-dispatch.17).
    Beneath,
    /// In a window of the reader's own, as a job whose supervisor runs inside
    /// that window (§FS-005-dispatch.22).
    InAWindow,
}

impl Session {
    /// Where this entry runs (§FS-005-dispatch.17, §FS-005-dispatch.22). The
    /// entry's own word, answered against what is actually bound: an entry that
    /// asks for a window and finds none takes the terminal as it always did,
    /// and the outcome line says so.
    ///
    /// The checkout row is never one of the other two: it is ephor's own move
    /// that makes the workspace every later step runs in
    /// (§FS-004-quick-actions.7).
    pub fn how(&self, entry: &MenuEntry) -> Runs {
        if entry.is_checkout {
            return Runs::Here;
        }
        if entry.action.background {
            return Runs::Beneath;
        }
        if entry.action.window && self.opener().is_some() {
            return Runs::InAWindow;
        }
        Runs::Here
    }

    /// Run one entry, here, now (§FS-011-command-line.1). The terminal is the
    /// caller's property, so a surface that has to give it up does that around
    /// this call rather than inside it (§AR-002-summons.2).
    ///
    /// A blocked entry is refused with the sentence the row carried, never
    /// discovered on the spawn: the gate was computed when the list was built,
    /// and answering differently now would mean two answers to one question.
    pub fn run_entry(&self, run: &Run, watching: Watching) -> views::Outcome {
        if let Some(reason) = run.entry.gate.refusal() {
            return views::Outcome::refused(reason);
        }
        // Where ephor's own narration goes. Under a reading it is beside the
        // answer, never in it — the same rule the entry's output follows.
        let aside = watching == Watching::Aside;
        let mut steps: Vec<views::Step> = Vec::new();
        let mut workspace = run.workspace.clone();

        // A menu entry is a summons like every other command ephor runs
        // (§AR-002-summons): one spawn path, one environment contract, one
        // reading of the exit code.
        let step = |link: &Link, steps: &mut Vec<views::Step>| -> Result<(), String> {
            let action = &link.action;
            let description = &action.description;
            let place = link
                .site
                .resolve(&link.place)
                .map_err(|err| format!("{description}: {err}"))?;
            let banner = format!(
                "\n▶ {} {description}   ({})\n  $ {}\n",
                action.icon,
                place.display(),
                action.command
            );
            match aside {
                true => eprint!("{banner}"),
                false => print!("{banner}"),
            }
            // The forest of the place it runs in, so a command that folds over
            // repositories folds over the same ones ephor does
            // (§AR-004-forest.1).
            let workspace: &Path = &link.workspace;
            let forest = self
                .placement(run.about.project())
                .map(|placement| placement.forest(workspace));
            let carrying = dossier_of(
                &run.about,
                &run.root,
                workspace,
                run.branch.as_ref(),
                forest.as_ref(),
            );
            // Placed where the link says, never where a default says: a
            // `Summons` built without `.at` runs in the workspace, so an entry
            // written `cwd: repo:app` ran one directory too high while the
            // banner, the step's `cwd` and the supervisor's own replay all said
            // `repo:app` — three answers to where one command runs
            // (§AR-002-summons.1). The place was already resolved when the
            // chain was built, so this is the same one `--dry-run` reports and
            // the same one a job records.
            let summons = summons::Summons::new(description, &action.command)
                .at(link.place.clone())
                .carrying(carrying);
            let mode = match aside {
                true => summons::Mode::Aside,
                false => summons::Mode::Interactive,
            };
            let outcome = summons::run(&summons, &link.site, mode)
                .map_err(|err| err.to_string())
                .and_then(|answer| match answer.outcome {
                    Answered::Done => Ok(()),
                    _ => Err(answer.refusal(description)),
                });
            steps.push(views::Step {
                icon: action.icon.clone(),
                description: description.clone(),
                command: action.command.clone(),
                cwd: place,
                ok: outcome.is_ok(),
                refusal: outcome.as_ref().err().cloned(),
            });
            outcome
        };

        let says = (|| -> Result<String, String> {
            // The one chain, which is also what `--dry-run` reports and what a
            // job is written from (§AR-009-surfaces.1).
            let chain = run.chain()?;
            let mut said = format!("✓ checked out {}", workspace.display());
            for link in &chain {
                step(link, &mut steps)?;
                // A checkout is trusted to have made its target no further
                // than this: ephor verifies the directory rather than the exit
                // code, because a command that exits 0 having made nothing
                // would leave every later link running in the wrong place.
                if let Some(target) = &link.creates {
                    if !target.is_dir() {
                        return Err(format!(
                            "{}: did not create {}",
                            link.action.description,
                            target.display()
                        ));
                    }
                    workspace = target.clone();
                    said = format!("✓ checked out {}", workspace.display());
                } else {
                    said = format!("{} {}: ok", link.action.icon, link.action.description);
                }
            }
            Ok(said)
        })();

        match says {
            Ok(says) => views::Outcome {
                steps,
                // Where no window can be opened, the entry takes the terminal
                // as it always did — and says so rather than leaving the
                // reader to notice (§FS-005-dispatch.22). Asked here rather
                // than asserted: the sentence used to be true only because
                // every caller happened to have checked, and a third one — or
                // a change to [`Session::how`] — would have made it a
                // falsehood printed with confidence.
                ..views::Outcome::ok(match run.entry.action.window && self.opener().is_none() {
                    true => format!("{says} · nothing bound a window, so it took the terminal"),
                    false => says,
                })
            },
            Err(says) => views::Outcome {
                steps,
                ..views::Outcome::refused(says)
            },
        }
    }

    /// Watch a live run by attaching to it (§FS-005-dispatch.20): the binding's
    /// own surface for a run it did not start, opened where the reader can type
    /// into it. Leaving it detaches and never stops the run.
    ///
    /// The board's key and `ephor operations attach` are this one call
    /// (§FS-011-command-line.8, §AR-009-surfaces.1). `root` is only where the
    /// surface is started from — a run is reached by its id.
    pub fn attach_run(&self, root: &Path, id: &str, watching: Watching) -> views::Outcome {
        self.open_running(
            &super::offers::Running::Run {
                root: root.to_path_buf(),
                id: Some(id.to_string()),
                control_url: None,
                attach: None,
                since: None,
                doing: String::new(),
            },
            watching,
        )
    }

    /// Where a command's *open* lands (§FS-011-command-line.8).
    ///
    /// A window of the reader's own where one is bound — **by the same binding
    /// the key uses** (§FS-005-dispatch.22), so that `ephor actions open` and
    /// the key on the row do not come to differ about where an attach goes.
    /// Where nothing is bound the terminal is the floor and is never removed;
    /// and under a reading what the surface writes goes beside the answer
    /// rather than into it (§FS-011-command-line.7).
    pub fn watching(&self, json: bool) -> Watching {
        match (self.opener().is_some(), json) {
            (true, _) => Watching::Window,
            (false, true) => Watching::Aside,
            (false, false) => Watching::Terminal,
        }
    }

    /// The runner's own surface on one run, put where the reader can type into
    /// it: a window of their own where one is bound (§FS-005-dispatch.22), and
    /// the terminal otherwise — the floor, which is never removed
    /// (§AR-002-summons.6). Leaving it detaches and never stops the run.
    ///
    /// One implementation, because the board's `a`, `ephor operations attach`,
    /// the key on a running row and `ephor actions open` are one ability
    /// (§AR-009-surfaces.1). `root` is only where the surface is started from —
    /// a run is reached by its id.
    fn attach_to(&self, root: &Path, id: &str, watching: Watching) -> views::Outcome {
        // A surface on a run is *not* recorded as a job: the run is the
        // operation, and a surface on it is not a second one
        // (§AR-002-summons.6).
        if watching == Watching::Window {
            if let Some(opener) = self.opener() {
                let command = crate::work::runtime::attach_command(&self.work_config, id);
                let at = crate::seams::window::site(root);
                return match opener.open(&format!("ephor · run {id}"), &command, &at) {
                    Ok(Some(handle)) => views::Outcome::ok(format!(
                        "▶ watching run {id} in window {handle} — leaving it detaches"
                    )),
                    // The window is open and the surface is in it; what the
                    // opener did not say is which window (§AR-002-summons.6).
                    // A surface on a run is not recorded anywhere, so there is
                    // nothing else the handle would have been for.
                    Ok(None) => views::Outcome::ok(format!(
                        "▶ watching run {id} in a window the opener did not name — leaving it \
                         detaches"
                    )),
                    Err(err) => views::Outcome::refused(err.to_string()),
                };
            }
        }
        let summons = crate::work::runtime::attach_summons(&self.work_config, id);
        let mode = match watching == Watching::Aside {
            true => summons::Mode::Aside,
            false => summons::Mode::Interactive,
        };
        match summons::run(&summons, &Site::root(root), mode) {
            Ok(answer) if answer.is_done() => {
                views::Outcome::ok(format!("▶ detached from run {id}; it goes on"))
            }
            Ok(answer) => {
                views::Outcome::refused(answer.refusal(&format!("attaching to run {id}")))
            }
            Err(err) => views::Outcome::refused(err.to_string()),
        }
    }

    /// Go to what is already going about an entry, and never start a second
    /// copy of it (§FS-005-dispatch.21, §FS-011-command-line.8): follow the
    /// job's log, attach to the run, or bring the window forward — by the same
    /// binding the key uses.
    ///
    /// The way in was resolved when the menu was assembled, so this acts on
    /// what the reader was shown rather than looking the subject up a second
    /// time (§AR-009-surfaces.1).
    pub fn open_running(
        &self,
        running: &super::offers::Running,
        watching: Watching,
    ) -> views::Outcome {
        use super::offers::Running;
        let aside = watching == Watching::Aside;
        // `Window` falls back to the terminal where nothing is bound, which is
        // the floor and is never removed (§AR-002-summons.6).
        match running {
            // Everything the reader would have watched, kept and followed as
            // it is written (§FS-005-dispatch.17). It ends when the job does.
            Running::Job { id, log, .. } => {
                let Some(job) = crate::seams::jobs::find(id) else {
                    return views::Outcome::refused(format!("No job called '{id}' any more"));
                };
                let mut out = std::io::stdout();
                let mut errors = std::io::stderr();
                crate::seams::jobs::follow(&job, |fresh| {
                    use std::io::Write;
                    // Under a reading the log goes beside the answer, never
                    // into it (§FS-011-command-line.7).
                    let sink: &mut dyn Write = match aside {
                        true => &mut errors,
                        false => &mut out,
                    };
                    let _ = sink.write_all(fresh);
                    let _ = sink.flush();
                });
                views::Outcome::ok(format!("📖 {}", log.display()))
            }
            // The binding's own surface on a run it did not start. Leaving it
            // detaches and never stops the run (§FS-005-dispatch.20). A parked
            // ticket is reached the same way while a run still stands at its
            // gate: §20's own answer to a question a detached run asks is
            // *attach*.
            Running::Run {
                root, id: Some(id), ..
            }
            | Running::Queued {
                root, id: Some(id), ..
            }
            | Running::Waiting {
                root, id: Some(id), ..
            } => self.attach_to(root, id, watching),
            // A question nothing is standing at any more: the answer belongs
            // beside it, in the plan (§FS-005-dispatch.9). The reader's own
            // editor, the terminal theirs while they have it — the same move
            // `e` makes on the work screen (§AR-009-surfaces.1).
            Running::Waiting { plan, ticket, .. } => {
                let mode = match aside {
                    true => summons::Mode::Aside,
                    false => summons::Mode::Interactive,
                };
                let at = plan.parent().unwrap_or(std::path::Path::new("."));
                match summons::run(&editing(plan), &Site::root(at), mode) {
                    Ok(answer) if answer.is_done() => {
                        views::Outcome::ok(format!("📖 {ticket} is in {}", plan.display()))
                    }
                    Ok(answer) => views::Outcome::refused(answer.refusal("opening the plan")),
                    Err(err) => views::Outcome::refused(err.to_string()),
                }
            }
            // A run that named itself nothing has no surface to put on it: the
            // row is live from the lock alone (§AR-007-runtime.3).
            Running::Run { root, .. } | Running::Queued { root, .. } => {
                views::Outcome::refused(format!(
                    "The run on {} left no id, so there is nothing to attach to",
                    root.display()
                ))
            }
            // Opening a windowed program is bringing its window forward, and
            // never a second copy of it (§FS-005-dispatch.22). ephor opens a
            // window and focuses one; it never closes one and never ends what
            // is in it.
            // A window the opener never named is a window all the same: it
            // holds its lock and its row says where the program went, and what
            // is missing is only the handle that would bring it forward
            // (§AR-002-summons.6). Said, not silently done nothing about
            // (§REQ-001-boundary.1).
            Running::Window { handle, .. } if handle.is_empty() => views::Outcome::refused(
                "It is running in a window the opener did not name, so there is nothing to bring \
                 forward"
                    .to_string(),
            ),
            Running::Window { handle, .. } => match self.opener() {
                Some(opener) => {
                    let at = crate::seams::window::site(&crate::paths::state_dir());
                    match opener.focus(handle, &at) {
                        Ok(()) => views::Outcome::ok(format!("▶ window {handle} is forward")),
                        Err(err) => views::Outcome::refused(err.to_string()),
                    }
                }
                None => views::Outcome::refused(format!(
                    "It is running in window {handle}, and nothing here binds a window to bring \
                     forward"
                )),
            },
        }
    }

    /// Start one entry beneath the screen (§FS-005-dispatch.17): write down
    /// what it is, hand it to a supervisor of its own, and return at once.
    /// Nothing is waited on — the row it takes among the operations is how it
    /// is watched, from either surface.
    ///
    /// `place` is which of the two beneath-the-screen places this is, decided
    /// by the caller off [`Session::how`] — or by a reader who asked for the
    /// background outright. Answered once and handed down, so the key and the
    /// command cannot come to disagree about where an entry runs
    /// (§AR-009-surfaces.1).
    ///
    /// The chain travels with it. An entry needing the branch workspace
    /// carries the checkout as the job's first step, with the directory it has
    /// to create named on the step: the supervisor verifies it rather than
    /// trusting it, exactly as a foreground run does
    /// (§FS-006-project-interface.8).
    pub fn start_job(&self, run: &Run, place: Runs) -> views::Outcome {
        use crate::seams::jobs;

        if let Some(reason) = run.entry.gate.refusal() {
            return views::Outcome::refused(reason);
        }
        // The same chain a foreground run walks (§AR-009-surfaces.1), turned
        // into the steps the supervisor replays. Nothing is added here: an
        // entry that *is* the checkout has one step, because its `action` is
        // that checkout and a second step would run the make-the-workspace
        // command again — inside the workspace it had just created.
        let chain = match run.chain() {
            Ok(chain) => chain,
            Err(refusal) => return views::Outcome::refused(refusal),
        };
        let steps: Vec<jobs::Step> = chain
            .iter()
            .map(|link| jobs::Step {
                icon: link.action.icon.clone(),
                description: link.action.description.clone(),
                command: link.action.command.clone(),
                cwd: link.cwd_spec.clone(),
                creates: link.creates.clone(),
                becomes_workspace: link.creates.is_some(),
            })
            .collect();
        // Where the action itself will run, which is the workspace the
        // checkout is about to make when there is one to make.
        // The link's own workspace, never its `cwd`: an entry placed at
        // `repo:<name>` runs one repository deep and the dossier is still
        // about the checkout that holds it (§AR-004-forest.1).
        let workspace = chain
            .last()
            .map(|link| link.workspace.clone())
            .unwrap_or_else(|| run.workspace.clone());
        let action = &run.entry.action;

        let forest = self
            .placement(run.about.project())
            .map(|placement| placement.forest(&workspace));
        let record = jobs::Record {
            version: jobs::VERSION,
            project: run.about.project().to_string(),
            item: run.about.item().map(|item| item.id.clone()),
            // Which entry this came from, and — on a branch row — which branch
            // (§FS-005-dispatch.21): the menu matches a live job back to the
            // row that started it by exactly these, and a job that recorded
            // neither could never be matched at all.
            action: Some(run.entry.key()),
            branch: match &run.about {
                About::Branch { branch, .. } => Some(branch.clone()),
                About::Item(_) => None,
            },
            // Both filled in by [`jobs::start_windowed`] where a window was
            // asked for (§FS-005-dispatch.22); a job that took no window has
            // neither.
            window: None,
            windowed: false,
            icon: action.icon.clone(),
            description: action.description.clone(),
            root: run.root.clone(),
            // None where the checkout is still to make it: a site pointed at a
            // directory that is not there yet would refuse the first step.
            workspace: (!run.needs_checkout()).then(|| workspace.clone()),
            steps,
            dossier: dossier_of(
                &run.about,
                &run.root,
                &workspace,
                run.branch.as_ref(),
                forest.as_ref(),
            ),
            started: chrono::Utc::now().to_rfc3339(),
        };
        // An entry that says `window` is a job whose supervisor runs inside a
        // window of the reader's own (§FS-005-dispatch.22): the record, the
        // lock and the outcome are a job's, and what changes is only who holds
        // the other end of the streams (§AR-002-summons.6).
        //
        // Which place, off [`Session::how`] rather than re-derived from the
        // entry here (§AR-009-surfaces.1). Two derivations disagreed: an entry
        // saying both `background` and `window` was *decided* beneath the screen
        // and then opened a window anyway, and `--background` on a windowed
        // entry did the same.
        let started = match (place, self.opener()) {
            (Runs::InAWindow, Some(opener)) => {
                let title = format!("ephor · {}", action.description);
                let at = crate::seams::window::site(&run.root);
                jobs::start_windowed(record, |command| opener.open(&title, command, &at))
            }
            _ => jobs::start(record),
        };
        match started {
            Ok(job) => views::Outcome {
                job: Some(job.id.clone()),
                ..views::Outcome::ok(match (&job.record.window, job.windowed()) {
                    // The window is its inspection where a log would have been,
                    // so the line that says it started says which window
                    // (§FS-005-dispatch.22) — and says so even where the opener
                    // named none, because the program is in a window either way
                    // and *where it went* is what this line is for.
                    (Some(handle), _) => format!(
                        "{} {}: started in window {handle}",
                        job.record.icon, job.record.description
                    ),
                    (None, true) => format!(
                        "{} {}: started in a window the opener did not name",
                        job.record.icon, job.record.description
                    ),
                    (None, false) => {
                        format!("{} {}: started", job.record.icon, job.record.description)
                    }
                })
            },
            Err(err) => views::Outcome::refused(err.to_string()),
        }
    }
}

impl Session {
    /// The matter's conversation, with what a move needs to act on it
    /// (§FS-011-command-line.4). One walk for both surfaces, so the index a
    /// reading printed is the index a move takes.
    pub fn conversation(&self, item: &Item) -> super::conversation::Conversation {
        let proposal = self
            .dispatcher
            .as_ref()
            .and_then(|dispatcher| dispatcher.proposal(item));
        super::conversation::Conversation::of(item, proposal)
    }

    /// Post a reaction on one message (§FS-004-quick-actions). `content` is a
    /// palette name; a source that reported no way to react is refused by
    /// name rather than by an error from the far side.
    pub fn react(&self, item: &Item, message: usize, content: &str) -> views::Outcome {
        let conversation = self.conversation(item);
        let Some(message) = conversation.messages.get(message) else {
            return views::Outcome::refused(format!(
                "This matter has {} messages",
                conversation.messages.len()
            ));
        };
        let Some(target) = &message.react else {
            return views::Outcome::refused("Posting reactions is not supported for this message");
        };
        let Some((emoji, name)) = crate::feed::react::PALETTE
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(content))
        else {
            return views::Outcome::refused(format!(
                "'{content}' is not one of: {}",
                crate::feed::react::PALETTE
                    .iter()
                    .map(|(_, name)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };
        let blocks = self.blocks_for(&item.project);
        match crate::feed::react::post(
            target,
            name,
            emoji,
            &blocks,
            &item.project,
            &self.config.defaults,
        ) {
            Ok(()) => views::Outcome::ok(format!("{emoji} posted")),
            Err(err) => views::Outcome::refused(err.to_string()),
        }
    }

    /// Tick a task the source reported on a message
    /// (§FS-004-quick-actions.5). A message carrying none, and one already
    /// ticked, are both refused by name: the source that reported the task is
    /// the one that can transition it, and asking it twice is not a second
    /// answer.
    pub fn tick(&self, item: &Item, message: usize) -> views::Outcome {
        let conversation = self.conversation(item);
        let Some(message) = conversation.messages.get(message) else {
            return views::Outcome::refused(format!(
                "This matter has {} messages",
                conversation.messages.len()
            ));
        };
        let Some(task) = &message.task else {
            return views::Outcome::refused("That message is not a task");
        };
        if task.resolved {
            return views::Outcome::refused("That task is already ticked");
        }
        let blocks = self.blocks_for(&item.project);
        match crate::feed::task::resolve(task, &blocks, &item.project, &self.config.defaults) {
            Ok(()) => views::Outcome::ok("☑ ticked"),
            Err(err) => views::Outcome::refused(err.to_string()),
        }
    }

    /// Send a reply (§FS-005-dispatch.13, §FS-007-matters.4). With no words
    /// given it is the reply a run drafted, as it now stands on disk — posting
    /// is also what retires the draft. Where the channel declared no way to
    /// carry a reply, the refusal says where the words are rather than only
    /// that it cannot: a stated degrade, not a failure (§REQ-001-boundary.1).
    ///
    /// [`Sending::Dry`] is the same call with the post left out, and that is
    /// the whole of the difference: every refusal is reached by walking the
    /// move's own path, so what `--dry-run` reports is what would happen. A
    /// second implementation that assembled the words and declared success
    /// answered `ok` on a channel the real move refuses, which is precisely
    /// the reading a program checks before letting the move happen.
    pub fn reply(&self, item: &Item, words: Option<&str>, sending: Sending) -> views::Outcome {
        let conversation = self.conversation(item);
        let (text, draft) = match words {
            Some(words) => (words.to_string(), None),
            None => match &conversation.draft {
                Some(draft) => (draft.text.clone(), Some(draft)),
                None => {
                    return views::Outcome::refused(
                        "No reply has been drafted here — give the words to send some",
                    )
                }
            },
        };
        // The target of the draft's own conversation where there is a draft,
        // and the last conversation that can carry one otherwise.
        let target = match draft {
            Some(draft) => draft.target.clone(),
            None => sendable_target(item),
        };
        let Some(target) = target else {
            // A stated degrade names *these* words, never some others
            // (§REQ-001-boundary.1). The draft's path is where the run's words
            // are — which is the right answer only when the run's words are
            // what was being sent. A reader who typed their own and was told
            // "the words are at <the run's draft>" would go and find a
            // different reply sitting there.
            return views::Outcome::refused(match (&draft, &conversation.draft) {
                (Some(draft), _) => format!(
                    "This channel declared no way to send a reply — the words are at {}",
                    draft.path.display()
                ),
                (None, Some(draft)) => format!(
                    "This channel declared no way to send a reply. The words you gave were not \
                     written anywhere; the reply a run drafted is still at {}",
                    draft.path.display()
                ),
                (None, None) => "This channel declared no way to send a reply".to_string(),
            });
        };
        // Everything above is what decides whether the reply can go out at all;
        // only the post itself is skipped. `says` carries the words, because
        // what a reader checks a dry run for is what would be sent
        // (§REQ-002-parity.3).
        if sending == Sending::Dry {
            return views::Outcome::ok(format!("would send to {}:\n\n{text}", item.title));
        }
        let blocks = self.blocks_for(&item.project);
        match crate::feed::reply::post(
            &target,
            &text,
            &blocks,
            &item.project,
            &self.config.defaults,
        ) {
            Ok(()) => {
                // Retiring the draft is the post's own second half: a reply
                // that went out and is still offered would be sent twice.
                let retired = match (draft, &self.dispatcher) {
                    (Some(_), Some(dispatcher)) => dispatcher.proposal_posted(item),
                    _ => Ok(()),
                };
                match retired {
                    Ok(()) => views::Outcome::ok("↩ reply posted"),
                    Err(err) => views::Outcome::ok(format!("↩ reply posted ({err})")),
                }
            }
            Err(err) => views::Outcome::refused(err.to_string()),
        }
    }
}

/// The last of a matter's conversations that declared it can carry a reply
/// (§FS-007-matters.4) — where words typed with no draft behind them go.
fn sendable_target(item: &Item) -> Option<crate::feed::reply::ReplyTarget> {
    item.raw
        .get("threads")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .filter_map(|thread| crate::feed::reply::parse_target(thread, &item.source))
        .next_back()
}

impl Session {
    /// Hand one entry's work over about one matter (§FS-005-dispatch.4): a
    /// recipe entry opens a ticket, a workflow entry lays a plan down beside
    /// the matter's (§FS-005-dispatch.19). One path for both surfaces, so
    /// nothing the entry carries — its opening move, its own hand — is lost on
    /// the way.
    ///
    /// `answers` are `input=value` pairs answering the workflow's inputs for
    /// this instantiation alone. An input nobody answered is refused by name:
    /// where a screen can open a prompt or an editor, a command says what is
    /// missing and lets the reader pass it (§REQ-002-parity.2).
    pub fn hand_over(
        &mut self,
        item: &Item,
        entry: &MenuEntry,
        answers: &[String],
        dry_run: bool,
    ) -> views::Outcome {
        if let Some(reason) = entry.gate.refusal() {
            return views::Outcome::refused(reason);
        }
        let picked = entry.picked.clone();
        if let Some(recipe) = &entry.action.agent {
            let Some(dispatcher) = &mut self.dispatcher else {
                return views::Outcome::refused("Work needs the registry, which could not be read");
            };
            return match dispatcher.dispatch(item, recipe, picked.as_ref(), dry_run) {
                Ok(outcome @ crate::work::Outcome::Opened { .. })
                | Ok(outcome @ crate::work::Outcome::Reopened { .. }) => {
                    let ticket = match &outcome {
                        crate::work::Outcome::Opened { ticket, .. }
                        | crate::work::Outcome::Reopened { ticket, .. } => ticket.clone(),
                        _ => String::new(),
                    };
                    if dry_run {
                        return views::Outcome {
                            tickets: vec![ticket],
                            ..views::Outcome::ok(outcome.describe())
                        };
                    }
                    match dispatcher.save() {
                        Ok(()) => {
                            let mut says =
                                format!("{} {} — {ticket}", recipe.icon, recipe.description);
                            // Work that needs nobody to start it gets its run
                            // in the same breath as the ticket
                            // (§FS-005-dispatch.24). The sweep is what starts
                            // it, so this is the same act the timer makes and
                            // is safe on a root that already has a run: it
                            // starts nothing there.
                            if recipe.autorun {
                                let project = item.project.clone();
                                match dispatcher.start_due(
                                    chrono::Utc::now(),
                                    std::slice::from_ref(&project),
                                    &[],
                                    None,
                                    // A person pressed a key, so a full budget
                                    // is said and the run starts
                                    // (§FS-015-spend-ceiling.6). There is no
                                    // error stream behind a screen, so it is
                                    // said where everything else about this
                                    // entry is said.
                                    crate::work::spend::Budget::Warns,
                                ) {
                                    Ok(swept) => {
                                        for full in swept.warned {
                                            says = format!("{says} · {full}");
                                        }
                                        for run in swept.runs {
                                            says = format!("{says} · {}", run.says());
                                        }
                                    }
                                    Err(err) => says = format!("{says} · {err}"),
                                }
                            }
                            self.reload_work();
                            views::Outcome {
                                tickets: vec![ticket],
                                ..views::Outcome::ok(says)
                            }
                        }
                        Err(err) => views::Outcome::refused(err.to_string()),
                    }
                }
                Ok(outcome) => views::Outcome::ok(outcome.describe()),
                Err(err) => views::Outcome::refused(err.to_string()),
            };
        }
        if entry.action.workflow.is_none() {
            return views::Outcome::refused("That entry hands no work over");
        }
        let typed = match parse_answers(answers) {
            Ok(typed) => typed,
            Err(refusal) => return views::Outcome::refused(refusal),
        };
        let Some(dispatcher) = &mut self.dispatcher else {
            return views::Outcome::refused("Work needs the registry, which could not be read");
        };
        let laying = match dispatcher.laying(item, &entry.action, &typed, picked.as_ref()) {
            Ok(laying) => laying,
            Err(err) => return views::Outcome::refused(err.to_string()),
        };
        if !laying.answered.refusals.is_empty() {
            return views::Outcome::refused(laying.answered.refusals.join("; "));
        }
        // What a screen would ask for, a command names (§REQ-002-parity.2):
        // each unanswered input with what it wants, so one more invocation
        // finishes the job.
        if !laying.answered.missing.is_empty() {
            let wants: Vec<String> = laying
                .answered
                .missing
                .iter()
                .map(|name| match laying.workflow.input(name) {
                    Some(input) if !input.description.is_empty() => {
                        format!(
                            "--set {name}=<{}>  ({})",
                            input.kind.label(),
                            input.description
                        )
                    }
                    Some(input) => format!("--set {name}=<{}>", input.kind.label()),
                    None => format!("--set {name}=<value>"),
                })
                .collect();
            return views::Outcome::refused(format!(
                "{} still wants:\n  {}",
                laying.workflow.id,
                wants.join("\n  ")
            ));
        }
        let plan = laying.root().join(&laying.plan_id);
        match dispatcher.lay(item, &laying, dry_run) {
            Ok(laid) => {
                let says = format!("{} {}", entry.action.icon, laid.outcome.describe());
                if dry_run {
                    return views::Outcome {
                        plan: Some(plan),
                        ..views::Outcome::ok(says)
                    };
                }
                match dispatcher.save() {
                    Ok(()) => {
                        self.reload_work();
                        views::Outcome {
                            plan: Some(plan),
                            ..views::Outcome::ok(says)
                        }
                    }
                    Err(err) => views::Outcome::refused(err.to_string()),
                }
            }
            Err(err) => views::Outcome::refused(err.to_string()),
        }
    }
}

/// `input=value` pairs, refused by name where one is not a pair: a mistyped
/// answer silently dropped would lay a workflow down with the wrong input.
fn parse_answers(answers: &[String]) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut typed = std::collections::BTreeMap::new();
    for answer in answers {
        let Some((name, value)) = answer.split_once('=') else {
            return Err(format!("--set wants <input>=<value>, and got '{answer}'"));
        };
        typed.insert(name.trim().to_string(), value.to_string());
    }
    Ok(typed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(action: ActionConfig, is_checkout: bool, gate: Gate) -> MenuEntry {
        MenuEntry {
            action,
            is_checkout,
            is_freehand: false,
            is_workflows: false,
            picked: None,
            gate,
            running: None,
        }
    }

    /// A `Run` about a branch row, which needs no feed behind it: the chain is
    /// a fact about the entry and where it is placed, not about the matter.
    fn run_of(entry: MenuEntry, state: WorkspaceState) -> Run {
        Run {
            about: About::Branch {
                project: "demo".to_string(),
                branch: "you/retry".to_string(),
            },
            root: PathBuf::from("/w/demo"),
            workspace: PathBuf::from("/w/demo"),
            state,
            checkout: Some(CheckoutConfig {
                icon: "⇣".to_string(),
                description: "check out branch workspace".to_string(),
                command: "git worktree add \"$EPHOR_WORKSPACE\"".to_string(),
            }),
            branch: None,
            entry,
        }
    }

    /// The checkout row is the checkout, and it runs once. Its `action` is the
    /// same command the chain already places first, so a second link would run
    /// `git worktree add` twice — the second time inside the working tree it
    /// had just created (§FS-004-quick-actions.7).
    #[test]
    fn a_checkout_entry_runs_its_command_once() {
        let target = PathBuf::from("/w/demo-retry");
        let checkout = crate::api::offers::checkout_step(
            &WorkspaceState::Missing(target.clone()),
            &Some(CheckoutConfig {
                icon: "⇣".to_string(),
                description: "check out branch workspace".to_string(),
                command: "git worktree add \"$EPHOR_WORKSPACE\"".to_string(),
            }),
        )
        .expect("a missing workspace has a checkout")
        .0;
        let run = run_of(
            entry(checkout, true, Gate::NeedsCheckout),
            WorkspaceState::Missing(target.clone()),
        );
        let chain = run.chain().expect("the chain");
        assert_eq!(chain.len(), 1, "one step, not the same command twice");
        assert_eq!(chain[0].creates.as_ref(), Some(&target));
        assert_eq!(
            chain[0].cwd,
            PathBuf::from("/w/demo"),
            "it runs in the root"
        );
        assert_eq!(
            chain[0].workspace, target,
            "and its `$EPHOR_WORKSPACE` is what it has to make"
        );
    }

    /// An entry that needs the workspace runs after the checkout, and *in the
    /// workspace the checkout is about to make* — not in the one the reader is
    /// standing in. This is the fact `--dry-run` reports and the one a job is
    /// written from, off this single derivation (§AR-009-surfaces.1).
    #[test]
    fn an_entry_needing_the_workspace_runs_after_the_checkout_and_inside_it() {
        let target = PathBuf::from("/w/demo-retry");
        let run = run_of(
            entry(
                ActionConfig {
                    id: "build".to_string(),
                    icon: "🔨".to_string(),
                    description: "build it".to_string(),
                    command: "make".to_string(),
                    requires_checkout: true,
                    ..ActionConfig::default()
                },
                false,
                Gate::NeedsCheckout,
            ),
            WorkspaceState::Missing(target.clone()),
        );
        let chain = run.chain().expect("the chain");
        assert_eq!(chain.len(), 2, "the checkout, then the entry");
        assert_eq!(chain[0].creates.as_ref(), Some(&target));
        assert_eq!(chain[1].action.command, "make");
        assert_eq!(chain[1].cwd, target, "the entry runs in the new workspace");
        assert_eq!(chain[1].workspace, target);
        assert_eq!(chain[0].workspace, target, "the checkout is asked for it");
    }

    /// An entry whose workspace is already there is one step in it, and a
    /// `cwd` naming a repository of the forest lands one level deeper
    /// (§AR-002-summons.1).
    ///
    /// The `place` is asserted beside the `cwd` because they are two halves of
    /// one fact and only one of them used to be honoured: the `cwd` is what a
    /// reading publishes and the `place` is what the summons is actually given,
    /// and a run that took the second from a default published a directory it
    /// never ran in.
    #[test]
    fn a_ready_workspace_is_one_step_placed_where_the_entry_said() {
        let run = run_of(
            entry(
                ActionConfig {
                    icon: "▸".to_string(),
                    description: "test app".to_string(),
                    command: "cargo test".to_string(),
                    cwd: Some("repo:app".to_string()),
                    ..ActionConfig::default()
                },
                false,
                Gate::Ready,
            ),
            WorkspaceState::Ready,
        );
        let chain = run.chain().expect("the chain");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].cwd, PathBuf::from("/w/demo/app"));
        assert_eq!(chain[0].cwd_spec.as_deref(), Some("repo:app"));
        assert_eq!(
            chain[0].place,
            Place::Repo("app".to_string()),
            "and the summons is placed there rather than at the default"
        );
        assert_eq!(
            chain[0].workspace,
            PathBuf::from("/w/demo"),
            "one repository deep, and the dossier is still about the checkout"
        );
    }

    /// A `cwd` that is not a place is refused before anything runs, on either
    /// surface — the same refusal a foreground run gives, rather than one
    /// discovered by the supervisor after the job was written
    /// (§AR-005-capabilities.2).
    #[test]
    fn a_cwd_that_is_not_a_place_refuses_the_whole_chain() {
        let run = run_of(
            entry(
                ActionConfig {
                    icon: "▸".to_string(),
                    description: "somewhere".to_string(),
                    command: "true".to_string(),
                    cwd: Some("elsewhere".to_string()),
                    ..ActionConfig::default()
                },
                false,
                Gate::Ready,
            ),
            WorkspaceState::Ready,
        );
        let refusal = run.chain().expect_err("an unknown place is refused");
        assert!(refusal.contains("elsewhere"), "{refusal}");
    }
}
