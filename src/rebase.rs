//! `ephor rebase` — the deterministic move, run on its own
//! (§FS-004-quick-actions.6), onto the project's main branch or onto the
//! branch's own published copy (§FS-004-quick-actions.8).
//!
//! The reader presses a key for it and a state machine runs it as a program;
//! both arrive here, so there is one answer to what a clean rebase is
//! (§FS-005-dispatch.12). What it cannot finish — a conflict — it hands over
//! as work rather than deciding.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::branches::Placement;
use crate::cli::RebaseArgs;
use crate::error::{EphorError, Result};
use crate::feed::cache;
use crate::feed::config::load_config;
use crate::feed::model::Item;
use crate::git;
use crate::given;
use crate::work::Dispatcher;

/// A conflict is not a failure: it is where the work starts.
const CONFLICT: u8 = 3;

pub fn rebase(args: &RebaseArgs) -> Result<ExitCode> {
    // A state machine hands its program everything through `env:` (the manual's
    // §8.5), so the flags a reader types and the names a state sets are the
    // same inputs spelled two ways, and each is honoured or refused naming the
    // spelling it arrived in (§FS-011-command-line.9). A `{meta.branch}` the
    // runtime could not fill arrives as itself: an unresolved placeholder is
    // not a value, and turning one into "rebase the working directory" ran a
    // different rebase than the state asked for.
    let checkout = match given::value(&args.checkout, "CHECKOUT")? {
        Some(path) => crate::paths::resolve_path(&path),
        None => std::env::current_dir().map_err(|err| {
            EphorError::Command(format!("Cannot read the working directory: {err}"))
        })?,
    };
    if !checkout.is_dir() {
        return Err(EphorError::Command(format!(
            "No checkout at {} — nothing to rebase.",
            checkout.display()
        )));
    }

    let item = match given::value(&args.item, "ITEM")? {
        Some(id) => Some(find_item(&id)?),
        None => None,
    };
    // Who resolves a conflict this run hands over, for this dispatch alone
    // (§FS-005-dispatch.14) — the same flag `work dispatch` takes, because the
    // key and the command line are one operation (§FS-005-dispatch.12). Parsed
    // before the replay runs: the refusal of a pick that is not one is
    // computed here, not discovered on the conflicted run (§AR-002-summons.4).
    let picked = given::value(&args.hand, "HAND")?
        .map(|text| crate::work::recipe::HandPin::parse(&text))
        .transpose()
        .map_err(EphorError::Command)?;
    if picked.is_some() && !args.dispatch {
        eprintln!("note: --hand rides --dispatch — without it, nothing is handed over.");
    }
    let project = given::value(&args.project, "PROJECT")?
        .or_else(|| item.as_ref().map(|item| item.project.clone()));
    let placement = project.as_deref().and_then(|project| {
        Placement::load(&crate::feed::commands::load_registry_doc().ok()?, project)
    });

    // The forest this checkout is, declared where the registry says and probed
    // where it does not (§AR-004-forest.2).
    let forest = match &placement {
        Some(placement) => placement.forest(&checkout),
        None => crate::forest::Forest::resolve(&checkout, None, &[]),
    };
    // What to replay onto. `--upstream` asks for each branch's own published
    // copy, which is a different ref in every repository and so is named by
    // nothing here (§FS-004-quick-actions.8); otherwise it is one branch name
    // for the whole forest. The flag has the environment spelling every other
    // argument has — a program state cannot press `--upstream` — and because
    // clap's `conflicts_with` sees only the flags, the refusal of the two
    // together is repeated here across both spellings: silently preferring
    // one would run a different rebase than the state asked for.
    let upstream = args.upstream || given::value(&None, "UPSTREAM")?.is_some();
    let onto_named = given::value(&args.onto, "ONTO")?;
    if upstream && onto_named.is_some() {
        return Err(EphorError::Command(
            "--upstream (UPSTREAM) and --onto (ONTO) name two different things to replay \
             onto — pass one of them."
                .to_string(),
        ));
    }
    let onto = if upstream {
        git::Onto::Upstream
    } else {
        git::Onto::Base(
            match onto_named.or_else(|| {
                placement
                    .as_ref()
                    .and_then(|placement| placement.main_branch.clone())
            }) {
                Some(base) => base,
                // No registry to ask: what origin itself calls its default is
                // the closest thing to an answer, and saying which one was
                // chosen is what makes a wrong guess visible.
                None => forest
                    .repos
                    .first()
                    .and_then(|repo| git::default_base(&repo.path, &repo.remote))
                    .ok_or_else(|| {
                        EphorError::Command(format!(
                            "Nothing says what to rebase {} onto — pass --onto or --upstream, \
                             or --project for a registry entry with a main_branch.",
                            checkout.display()
                        ))
                    })?,
            },
        )
    };

    let outcome = git::rebase(&forest, &onto);
    let conflicted = outcome.conflicted().len();
    // Everything the algorithm could do, it did; the rest is a question about
    // the code (§FS-005-dispatch.12). Handed over *before* anything is
    // printed, so what it opened is a field of the reading rather than three
    // lines of prose after it — a script that read only the JSON would
    // otherwise not learn that a ticket exists, which is exactly the half a
    // reading may not be missing (§REQ-002-parity.3).
    let mut refused: Option<EphorError> = None;
    let handed = match (conflicted > 0 && args.dispatch, &item) {
        (true, Some(item)) => match hand_over(item, &outcome, picked.as_ref()) {
            Ok(handed) => Some(handed),
            // A dispatch that could not be made does not swallow the replay's
            // own report: it still prints, and the refusal is this command's
            // exit (§REQ-001-boundary.1).
            Err(err) => {
                refused = Some(err);
                None
            }
        },
        (true, None) => {
            eprintln!("note: --dispatch needs --item to know whose work this conflict is.");
            None
        }
        _ => None,
    };
    // Under `--json` the report is the reading's own field rather than a
    // second thing on standard output (§FS-011-command-line.7).
    if args.json {
        let mut view = outcome.view();
        if let Some(object) = view.as_object_mut() {
            object.insert(
                "report".to_string(),
                serde_json::Value::String(outcome.report()),
            );
            object.insert(
                "dispatched".to_string(),
                match &handed {
                    Some(handed) => {
                        let mut row = serde_json::json!({
                            "item": handed.item,
                            "says": handed.says,
                            "notes": handed.notes,
                        });
                        // A dispatch that reopened nothing opened no ticket, and
                        // says so by not naming one: the published shape
                        // declares `ticket` a string (§REQ-002-parity.4).
                        if let (Some(row), Some(ticket)) =
                            (row.as_object_mut(), handed.ticket.as_ref())
                        {
                            row.insert("ticket".to_string(), serde_json::json!(ticket));
                        }
                        row
                    }
                    None => serde_json::Value::Null,
                },
            );
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&view).unwrap_or_else(|_| "null".to_string())
        );
    } else {
        print!("{}", outcome.report());
        if let Some(handed) = &handed {
            println!("\nhanded over: {}", handed.says);
            // What the resolution had to say about who got it — an effort
            // completed, a hand nobody can be asked for — said here, where the
            // reader still is (§FS-005-dispatch.14).
            for note in &handed.notes {
                println!("note: {note}");
            }
            println!("  ephor work run --item {}", handed.item);
        }
    }
    // The report lands before anything can leave this function, and in
    // particular before a refused `--dispatch` becomes this command's exit: a
    // state machine reads `REPORT` to learn what the replay stopped at, and no
    // recipe named 'rebase' is exactly the moment it needs the file most
    // (§FS-005-dispatch.12). Writing it only on the happy path made the
    // conflict report disappear on ordinary conditions — an unwritable ledger,
    // an unconfigured recipe — which is the absence §REQ-001-boundary.1 forbids.
    if let Some(path) = given::value(&args.report, "REPORT")? {
        write_report(&path, &outcome.report())?;
    }
    if let Some(err) = refused {
        return Err(err);
    }

    if outcome.repos.is_empty() {
        return Err(EphorError::Command(format!(
            "No git repository under {}.",
            checkout.display()
        )));
    }

    if conflicted > 0 {
        if let (false, Some(item), false) = (args.dispatch, &item, args.json) {
            println!(
                "Hand the conflict to the runtime:\n  ephor work dispatch --item {} \
                 --recipe rebase",
                item.id
            );
        }
        return Ok(ExitCode::from(CONFLICT));
    }
    if !outcome.stuck().is_empty() {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// What handing the conflict over came to: the ticket it opened, the sentence
/// the dispatch describes itself with, and whatever resolving who gets it had
/// to say (§FS-005-dispatch.14). Returned rather than printed, because the
/// same facts have to reach a line of prose and a field of the reading
/// (§REQ-002-parity.3).
struct Handed {
    item: String,
    ticket: Option<String>,
    says: String,
    notes: Vec<String>,
}

/// Open the ticket the conflict is about, carrying what the rebase reached.
/// `picked` is the reader's own choice of who resolves it, spent by this one
/// dispatch (§FS-005-dispatch.14).
fn hand_over(
    item: &Item,
    outcome: &git::Rebase,
    picked: Option<&crate::work::recipe::HandPin>,
) -> Result<Handed> {
    let config = load_config()?;
    let mut dispatcher = Dispatcher::load(&config)?;
    let recipe = dispatcher
        .recipes(&item.project)
        .into_iter()
        .find(|recipe| recipe.id == "rebase")
        .ok_or_else(|| {
            EphorError::Command("No recipe named 'rebase' is configured.".to_string())
        })?;
    // The brief carries the situation, not the request to reproduce it: the
    // repository is standing in the conflict this text describes. The replay
    // has already happened — this is what it stopped at — so the recipe's own
    // opening move is cleared rather than run a second time over a working
    // tree that is mid-rebase (§FS-005-dispatch.12).
    let recipe = crate::work::recipe::Recipe {
        brief: format!("{}\n\n{}", recipe.brief, outcome.report()),
        opens_with: None,
        ..recipe
    };
    let opened = dispatcher.dispatch(item, &recipe, picked, false)?;
    dispatcher.save()?;
    Ok(Handed {
        item: item.id.clone(),
        ticket: match &opened {
            crate::work::Outcome::Opened { ticket, .. }
            | crate::work::Outcome::Reopened { ticket, .. } => Some(ticket.clone()),
            _ => None,
        },
        says: opened.describe(),
        notes: dispatcher.notes().iter().map(ToString::to_string).collect(),
    })
}

fn write_report(path: &str, contents: &str) -> Result<()> {
    let path = PathBuf::from(crate::paths::resolve_path(path));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            EphorError::Command(format!("Cannot create {}: {err}", parent.display()))
        })?;
    }
    std::fs::write(&path, contents)
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", path.display())))
}

/// The item by its feed id, out of whatever the last refresh cached.
fn find_item(id: &str) -> Result<Item> {
    let config = load_config()?;
    for project in config.projects.keys() {
        let Some(feed) = cache::load_feed(project)? else {
            continue;
        };
        let found = feed.items().find(|item| item.id == id);
        if let Some(item) = found {
            return Ok(item);
        }
    }
    Err(EphorError::Command(format!(
        "{id} is not in any cached feed — run `ephor refresh` first."
    )))
}
