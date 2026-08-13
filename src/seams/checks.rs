//! Check verbs: how a project says whether it is well
//! (§FS-006-project-interface.5).
//!
//! Three well-known names probed at the forest root — `./check.sh` the
//! aggregate, `./check-style.sh` the fast style pass, `./smoke-test.sh` the
//! smoke — or the same three declared in the manifest under whatever paths the
//! project prefers, or bound in site configuration. Each is self-contained: a
//! smoke test that needs a build performs its build, because how a project
//! builds is the project's knowledge and stays there.
//!
//! **Which** verbs run, and in what order, is not decided here: that is policy
//! above the interface, sequenced from configuration one summons at a time.
//! This module answers only "what fills this verb, and what did it say".

use std::path::Path;

use crate::error::Result;
use crate::manifest::{self, Manifest};
use crate::seams::answer::{Failure, Feature, Normalized};
use crate::seams::summons::{Answer, Mode, Place, Site, Summons};

/// The three verbs a project may fill (§FS-006-project-interface.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// The aggregate: everything the project considers a check.
    Check,
    /// The fast style pass.
    Style,
    /// The smoke, which may enumerate features.
    Smoke,
}

impl Verb {
    pub fn name(self) -> &'static str {
        match self {
            Verb::Check => "check",
            Verb::Style => "style",
            Verb::Smoke => "smoke",
        }
    }

    /// The well-known name probed at the forest root. Conventions are probed
    /// in the checkout — a name a project carries for its own sake
    /// (§REQ-001-boundary.2).
    pub fn probed(self) -> &'static str {
        match self {
            Verb::Check => "check.sh",
            Verb::Style => "check-style.sh",
            Verb::Smoke => "smoke-test.sh",
        }
    }

    pub fn all() -> [Verb; 3] {
        [Verb::Check, Verb::Style, Verb::Smoke]
    }
}

/// What fills one verb, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub verb: Verb,
    /// The command line, run via `sh -c` in the resolved place.
    pub command: String,
    /// `root` or `repo:<name>`, as the binding spells it.
    pub cwd: Option<String>,
}

impl Bound {
    /// The summons that runs it, carrying the dossier a caller hands over.
    pub fn summons(&self, dossier: Vec<(String, String)>) -> Result<Summons> {
        let place = match &self.cwd {
            Some(spec) => Place::parse(spec)?,
            None => Place::Root,
        };
        Ok(
            Summons::new(format!("check.{}", self.verb.name()), &self.command)
                .at(place)
                .carrying(dossier),
        )
    }
}

/// What fills a verb for this project: site configuration over manifest over
/// probe (§FS-006-project-interface.1, §REQ-001-boundary.2). None where
/// nothing fills it, which is a complete answer — a project with no checks is
/// still watched, and the *checkable* rung simply does not hold
/// (§FS-006-project-interface.10).
pub fn bind(
    verb: Verb,
    root: &Path,
    manifest: Option<&Manifest>,
    site: Option<&str>,
) -> Option<Bound> {
    let from_site = site.map(|command| Bound {
        verb,
        command: command.to_string(),
        cwd: None,
    });
    let from_manifest = manifest
        .and_then(|manifest| declared(verb, manifest))
        .map(|binding| Bound {
            verb,
            command: binding.command().to_string(),
            cwd: binding.cwd().map(String::from),
        });
    let probed = root.join(verb.probed()).is_file().then(|| Bound {
        verb,
        command: format!("./{}", verb.probed()),
        cwd: None,
    });
    manifest::resolve(from_site, from_manifest, probed)
}

/// What the manifest declares for one verb. Smoke is an object because it may
/// also say how its features are enumerated.
fn declared(verb: Verb, manifest: &Manifest) -> Option<manifest::Binding> {
    match verb {
        Verb::Check => manifest.checks.check.clone(),
        Verb::Style => manifest.checks.style.clone(),
        Verb::Smoke => manifest
            .checks
            .smoke
            .as_ref()
            .and_then(|smoke| serde_json::from_value(smoke.clone()).ok()),
    }
}

/// How a project's smoke says which features it has
/// (§FS-006-project-interface.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enumeration {
    /// The manifest listed them outright.
    Listed(Vec<Feature>),
    /// Ask the command: `--list` prints one id per line, or an envelope.
    Ask,
    /// Smoke is one opaque verb, which is a complete implementation.
    Opaque,
}

/// How this project's smoke enumerates, if it does.
pub fn enumeration(manifest: Option<&Manifest>) -> Enumeration {
    let Some(smoke) = manifest.and_then(|manifest| manifest.checks.smoke.as_ref()) else {
        return Enumeration::Opaque;
    };
    match smoke.get("features") {
        Some(serde_json::Value::String(word)) if word == "list" => Enumeration::Ask,
        Some(features @ serde_json::Value::Array(_)) => {
            match serde_json::from_value::<Vec<Feature>>(features.clone()) {
                Ok(features) => Enumeration::Listed(features),
                Err(_) => Enumeration::Opaque,
            }
        }
        _ => Enumeration::Opaque,
    }
}

/// The features a `--list` run reported. An envelope answer is read first
/// (§FS-006-project-interface.4); a command that only prints one id per line
/// is read that way, because a list of names is a complete answer and asking
/// for JSON would be ephor requiring an artifact.
pub fn features_of(answer: &Answer) -> Vec<Feature> {
    if let Some(Normalized { facts, .. }) = &answer.answer {
        if !facts.features.is_empty() {
            return facts.features.clone();
        }
    }
    answer
        .output
        .as_deref()
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|id| Feature {
            id: id.to_string(),
            description: None,
            paths: Vec::new(),
        })
        .collect()
}

/// What a verb's answer says went wrong, for the dossier a verify step writes
/// (§FS-006-project-interface.4). A verb that failed and said nothing
/// structured still failed: the exit code is the answer, and this is the
/// detail where there is any.
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

/// The one line a verb's answer gave about itself, where it gave one.
pub fn summary_of(answer: &Answer) -> Option<String> {
    answer
        .answer
        .as_ref()
        .and_then(|normalized| normalized.facts.summary.clone())
}

/// Run one bound verb at a project's root. `feature` runs that feature's
/// smoke alone (§FS-006-project-interface.5).
///
/// The mode is the caller's, as it is for every summons
/// (§AR-002-summons.2): a dossier being assembled wants the output back and
/// captures it; a person or a CI log watching a gate run wants it streamed,
/// which is also the only way the command's own standard error reaches them.
pub fn run(
    bound: &Bound,
    root: &Path,
    dossier: Vec<(String, String)>,
    feature: Option<&str>,
    mode: Mode,
) -> Result<Answer> {
    let mut summons = bound.summons(dossier)?;
    if let Some(feature) = feature {
        summons.binding = format!(
            "{} {}",
            summons.binding,
            crate::seams::summons::quote(feature)
        );
    }
    crate::seams::summons::run(&summons, &Site::root(root), mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest;

    fn manifest_of(text: &str) -> Manifest {
        manifest::parse(text, "ephor.json").unwrap()
    }

    #[test]
    fn a_well_known_name_at_the_root_is_the_verb_where_nothing_else_says() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(bind(Verb::Check, tmp.path(), None, None), None);

        std::fs::write(tmp.path().join("check.sh"), "#!/bin/sh\n").unwrap();
        let bound = bind(Verb::Check, tmp.path(), None, None).unwrap();
        assert_eq!(bound.command, "./check.sh");
        assert_eq!(bound.cwd, None);
    }

    #[test]
    fn the_project_outranks_the_probe_and_the_person_outranks_both() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("check.sh"), "#!/bin/sh\n").unwrap();
        let declared =
            manifest_of(r#"{"checks": {"check": {"command": "mx gate", "cwd": "repo:ce"}}}"#);

        let manifested = bind(Verb::Check, tmp.path(), Some(&declared), None).unwrap();
        assert_eq!(manifested.command, "mx gate");
        assert_eq!(manifested.cwd.as_deref(), Some("repo:ce"));

        let mine = bind(Verb::Check, tmp.path(), Some(&declared), Some("./mine.sh")).unwrap();
        assert_eq!(mine.command, "./mine.sh");
    }

    #[test]
    fn each_verb_probes_its_own_name() {
        let tmp = tempfile::tempdir().unwrap();
        for verb in Verb::all() {
            std::fs::write(tmp.path().join(verb.probed()), "#!/bin/sh\n").unwrap();
            let bound = bind(verb, tmp.path(), None, None).unwrap();
            assert_eq!(bound.command, format!("./{}", verb.probed()));
        }
    }

    #[test]
    fn smoke_says_how_it_enumerates_or_says_nothing() {
        assert_eq!(enumeration(None), Enumeration::Opaque);

        let opaque = manifest_of(r#"{"checks": {"smoke": {"command": "./s.sh"}}}"#);
        assert_eq!(enumeration(Some(&opaque)), Enumeration::Opaque);

        let ask =
            manifest_of(r#"{"checks": {"smoke": {"command": "./s.sh", "features": "list"}}}"#);
        assert_eq!(enumeration(Some(&ask)), Enumeration::Ask);

        let listed = manifest_of(
            r#"{"checks": {"smoke": {"command": "./s.sh",
                 "features": [{"id": "reflection", "description": "Reflection"}]}}}"#,
        );
        match enumeration(Some(&listed)) {
            Enumeration::Listed(features) => {
                assert_eq!(features.len(), 1);
                assert_eq!(features[0].id, "reflection");
            }
            other => panic!("expected a listed enumeration, got {other:?}"),
        }
    }

    #[test]
    fn a_smoke_that_lists_one_id_per_line_is_a_complete_answer() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("smoke-test.sh");
        std::fs::write(&script, "#!/bin/sh\nprintf 'reflection\\nresources\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let bound = bind(Verb::Smoke, tmp.path(), None, None).unwrap();
        let answer = run(
            &bound,
            tmp.path(),
            Vec::new(),
            Some("--list"),
            Mode::Captured(std::time::Duration::from_secs(10)),
        )
        .unwrap();
        let features: Vec<String> = features_of(&answer)
            .into_iter()
            .map(|feature| feature.id)
            .collect();
        assert_eq!(features, vec!["reflection", "resources"]);
    }

    #[test]
    fn what_a_verb_says_in_the_envelope_reaches_the_dossier() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("check.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\ncat > \"$EPHOR_ANSWER\" <<'JSON'\n\
             {\"v\":1,\"summary\":\"2 failed\",\
             \"failures\":[{\"job\":\"style\"},{\"job\":\"build\"}]}\nJSON\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let bound = bind(Verb::Check, tmp.path(), None, None).unwrap();
        let answer = run(
            &bound,
            tmp.path(),
            Vec::new(),
            None,
            Mode::Captured(std::time::Duration::from_secs(10)),
        )
        .unwrap();
        assert!(!answer.is_done(), "the exit code is the answer");
        assert_eq!(summary_of(&answer).as_deref(), Some("2 failed"));
        let jobs: Vec<String> = failures_of(&answer)
            .into_iter()
            .map(|failure| failure.job)
            .collect();
        assert_eq!(jobs, vec!["style", "build"]);
    }
}
