//! `ephor check`: run a project's own check verbs, from the repository alone
//! (§FS-006-project-interface.5).
//!
//! This is the command a shipped CI step stands on
//! (§FS-009-shipped-actions.1), so it takes nothing a repository does not
//! carry: a forest root, the manifest sitting in it, and the well-known names
//! it may be probed for. No registry, no site configuration, no credentials —
//! a checkout is enough (§REQ-001-boundary.2).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use crate::cli::CheckArgs;
use crate::error::{EphorError, Result};
use crate::manifest::Manifest;
use crate::seams::checks::{self, Bound, Enumeration, Verb};
use crate::seams::summons::{Answer, Mode, Outcome};

/// How long a `--list` run may take. Listing what a project has is a question
/// a command answers immediately or has answered wrong; the verbs themselves
/// are not timed here, because a gate takes as long as it takes and the
/// workflow running it has a timeout that means something.
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

pub fn check(args: &CheckArgs) -> Result<ExitCode> {
    let root = PathBuf::from(args.root.as_deref().unwrap_or("."));
    if !root.is_dir() {
        return Err(EphorError::Command(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    // The manifest is read where it sits, at full trust: the trust switch is a
    // property of a person's registry row, and there is no registry here. A
    // repository running its own checks in its own CI is not a third party to
    // itself (§FS-006-project-interface.2).
    let manifest = Manifest::read(&root, crate::manifest::Trust::Full)?;

    if args.list_features {
        return list_features(&root, manifest.as_ref(), args.json);
    }

    let bound = wanted(&root, manifest.as_ref(), &args.verbs)?;
    let mut failed = Vec::new();
    let mut verbs = Vec::new();
    for verb in &bound {
        // Streamed, not captured: the project's own output — standard error
        // included — is what a CI log is for, and a gate's log does not
        // belong in memory (§AR-002-summons.2). Under `--json` it streams to
        // the error stream instead, because standard output is spoken for by
        // the reading and a build log interleaved with it is a program parsing
        // somebody's compiler warnings (§FS-011-command-line.7).
        let answer = checks::run(
            verb,
            &root,
            Vec::new(),
            args.feature.as_deref(),
            match args.json {
                true => Mode::Aside,
                false => Mode::Interactive,
            },
        )?;
        let passed = match args.json {
            true => noted(verb, &answer, &mut verbs),
            false => report(verb, &answer),
        };
        if !passed {
            failed.push(verb.verb.name());
        }
    }
    // The same account the prose gives, in the shape a workflow acts on: which
    // verb ran, what it came to, and the failures it named. `ephor check
    // --json` printed the project's own output and no JSON at all, so the one
    // command a shipped CI step stands on was the one with no machine form
    // (§FS-009-shipped-actions.1, §REQ-002-parity.3).
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "passed": failed.is_empty(),
                "verbs": verbs,
            }))
            .unwrap_or_else(|_| "null".to_string())
        );
    }
    if failed.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    if !args.json {
        // Named rather than counted: which verb failed is the first thing a
        // person reads a red gate for.
        eprintln!("\nfailed: {}", failed.join(", "));
    }
    Ok(ExitCode::from(1))
}

/// One verb as a row of the reading, and whether it passed. The same three
/// facts `report` puts on a line — what ran, how it ended, what it said — so
/// the prose form cannot know something the machine form does not
/// (§REQ-002-parity.3).
fn noted(bound: &Bound, answer: &Answer, verbs: &mut Vec<serde_json::Value>) -> bool {
    let outcome = match answer.outcome {
        Outcome::Done => "done",
        // Not applicable now, ask again later (§FS-006-project-interface.3) —
        // a verb that parks has not failed, and a CI step that fails on it
        // would be reading the exit code the way this contract exists to stop.
        Outcome::Parked => "parked",
        Outcome::Failed => "failed",
    };
    verbs.push(serde_json::json!({
        "verb": bound.verb.name(),
        "outcome": outcome,
        "summary": checks::summary_of(answer),
        "failures": checks::failures_of(answer)
            .into_iter()
            .map(|failure| serde_json::json!({
                "job": failure.job,
                "trace": failure.trace,
            }))
            .collect::<Vec<_>>(),
    }));
    !matches!(answer.outcome, Outcome::Failed)
}

/// Which verbs to run: what was asked for, or the project's own answer to "am
/// I well" — the aggregate where it declares one, and everything else it
/// declares where it does not (§FS-006-project-interface.5). Running all three
/// unasked would run a project's style pass twice, since the aggregate is
/// defined as everything the project considers a check.
fn wanted(root: &Path, manifest: Option<&Manifest>, asked: &[String]) -> Result<Vec<Bound>> {
    let bind = |verb: Verb| checks::bind(verb, root, manifest, None);
    if !asked.is_empty() {
        let mut bound = Vec::new();
        for name in asked {
            let verb = Verb::all()
                .into_iter()
                .find(|verb| verb.name() == name)
                .ok_or_else(|| {
                    EphorError::Command(format!(
                        "unknown verb '{name}': expected {}",
                        Verb::all().map(|verb| verb.name()).join(", ")
                    ))
                })?;
            // Asked for by name and not there: refused, because a verb a
            // workflow named and ephor skipped is a check nobody ran that
            // nobody was told about.
            bound.push(bind(verb).ok_or_else(|| {
                EphorError::Command(format!(
                    "{} declares no '{name}' verb: nothing at {}/{}, and its manifest binds none",
                    root.display(),
                    root.display(),
                    verb.probed()
                ))
            })?);
        }
        return Ok(bound);
    }
    if let Some(aggregate) = bind(Verb::Check) {
        return Ok(vec![aggregate]);
    }
    let rest: Vec<Bound> = [Verb::Style, Verb::Smoke]
        .into_iter()
        .filter_map(bind)
        .collect();
    if rest.is_empty() {
        return Err(EphorError::Command(format!(
            "{} declares no check verbs: nothing to run. A project says how it is checked \
             with {} at its root, or with `checks` in its {}.",
            root.display(),
            Verb::all().map(|verb| verb.probed()).join(", "),
            crate::manifest::FILE
        )));
    }
    Ok(rest)
}

/// What one verb said, and whether it passed. The captured output is printed
/// on failure only: a green verb's log is noise, and a red one's is the
/// question.
fn report(bound: &Bound, answer: &Answer) -> bool {
    let name = bound.verb.name();
    match answer.outcome {
        Outcome::Done => {
            match checks::summary_of(answer) {
                Some(summary) => println!("✓ {name} — {summary}"),
                None => println!("✓ {name}"),
            }
            true
        }
        // Not applicable now, ask again later (§FS-006-project-interface.3) —
        // a verb that parks has not failed, and a CI step that fails on it
        // would be reading the exit code the way this contract exists to
        // stop.
        Outcome::Parked => {
            println!("· {name} — parked (not applicable now)");
            true
        }
        Outcome::Failed => {
            match checks::summary_of(answer) {
                Some(summary) => eprintln!("✗ {name} — {summary}"),
                None => eprintln!("✗ {}", answer.refusal(name)),
            }
            for failure in checks::failures_of(answer) {
                eprintln!("    {}", failure.job);
                if let Some(trace) = &failure.trace {
                    for line in trace.lines() {
                        eprintln!("      {line}");
                    }
                }
            }
            if let Some(output) = answer.output.as_deref().map(str::trim) {
                if !output.is_empty() {
                    eprintln!("{output}");
                }
            }
            false
        }
    }
}

/// The features a project's smoke enumerates, for a workflow that fans one
/// job out per feature (§FS-006-project-interface.5). A project whose smoke
/// is one opaque verb enumerates nothing, which is a complete answer and
/// prints an empty list rather than failing.
fn list_features(root: &Path, manifest: Option<&Manifest>, json: bool) -> Result<ExitCode> {
    let features = match checks::enumeration(manifest) {
        Enumeration::Listed(features) => features,
        Enumeration::Opaque => Vec::new(),
        Enumeration::Ask => match checks::bind(Verb::Smoke, root, manifest, None) {
            Some(smoke) => {
                let answer = checks::run(
                    &smoke,
                    root,
                    Vec::new(),
                    Some("--list"),
                    Mode::Captured(LIST_TIMEOUT),
                )?;
                match answer.outcome {
                    Outcome::Done => checks::features_of(&answer),
                    // A project that says its smoke lists and then cannot is
                    // broken in a way a matrix would hide as "no features".
                    _ => {
                        return Err(EphorError::Command(format!(
                            "{} says its smoke enumerates features, but listing them {}",
                            root.display(),
                            answer.refusal("smoke --list")
                        )))
                    }
                }
            }
            None => Vec::new(),
        },
    };
    if json {
        println!(
            "{}",
            serde_json::to_string(&features.iter().map(|f| &f.id).collect::<Vec<_>>())
                .expect("a list of strings serializes")
        );
        return Ok(ExitCode::SUCCESS);
    }
    for feature in &features {
        match &feature.description {
            Some(description) => println!("{}\t{description}", feature.id),
            None => println!("{}", feature.id),
        }
    }
    Ok(ExitCode::SUCCESS)
}
