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

pub mod plan;
pub mod results;

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

/// `<runner> run <root> [--rhei <plan>]… [extra…]`, quoted for `sh`.
pub fn invocation_with(runner: &str, root: &Path, plans: &[String], extra: &[String]) -> String {
    let mut words = vec![
        runner.to_string(),
        "run".to_string(),
        quote(&root.to_string_lossy()),
    ];
    for plan in plans {
        words.push(PLAN_FLAG.to_string());
        words.push(quote(plan));
    }
    words.extend(extra.iter().map(|arg| quote(arg)));
    words.join(" ")
}

/// How the runner is told which plan to run. Part of the coupling, and so
/// part of this module (§AR-007-runtime).
const PLAN_FLAG: &str = "--rhei";

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

/// The invocation with the shipped default runner.
pub fn invocation(root: &Path, plans: &[String], extra: &[String]) -> String {
    invocation_with(RUNNER, root, plans, extra)
}

/// Why running is refused, or None where the runtime is there
/// (§AR-005-capabilities.2). Where the work is and whether it is checked out
/// belongs to the project's own ladder; this is the runtime rung alone.
pub fn refusal(config: &crate::work::recipe::WorkConfig) -> Option<String> {
    capabilities::workable(Some(runner(config)))
}

/// The summons that runs one work root's plans.
pub fn summons(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    plans: &[String],
    extra: &[String],
) -> Summons {
    Summons::new(VERB, invocation_with(runner(config), root, plans, extra))
}

/// Run it from the checkout the work is about — not from wherever this was
/// typed: where the runtime puts the agent falls back to its own working
/// directory, and a multi-repository workspace has no single repository to be
/// found by looking (§FS-005-dispatch.3). The person is watching, so the
/// terminal is theirs while it runs.
pub fn run(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    checkout: &Path,
    plans: &[String],
    extra: &[String],
) -> Result<Answer> {
    summons::run(
        &summons(config, root, plans, extra),
        &Site::root(checkout),
        Mode::Interactive,
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
        let command = summons(&bound, Path::new("/w/panta"), &["a".to_string()], &[]).binding;
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
