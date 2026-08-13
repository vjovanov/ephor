//! Invoking the runtime: one construction of the runner command, and one
//! process path to it (§AR-007-runtime, §AR-002-summons).
//!
//! Running a plan is a summons like every other command ephor asks of the
//! world — the same place resolution, the same exit semantics, the same
//! terminal handover — so the key in the inbox and `ephor work run` cannot
//! drift into two different invocations (§FS-005-dispatch.12).

use std::path::Path;

use crate::capabilities;
use crate::error::Result;
use crate::seams::summons::{self, quote, Answer, Mode, Site, Summons};

/// The shipped default runner (§REQ-001-boundary.1,
/// §DA-001-runtime-bound-default).
pub const RUNNER: &str = "rhei";

/// The verb this seam fills, for messages.
pub const VERB: &str = "work.run";

/// `<runner> run <root> [--rhei <plan>]… [extra…]`, quoted for `sh`.
pub fn invocation(root: &Path, plans: &[String], extra: &[String]) -> String {
    let mut words = vec![
        RUNNER.to_string(),
        "run".to_string(),
        quote(&root.to_string_lossy()),
    ];
    for plan in plans {
        words.push("--rhei".to_string());
        words.push(quote(plan));
    }
    words.extend(extra.iter().map(|arg| quote(arg)));
    words.join(" ")
}

/// Why running is refused, or None where the runtime is there
/// (§AR-005-capabilities.2). Where the work is and whether it is checked out
/// belongs to the project's own ladder; this is the runtime rung alone.
pub fn refusal() -> Option<String> {
    capabilities::workable(Some(RUNNER))
}

/// The summons that runs one work root's plans.
pub fn summons(root: &Path, plans: &[String], extra: &[String]) -> Summons {
    Summons::new(VERB, invocation(root, plans, extra))
}

/// Run it from the checkout the work is about — not from wherever this was
/// typed: where the runtime puts the agent falls back to its own working
/// directory, and a multi-repository workspace has no single repository to be
/// found by looking (§FS-005-dispatch.3). The person is watching, so the
/// terminal is theirs while it runs.
pub fn run(root: &Path, checkout: &Path, plans: &[String], extra: &[String]) -> Result<Answer> {
    summons::run(
        &summons(root, plans, extra),
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
