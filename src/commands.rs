//! The command-line surface for the abilities the screen used to hold alone
//! (§FS-011-command-line, §REQ-002-parity).
//!
//! Presentation only (§AR-009-surfaces.4): every one of these opens the
//! session, calls a reading or a move, and renders what came back — as prose,
//! or as the same answer in JSON. None of them decides anything.

use std::process::ExitCode;

use crate::api::read::Subject;
use crate::api::{offers, views, Session};
use crate::cli::{
    ActionsArgs, ActionsCommand, ActionsListArgs, ActionsOpenArgs, ActionsRunArgs, BranchesArgs,
    OperationsArgs, ReactArgs, ReplyArgs, SubjectArgs, ThreadArgs, TickArgs,
};
use crate::error::{registry_error, EphorError, Result};
use crate::feed::config::load_config;
use crate::feed::model::Item;
use crate::feed::render::Style;

/// The one place a reading becomes standard output. Under `--json` the
/// reading is alone on it; notes and progress belong on the error stream,
/// where they narrate the run without becoming part of it
/// (§FS-011-command-line.7).
fn emit<T: serde::Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string())
    );
}

/// A move's outcome, printed and turned into an exit code. `1` where it was
/// refused, because a script that reads only the status still learns whether
/// the thing happened (§REQ-002-parity.3).
fn report(outcome: &views::Outcome, json: bool) -> ExitCode {
    if json {
        emit(outcome);
    } else if outcome.ok {
        println!("{}", outcome.says);
    } else {
        eprintln!("{}", outcome.says);
    }
    match outcome.ok {
        true => ExitCode::SUCCESS,
        false => ExitCode::from(1),
    }
}

/// Which matter or branch was named. Refused rather than guessed: a command
/// that acted on the nearest match is a command nobody can script against.
fn subject_of(session: &Session, args: &SubjectArgs) -> Result<Named> {
    match (&args.item, &args.branch, &args.project) {
        (Some(id), None, _) => {
            let item = session.item(id).ok_or_else(|| {
                registry_error(format!(
                    "'{id}' is not in any cached feed — `ephor feed` lists what is."
                ))
            })?;
            Ok(Named::Item(Box::new(item)))
        }
        (None, Some(branch), Some(project)) => Ok(Named::Branch {
            project: project.clone(),
            branch: branch.clone(),
        }),
        (None, None, _) => Err(registry_error(
            "Name what this is about: --item ID, or --project P --branch B.".to_string(),
        )),
        (Some(_), Some(_), _) => Err(registry_error(
            "A matter and a branch are two subjects: pass --item or --branch, not both."
                .to_string(),
        )),
        (None, Some(_), None) => Err(registry_error(
            "--branch needs --project: a branch name alone does not say whose it is.".to_string(),
        )),
    }
}

enum Named {
    Item(Box<Item>),
    Branch { project: String, branch: String },
}

impl Named {
    fn as_subject(&self) -> Subject<'_> {
        match self {
            Named::Item(item) => Subject::Item(item),
            Named::Branch { project, branch } => Subject::Branch { project, branch },
        }
    }
}

/// `ephor actions` (§FS-011-command-line.1). With no subcommand it lists,
/// because that is the question asked far more often than "run this one".
pub fn actions(args: &ActionsArgs) -> Result<ExitCode> {
    match &args.command {
        Some(ActionsCommand::Run(run)) => actions_run(run),
        Some(ActionsCommand::Open(open)) => actions_open(open),
        Some(ActionsCommand::List(list)) => actions_list(list),
        None => actions_list(&ActionsListArgs {
            subject: args.subject.clone(),
            json: args.json,
        }),
    }
}

fn actions_list(args: &ActionsListArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let mut session = Session::open(&config)?;
    let named = subject_of(&session, &args.subject)?;
    let view = session
        .actions(&named.as_subject())
        .map_err(EphorError::Command)?;
    if args.json {
        emit(&view);
        return Ok(ExitCode::SUCCESS);
    }
    let style = Style::detect();
    println!("{}\n", style.bold(&view.title));
    println!("  in {}", view.workspace.display());
    if view.workspace_state != "ready" {
        println!("  the branch workspace is {}", view.workspace_state);
    }
    println!();
    let width = view
        .offers
        .iter()
        .map(|offer| offer.id.chars().count())
        .max()
        .unwrap_or(0);
    for offer in &view.offers {
        // The row says whether it can run before it says what it is: a reader
        // scanning for something to run is scanning this column
        // (§FS-004-quick-actions.2).
        let mark = match offer.gate {
            "ready" => "·",
            "needs-checkout" => "⇣",
            _ => "✗",
        };
        println!(
            "{mark} {:width$}  {} {}",
            offer.id, offer.icon, offer.description
        );
        // What is already going about this row, and the way in — the same mark
        // the screen sets apart, with the same facts, so a program reading the
        // menu cannot start what a person reading it would have opened
        // (§FS-005-dispatch.21, §FS-011-command-line.8).
        if let Some(running) = &offer.running {
            let since = running
                .since_seconds
                .map(|seconds| format!("{} · ", crate::feed::render::span(seconds)))
                .unwrap_or_default();
            println!(
                "  {:width$}    ▶ {} · {since}{}",
                "", running.kind, running.says
            );
            let way_in = running
                .log
                .as_ref()
                .map(|log| log.display().to_string())
                .or_else(|| running.attach.clone())
                .or_else(|| running.window.clone());
            if let Some(way_in) = way_in {
                println!("  {:width$}      {way_in}", "");
            }
            if let Some(url) = &running.control_url {
                println!("  {:width$}      {url}", "");
            }
            println!("  {:width$}      ephor actions open {}", "", offer.id);
        }
        if let Some(hand) = &offer.hand {
            println!("  {:width$}    → {hand}", "");
        }
        if let Some(refusal) = &offer.refusal {
            println!("  {:width$}    {refusal}", "");
        }
    }
    if !view.roster.is_empty() {
        println!("\n  --hand may name:");
        for hand in &view.roster {
            match &hand.unavailable {
                Some(why) => println!("    {} (unavailable: {why})", hand.id),
                None if hand.efforts.is_empty() => println!("    {}", hand.id),
                None => println!("    {} at {}", hand.id, hand.efforts.join(", ")),
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `ephor actions open` (§FS-011-command-line.8): the key on a running row as
/// a command (§FS-005-dispatch.21). It follows the log, attaches to the run, or
/// brings the window forward, by the same binding the key uses — and refuses by
/// name where the entry has nothing going.
fn actions_open(args: &ActionsOpenArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let mut session = Session::open(&config)?;
    let named = subject_of(&session, &args.subject)?;
    let subject = named.as_subject();
    let entries = session.menu(&subject).map_err(EphorError::Command)?;
    let entry = entries
        .iter()
        .find(|entry| entry.key() == args.entry)
        .ok_or_else(|| {
            registry_error(format!(
                "No entry called '{}' here. `ephor actions` lists what there is.",
                args.entry
            ))
        })?;
    // Refused by name, not silently: an entry with nothing going is not an
    // entry to open, and a command that quietly did nothing would read exactly
    // like one that worked (§REQ-001-boundary.1).
    let Some(running) = entry.running.clone() else {
        return Ok(report(
            &views::Outcome::refused(format!(
                "Nothing is going about '{}': `ephor actions run {}` starts it.",
                args.entry, args.entry
            )),
            args.json,
        ));
    };
    // Under `--json` what the surface writes goes beside the reading rather
    // than into it (§FS-011-command-line.7).
    let watching = match args.json {
        true => crate::api::act::Watching::Aside,
        false => crate::api::act::Watching::Terminal,
    };
    Ok(report(&session.open_running(&running, watching), args.json))
}

fn actions_run(args: &ActionsRunArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let mut session = Session::open(&config)?;
    let named = subject_of(&session, &args.subject)?;
    let subject = named.as_subject();
    let placed = session.place(&subject).map_err(EphorError::Command)?;
    let entries = session.menu(&subject).map_err(EphorError::Command)?;

    // The freehand row (§FS-005-dispatch.10). It carries no id because there
    // is nothing configured behind it — `--command` is how it is named.
    let mut entry = match (&args.command, &args.entry) {
        (Some(command), _) => {
            let mut freehand = entries
                .iter()
                .find(|entry| entry.is_freehand)
                .cloned()
                .ok_or_else(|| registry_error("Nothing here can run a command".to_string()))?;
            freehand.action.command = command.clone();
            freehand.action.description = command.clone();
            freehand.is_freehand = false;
            freehand
        }
        (None, Some(id)) => entries
            .iter()
            .find(|entry| &entry.key() == id)
            .cloned()
            .ok_or_else(|| {
                registry_error(format!(
                    "No entry called '{id}' here. `ephor actions` lists what there is."
                ))
            })?,
        (None, None) => {
            return Err(registry_error(
                "Name an entry, or pass --command to run one of your own.".to_string(),
            ))
        }
    };

    if let Some(reason) = entry.gate.refusal() {
        return Ok(report(&views::Outcome::refused(reason), args.json));
    }
    // The row that opens the runtime's own workflows is a screen, not a move:
    // on the command line the listing it opens is already a command
    // (§FS-011-command-line.1).
    if entry.is_workflows {
        return Err(registry_error(
            "That row opens the runtime's workflows: `ephor work workflows` lists them, \
             and `ephor work lay` lays one down."
                .to_string(),
        ));
    }
    // A confirmation that a screen asks with a second keystroke, a command
    // asks with a flag (§FS-006-project-interface.9).
    if entry.action.confirm && !args.yes && !args.dry_run {
        return Err(registry_error(format!(
            "'{}' asks to be confirmed: pass --yes to run it.",
            entry.key()
        )));
    }
    // The pick for this run alone (§FS-005-dispatch.14), spent by the dispatch
    // and remembered by nothing. It answers "who does this work", so an entry
    // that hands none over is refused by name rather than run with the flag
    // quietly dropped: a reader who named a hand and got the default would
    // have no way to tell (§FS-004-quick-actions.2). The same reasoning
    // refuses `--set` on an entry that lays no workflow down.
    let hands_work = entry.action.agent.is_some() || entry.action.workflow.is_some();
    if let Some(hand) = &args.hand {
        if !hands_work {
            return Err(registry_error(format!(
                "--hand says who does a piece of work, and '{}' hands none over: it {}. \
                 `ephor actions --item …` says which entries do.",
                entry.key(),
                match entry.kind() {
                    "checkout" => "makes the branch workspace",
                    "workflows" => "opens the runtime's workflows",
                    _ => "runs a command here",
                }
            )));
        }
        entry.picked =
            Some(crate::work::recipe::HandPin::parse(hand).map_err(EphorError::Command)?);
    }
    // The same for the answers a workflow's inputs take (§FS-005-dispatch.19).
    if !args.set.is_empty() && entry.action.workflow.is_none() {
        return Err(registry_error(format!(
            "--set answers a workflow's inputs, and '{}' lays no workflow down.",
            entry.key()
        )));
    }

    // An entry that hands work over is dispatched, and one that lays a
    // workflow down is laid: both go through the path the work screen uses, so
    // nothing they carry is lost on the way (§FS-005-dispatch.4).
    if entry.action.agent.is_some() || entry.action.workflow.is_some() {
        let Named::Item(item) = &named else {
            return Err(registry_error(
                "Work is asked for about a matter, and a branch row has none.".to_string(),
            ));
        };
        return Ok(report(
            &session.hand_over(item, &entry, &args.set, args.dry_run),
            args.json,
        ));
    }

    let about = match &named {
        Named::Item(item) => crate::api::act::About::Item(item.clone()),
        Named::Branch { project, branch } => crate::api::act::About::Branch {
            project: project.clone(),
            branch: branch.clone(),
        },
    };
    let request = crate::api::act::Run {
        about,
        root: placed.root,
        workspace: placed.workspace,
        state: placed.state,
        checkout: session.checkouts.get(subject.project()).cloned(),
        branch: placed.branch,
        entry,
    };
    if args.dry_run {
        return Ok(report(&dry_run_of(&request), args.json));
    }
    // Beneath the terminal where the entry says so, or where the reader asked
    // (§FS-005-dispatch.17); here otherwise, because a menu entry has always
    // been allowed to *be* the reader's session.
    let outcome = match args.background || request.entry.action.background {
        true => session.start_job(&request),
        // Under `--json` the entry's own output goes beside the reading, so
        // that what a program parses is the outcome alone
        // (§FS-011-command-line.7).
        false => session.run_entry(
            &request,
            match args.json {
                true => crate::api::act::Watching::Aside,
                false => crate::api::act::Watching::Terminal,
            },
        ),
    };
    Ok(report(&outcome, args.json))
}

/// What would run, and where — without running it.
///
/// The whole chain, off the same derivation the run itself walks
/// (§AR-009-surfaces.1): an entry that needs the branch workspace runs the
/// checkout first (§FS-004-quick-actions.7) and then runs *in the workspace
/// that checkout is about to create*, not in the one the reader is standing
/// in. A dry run that named one step and the current directory would be
/// answering a different question than the run it is describing, which is the
/// one thing `--dry-run` may never do.
fn dry_run_of(request: &crate::api::act::Run) -> views::Outcome {
    let chain = match request.chain() {
        Ok(chain) => chain,
        Err(refusal) => return views::Outcome::refused(refusal),
    };
    let steps: Vec<views::Step> = chain
        .iter()
        .map(|link| views::Step {
            icon: link.action.icon.clone(),
            description: link.action.description.clone(),
            command: link.action.command.clone(),
            cwd: link.cwd.clone(),
            // A dry run reports the plan, not a verdict on it: nothing was
            // asked of the world, so nothing here has been refused.
            ok: true,
            refusal: None,
        })
        .collect();
    let says = steps
        .iter()
        .map(|step| format!("would run: {} (in {})", step.command, step.cwd.display()))
        .collect::<Vec<_>>()
        .join("\n");
    views::Outcome {
        steps,
        ..views::Outcome::ok(says)
    }
}

/// `ephor branches` (§FS-011-command-line.2).
pub fn branches(args: &BranchesArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let session = Session::open(&config)?;
    let projects: Vec<String> = match &args.project {
        Some(project) => {
            if !config.projects.contains_key(project) {
                return Err(registry_error(format!(
                    "Project '{project}' has no feed configuration."
                )));
            }
            vec![project.clone()]
        }
        None => session.projects.clone(),
    };
    let rows: Vec<views::Branch> = projects
        .iter()
        .flat_map(|project| session.branch_rows(project))
        .filter(|row| !args.checked_out || row.checked_out)
        .collect();
    if args.json {
        emit(&rows);
        return Ok(ExitCode::SUCCESS);
    }
    if rows.is_empty() {
        println!("No branches. A project's registry row names them, and ephor finds the rest.");
        return Ok(ExitCode::SUCCESS);
    }
    let width = rows
        .iter()
        .map(|row| row.branch.chars().count())
        .max()
        .unwrap_or(0);
    let owner = rows
        .iter()
        .map(|row| row.project.chars().count())
        .max()
        .unwrap_or(0);
    for row in &rows {
        let mut says: Vec<String> = Vec::new();
        // A distance with no day on it is a claim about now that nothing
        // measured, so every one of them carries its own (§FS-004-quick-actions.6).
        if let Some(behind) = row.behind {
            says.push(format!(
                "{} behind {}{}",
                behind.behind,
                row.main_branch.as_deref().unwrap_or("its base"),
                as_of(behind)
            ));
        }
        if let Some(behind) = row.behind_upstream {
            says.push(format!(
                "{} behind {}{}",
                behind.behind,
                row.published.as_deref().unwrap_or("its published copy"),
                as_of(behind)
            ));
        }
        if row.items > 0 {
            says.push(format!("{} here", row.items));
        }
        println!(
            "{} {:owner$}  {:width$}  {}",
            if row.checked_out { "✓" } else { "·" },
            row.project,
            row.branch,
            says.join(" · ")
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn as_of(distance: views::Distance) -> String {
    match distance.as_of {
        Some(seen) => format!(" (as of {})", seen.format("%b %-d")),
        None => String::new(),
    }
}

/// `ephor operations` (§FS-011-command-line.3).
pub fn operations(args: &OperationsArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let mut session = Session::open(&config)?;
    let mut board = session.operations();
    if args.live {
        board.operations.retain(|op| op.live);
    }
    if args.json {
        emit(&board);
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(refusal) = &board.refusal {
        eprintln!("note: {refusal}");
    }
    if board.operations.is_empty() {
        println!("Nothing is running — a live run or job appears here on its own.");
        return Ok(ExitCode::SUCCESS);
    }
    for op in &board.operations {
        let marker = if op.live { "▶" } else { "✋" };
        println!("{marker} {} · {}   {}", op.project, op.id, op.state);
        println!("    {}", op.says);
        for ticket in &op.tickets {
            println!("      {} [{}]  {}", ticket.id, ticket.state, ticket.says);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The matter a conversation command was aimed at.
fn matter(session: &Session, id: &str) -> Result<Item> {
    session.item(id).ok_or_else(|| {
        registry_error(format!(
            "'{id}' is not in any cached feed — `ephor feed` lists what is."
        ))
    })
}

/// `ephor thread` (§FS-011-command-line.4).
pub fn thread(args: &ThreadArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let session = Session::open(&config)?;
    let item = matter(&session, &args.item)?;
    let view = session.conversation(&item).view(&item);
    if args.json {
        emit(&view);
        return Ok(ExitCode::SUCCESS);
    }
    let style = Style::detect();
    println!("{}\n", style.bold(&view.title));
    if view.messages.is_empty() {
        println!("Nothing has been said on this matter that ephor recorded.");
    }
    for (index, message) in view.messages.iter().enumerate() {
        let when = match message.at {
            Some(at) => format!("  {}", at.format("%b %-d %H:%M")),
            None => String::new(),
        };
        // The number is the address a move takes (§REQ-002-parity.2), so it
        // leads the line rather than trailing it.
        let box_glyph = match &message.task {
            Some(task) if task.resolved => "☑ ",
            Some(_) => "☐ ",
            None => "",
        };
        println!("[{index}] {box_glyph}{}{when}", style.bold(&message.author));
        for line in message.text.lines() {
            println!("    {line}");
        }
        for reaction in &message.reactions {
            println!(
                "    {} {} ({})",
                reaction.emoji,
                reaction.users.len(),
                reaction.users.join(", ")
            );
        }
        println!();
    }
    if let Some(draft) = &view.draft {
        println!("{}", style.bold("── a run drafted this reply, unsent ──"));
        for line in draft.text.lines() {
            println!("    {line}");
        }
        match draft.sendable {
            true => println!("\n    `ephor reply {}` sends it.", view.item),
            // A stated degrade, not a failure (§REQ-001-boundary.1): the words
            // are still here, and this says where they sit.
            false => println!(
                "\n    This channel declared no way to send it — the words are at {}",
                draft.path.display()
            ),
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `ephor react` (§FS-011-command-line.4). The three moves inside a
/// conversation take `--json` like every other move: these are the moves the
/// command line exists for — a runtime that can read a feed but cannot post
/// the reply its own run drafted holds half a tool (§REQ-002-parity).
pub fn react(args: &ReactArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let session = Session::open(&config)?;
    let item = matter(&session, &args.item)?;
    Ok(report(
        &session.react(&item, args.message, &args.content),
        args.json,
    ))
}

/// `ephor tick` (§FS-004-quick-actions.5).
pub fn tick(args: &TickArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let session = Session::open(&config)?;
    let item = matter(&session, &args.item)?;
    Ok(report(&session.tick(&item, args.message), args.json))
}

/// `ephor reply` (§FS-005-dispatch.13).
pub fn reply(args: &ReplyArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let session = Session::open(&config)?;
    let item = matter(&session, &args.item)?;
    let words = match args.words.is_empty() {
        true => None,
        false => Some(args.words.join(" ")),
    };
    // A dry run is a reading of what the move would do, so it is the move with
    // the post left out rather than a second description of it: `ok` means it
    // would go out, and `says` carries the words themselves
    // (§REQ-002-parity.3). Assembling the answer here instead reported success
    // on a channel that declares no way to reply — the one thing a flag
    // documented as "the reading a program checks before letting the move
    // happen" may never do.
    let sending = match args.dry_run {
        true => crate::api::act::Sending::Dry,
        false => crate::api::act::Sending::Now,
    };
    Ok(report(
        &session.reply(&item, words.as_deref(), sending),
        args.json,
    ))
}

/// Kept beside the rest so a surface never reaches past the API for the one
/// name a freehand entry answers to (§AR-009-surfaces.5).
pub const FREEHAND: &str = offers::FREEHAND_ID;
