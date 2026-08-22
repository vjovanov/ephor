use ephor::{agents, checkout, cli, error, feed, paths, rebase, registry, table, update, work};

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use serde_json::Value;

use cli::{Cli, Command};
use error::{EphorError, Result};

fn main() -> ExitCode {
    // Parsed through the matches rather than straight into the struct, because
    // one thing has to be known before any command runs: whether the reader
    // asked for a machine form. A refusal is an answer too, and under `--json`
    // it belongs on standard output in a shape a program can read
    // (§REQ-002-parity.3).
    let matches = Cli::command().get_matches();
    let json = asked_for_json(&matches);
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };
    match run(cli) {
        Ok(code) => code,
        Err(err) => refused(err, json),
    }
}

/// Whether the command that was invoked was given `--json`, at whatever depth
/// it lives.
///
/// Walked off the matches rather than matched over the command enum: a
/// subcommand added later takes part without anyone remembering to add it, and
/// forgetting is exactly how the answer would go back to being "prose on the
/// error stream and nothing on standard output" for one command
/// (§REQ-002-parity.5).
fn asked_for_json(matches: &ArgMatches) -> bool {
    if matches.try_get_one::<bool>("json").ok().flatten() == Some(&true) {
        return true;
    }
    matches
        .subcommand()
        .is_some_and(|(_, inner)| asked_for_json(inner))
}

/// What a command that could not do what it was asked exits with, and says.
///
/// Under `--json` the sentence lands on standard output as an outcome — the
/// same shape a move that *did* happen prints, with `ok` false — so a program
/// that reads only the reading still learns that it did not happen and why.
/// Printing prose to the error stream and leaving standard output empty read
/// to a script exactly like a move that worked and had nothing to say, which
/// is the degraded surface §GRUND-001-overseer.2 says a script must never be.
/// The prose goes to the error stream either way: under a reading it narrates
/// the run without becoming part of it (§FS-011-command-line.7).
fn refused(err: EphorError, json: bool) -> ExitCode {
    let (code, says) = match err {
        EphorError::Registry(msg) => (2, msg),
        EphorError::Command(msg) => (1, msg),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ephor::api::views::Outcome::refused(says.clone()))
                .unwrap_or_else(|_| "null".to_string())
        );
    }
    eprintln!("ERROR: {says}");
    ExitCode::from(code)
}

/// Print one published schema (§FS-006-project-interface.11). What a release
/// may change is answerable by diffing these, so they are printed verbatim
/// rather than re-rendered.
fn print_schema(name: &str) -> Result<ExitCode> {
    let schema = match name {
        "manifest" => ephor::manifest::MANIFEST_SCHEMA,
        "answer" => ephor::seams::answer::ANSWER_SCHEMA,
        "registry" => registry::EMBEDDED_SCHEMA,
        "forge" => include_str!("../assets/ephor-forge.schema.json"),
        // What every command prints under `--json` (§AR-009-surfaces.3). It is
        // a published schema for the same reason the others are: whoever
        // automated against a reading is owed an answer to "what may change".
        "views" => ephor::api::schema::VIEWS_SCHEMA,
        other => {
            return Err(EphorError::Registry(format!(
                "unknown schema '{other}': expected 'manifest', 'answer', 'registry', \
                 'forge', or 'views'"
            )))
        }
    };
    print!("{schema}");
    Ok(ExitCode::SUCCESS)
}

/// Validate a project's own manifest, given the file or the forest root it
/// sits at.
///
/// It answers a program like every other reading (§REQ-002-parity.3): the flag
/// was honoured on the registry paths of `validate` and dropped on this one, so
/// a workflow that validated a checkout's manifest under `--json` got a line of
/// prose where it expected a document — a surface that is machine-readable in
/// three of its four shapes is not machine-readable.
fn validate_manifest(path: &str, json: bool) -> Result<ExitCode> {
    let path = paths::resolve_path(path);
    let file = if path.is_dir() {
        path.join(ephor::manifest::FILE)
    } else {
        path
    };
    if !file.is_file() {
        return Err(EphorError::Registry(format!(
            "No manifest at {} — a project that places none is fully watchable as it stands.",
            file.display()
        )));
    }
    let text = std::fs::read_to_string(&file)
        .map_err(|err| EphorError::Command(format!("Cannot read {}: {err}", file.display())))?;
    ephor::manifest::parse(&text, &file.display().to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "valid": true,
                "manifest": file.display().to_string(),
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!("Validated {}", file.display());
    Ok(ExitCode::SUCCESS)
}

fn run(cli: Cli) -> Result<ExitCode> {
    // Before anything dispatches: the subcommands below return early and
    // resolve the registry for themselves, several calls deep, so the flag has
    // to be recorded where they will look rather than handed to the one branch
    // that used to read it (§FS-001-forge-interface.5).
    if let Some(path) = cli.registry.as_deref() {
        paths::set_registry_override(PathBuf::from(path));
    }

    match &cli.command {
        Command::Status(args) => return feed::commands::status(args),
        Command::Feed(args) => return feed::commands::feed(args),
        Command::Refresh(args) => return feed::commands::refresh(args),
        Command::MarkRead(args) => return feed::commands::mark_read(args, cli.all),
        Command::Failures(args) => return feed::commands::failures(args),
        Command::Restart(args) => return feed::commands::restart(args),
        Command::Rebase(args) => return rebase::rebase(args),
        Command::Checkout(args) => return checkout::checkout(args),
        Command::Work(args) => return work::commands::work(args),
        // The abilities the screen used to hold alone (§FS-011-command-line).
        // Each opens the same session a key would have (§AR-009-surfaces.2).
        Command::Actions(args) => return ephor::commands::actions(args),
        Command::Branches(args) => return ephor::commands::branches(args),
        Command::Operations(args) => return ephor::commands::operations(args),
        Command::Thread(args) => return ephor::commands::thread(args),
        Command::React(args) => return ephor::commands::react(args),
        Command::Tick(args) => return ephor::commands::tick(args),
        Command::Reply(args) => return ephor::commands::reply(args),
        // A job outlives the interface that started it (§FS-005-dispatch.17), so
        // it is answerable without one — and the supervisor itself is here.
        Command::Job(args) => return ephor::seams::jobs::job(args),
        // Both read the site rather than the registry alone, and the self
        // pass reads neither — so neither loads the registry up front
        // (§FS-010-doctor).
        Command::Capabilities(args) => return ephor::doctor::capabilities(args),
        Command::Doctor(args) => return ephor::doctor::doctor(args),
        Command::Tui => return feed::tui::run(),
        // The schemas are the interface's stability surface, printable without
        // a registry to read (§FS-006-project-interface.11).
        Command::Schema(args) => return print_schema(&args.name),
        // A manifest is validated where it sits, with no registry involved:
        // that is the point of publishing the schema.
        Command::Validate(args) if args.manifest.is_some() => {
            return validate_manifest(args.manifest.as_deref().unwrap(), args.json)
        }
        // A project's checks are the project's, run from its checkout with no
        // registry and no site (§FS-009-shipped-actions.1).
        Command::Check(args) => return ephor::seams::commands::check(args),
        _ => {}
    }

    // Which already accounts for `--registry`, recorded at the top of `run`.
    let registry_path = paths::default_registry_path();
    let schema = load_schema(cli.schema.as_deref())?;
    let registry = registry::load_registry(&registry_path, &schema)?;

    // Loading held it to the published schema, which is all a repository
    // carrying a committed registry can check: the checkouts its rows name
    // are on somebody's machine, not in CI (§FS-009-shipped-actions.1).
    if let Command::Validate(args) = &cli.command {
        if args.schema_only {
            return Ok(validated(
                args.json,
                &registry_path.display().to_string(),
                0,
            ));
        }
    }

    match &cli.command {
        Command::List(args) => {
            let projects = registry::select_projects(
                &registry,
                &cli.workspace,
                &cli.tag,
                cli.organization.as_deref(),
            )?;
            // The rows themselves under `--json`: the registry is what this
            // command is a rendering of, so the machine form is the rows
            // rather than a re-description of them (§REQ-002-parity.3).
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&projects).unwrap_or_else(|_| "[]".to_string())
                );
                return Ok(ExitCode::SUCCESS);
            }
            table::print_project_table(&projects);
            Ok(ExitCode::SUCCESS)
        }
        Command::EnsureAgents(args) if args.project_type.is_some() => {
            let workspace = registry::build_ad_hoc_workspace(
                args.project_type.as_deref().unwrap(),
                args.root.as_deref(),
                &args.display_name,
                &args.vars,
            )?;
            let written = agents::write_agents_file(&registry, &registry_path, &workspace)?;
            Ok(wrote(
                args.json,
                "written",
                vec![serde_json::json!({
                    "workspace": workspace.id,
                    "path": written.display().to_string(),
                })],
            ))
        }
        Command::Validate(_) | Command::EnsureAgents(_) | Command::Update(_) => {
            let workspaces = registry::select_managed_workspaces(
                &registry,
                &cli.workspace,
                cli.all,
                &cli.tag,
                cli.organization.as_deref(),
            )?;
            match &cli.command {
                Command::Validate(args) => {
                    for workspace in &workspaces {
                        let project_type =
                            registry::get_project_type(&registry, &workspace.type_id)?;
                        update::validate_workspace_paths(project_type, workspace)?;
                    }
                    return Ok(validated(
                        args.json,
                        &registry_path.display().to_string(),
                        workspaces.len(),
                    ));
                }
                Command::EnsureAgents(args) => {
                    let mut written = Vec::new();
                    for workspace in &workspaces {
                        let path = agents::write_agents_file(&registry, &registry_path, workspace)?;
                        written.push(serde_json::json!({
                            "workspace": workspace.id,
                            "path": path.display().to_string(),
                        }));
                    }
                    return Ok(wrote(args.json, "written", written));
                }
                Command::Update(args) => {
                    let mut updated = Vec::new();
                    for workspace in &workspaces {
                        update::update_workspace(
                            &registry,
                            &registry_path,
                            workspace,
                            args.debug,
                            args.skip_agents,
                        )?;
                        updated.push(serde_json::json!({
                            "workspace": workspace.id,
                            "root": paths::resolve_path(&workspace.root)
                                .display()
                                .to_string(),
                        }));
                    }
                    return Ok(wrote(args.json, "updated", updated));
                }
                _ => unreachable!(),
            }
        }
        Command::Status(_)
        | Command::Feed(_)
        | Command::Refresh(_)
        | Command::MarkRead(_)
        | Command::Failures(_)
        | Command::Restart(_)
        | Command::Rebase(_)
        | Command::Checkout(_)
        | Command::Work(_)
        | Command::Job(_)
        | Command::Schema(_)
        | Command::Check(_)
        | Command::Capabilities(_)
        | Command::Doctor(_)
        | Command::Actions(_)
        | Command::Branches(_)
        | Command::Operations(_)
        | Command::Thread(_)
        | Command::React(_)
        | Command::Tick(_)
        | Command::Reply(_)
        | Command::Tui => {
            unreachable!("handled above")
        }
    }
}

/// What a command that changed the world on disk changed (§REQ-002-parity.3).
///
/// `ephor update` and `ephor ensure-agents` rewrite files in every managed
/// workspace and used to say nothing at all — success was an exit code and a
/// changed disk, so a program that ran one had to go and look to find out what
/// it had touched. Under `--json` the rows say which workspace and which file;
/// with no flag the silence stands, because that is what a person running it in
/// a loop over forty workspaces asked for.
fn wrote(json: bool, field: &str, rows: Vec<Value>) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "count": rows.len(),
                field: rows,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
    }
    ExitCode::SUCCESS
}

/// What `validate` says it checked (§FS-011-command-line.7). A count of zero
/// with `--schema-only` is not "nothing was checked": it is the registry held
/// to the schema and no checkout looked for (§FS-009-shipped-actions.1).
fn validated(json: bool, registry: &str, workspaces: usize) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "valid": true,
                "registry": registry,
                "workspaces": workspaces,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
        return ExitCode::SUCCESS;
    }
    match workspaces {
        0 => println!("Validated {registry}"),
        count => println!("Validated {count} managed workspaces from {registry}"),
    }
    ExitCode::SUCCESS
}

fn load_schema(schema_arg: Option<&str>) -> Result<Value> {
    let schema_path = schema_arg
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("EPHOR_SCHEMA").map(PathBuf::from));
    match schema_path {
        Some(path) => registry::load_json(&path),
        None => serde_json::from_str(registry::EMBEDDED_SCHEMA)
            .map_err(|err| error::registry_error(format!("Invalid embedded schema: {err}"))),
    }
}
