use ephor::{agents, cli, error, feed, paths, registry, table, update};

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde_json::Value;

use cli::{Cli, Command};
use error::{EphorError, Result};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(EphorError::Registry(msg)) => {
            eprintln!("ERROR: {msg}");
            ExitCode::from(2)
        }
        Err(EphorError::Command(msg)) => {
            eprintln!("ERROR: {msg}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match &cli.command {
        Command::Status(args) => return feed::commands::status(args),
        Command::Feed(args) => return feed::commands::feed(args),
        Command::Refresh(args) => return feed::commands::refresh(args),
        Command::MarkRead(args) => return feed::commands::mark_read(args, cli.all),
        Command::Tui => return feed::tui::run(),
        _ => {}
    }

    let registry_path = cli
        .registry
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(paths::default_registry_path);
    let schema = load_schema(cli.schema.as_deref())?;
    let registry = registry::load_registry(&registry_path, &schema)?;

    match &cli.command {
        Command::List => {
            let projects = registry::select_projects(
                &registry,
                &cli.workspace,
                &cli.tag,
                cli.organization.as_deref(),
            )?;
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
            agents::write_agents_file(&registry, &registry_path, &workspace)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate | Command::EnsureAgents(_) | Command::Update(_) => {
            let workspaces = registry::select_managed_workspaces(
                &registry,
                &cli.workspace,
                cli.all,
                &cli.tag,
                cli.organization.as_deref(),
            )?;
            match &cli.command {
                Command::Validate => {
                    for workspace in &workspaces {
                        let project_type =
                            registry::get_project_type(&registry, &workspace.type_id)?;
                        update::validate_workspace_paths(project_type, workspace)?;
                    }
                    println!(
                        "Validated {} managed workspaces from {}",
                        workspaces.len(),
                        registry_path.display()
                    );
                }
                Command::EnsureAgents(_) => {
                    for workspace in &workspaces {
                        agents::write_agents_file(&registry, &registry_path, workspace)?;
                    }
                }
                Command::Update(args) => {
                    for workspace in &workspaces {
                        update::update_workspace(
                            &registry,
                            &registry_path,
                            workspace,
                            args.debug,
                            args.skip_agents,
                        )?;
                    }
                }
                _ => unreachable!(),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Status(_)
        | Command::Feed(_)
        | Command::Refresh(_)
        | Command::MarkRead(_)
        | Command::Tui => {
            unreachable!("handled above")
        }
    }
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
