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
pub mod workflow;

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

/// The verb a cancel fills, for messages (§FS-005-dispatch.16).
pub const CANCEL_VERB: &str = "work.cancel";

/// How long the runner gets to move one ticket before ephor stops waiting.
/// A transition is a file rewrite and, at most, a callback the machine
/// hangs on it — seconds are generous, and a reader is holding the screen.
const CANCEL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// `<runner> transition <plan> --task <ticket> --from <state> --to cancelled
/// --result <why>`, quoted for `sh`, with the runner's own standard error
/// folded into what is captured — its refusal is written there, and it is the
/// one thing worth reading back when the move does not happen
/// (§FS-005-dispatch.16, §DA-005-cancel-is-the-runtimes-move). The
/// abandonment state is the plan language's word (§AR-007-runtime.1), and the
/// result carries the reader's reason: the runtime records why a ticket ended
/// where it did, and refuses a terminal move that says nothing.
pub fn cancel_command(
    config: &crate::work::recipe::WorkConfig,
    plan: &Path,
    ticket: &str,
    from: &str,
    why: &str,
) -> String {
    format!(
        "{} transition {} --task {} --from {} --to {} --result {} 2>&1",
        runner(config),
        quote(&plan.to_string_lossy()),
        quote(ticket),
        quote(from),
        quote(plan::CANCELLED),
        quote(why),
    )
}

/// Ask the runner to move one ticket into the abandonment state, from the
/// work root, captured (§FS-005-dispatch.16). `Ok` is the ticket cancelled;
/// `Err` is the runner's own refusal in its own first words — what it printed
/// is what the reader is told, since ephor neither works around it nor knows
/// better (§DA-005-cancel-is-the-runtimes-move).
pub fn cancel(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    plan: &Path,
    ticket: &str,
    from: &str,
    why: &str,
) -> Result<()> {
    let answer = summons::run(
        &Summons::new(CANCEL_VERB, cancel_command(config, plan, ticket, from, why)),
        &Site::root(root),
        Mode::Captured(CANCEL_TIMEOUT),
    )?;
    if answer.is_done() {
        return Ok(());
    }
    let said = said(answer.output.as_deref().unwrap_or(""));
    Err(crate::error::EphorError::Command(match said.is_empty() {
        true => format!(
            "{} refused: {}",
            label_of(config, "transition"),
            answer.refusal(CANCEL_VERB)
        ),
        false => format!("{} refused: {said}", label_of(config, "transition")),
    }))
}

/// The runner's own words out of what it printed: the first two lines that
/// say anything, with the box-drawing a pretty error report wraps them in
/// stripped, joined into one sentence a message line can carry.
fn said(output: &str) -> String {
    output
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['×', '│', '╰', '╭', '─', '┬', '├', '┤', '▶', '·', ' '])
                .trim()
        })
        .filter(|line| !line.is_empty() && !line.starts_with("help:"))
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

/// How a surface names one of the runtime's verbs in a message — the bound
/// command and the verb, never a word compiled in above this module.
fn label_of(config: &crate::work::recipe::WorkConfig, verb: &str) -> String {
    format!("{} {verb}", runner(config))
}

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
///
/// `mode` is the caller's, because it is the caller that knows whether its
/// standard output is already spoken for: a run started for a person takes the
/// terminal outright, and one started under `--json` puts the runtime's own
/// output beside the reading instead ([`Mode::Aside`]) so that what a program
/// parses is the outcome alone (§REQ-002-parity.3, §FS-011-command-line.7).
/// The runtime still has a terminal to ask its questions on either way.
pub fn run(
    config: &crate::work::recipe::WorkConfig,
    root: &Path,
    checkout: &Path,
    plans: &[String],
    hand: Option<&roster::HandFlags>,
    extra: &[String],
    mode: Mode,
) -> Result<Answer> {
    summons::run(
        &summons_with(config, root, plans, hand, extra),
        &Site::root(checkout),
        mode,
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

    /// A cancel is the runner's transition verb into the abandonment state,
    /// carrying the reader's reason as the result, with the runner's standard
    /// error folded in so its refusal can be read back
    /// (§FS-005-dispatch.16, §DA-005-cancel-is-the-runtimes-move).
    #[test]
    fn a_cancel_is_the_runners_transition_into_the_abandonment_state() {
        let command = cancel_command(
            &crate::work::recipe::WorkConfig::default(),
            Path::new("/w/panta/forge-demo-17.rhei.md"),
            "fix-gate-2",
            "collect",
            "asked twice by mistake",
        );
        assert_eq!(
            command,
            "rhei transition '/w/panta/forge-demo-17.rhei.md' --task 'fix-gate-2' \
             --from 'collect' --to 'cancelled' --result 'asked twice by mistake' 2>&1"
        );
        // Under another binding it is that binding's verb.
        let bound = crate::work::recipe::WorkConfig {
            runner: Some("my-runtime".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        assert!(
            cancel_command(&bound, Path::new("/w/p.rhei.md"), "a-1", "fix", "why")
                .starts_with("my-runtime transition ")
        );
    }

    /// What the runner printed comes back as its own sentence, unwrapped from
    /// the report decoration and cut to what a message line can carry.
    #[test]
    fn the_runners_refusal_is_read_back_in_its_own_words() {
        let printed = "  × Task item.fix-gate-1 cannot leave state collect.\n  │ Missing required output artifact: failures\n  │ (runtime/ephor/item.fix-gate-1.failures.md)\n  help: the state's work is not finished until that file exists.\n";
        assert_eq!(
            said(printed),
            "Task item.fix-gate-1 cannot leave state collect. Missing required output artifact: failures"
        );
        assert_eq!(said("\n   \n"), "");
    }

    /// Run against a stand-in runner: a stand-in that agrees moves nothing
    /// ephor can see but answers done, and one that refuses hands its words
    /// back as the error (§FS-005-dispatch.16).
    #[test]
    fn a_cancel_is_done_or_carries_the_runners_words() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("panta");
        std::fs::create_dir_all(&root).unwrap();
        let plan = root.join("p.rhei.md");
        std::fs::write(&plan, "# Rhei: p\n").unwrap();
        // The binding is a shell function of the reader's: `sh -c` resolves
        // it as any command, and a name on PATH is not needed for the seam.
        let agrees = crate::work::recipe::WorkConfig {
            runner: Some("echo".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        cancel(&agrees, &root, &plan, "a-1", "fix", "why").expect("echo agrees to anything");

        let refuses = crate::work::recipe::WorkConfig {
            runner: Some(
                "sh -c 'printf \"  × Task a-1 cannot leave state fix.\\n\" >&2; exit 1' --"
                    .to_string(),
            ),
            ..crate::work::recipe::WorkConfig::default()
        };
        let err = cancel(&refuses, &root, &plan, "a-1", "fix", "why").expect_err("it refused");
        let said = err.to_string();
        assert!(
            said.contains("refused: Task a-1 cannot leave state fix."),
            "{said}"
        );
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
