//! `ephor doctor` and `ephor capabilities` — is this still working
//! (§FS-010-doctor)?
//!
//! Everything that makes the watch untrue is quiet: a credential that
//! expired, an extension that left `PATH`, a checkout somebody deleted. None
//! of them announces itself; each one makes a section of the feed empty, which
//! is the one thing an empty section must never mean
//! (§FS-001-forge-interface.6). So ephor can be asked.
//!
//! This module composes and never judges (§FS-010-doctor.1). What a project
//! can do is [`CapabilitySet`]'s answer and why a rung is missing is its
//! sentence; what a source did is the refresh's answer with its own
//! unreachable split. A second opinion here would drift from the one the menu
//! refuses with, and a reader holding two answers has none.

mod scratch;

use std::process::ExitCode;

use serde_json::{json, Value};

use crate::capabilities::{Bindings, CapabilitySet, Rung};
use crate::cli::{CapabilitiesArgs, DoctorArgs};
use crate::error::Result;
use crate::feed::cache::ProjectFeed;
use crate::feed::config::{load_config, StatusConfig};
use crate::feed::render::Style;

/// How a run came out, in the vocabulary the exit code speaks
/// (§FS-010-doctor.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Health {
    /// Everything asked answered, and every rung a project needs holds.
    Well,
    /// A rung is missing or a source was lost — the condition a person acts
    /// on.
    Degraded,
    /// Nothing on the site could be reached at all.
    Unreachable,
    /// ephor itself is wrong, which is the one answer waiting does not improve.
    Broken,
}

impl Health {
    pub fn code(self) -> ExitCode {
        match self {
            Health::Well => ExitCode::SUCCESS,
            Health::Broken => ExitCode::from(1),
            Health::Unreachable => ExitCode::from(3),
            Health::Degraded => ExitCode::from(4),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Health::Well => "well",
            Health::Degraded => "degraded",
            Health::Unreachable => "unreachable",
            Health::Broken => "broken",
        }
    }
}

/// The rungs whose absence is a fault rather than a choice
/// (§FS-010-doctor.1). Every other rung is something a project may simply not
/// be, and the ladder says so without anybody having to act.
const FAULTS: [Rung; 2] = [Rung::Observable, Rung::Placed];

/// One project's answer: its ladder, and what its sources did.
struct Diagnosis {
    project: String,
    /// Rungs held, in ladder order.
    held: Vec<Rung>,
    /// Rungs missing, each with the sentence that already says why.
    missing: Vec<(Rung, String)>,
    /// Sources that did not answer: name, what they said, and whether it is a
    /// network to wait out rather than a configuration to fix
    /// (§FS-001-forge-interface.6).
    silent: Vec<(String, String, bool)>,
    /// How many sources answered, and how many were asked. None where no
    /// refresh has produced a cache for this project at all.
    answering: Option<usize>,
    asked: Option<usize>,
    /// How many the configuration names — which is not the same number: a
    /// ticket store is probed and a shared source is site-level, and both are
    /// sources of this project's matters.
    configured: usize,
}

impl Diagnosis {
    /// A project is well when every source answered and every rung it cannot
    /// have chosen to be without holds.
    ///
    /// Most rungs are choices: a project with no runtime bound, no gate verbs
    /// and no ticket store is a project somebody watches and does not hand
    /// work to, which is the ladder working as intended
    /// (§FS-006-project-interface.10). Two are not. *observable* failing means
    /// nothing is watching it at all, and *placed* failing means the registry
    /// names a root that is not there — a row nobody updated after a directory
    /// moved, which is exactly the quiet rot a diagnosis exists to find.
    fn health(&self) -> Health {
        if self.configured > 0 && self.answering == Some(0) {
            return Health::Unreachable;
        }
        if !self.silent.is_empty() || FAULTS.iter().any(|rung| !self.holds(*rung)) {
            return Health::Degraded;
        }
        Health::Well
    }

    fn holds(&self, rung: Rung) -> bool {
        self.held.contains(&rung)
    }

    /// The missing rungs a reader has to act on, and the ones that are merely
    /// what this project is.
    fn faults(&self) -> (Vec<&(Rung, String)>, Vec<&(Rung, String)>) {
        self.missing
            .iter()
            .partition(|(rung, _)| FAULTS.contains(rung))
    }

    fn to_json(&self) -> Value {
        json!({
            "project": self.project,
            "health": self.health().label(),
            "sources": {
                "configured": self.configured,
                "asked": self.asked,
                "answering": self.answering,
            },
            "held": self.held.iter().map(|rung| rung.name()).collect::<Vec<_>>(),
            "missing": self
                .missing
                .iter()
                .map(|(rung, why)| json!({ "rung": rung.name(), "why": why }))
                .collect::<Vec<_>>(),
            "silent": self
                .silent
                .iter()
                .map(|(name, why, unreachable)| {
                    json!({ "source": name, "why": why, "unreachable": unreachable })
                })
                .collect::<Vec<_>>(),
        })
    }
}

/// Resolve one project's ladder from what is on disk and what the last refresh
/// left behind.
///
/// The judgement is [`CapabilitySet::resolve`]'s, once, exactly as the inbox
/// asks it (§FS-010-doctor.1) — this only hands it what it needs.
fn diagnose(
    registry_doc: &Value,
    config: &StatusConfig,
    project: &str,
    feed: Option<&ProjectFeed>,
) -> Diagnosis {
    let placement = crate::branches::Placement::load(registry_doc, project);
    let manifest = placement
        .as_ref()
        .and_then(crate::branches::Placement::manifest);
    let project_config = config.projects.get(project);
    let configured = project_config.map(|p| p.providers.len()).unwrap_or(0);
    let answering = feed.and_then(ProjectFeed::answering);
    let asked = feed.and_then(ProjectFeed::asked);

    let bindings = Bindings {
        sources: configured,
        answering,
        checkout: project_config
            .and_then(|p| p.checkout.as_ref())
            .map(|checkout| checkout.command.as_str()),
        runner: Some(crate::work::runtime::RUNNER),
        gate_reported: feed.is_some_and(ProjectFeed::reports_a_gate),
        manifest: manifest.as_ref(),
    };
    let set = CapabilitySet::resolve(project, placement.as_ref(), &bindings);

    let mut missing = Vec::new();
    for rung in Rung::all() {
        if let Some(why) = set.reason(rung) {
            missing.push((rung, why.to_string()));
        }
    }
    Diagnosis {
        project: project.to_string(),
        held: set.held(),
        missing,
        silent: feed
            .map(|feed| {
                feed.silent()
                    .into_iter()
                    .map(|(name, why, unreachable)| {
                        (name.to_string(), why.to_string(), unreachable)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        answering,
        asked,
        configured,
    }
}

/// The projects to look at: the one named, or everything configured.
fn selected(config: &StatusConfig, project: Option<&str>) -> Result<Vec<String>> {
    match project {
        Some(project) => {
            if !config.projects.contains_key(project) {
                return Err(crate::error::registry_error(format!(
                    "Project '{project}' has no feed configuration in {} (configured: {}).",
                    crate::feed::config::config_path().display(),
                    config
                        .projects
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            Ok(vec![project.to_string()])
        }
        None => Ok(config.projects.keys().cloned().collect()),
    }
}

/// `ephor capabilities` — one project's ladder, or every project's
/// (§FS-010-doctor.2). Reads what is on disk and the last refresh; asks
/// nothing of any forge, so it costs a few stat calls.
pub fn capabilities(args: &CapabilitiesArgs) -> Result<ExitCode> {
    let config = load_config()?;
    let registry_doc = crate::feed::commands::load_registry_doc()?;
    let style = Style::detect();

    let mut rows = Vec::new();
    for project in selected(&config, args.project.as_deref())? {
        let feed = crate::feed::cache::load_feed(&project)?;
        rows.push(diagnose(&registry_doc, &config, &project, feed.as_ref()));
    }

    if args.json {
        let out: Vec<Value> = rows.iter().map(Diagnosis::to_json).collect();
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return Ok(ExitCode::SUCCESS);
    }
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            println!();
        }
        render_ladder(row, &style);
    }
    Ok(ExitCode::SUCCESS)
}

/// One project's rungs, held first and then what is missing with its reason.
fn render_ladder(row: &Diagnosis, style: &Style) {
    println!("{}", style.bold(&row.project));
    let held: Vec<&str> = row.held.iter().map(|rung| rung.name()).collect();
    if held.is_empty() {
        println!("  {}", style.dim("holds nothing"));
    } else {
        println!("  ✓ {}", held.join(", "));
    }
    for (rung, why) in &row.missing {
        println!("  ✗ {:<18} {}", rung.name(), style.dim(why));
    }
}

/// `ephor doctor` — the whole site, then ephor itself (§FS-010-doctor.3).
pub fn doctor(args: &DoctorArgs) -> Result<ExitCode> {
    let style = Style::detect();
    let mut worst = Health::Well;

    if !args.self_only {
        worst = worst.max(site(args, &style)?);
    }
    if !args.skip_self {
        if !args.self_only && !args.json {
            println!();
        }
        let report = scratch::run();
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report.to_json()).unwrap()
            );
        } else {
            render_self(&report, &style);
        }
        if !report.passed() {
            worst = Health::Broken;
        }
    }

    if !args.json {
        println!();
        println!("{}", summary(worst, &style));
    }
    Ok(worst.code())
}

/// The site pass: refresh every configured source, then read each project's
/// ladder (§FS-010-doctor.3). Refreshed rather than cached, because a cached
/// answer cannot say whether a source still answers.
fn site(args: &DoctorArgs, style: &Style) -> Result<Health> {
    let config = load_config()?;
    let registry_doc = crate::feed::commands::load_registry_doc()?;
    let projects = selected(&config, args.project.as_deref())?;

    // Ask the world before reading the table: the *observable* rung is about
    // sources answering, and the answer has to be this run's.
    for project in &projects {
        let Some(project_config) = config.projects.get(project) else {
            continue;
        };
        // A refresh that could not run at all is the registry being wrong,
        // which the ladder reports per project rather than ending the run.
        let _ = crate::feed::refresh::refresh_project(
            &registry_doc,
            project,
            project_config,
            &config.defaults,
        );
    }
    if args.project.is_none() {
        let _ = crate::feed::refresh::refresh_shared(&registry_doc, &config);
    }

    let mut rows = Vec::new();
    for project in projects {
        let feed = crate::feed::cache::load_feed(&project)?;
        rows.push(diagnose(&registry_doc, &config, &project, feed.as_ref()));
    }

    let worst = rows
        .iter()
        .map(Diagnosis::health)
        .max()
        .unwrap_or(Health::Well);

    if args.json {
        let out: Vec<Value> = rows.iter().map(Diagnosis::to_json).collect();
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return Ok(worst);
    }

    println!("{}", style.bold("The site"));
    if rows.is_empty() {
        println!(
            "  {}",
            style.dim("no project is configured — nothing to watch")
        );
    }
    for row in &rows {
        render_project(row, style);
    }
    Ok(worst)
}

/// One project's line, and the detail only a reader with a problem wants: a
/// project that is entirely well says so in one line (§FS-010-doctor.3).
fn render_project(row: &Diagnosis, style: &Style) {
    let health = row.health();
    let mark = match health {
        Health::Well => "✓",
        _ => "✗",
    };
    println!(
        "  {mark} {:<24} {}",
        row.project,
        style.dim(&format!(
            "{}/{} source(s) answering · {}",
            row.answering
                .map(|count| count.to_string())
                .unwrap_or_else(|| "?".to_string()),
            // Nothing was asked yet, so the configuration is the only number
            // there is to show.
            row.asked
                .map(|count| count.to_string())
                .unwrap_or_else(|| row.configured.to_string()),
            row.held
                .iter()
                .map(|rung| rung.name())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    );
    // What did not answer, and whether it is a network to wait out or a
    // configuration to go and fix (§FS-001-forge-interface.6).
    for (name, why, unreachable) in &row.silent {
        let kind = if *unreachable {
            "unreachable"
        } else {
            "failed"
        };
        println!("      {} {name}: {kind} — {why}", style.red("✗"));
    }
    let (faults, choices) = row.faults();
    // What has to be acted on leads, in its own mark.
    for (rung, why) in faults {
        println!("      {} {:<18} {why}", style.red("✗"), rung.name());
    }
    // And what the project simply is not, shown only where something is
    // already wrong — on a well project it is noise, and on a degraded one it
    // is the context for what went wrong.
    if health != Health::Well {
        for (rung, why) in choices {
            println!(
                "      {} {:<18} {}",
                style.dim("·"),
                rung.name(),
                style.dim(why)
            );
        }
    }
}

fn render_self(report: &scratch::Report, style: &Style) {
    println!("{}", style.bold("ephor itself"));
    for check in &report.checks {
        match &check.failure {
            None => println!("  ✓ {}", check.name),
            Some(why) => println!("  {} {} — {why}", style.red("✗"), check.name),
        }
    }
}

fn summary(health: Health, style: &Style) -> String {
    match health {
        Health::Well => style.bold("well — everything asked answered"),
        Health::Degraded => style.bold("degraded — see the lines marked ✗ above"),
        Health::Unreachable => style.bold("unreachable — no source could be reached at all"),
        Health::Broken => style.bold("broken — ephor itself failed its own checks"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnosis(configured: usize, answering: Option<usize>, missing: Vec<Rung>) -> Diagnosis {
        let asked = answering.map(|_| configured);
        Diagnosis {
            project: "widget".to_string(),
            held: Rung::all()
                .into_iter()
                .filter(|rung| !missing.contains(rung))
                .collect(),
            missing: missing
                .into_iter()
                .map(|rung| (rung, "because".to_string()))
                .collect(),
            silent: Vec::new(),
            answering,
            asked,
            configured,
        }
    }

    /// Most rungs are choices: a project nobody bound a runtime to is not
    /// broken, it is watched and not worked (§FS-006-project-interface.10).
    #[test]
    fn a_rung_a_project_may_simply_not_have_is_not_a_fault() {
        assert_eq!(diagnosis(2, Some(2), vec![]).health(), Health::Well);
        assert_eq!(
            diagnosis(
                2,
                Some(2),
                vec![
                    Rung::Workable,
                    Rung::Gated,
                    Rung::Ticketed,
                    Rung::Checkable,
                    Rung::BranchAddressable,
                    Rung::CheckoutAble,
                ]
            )
            .health(),
            Health::Well
        );
    }

    /// Two are not choices. Nothing watching it, and a registry row naming a
    /// root that is not there — a row nobody updated after a directory moved
    /// is exactly the quiet rot a diagnosis exists to find (§FS-010-doctor).
    #[test]
    fn nothing_watching_it_and_a_root_that_is_gone_are_faults() {
        assert_eq!(
            diagnosis(2, Some(2), vec![Rung::Observable]).health(),
            Health::Degraded
        );
        assert_eq!(
            diagnosis(2, Some(2), vec![Rung::Placed]).health(),
            Health::Degraded
        );
        // And they are the ones a reader is shown first.
        let row = diagnosis(2, Some(2), vec![Rung::Placed, Rung::Workable]);
        let (faults, choices) = row.faults();
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].0, Rung::Placed);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].0, Rung::Workable);
    }

    /// Every source silent is its own condition, not a worse degrade: it is
    /// the answer that says "the network, not your configuration"
    /// (§FS-001-forge-interface.6).
    #[test]
    fn nothing_answering_is_unreachable_rather_than_degraded() {
        assert_eq!(diagnosis(2, Some(0), vec![]).health(), Health::Unreachable);
        assert_eq!(diagnosis(2, Some(1), vec![]).health(), Health::Well);
        // A project with no source configured is not unreachable; it simply
        // is not watched, which the ladder says in its own words.
        assert_eq!(
            diagnosis(0, None, vec![Rung::Observable]).health(),
            Health::Degraded
        );
    }

    /// A source that failed makes the project degraded even where every rung
    /// holds: the rung speaks for the last refresh and this one just failed.
    #[test]
    fn a_lost_source_is_degraded_however_the_ladder_reads() {
        let mut row = diagnosis(2, Some(1), vec![]);
        row.silent = vec![("mail".to_string(), "no credential".to_string(), false)];
        assert_eq!(row.health(), Health::Degraded);
    }

    /// The codes are the manual's, and the worst one wins
    /// (§FS-010-doctor.5).
    #[test]
    fn the_exit_code_is_the_worst_thing_found() {
        assert!(Health::Broken > Health::Unreachable);
        assert!(Health::Unreachable > Health::Degraded);
        assert!(Health::Degraded > Health::Well);
    }
}
