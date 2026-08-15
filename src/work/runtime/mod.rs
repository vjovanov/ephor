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
pub mod roster;
pub mod watch;

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
pub fn run(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    checkout: &Path,
    plans: &[String],
    hand: Option<&roster::HandFlags>,
    extra: &[String],
) -> Result<Answer> {
    summons::run(
        &summons_with(config, root, plans, hand, extra),
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
