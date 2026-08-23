//! The runtime adapter (§AR-007-runtime).
//!
//! Everything runtime-specific lives here: the plan language ephor writes, the
//! runner command that executes it, and what is read back from its results —
//! the verdict a ticket reached, and the reply a ticket about a conversation
//! drafted (§FS-005-dispatch.13). That is the whole coupling — a contract in
//! files, never a linked process — and no product literal for the shipped
//! runtime exists outside this module, shipped assets, examples, and
//! documentation (§REQ-001-boundary.5, §DA-001-runtime-bound-default).
//!
//! Running a plan is a summons like every other command ephor asks of the
//! world — the same place resolution, the same exit semantics, the same
//! terminal handover — so the key in the inbox and `ephor work run` cannot
//! drift into two different invocations (§FS-005-dispatch.12).

pub mod events;
pub mod plan;
pub mod results;
pub mod roster;
pub mod watch;
pub mod workflow;

use std::path::Path;

use crate::capabilities;
use crate::error::Result;
use crate::seams::summons::{self, quote, Answer, Mode, Site, Summons};

/// The shipped default runner (§REQ-001-boundary.1,
/// §DA-001-runtime-bound-default). Bound, not fused: `work.runner` in site
/// configuration replaces it, and everything above this module names the
/// binding rather than this word.
pub const RUNNER: &str = "rhei";

/// What runs plans here: the person's binding where they set one, the shipped
/// default otherwise. Choosing a runtime is a property of how a person works,
/// which is why one ships wired and ready (§FS-005-dispatch lead).
pub fn runner(config: &crate::work::recipe::WorkConfig) -> &str {
    config.runner.as_deref().unwrap_or(RUNNER)
}

/// How a surface names the runtime in a message — the bound command, never a
/// word compiled in above this module.
pub fn label(config: &crate::work::recipe::WorkConfig) -> String {
    format!("{} run", runner(config))
}

/// How a person moves a ticket the runtime parked on by hand, in the bound
/// runner's own words (§FS-005-dispatch.10). Part of the coupling, and so part
/// of this module — a surface shows the line, it does not compose it.
pub fn advance_command(
    config: &crate::work::recipe::WorkConfig,
    ticket: &str,
    state: &str,
) -> String {
    format!(
        "{} transition {ticket} --from {state} --to <state>",
        runner(config)
    )
}

/// The verb this seam fills, for messages.
pub const VERB: &str = "work.run";

/// The verb a cancel fills, for messages (§FS-005-dispatch.16).
pub const CANCEL_VERB: &str = "work.cancel";

/// The verb making the runtime's own project fills, for messages
/// (§FS-006-project-interface.7).
pub const INIT_VERB: &str = "work.init";

/// How long the runner gets to make a project before ephor stops waiting.
/// It writes a handful of small files in a directory ephor already made;
/// seconds are generous, and a checkout is waiting on it.
const INIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// How the runner is told to make the directory it was given the project,
/// rather than a subdirectory of that directory under a name of its own. Ephor
/// resolves the work root itself and a project anywhere else is a project
/// nothing here would read again (§FS-006-project-interface.7). Part of the
/// same coupling as the plan flag (§AR-007-runtime.1).
const HERE_FLAG: &str = "--here";

/// How the runner is told not to leave a discovery note beside the project it
/// makes. Its note goes in the *host* directory's `AGENTS.md`, which for a
/// work root is the checkout — a file the project tracks, or one that was not
/// there at all, either way a change to the branch
/// (§FS-006-project-interface.7). Ephor promised the checkout would be
/// byte-for-byte what it was (§REQ-001-boundary.3) and the runner promised
/// nothing, so the note is declined here. Part of the same coupling as the
/// plan flag (§AR-007-runtime.1).
const NO_NOTE_FLAG: &str = "--no-agents";

/// How the runner is told what to call the project it makes. Left to itself it
/// names it after the directory, which under [`HERE_FLAG`] is the work root's
/// own name and the same word for every project on the machine.
const TITLE_FLAG: &str = "--title";

/// How long the runner gets to move one ticket before ephor stops waiting.
/// A transition is a file rewrite and, at most, a callback the machine
/// hangs on it — seconds are generous, and a reader is holding the screen.
const CANCEL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// `<runner> transition <plan> --task <ticket> --from <state> --to cancelled
/// --result <why>`, quoted for `sh`, with the runner's own standard error
/// folded into what is captured — its refusal is written there, and it is the
/// one thing worth reading back when the move does not happen
/// (§FS-005-dispatch.16, §DA-005-cancel-is-the-runtimes-move). The
/// abandonment state is the plan language's word (§AR-007-runtime.1), and the
/// result carries the reader's reason: the runtime records why a ticket ended
/// where it did, and refuses a terminal move that says nothing.
pub fn cancel_command(
    config: &crate::work::recipe::WorkConfig,
    plan: &Path,
    ticket: &str,
    from: &str,
    why: &str,
) -> String {
    format!(
        "{} transition {} --task {} --from {} --to {} --result {} 2>&1",
        runner(config),
        quote(&plan.to_string_lossy()),
        quote(ticket),
        quote(from),
        quote(plan::CANCELLED),
        quote(why),
    )
}

/// Ask the runner to move one ticket into the abandonment state, from the
/// work root, captured (§FS-005-dispatch.16). `Ok` is the ticket cancelled;
/// `Err` is the runner's own refusal in its own first words — what it printed
/// is what the reader is told, since ephor neither works around it nor knows
/// better (§DA-005-cancel-is-the-runtimes-move).
pub fn cancel(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    plan: &Path,
    ticket: &str,
    from: &str,
    why: &str,
) -> Result<()> {
    let answer = summons::run(
        &Summons::new(CANCEL_VERB, cancel_command(config, plan, ticket, from, why)),
        &Site::root(root),
        Mode::Captured(CANCEL_TIMEOUT),
    )?;
    if answer.is_done() {
        return Ok(());
    }
    let said = said(answer.output.as_deref().unwrap_or(""));
    Err(crate::error::EphorError::Command(match said.is_empty() {
        true => format!(
            "{} refused: {}",
            label_of(config, "transition"),
            answer.refusal(CANCEL_VERB)
        ),
        false => format!("{} refused: {said}", label_of(config, "transition")),
    }))
}

/// `<runner> init --here --no-agents --title <name> <dir>`, quoted for `sh`,
/// with the runner's own standard error folded into what is captured — its
/// refusal is written there, and it is the one thing worth reading back when
/// the project does not appear. The directory is ephor's own work root, named
/// outright, because ephor resolves where work lives and a project anywhere
/// else is one nothing here reads again (§FS-006-project-interface.7).
pub fn init_command(config: &crate::work::recipe::WorkConfig, dir: &Path) -> String {
    format!(
        "{} init {HERE_FLAG} {NO_NOTE_FLAG} {TITLE_FLAG} {} {} 2>&1",
        runner(config),
        quote(&project_title(dir)),
        quote(&dir.to_string_lossy()),
    )
}

/// What to call the project in the work root at `dir`: the workspace it stands
/// in, which is the branch's own directory where a project keeps one checkout
/// per branch. The same name [`plan::WorkRoot::ensure`] writes into a manifest
/// it makes itself, so the two do not disagree about one project.
fn project_title(dir: &Path) -> String {
    dir.parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "work".to_string())
}

/// What asking the runner for its own project came to
/// (§FS-006-project-interface.7).
pub enum Initialized {
    /// The runner made one, or there already was one — either way the
    /// directory is a runtime project now.
    Project,
    /// It could not be asked, or it refused. Carries the sentence to show;
    /// never fatal, because the checkout around this succeeded
    /// (§FS-004-quick-actions.7).
    Refused(String),
}

/// Ask the runner to make a project of its own in `dir`
/// (§FS-006-project-interface.7).
///
/// A directory that already holds one is left alone rather than offered to the
/// runner for it to refuse: whether there is a project here is a question about
/// a file, and asking a process it in order to read its prose back would be
/// ephor parsing a refusal for a fact it can see.
pub fn init(config: &crate::work::recipe::WorkConfig, dir: &Path) -> Initialized {
    if plan::is_project(dir) {
        return Initialized::Project;
    }
    if let Some(refusal) = refusal(config) {
        return Initialized::Refused(refusal);
    }
    // From the directory being made, which the caller has already created: the
    // runner is told the place twice over, and a summons resolved against a
    // path that is not there fails as a missing place rather than as the
    // runner's own refusal (§AR-002-summons.2).
    let answer = summons::run(
        &Summons::new(INIT_VERB, init_command(config, dir)),
        &Site::root(dir),
        Mode::Captured(INIT_TIMEOUT),
    );
    match answer {
        Ok(answer) if answer.is_done() => Initialized::Project,
        Ok(answer) => {
            let said = said(answer.output.as_deref().unwrap_or(""));
            Initialized::Refused(match said.is_empty() {
                true => format!(
                    "{} refused: {}",
                    label_of(config, "init"),
                    answer.refusal(INIT_VERB)
                ),
                false => format!("{} refused: {said}", label_of(config, "init")),
            })
        }
        Err(err) => Initialized::Refused(err.to_string()),
    }
}

/// The runner's own words out of what it printed: the first two lines that
/// say anything, with the box-drawing a pretty error report wraps them in
/// stripped, joined into one sentence a message line can carry.
fn said(output: &str) -> String {
    output
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['×', '│', '╰', '╭', '─', '┬', '├', '┤', '▶', '·', ' '])
                .trim()
        })
        .filter(|line| !line.is_empty() && !line.starts_with("help:"))
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

/// How a surface names one of the runtime's verbs in a message — the bound
/// command and the verb, never a word compiled in above this module.
fn label_of(config: &crate::work::recipe::WorkConfig, verb: &str) -> String {
    format!("{} {verb}", runner(config))
}

/// `<runner> run <root> [--rhei <plan>]… [--agent <agent> [--agent-mode
/// <effort>]] [extra…]`, quoted for `sh`. The hand flags carry a chosen hand
/// the plan language cannot spell — one naming an agent and no model
/// (§FS-005-dispatch.14); a hand carrying a model is pinned on its ticket
/// instead and never travels here: one choice binds in one spelling, and the
/// ticket's full line is the stronger one — the runner resolves such a ticket
/// from the line alone, with these flags invisible to it, while a bare model
/// line would take its carrier from them. `extra` stays last, so what the
/// reader passed through can still have the final word.
pub fn invocation_with(
    runner: &str,
    root: &Path,
    plans: &[String],
    hand: Option<&roster::HandFlags>,
    extra: &[String],
) -> String {
    let mut words = vec![
        runner.to_string(),
        "run".to_string(),
        quote(&root.to_string_lossy()),
    ];
    for plan in plans {
        words.push(PLAN_FLAG.to_string());
        words.push(quote(plan));
    }
    if let Some(hand) = hand {
        words.push(AGENT_FLAG.to_string());
        words.push(quote(&hand.agent));
        if let Some(effort) = &hand.effort {
            words.push(AGENT_MODE_FLAG.to_string());
            words.push(quote(effort));
        }
    }
    words.extend(extra.iter().map(|arg| quote(arg)));
    words.join(" ")
}

/// How the runner is told which plan to run. Part of the coupling, and so
/// part of this module (§AR-007-runtime).
const PLAN_FLAG: &str = "--rhei";

/// How the runner is told to detach: the run put in a session of its own,
/// outliving the screen that started it, with the launcher printing what it
/// started (§FS-005-dispatch.20). Part of the same coupling as the plan flag
/// (§AR-007-runtime.1).
const HEADLESS_FLAG: &str = "--headless";

/// How the launcher is asked for the run's descriptor instead of its human
/// block. It describes the *launcher's* output, not the run's
/// (§AR-007-runtime.1).
const LAUNCHER_JSON_FLAG: &str = "--json";

/// The runner's own verb for putting a surface on a run it did not start, and
/// its own verb for stopping one. Composed here and only ever shown, in the
/// stop's case (§FS-005-dispatch.20).
const ATTACH_VERB: &str = "attach";
const STOP_VERB: &str = "stop";

/// How the runner is told which agent a run's tickets go to, and at which of
/// its modes — the spelling for the hand the plan language has no line for
/// (§FS-005-dispatch.14). Part of the same coupling as the plan flag
/// (§AR-007-runtime.1).
const AGENT_FLAG: &str = "--agent";
const AGENT_MODE_FLAG: &str = "--agent-mode";

/// A work ledger written before the plan's field was named for the plan spells
/// it for the runtime instead. The word is this module's, so the migration
/// that still reads it is this module's too (§REQ-001-boundary.5): the ledger
/// hands its parsed document over on the way in and gets back one the current
/// field names read.
pub fn migrate_ledger(document: &mut serde_json::Value) {
    const WAS: &str = "rhei";
    const NOW: &str = "plan_id";
    let Some(entries) = document
        .get_mut("entries")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    for entry in entries.values_mut() {
        let Some(fields) = entry.as_object_mut() else {
            continue;
        };
        if fields.contains_key(NOW) {
            continue;
        }
        if let Some(value) = fields.remove(WAS) {
            fields.insert(NOW.to_string(), value);
        }
    }
}

/// The invocation with the shipped default runner and no hand riding it.
pub fn invocation(root: &Path, plans: &[String], extra: &[String]) -> String {
    invocation_with(RUNNER, root, plans, None, extra)
}

/// `<runner> run --headless --json <root> [--rhei <plan>]… [agent flags]`,
/// quoted for `sh` — the same grammar [`invocation_with`] builds, with the
/// runner's own detach flag beside the plan flag (§AR-007-runtime.1).
///
/// The detached form is one edit of the attached one and not a second
/// invocation: a run started beneath the screen has to be the run the key
/// would have started (§FS-005-dispatch.12). The launcher's standard error is
/// folded into what is captured, because the launcher's own diagnostic — the
/// tail of what the child said before it died — is the whole of what a refusal
/// has to carry (§FS-005-dispatch.20).
pub fn detached_invocation_with(
    runner: &str,
    root: &Path,
    plans: &[String],
    hand: Option<&roster::HandFlags>,
    extra: &[String],
) -> String {
    let attached = invocation_with(runner, root, plans, hand, extra);
    let (head, rest) = attached
        .split_once(' ')
        .map(|(head, rest)| (head.to_string(), rest.to_string()))
        .unwrap_or((attached.clone(), String::new()));
    // `run` is the first word after the runner, and the flags go straight
    // after it: the launcher reads them, the child never sees them.
    let rest = rest.strip_prefix("run").unwrap_or(&rest).trim_start();
    format!("{head} run {HEADLESS_FLAG} {LAUNCHER_JSON_FLAG} {rest} 2>&1")
}

/// The verb a detached start fills, for messages.
pub const DETACH_VERB: &str = "work.run.headless";

/// How long the launcher gets to hand back a run id. It waits for the child to
/// publish its descriptor and then returns, so this is the child's startup and
/// not the run: generous, and spent once, beneath a screen that stays where it
/// was (§FS-005-dispatch.20).
const DETACH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

/// What the launcher handed back about the run it started
/// (§FS-005-dispatch.20, §AR-007-runtime.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Started {
    /// What the run calls itself, where the launcher named it. None is a run
    /// that started and named itself nothing ephor could read — the row is
    /// still live from the lock, with nothing to attach to
    /// (§AR-007-runtime.3).
    pub id: Option<String>,
    /// The launcher's own descriptor already said the run was over: it had
    /// nothing to do and exited inside the handshake. Read from the same
    /// document the id is, because announcing such a run as *started* sends a
    /// reader who presses the board key straight to an empty board.
    pub finished: bool,
}

/// Start a run beneath the screen and come back with what it is called
/// (§FS-005-dispatch.20).
///
/// Captured rather than handed the terminal: the launcher is not the run — it
/// waits for the child to publish its descriptor, prints it, and exits, and
/// what ephor wants from it is the id. `Err` is the launcher's own diagnostic,
/// which is the child's: an invalid plan, a held lock and an unresolvable hand
/// all fail here, as loudly as they do in the foreground.
pub fn start_detached(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    checkout: &Path,
    plans: &[String],
    hand: Option<&roster::HandFlags>,
    extra: &[String],
) -> Result<Started> {
    let command = detached_invocation_with(runner(config), root, plans, hand, extra);
    let answer = summons::run(
        &Summons::new(DETACH_VERB, command),
        &Site::root(checkout),
        Mode::Captured(DETACH_TIMEOUT),
    )?;
    let printed = answer.output.as_deref().unwrap_or("");
    if answer.is_done() {
        return Ok(started(printed));
    }
    // The launcher's own diagnostic, which is the child's: an invalid plan, a
    // held lock and an unresolvable hand all fail here (§FS-005-dispatch.20).
    // Its error envelope first, since under `--json` that is where it puts the
    // sentence; the printed lines otherwise.
    let said = launcher_refusal(printed).unwrap_or_else(|| said(printed));
    Err(crate::error::EphorError::Command(match said.is_empty() {
        true => format!("{} refused: {}", label(config), answer.refusal(DETACH_VERB)),
        false => format!("{} refused: {said}", label(config)),
    }))
}

/// The sentence out of the launcher's own error envelope, where it printed one.
/// The binding's own document, read by this one binding the way its listing's
/// stdout is (§AR-002-summons.3) — and cut to what a message line can carry.
fn launcher_refusal(output: &str) -> Option<String> {
    for (at, _) in output.match_indices('{') {
        let mut values =
            serde_json::Deserializer::from_str(&output[at..]).into_iter::<serde_json::Value>();
        if let Some(Ok(value)) = values.next() {
            if let Some(message) = value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(serde_json::Value::as_str)
            {
                return Some(said(message)).filter(|said| !said.is_empty());
            }
        }
    }
    None
}

/// What the launcher said about the run, out of the descriptor it printed —
/// wherever in the captured text that sits. Read leniently on purpose: the
/// launcher's own warnings share the stream with it, and a reader that insisted
/// the object began at byte zero would lose the id to a note about a registry
/// file.
///
/// Both facts come out of the *same* document. A launcher that returns
/// `{"id": …, "status": "finished"}` started a run that had nothing to do and
/// was over before the handshake returned; reading only the id announced it as
/// started and sent the reader to a board with nothing on it.
fn started(output: &str) -> Started {
    for (at, _) in output.match_indices('{') {
        let mut values =
            serde_json::Deserializer::from_str(&output[at..]).into_iter::<serde_json::Value>();
        if let Some(Ok(value)) = values.next() {
            let id = value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty());
            if let Some(id) = id {
                let over = value
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|status| FINISHED.contains(&status))
                    || value.get("exit_code").is_some_and(|code| !code.is_null());
                return Started {
                    id: Some(id.to_string()),
                    finished: over,
                };
            }
        }
    }
    Started::default()
}

/// The launcher's own words for a run that is over. Part of the coupling, and
/// so part of this module (§AR-007-runtime.1).
const FINISHED: &[&str] = &["finished", "done", "complete", "completed", "failed"];

/// How a reader watches a run they did not start: the runner's own surface on
/// it, in the runner's own words (§FS-005-dispatch.20, §AR-007-runtime.1).
/// Composed here; where it runs — the terminal, or a window of the reader's
/// own — is the executor's (§AR-002-summons.6).
pub fn attach_command(config: &crate::work::recipe::WorkConfig, id: &str) -> String {
    format!("{} {ATTACH_VERB} {}", runner(config), quote(id))
}

/// The verb an attach fills, for messages.
pub const ATTACH_VERB_NAME: &str = "work.attach";

/// The attach as a summons: something the reader types into, so it takes a
/// terminal — theirs, or a window of their own where one is bound
/// (§AR-002-summons.6). Leaving it detaches and never stops the run, which is
/// the runner's own contract and not something ephor adds (§FS-005-dispatch.20).
pub fn attach_summons(config: &crate::work::recipe::WorkConfig, id: &str) -> Summons {
    Summons::new(ATTACH_VERB_NAME, attach_command(config, id))
}

/// How a person stops a run, in the bound runner's own words
/// (§FS-005-dispatch.20). Composed here and **only ever shown**: the board
/// starts nothing and stops nothing, and a key that stopped a run would be a
/// channel to the run ephor promised never to hold. The same standing the
/// release command has (§FS-005-dispatch.10).
///
/// The id is quoted exactly as the attach's is. It comes out of a file the
/// *run* wrote, and this line exists to be copied into a shell: an id with a
/// space in it would produce a command that does not work, and one carrying
/// `;` or `$( )` a line that runs something else when pasted.
pub fn stop_command(config: &crate::work::recipe::WorkConfig, id: &str) -> String {
    format!("{} {STOP_VERB} {}", runner(config), quote(id))
}

/// The verb the detach probe fills, for messages.
const DETACHES_VERB: &str = "work.run.detaches";

/// How long the probe gets. It is a help text, printed by a binary already on
/// PATH — seconds are generous.
const DETACHES_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// What each bound runner answered about detaching, once per process. The
/// capability table's own rule: a capability is resolved when it is first
/// needed and not re-established per keystroke (§AR-005-capabilities.1), and a
/// probe that forked the runner on every menu build would be discovery by
/// spawning on the draw path.
static DETACHES: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, bool>>> =
    std::sync::OnceLock::new();

/// Whether the bound runner has a detached shape at all (§AR-007-runtime.3).
///
/// Asked of the runner's own help rather than of a version number: what the
/// surfaces need to know is whether the flag exists, and the binary is the only
/// thing that can say. False with nothing bound, and false for a runner whose
/// help does not name the flag — an older one, or a platform with no detached
/// shape — and the run is then the attached invocation of before, with the
/// outcome line saying so.
pub fn can_detach(config: &crate::work::recipe::WorkConfig) -> bool {
    if refusal(config).is_some() {
        return false;
    }
    let runner = runner(config).to_string();
    let seen = DETACHES.get_or_init(Default::default);
    if let Ok(known) = seen.lock() {
        if let Some(answer) = known.get(&runner) {
            return *answer;
        }
    }
    let answer = probe_detaches(&runner);
    if let Ok(mut known) = seen.lock() {
        known.insert(runner, answer);
    }
    answer
}

/// The probe itself: the runner's own `run --help`, captured, asked whether it
/// names the detach flag.
fn probe_detaches(runner: &str) -> bool {
    let here = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let command = format!("{runner} run --help 2>&1");
    match summons::run(
        &Summons::new(DETACHES_VERB, command),
        &Site::root(&here),
        Mode::Captured(DETACHES_TIMEOUT),
    ) {
        Ok(answer) => answer.output.as_deref().is_some_and(help_detaches),
        // A runner that could not be asked is a runner ephor cannot claim
        // detaches: the floor is the attached run, and the floor is never
        // removed (§AR-007-runtime.3).
        Err(_) => false,
    }
}

/// Whether a runner's own help names a detached shape. The whole of what the
/// probe reads, so the reading can be held to on its own.
fn help_detaches(help: &str) -> bool {
    help.contains(HEADLESS_FLAG)
}

/// Why running is refused, or None where the runtime is there
/// (§AR-005-capabilities.2). Where the work is and whether it is checked out
/// belongs to the project's own ladder; this is the runtime rung alone.
pub fn refusal(config: &crate::work::recipe::WorkConfig) -> Option<String> {
    capabilities::workable(Some(runner(config)))
}

/// The summons that runs one work root's plans under a chosen hand the plan
/// language cannot spell (§FS-005-dispatch.14). The one way to build a run —
/// the key in the interface and `work run` both come through here, so neither
/// can drift into an invocation the other does not make
/// (§FS-005-dispatch.12). `hand` is None where every ticket carries its own
/// line, or where the run's tickets do not agree on one spelling — flags ride
/// a run only where they can contradict nothing.
pub fn summons_with(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    plans: &[String],
    hand: Option<&roster::HandFlags>,
    extra: &[String],
) -> Summons {
    Summons::new(
        VERB,
        invocation_with(runner(config), root, plans, hand, extra),
    )
}

/// Run it from the checkout the work is about — not from wherever this was
/// typed: where the runtime puts the agent falls back to its own working
/// directory, and a multi-repository workspace has no single repository to be
/// found by looking (§FS-005-dispatch.3). The person is watching, so the
/// terminal is theirs while it runs.
///
/// `mode` is the caller's, because it is the caller that knows whether its
/// standard output is already spoken for: a run started for a person takes the
/// terminal outright, and one started under `--json` puts the runtime's own
/// output beside the reading instead ([`Mode::Aside`]) so that what a program
/// parses is the outcome alone (§REQ-002-parity.3, §FS-011-command-line.7).
/// The runtime still has a terminal to ask its questions on either way.
pub fn run(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    checkout: &Path,
    plans: &[String],
    hand: Option<&roster::HandFlags>,
    extra: &[String],
    mode: Mode,
) -> Result<Answer> {
    summons::run(
        &summons_with(config, root, plans, hand, extra),
        &Site::root(checkout),
        mode,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_root_names_every_plan_ephor_opened_in_it() {
        let command = invocation(
            Path::new("/w/panta"),
            &["a.rhei.md".to_string(), "b.rhei.md".to_string()],
            &[],
        );
        assert_eq!(
            command,
            "rhei run '/w/panta' --rhei 'a.rhei.md' --rhei 'b.rhei.md'"
        );
    }

    /// An agent-only hand rides the run as the runner's own agent flags —
    /// the spelling for the choice the plan language cannot carry
    /// (§FS-005-dispatch.14) — with the effort alongside where one was
    /// chosen, and the reader's passthrough still last.
    #[test]
    fn an_agent_only_hand_rides_the_run_as_agent_flags() {
        let hand = roster::HandFlags {
            agent: "pi".to_string(),
            effort: Some("high".to_string()),
        };
        let command = invocation_with(
            RUNNER,
            Path::new("/w/panta"),
            &["a.rhei.md".to_string()],
            Some(&hand),
            &["--dry-run".to_string()],
        );
        assert_eq!(
            command,
            "rhei run '/w/panta' --rhei 'a.rhei.md' --agent 'pi' --agent-mode 'high' '--dry-run'"
        );

        // A hand declaring no efforts rides as the agent flag alone — asked
        // plainly, with no mode for the runtime to apply; a hand that does
        // declare efforts always arrives here with one settled
        // (§FS-005-dispatch.14), because the bare flag would let the state
        // machine's own mode fall in, refused where the agent does not
        // declare it.
        let plain = roster::HandFlags {
            agent: "pi".to_string(),
            effort: None,
        };
        let command = invocation_with(RUNNER, Path::new("/w/panta"), &[], Some(&plain), &[]);
        assert_eq!(command, "rhei run '/w/panta' --agent 'pi'");
    }

    /// The runtime is a binding: pointing work at another one is
    /// configuration, not a fork (§REQ-001-boundary.1,
    /// §DA-001-runtime-bound-default).
    #[test]
    fn a_person_who_works_differently_points_work_at_their_own_runtime() {
        let shipped = crate::work::recipe::WorkConfig::default();
        assert_eq!(runner(&shipped), RUNNER);
        assert_eq!(label(&shipped), format!("{RUNNER} run"));

        let bound = crate::work::recipe::WorkConfig {
            runner: Some("my-runtime".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        assert_eq!(runner(&bound), "my-runtime");
        assert_eq!(label(&bound), "my-runtime run");
        let command =
            summons_with(&bound, Path::new("/w/panta"), &["a".to_string()], None, &[]).binding;
        assert!(command.starts_with("my-runtime run "), "{command}");
    }

    /// With no runtime on PATH, writing and reading are unchanged and only
    /// running refuses — with the bound runner named (§FS-005-dispatch lead).
    #[test]
    fn running_refuses_with_the_bound_runner_named() {
        let absent = crate::work::recipe::WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        let why = refusal(&absent).expect("nothing can run it");
        assert!(
            why.starts_with("no-such-runtime-anywhere is not on PATH"),
            "{why}"
        );
        // Something every machine has, to prove the held case is real.
        let present = crate::work::recipe::WorkConfig {
            runner: Some("sh".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        assert_eq!(refusal(&present), None);
    }

    /// A cancel is the runner's transition verb into the abandonment state,
    /// carrying the reader's reason as the result, with the runner's standard
    /// error folded in so its refusal can be read back
    /// (§FS-005-dispatch.16, §DA-005-cancel-is-the-runtimes-move).
    #[test]
    fn a_cancel_is_the_runners_transition_into_the_abandonment_state() {
        let command = cancel_command(
            &crate::work::recipe::WorkConfig::default(),
            Path::new("/w/panta/forge-demo-17.rhei.md"),
            "fix-gate-2",
            "collect",
            "asked twice by mistake",
        );
        assert_eq!(
            command,
            "rhei transition '/w/panta/forge-demo-17.rhei.md' --task 'fix-gate-2' \
             --from 'collect' --to 'cancelled' --result 'asked twice by mistake' 2>&1"
        );
        // Under another binding it is that binding's verb.
        let bound = crate::work::recipe::WorkConfig {
            runner: Some("my-runtime".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        assert!(
            cancel_command(&bound, Path::new("/w/p.rhei.md"), "a-1", "fix", "why")
                .starts_with("my-runtime transition ")
        );
    }

    /// What the runner printed comes back as its own sentence, unwrapped from
    /// the report decoration and cut to what a message line can carry.
    #[test]
    fn the_runners_refusal_is_read_back_in_its_own_words() {
        let printed = "  × Task item.fix-gate-1 cannot leave state collect.\n  │ Missing required output artifact: failures\n  │ (runtime/ephor/item.fix-gate-1.failures.md)\n  help: the state's work is not finished until that file exists.\n";
        assert_eq!(
            said(printed),
            "Task item.fix-gate-1 cannot leave state collect. Missing required output artifact: failures"
        );
        assert_eq!(said("\n   \n"), "");
    }

    /// Run against a stand-in runner: a stand-in that agrees moves nothing
    /// ephor can see but answers done, and one that refuses hands its words
    /// back as the error (§FS-005-dispatch.16).
    #[test]
    fn a_cancel_is_done_or_carries_the_runners_words() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        std::fs::create_dir_all(&root).unwrap();
        let plan = root.join("p.rhei.md");
        std::fs::write(&plan, "# Rhei: p\n").unwrap();
        // The binding is a shell function of the reader's: `sh -c` resolves
        // it as any command, and a name on PATH is not needed for the seam.
        let agrees = crate::work::recipe::WorkConfig {
            runner: Some("echo".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        cancel(&agrees, &root, &plan, "a-1", "fix", "why").expect("echo agrees to anything");

        let refuses = crate::work::recipe::WorkConfig {
            runner: Some(
                "sh -c 'printf \"  × Task a-1 cannot leave state fix.\\n\" >&2; exit 1' --"
                    .to_string(),
            ),
            ..crate::work::recipe::WorkConfig::default()
        };
        let err = cancel(&refuses, &root, &plan, "a-1", "fix", "why").expect_err("it refused");
        let said = err.to_string();
        assert!(
            said.contains("refused: Task a-1 cannot leave state fix."),
            "{said}"
        );
    }

    /// The detached start is one edit of the attached invocation: the runner's
    /// own detach flag beside the plan flag, the same root, the same plans, the
    /// same hand — so the key that starts a run beneath the screen and the key
    /// that watched one cannot drift into two invocations
    /// (§FS-005-dispatch.20, §AR-007-runtime.1).
    #[test]
    fn a_detached_start_is_the_same_run_with_the_detach_flag() {
        let hand = roster::HandFlags {
            agent: "pi".to_string(),
            effort: Some("high".to_string()),
        };
        let command = detached_invocation_with(
            RUNNER,
            Path::new("/w/panta"),
            &["a.rhei.md".to_string()],
            Some(&hand),
            &["--parallel".to_string()],
        );
        assert_eq!(
            command,
            "rhei run --headless --json '/w/panta' --rhei 'a.rhei.md' --agent 'pi' \
             --agent-mode 'high' '--parallel' 2>&1"
        );
        // And under another binding it is that binding's run.
        let bound = detached_invocation_with("my-runtime", Path::new("/w/p"), &[], None, &[]);
        assert_eq!(bound, "my-runtime run --headless --json '/w/p' 2>&1");
    }

    /// The id comes out of the launcher's own descriptor, wherever in what it
    /// printed the object sits: warnings share the stream with it
    /// (§FS-005-dispatch.20). And so does whether the run is already over —
    /// out of the *same* document, because a run reported as started sends a
    /// reader to the board, and a run that had nothing to do and exited inside
    /// the handshake would send them to an empty one.
    #[test]
    fn the_run_id_is_read_out_of_what_the_launcher_printed() {
        let printed = "warning: could not write the registry entry\n                       {\"id\":\"3f9a2c\",\"pid\":48213,\"status\":\"running\",\"exit_code\":null}\n";
        assert_eq!(
            started(printed),
            Started {
                id: Some("3f9a2c".to_string()),
                finished: false
            }
        );
        // Nothing to read is nothing claimed: a run that named itself nothing
        // is still a run, and the row is live from the lock alone.
        assert_eq!(started("started\n"), Started::default());
        assert_eq!(started("{\"pid\":1}"), Started::default());

        // Over before the launcher returned, said either way the launcher says
        // it: by its own word for finished, or by an exit code.
        assert!(started("{\"id\":\"3f9a2c\",\"status\":\"finished\"}").finished);
        assert!(started("{\"id\":\"3f9a2c\",\"status\":\"running\",\"exit_code\":0}").finished);
        assert!(!started("{\"id\":\"3f9a2c\",\"status\":\"running\"}").finished);
    }

    /// A launcher that refused says so in its own error envelope, and that
    /// sentence is what the reader is told (§FS-005-dispatch.20): an invalid
    /// plan, a held lock and an unresolvable hand all fail there, as loudly as
    /// they do in the foreground.
    #[test]
    fn the_launchers_own_diagnostic_is_the_refusal() {
        let envelope = concat!(
            "{\"error\":{\"help\":\"the run's console is at runtime/run.log\",",
            "\"message\":\"the run exited before it started (exit status 1):\\n",
            "    × failed to parse state machine\\n",
            "    │ 'states.yaml': missing field `version`\\n",
            "    help: fix it\"}}"
        );
        let said = launcher_refusal(envelope).expect("it said why");
        assert_eq!(
            said,
            "the run exited before it started (exit status 1): failed to parse state machine"
        );
        // Nothing shaped like an envelope is nothing to lift: the printed
        // lines answer instead.
        assert_eq!(launcher_refusal("could not start\n"), None);
    }

    /// The two verbs a run is reached by, in the runner's own words. The stop
    /// is only ever shown (§FS-005-dispatch.20).
    #[test]
    fn attaching_and_stopping_are_the_runners_own_words() {
        let shipped = crate::work::recipe::WorkConfig::default();
        assert_eq!(attach_command(&shipped, "3f9a2c"), "rhei attach '3f9a2c'");
        assert_eq!(stop_command(&shipped, "3f9a2c"), "rhei stop '3f9a2c'");
        let bound = crate::work::recipe::WorkConfig {
            runner: Some("my-runtime".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        assert_eq!(attach_command(&bound, "a b"), "my-runtime attach 'a b'");
        assert_eq!(stop_command(&bound, "3f9a2c"), "my-runtime stop '3f9a2c'");
        // Both come out of a file the run wrote and both are lines to paste,
        // so both are quoted: an id that carried a `;` used to make the stop
        // line run something else.
        assert_eq!(
            stop_command(&bound, "3f; rm -rf /"),
            "my-runtime stop '3f; rm -rf /'"
        );
    }

    /// Whether the binding can detach is asked of the binding
    /// (§AR-007-runtime.3), and read out of its own help: a runner that names
    /// the flag detaches, one that does not runs attached as it always did,
    /// and nothing bound detaches at all — and is never even asked. The probe
    /// itself is proven from the outside, against a stand-in runner on PATH.
    #[test]
    fn whether_the_runner_detaches_is_read_out_of_its_own_help() {
        assert!(help_detaches(
            "Options:\n      --headless  Detach the run into its own session\n"
        ));
        assert!(!help_detaches("Options:\n      --tui  Force TUI mode\n"));
        let absent = crate::work::recipe::WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        assert!(!can_detach(&absent));
    }

    #[test]
    fn a_path_with_a_space_stays_one_word() {
        let command = invocation(Path::new("/w/my work/panta"), &[], &[]);
        assert_eq!(command, "rhei run '/w/my work/panta'");
    }

    #[test]
    fn passthrough_arguments_reach_the_runner_last() {
        let command = invocation(
            Path::new("/w/panta"),
            &["a.rhei.md".to_string()],
            &["--dry-run".to_string(), "it's fine".to_string()],
        );
        assert!(command.ends_with(r"--rhei 'a.rhei.md' '--dry-run' 'it'\''s fine'"));
    }
}
