//! Gate verbs: how a project's CI is asked what it is doing
//! (§FS-006-project-interface.6).
//!
//! How to ask is project truth — the same for every person who works on it —
//! so its home is the manifest, with site configuration overriding where
//! credentials or variants demand. Three verbs: **status** answers the gate's
//! counts per repository of the forest, **failures** answers what actually
//! failed (the expensive question, asked on demand), and **restart** re-runs
//! the failing gate and everything downstream of it, committing nothing.
//!
//! A forge-hosted gate needs no manifest at all: the provider's own gate
//! capability is the shipped default binding, and nothing above the seam can
//! tell the difference between that and a project that binds three commands.

use std::time::Duration;

use crate::error::Result;
use crate::feed::gate::Gate;
use crate::manifest::{self, Manifest};
use crate::seams::answer::Failure;
use crate::seams::summons::{Answer, Mode, Outcome, Place, Site, Summons};

/// The three verbs a gate answers (§FS-006-project-interface.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// What the gate is doing, per repository of the forest.
    Status,
    /// What actually failed — the expensive question, asked on demand.
    Failures,
    /// Re-run the failing gate and every gate downstream of it
    /// (§FS-005-dispatch.11).
    Restart,
}

impl Verb {
    pub fn name(self) -> &'static str {
        match self {
            Verb::Status => "status",
            Verb::Failures => "failures",
            Verb::Restart => "restart",
        }
    }

    pub fn all() -> [Verb; 3] {
        [Verb::Status, Verb::Failures, Verb::Restart]
    }
}

/// What fills one gate verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// A command the project or the person bound.
    Command {
        command: String,
        cwd: Option<String>,
    },
    /// The forge the matter came from answers it — the shipped default, and
    /// why a forge-hosted gate needs no manifest at all
    /// (§REQ-001-boundary.1).
    Forge,
}

impl Bound {
    /// The summons that asks it, where a command fills it. The forge default
    /// is not a summons: it is answered through the provider interface.
    pub fn summons(&self, verb: Verb, dossier: Vec<(String, String)>) -> Result<Option<Summons>> {
        let Bound::Command { command, cwd } = self else {
            return Ok(None);
        };
        let place = match cwd {
            Some(spec) => Place::parse(spec)?,
            None => Place::Root,
        };
        Ok(Some(
            Summons::new(format!("gate.{}", verb.name()), command)
                .at(place)
                .carrying(dossier),
        ))
    }
}

/// What answers this verb: site configuration over manifest over the forge
/// (§FS-006-project-interface.1). `forge` says whether the matter's source
/// reports a gate of its own — where it does, that is the shipped default and
/// no project has to write anything down.
pub fn bind(
    verb: Verb,
    manifest: Option<&Manifest>,
    site: Option<&str>,
    forge: bool,
) -> Option<Bound> {
    let from_site = site.map(|command| Bound::Command {
        command: command.to_string(),
        cwd: None,
    });
    let from_manifest = manifest
        .and_then(|manifest| declared(verb, manifest))
        .map(|binding| Bound::Command {
            command: binding.command().to_string(),
            cwd: binding.cwd().map(String::from),
        });
    manifest::resolve(from_site, from_manifest, forge.then_some(Bound::Forge))
}

fn declared(verb: Verb, manifest: &Manifest) -> Option<manifest::Binding> {
    match verb {
        Verb::Status => manifest.gate.status.clone(),
        Verb::Failures => manifest.gate.failures.clone(),
        Verb::Restart => manifest.gate.restart.clone(),
    }
}

/// What a `status` answer said the gate is doing. The per-repository
/// breakdown comes from the envelope's `gate` (§AR-004-forest.1): one change
/// may gate across a tree, and a single number cannot say which repository
/// went red.
pub fn status_of(answer: &Answer) -> Option<Gate> {
    let normalized = answer.answer.as_ref()?;
    let event = normalized
        .events
        .iter()
        .find(|event| event.kind == crate::seams::answer::GATE)?;
    let gate = event.gate.as_ref()?;
    let repos = gate
        .repos
        .iter()
        .map(|repo| crate::feed::gate::RepoGate {
            repo: repo.repo.clone(),
            passed: repo.passed.unwrap_or(0),
            failed: repo.failed.unwrap_or(0),
            running: repo.running.unwrap_or(0),
        })
        .collect();
    Some(Gate {
        repos,
        blocked: gate.blocked.unwrap_or(false),
        blockers: gate.blockers.clone(),
    })
}

/// What a `failures` answer said actually failed.
pub fn failures_of(answer: &Answer) -> Vec<Failure> {
    answer
        .answer
        .as_ref()
        .map(|normalized| {
            normalized
                .events
                .iter()
                .flat_map(|event| event.failures.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// How a restart went (§FS-005-dispatch.11). Nothing is committed by it — the
/// change was never the problem — so the only answers are "asked for",
/// "still running, ask again later", and "refused".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restarted {
    /// The gate was asked to run again.
    Asked,
    /// Still running: not a failure, and not a reason to ask again now.
    Parked,
    /// It would not, and this is what it said.
    Refused(String),
}

/// Read a restart's answer. Exit 75 means "still running, ask again later",
/// which is the one outcome a retry loop must not treat as a failure.
pub fn restarted(answer: &Answer) -> Restarted {
    match answer.outcome {
        Outcome::Done => Restarted::Asked,
        Outcome::Parked => Restarted::Parked,
        Outcome::Failed => Restarted::Refused(answer.refusal("gate.restart")),
    }
}

/// How many restarts of one item are worth trying before the infrastructure
/// itself is the thing that is wrong (§FS-005-dispatch.11). Past this the work
/// stops for a person: an unhealthy runner pool answers every restart the same
/// way, and a loop that never stops never says why.
pub const RESTART_LIMIT: usize = 3;

/// Whether another restart is worth asking for.
pub fn may_restart(already: usize) -> bool {
    already < RESTART_LIMIT
}

/// Ask one gate verb.
///
/// The site is the caller's, not the project root flattened: a gate is asked
/// about one change, and a verb that says `cwd: workspace` means the branch
/// workspace that change resolves to (§FS-006-project-interface.3). Passing a
/// rootless site here silently ran every such verb at the forest root instead.
pub fn run(
    bound: &Bound,
    verb: Verb,
    site: &Site,
    dossier: Vec<(String, String)>,
    timeout: Duration,
) -> Result<Option<Answer>> {
    let Some(summons) = bound.summons(verb, dossier)? else {
        return Ok(None);
    };
    crate::seams::summons::run(&summons, site, Mode::Captured(timeout)).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest;

    fn manifest_of(text: &str) -> Manifest {
        manifest::parse(text, "ephor.json").unwrap()
    }

    /// A forge-hosted gate needs no manifest at all
    /// (§FS-006-project-interface.6).
    #[test]
    fn the_forge_answers_where_nothing_else_is_bound() {
        assert_eq!(bind(Verb::Status, None, None, true), Some(Bound::Forge));
        // And where the forge reports no gate either, nothing does — which is
        // the *gated* rung not holding, not an error.
        assert_eq!(bind(Verb::Status, None, None, false), None);
    }

    #[test]
    fn a_project_with_an_internal_gate_binds_three_commands() {
        let declared = manifest_of(
            r#"{"ci": {"status": "./gate-status.sh",
                       "failures": {"command": "./gate-failures.sh", "cwd": "repo:ce"},
                       "restart": "./gate-restart.sh"}}"#,
        );
        // Bound commands outrank the forge, which is what lets an internal
        // gate be indistinguishable from a hosted one above the seam.
        assert_eq!(
            bind(Verb::Status, Some(&declared), None, true),
            Some(Bound::Command {
                command: "./gate-status.sh".to_string(),
                cwd: None
            })
        );
        assert_eq!(
            bind(Verb::Failures, Some(&declared), None, true),
            Some(Bound::Command {
                command: "./gate-failures.sh".to_string(),
                cwd: Some("repo:ce".to_string())
            })
        );
        // Site configuration overrides where credentials or variants demand.
        assert_eq!(
            bind(Verb::Restart, Some(&declared), Some("./mine.sh"), true),
            Some(Bound::Command {
                command: "./mine.sh".to_string(),
                cwd: None
            })
        );
    }

    #[test]
    fn the_forge_default_is_answered_through_the_provider_not_by_spawning() {
        assert!(Bound::Forge
            .summons(Verb::Status, Vec::new())
            .unwrap()
            .is_none());
        let bound = Bound::Command {
            command: "./gate.sh".to_string(),
            cwd: None,
        };
        let summons = bound.summons(Verb::Status, Vec::new()).unwrap().unwrap();
        assert_eq!(summons.verb, "gate.status");
        assert_eq!(summons.place, Place::Root);
    }

    fn run_stub(script: &str) -> Answer {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("gate.sh");
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let bound = Bound::Command {
            command: "./gate.sh".to_string(),
            cwd: None,
        };
        run(
            &bound,
            Verb::Status,
            &Site::root(tmp.path()),
            Vec::new(),
            Duration::from_secs(10),
        )
        .unwrap()
        .unwrap()
    }

    /// A verb that says where it runs is run there: a gate asked about one
    /// change resolves the workspace that change is in, exactly as any other
    /// summons does (§FS-006-project-interface.3).
    #[test]
    fn a_gate_verb_runs_in_the_matters_workspace_where_it_asks_for_one() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("you-ABC-42");
        std::fs::create_dir(&workspace).unwrap();
        let bound = Bound::Command {
            command: "pwd".to_string(),
            cwd: Some("workspace".to_string()),
        };
        let answer = run(
            &bound,
            Verb::Status,
            &Site::workspace(tmp.path(), &workspace),
            Vec::new(),
            Duration::from_secs(10),
        )
        .unwrap()
        .unwrap();
        let printed = answer.output.unwrap();
        assert!(printed.trim().ends_with("you-ABC-42"), "{printed}");
    }

    /// One change may gate across a tree, so what comes back is per
    /// repository and stays that way (§AR-004-forest.1).
    #[test]
    fn a_status_answer_carries_the_breakdown_per_repository() {
        let answer = run_stub(
            "#!/bin/sh\ncat > \"$EPHOR_ANSWER\" <<'JSON'\n\
             {\"v\":1,\"gate\":{\"repos\":[\
             {\"repo\":\"acme/ce\",\"passed\":3,\"failed\":1},\
             {\"repo\":\"acme/ee\",\"passed\":2,\"running\":1}],\
             \"blocked\":true,\"blockers\":[\"needs an approval\"]}}\nJSON\n",
        );
        let gate = status_of(&answer).expect("the gate answered");
        assert_eq!(gate.repos.len(), 2);
        assert_eq!(gate.failed(), 1);
        assert_eq!(gate.running(), 1);
        assert!(gate.is_red());
        assert_eq!(gate.blockers, vec!["needs an approval"]);
        // The breakdown survives: a single number could not say which
        // repository went red.
        assert!(gate.breakdown().contains("acme/ce"));
    }

    #[test]
    fn a_failures_answer_says_what_actually_failed() {
        let answer = run_stub(
            "#!/bin/sh\ncat > \"$EPHOR_ANSWER\" <<'JSON'\n\
             {\"v\":1,\"failures\":[{\"job\":\"build 4711\",\"repo\":\"acme/ce\",\
             \"trace\":\"boom\"}]}\nJSON\nexit 1\n",
        );
        let failures = failures_of(&answer);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].job, "build 4711");
        assert_eq!(failures[0].trace.as_deref(), Some("boom"));
    }

    /// A gate still running is not a failed restart, and a loop that treated
    /// it as one would spend a retry to learn nothing
    /// (§FS-005-dispatch.11).
    #[test]
    fn a_restart_that_is_still_running_is_parked_rather_than_failed() {
        assert_eq!(
            restarted(&run_stub("#!/bin/sh\nexit 0\n")),
            Restarted::Asked
        );
        assert_eq!(
            restarted(&run_stub("#!/bin/sh\nexit 75\n")),
            Restarted::Parked
        );
        match restarted(&run_stub("#!/bin/sh\nexit 4\n")) {
            Restarted::Refused(why) => assert!(why.contains("failed (4)"), "{why}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// Restarting is bounded: past a few, the infrastructure is what is wrong
    /// and no amount of retrying is the fix (§FS-005-dispatch.11).
    #[test]
    fn restarting_stops_for_a_person_rather_than_going_on_forever() {
        assert!(may_restart(0));
        assert!(may_restart(RESTART_LIMIT - 1));
        assert!(!may_restart(RESTART_LIMIT));
        assert!(!may_restart(RESTART_LIMIT + 5));
    }
}
