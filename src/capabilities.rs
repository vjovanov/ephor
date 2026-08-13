//! The capability table: what a project can do, computed once and consulted
//! everywhere (§AR-005-capabilities).
//!
//! The ladder of §FS-006-project-interface.10 is a set of **rungs**, each
//! either held or missing with the one sentence that says why. A feature names
//! the rungs it needs; offering is filtering on the table and refusing is
//! rendering the first missing rung's sentence, so §REQ-001-boundary.1's
//! degrade rule has one implementation and the reason a person reads on a
//! greyed menu entry is the same text the command line prints.
//!
//! Resolution is cheap by construction — stat calls, config lookups, one walk
//! of `PATH`, no spawning (§AR-005-capabilities.1) — so it reruns whenever the
//! world may have moved.

use std::collections::BTreeMap;
use std::path::Path;

use crate::branches::Placement;

/// One rung of the ladder (§FS-006-project-interface.10), in ladder order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rung {
    /// A registry row and at least one source answering. Buys the watch.
    Observable,
    /// The forest root on disk. Buys actions and update.
    Placed,
    /// A workspace template, so a matter resolves to a workspace of its own.
    BranchAddressable,
    /// Something that can make a branch workspace (§FS-006-project-interface.8).
    /// Buys work that edits.
    CheckoutAble,
    /// Checks that can be run (§FS-006-project-interface.5). Buys verification
    /// that means something.
    Checkable,
    /// A gate that can be asked (§FS-006-project-interface.6). Buys failure
    /// dossiers and the restart.
    Gated,
    /// A local ticket store (§FS-006-project-interface.7). Buys local matters.
    Ticketed,
    /// A bound runtime on PATH (§FS-005-dispatch). Buys the loop.
    Workable,
}

impl Rung {
    /// The rung's name, as the ladder spells it.
    pub fn name(self) -> &'static str {
        match self {
            Rung::Observable => "observable",
            Rung::Placed => "placed",
            Rung::BranchAddressable => "branch-addressable",
            Rung::CheckoutAble => "checkout-able",
            Rung::Checkable => "checkable",
            Rung::Gated => "gated",
            Rung::Ticketed => "ticketed",
            Rung::Workable => "workable",
        }
    }

    /// Every rung, in ladder order.
    pub fn all() -> [Rung; 8] {
        [
            Rung::Observable,
            Rung::Placed,
            Rung::BranchAddressable,
            Rung::CheckoutAble,
            Rung::Checkable,
            Rung::Gated,
            Rung::Ticketed,
            Rung::Workable,
        ]
    }
}

/// The well-known check names probed at a forest root
/// (§FS-006-project-interface.5). A manifest may declare others in their place
/// once there is a manifest to read (§FS-006-project-interface.2).
pub const CHECK_SCRIPTS: [&str; 3] = ["check.sh", "check-style.sh", "smoke-test.sh"];

/// The ticket stores probed by convention (§FS-006-project-interface.7). Each
/// is a project-native thing that exists without ephor; finding one is a rung,
/// never an obligation.
pub const TICKET_STORES: [&str; 2] = ["panta", ".beads"];

/// What the caller knows that the checkout cannot be asked: the bindings a
/// person configured and what the sources last answered.
#[derive(Debug, Clone, Default)]
pub struct Bindings<'a> {
    /// How many sources the project has configured.
    pub sources: usize,
    /// The configured checkout command, where one is bound
    /// (§FS-006-project-interface.8).
    pub checkout: Option<&'a str>,
    /// The runtime binding — the command that runs a plan.
    pub runner: Option<&'a str>,
    /// Whether any source has reported a gate for this project. The sources'
    /// last answer is what establishes the rung until gate verbs are bound
    /// (§AR-005-capabilities.1).
    pub gate_reported: bool,
}

/// One project's ladder.
#[derive(Debug, Clone)]
pub struct CapabilitySet {
    pub project: String,
    /// Rung → the sentence saying why it is missing. A rung absent from this
    /// map is held.
    missing: BTreeMap<Rung, String>,
}

impl CapabilitySet {
    /// Resolve every rung. `placement` is None where the registry does not
    /// describe the project at all, which costs it every rung below the watch.
    pub fn resolve(project: &str, placement: Option<&Placement>, bindings: &Bindings) -> Self {
        let mut missing = BTreeMap::new();
        let mut fails = |rung: Rung, reason: String| {
            missing.insert(rung, reason);
        };

        let Some(placement) = placement else {
            for rung in Rung::all() {
                fails(
                    rung,
                    format!("{project} has no registry row, so nothing is known about where it is"),
                );
            }
            return CapabilitySet {
                project: project.to_string(),
                missing,
            };
        };

        if bindings.sources == 0 {
            fails(
                Rung::Observable,
                format!("no source is configured for {project}, so there is nothing to watch"),
            );
        }

        let root = &placement.root;
        let placed = root.is_dir();
        if !placed {
            fails(Rung::Placed, format!("{} is not on disk", root.display()));
        }

        if placement.template.is_none() {
            fails(
                Rung::BranchAddressable,
                format!(
                    "{project} has no branch_root_template in the registry, so a branch has no \
                     workspace of its own — its root is the checkout"
                ),
            );
        }

        // Either a bound command makes the workspace, or ephor's own git
        // checkout does — and that one needs a checkout on disk to grow a
        // working tree from (§FS-004-quick-actions.7).
        if bindings.checkout.is_none() && placement.source_checkout().is_none() {
            fails(
                Rung::CheckoutAble,
                format!(
                    "nothing can make a branch workspace for {project}: no checkout command is \
                     bound and there is no checkout on disk to make one from"
                ),
            );
        }

        if placed {
            if !CHECK_SCRIPTS
                .iter()
                .any(|name| is_runnable(&root.join(name)))
            {
                fails(
                    Rung::Checkable,
                    format!(
                        "{} holds none of {}, so there is nothing to verify with",
                        root.display(),
                        CHECK_SCRIPTS.join(", ")
                    ),
                );
            }
            if !TICKET_STORES.iter().any(|name| root.join(name).is_dir()) {
                fails(
                    Rung::Ticketed,
                    format!(
                        "{} holds no ticket store ({})",
                        root.display(),
                        TICKET_STORES.join(", ")
                    ),
                );
            }
        } else {
            let unplaced = format!(
                "{} is not on disk, so it cannot be looked in",
                root.display()
            );
            fails(Rung::Checkable, unplaced.clone());
            fails(Rung::Ticketed, unplaced);
        }

        if !bindings.gate_reported {
            fails(
                Rung::Gated,
                format!("no source reports a gate for {project}, and no gate verbs are bound"),
            );
        }

        if let Some(reason) = workable(bindings.runner) {
            fails(Rung::Workable, reason);
        }

        CapabilitySet {
            project: project.to_string(),
            missing,
        }
    }

    /// Nothing is known about this project — used where a caller has no
    /// registry to consult at all.
    pub fn unknown(project: &str) -> Self {
        CapabilitySet::resolve(project, None, &Bindings::default())
    }

    pub fn holds(&self, rung: Rung) -> bool {
        !self.missing.contains_key(&rung)
    }

    /// Why a rung is missing, or None where it holds.
    pub fn reason(&self, rung: Rung) -> Option<&str> {
        self.missing.get(&rung).map(String::as_str)
    }

    /// The rungs this project holds, in ladder order — what it *can* do.
    pub fn held(&self) -> Vec<Rung> {
        Rung::all()
            .into_iter()
            .filter(|rung| self.holds(*rung))
            .collect()
    }

    /// The one sentence a feature needing these rungs is refused with: the
    /// first missing rung's, in the order the feature named them
    /// (§AR-005-capabilities.2). None where every rung holds, which is the
    /// feature being offered.
    pub fn refusal(&self, needs: &[Rung]) -> Option<String> {
        needs
            .iter()
            .find_map(|rung| self.reason(*rung))
            .map(String::from)
    }

    /// How a surface renders a refused offer (§AR-002-summons.4): the entry
    /// stays visible, marked with the reason rather than removed, because an
    /// entry that vanished teaches nothing.
    pub fn unavailable(&self, needs: &[Rung]) -> Option<String> {
        self.refusal(needs)
            .map(|reason| format!("(unavailable: {reason})"))
    }
}

/// Why the runtime rung is missing, or None where it holds — the one question
/// `ephor work run` asks before spawning anything, resolved here so the
/// command line and the inbox refuse in the same words
/// (§AR-005-capabilities.2).
pub fn workable(runner: Option<&str>) -> Option<String> {
    match runner {
        Some(runner) if crate::feed::provider::command_exists(runner) => None,
        Some(runner) => Some(format!(
            "{runner} is not on PATH; ephor writes the tickets but the runtime runs them."
        )),
        None => {
            Some("no runtime is bound, so tickets are written and read but never run".to_string())
        }
    }
}

/// A check verb is a file that can be run. Executability is not read: a script
/// a project keeps without the bit set is still the project's answer to
/// "how do I check this", and refusing to see it would be ephor deciding.
fn is_runnable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branches::Placement;
    use std::path::PathBuf;

    fn placement(root: &Path, template: Option<&str>) -> Placement {
        Placement {
            project: "widget".to_string(),
            root: root.to_path_buf(),
            template: template.map(String::from),
            branches: Vec::new(),
            main_branch: Some("master".to_string()),
            repos: Vec::new(),
        }
    }

    fn bindings<'a>() -> Bindings<'a> {
        Bindings {
            sources: 1,
            checkout: Some("gco \"$EPHOR_BRANCH\""),
            runner: None,
            gate_reported: true,
        }
    }

    #[test]
    fn a_project_with_nothing_on_disk_says_so_once_per_rung() {
        let missing = PathBuf::from("/nowhere/that/exists");
        let set = CapabilitySet::resolve("widget", Some(&placement(&missing, None)), &bindings());
        assert!(!set.holds(Rung::Placed));
        assert_eq!(
            set.reason(Rung::Placed),
            Some("/nowhere/that/exists is not on disk")
        );
        // What cannot be looked in cannot be probed either, and says why.
        assert!(set
            .reason(Rung::Checkable)
            .unwrap()
            .contains("cannot be looked in"));
        assert!(set
            .reason(Rung::Ticketed)
            .unwrap()
            .contains("cannot be looked in"));
    }

    #[test]
    fn probing_finds_the_check_verbs_and_the_ticket_store() {
        let tmp = tempfile::tempdir().unwrap();
        let set = CapabilitySet::resolve("widget", Some(&placement(tmp.path(), None)), &bindings());
        assert!(!set.holds(Rung::Checkable));
        assert!(!set.holds(Rung::Ticketed));

        std::fs::write(tmp.path().join("check.sh"), "#!/bin/sh\n").unwrap();
        std::fs::create_dir(tmp.path().join("panta")).unwrap();
        let set = CapabilitySet::resolve("widget", Some(&placement(tmp.path(), None)), &bindings());
        assert!(set.holds(Rung::Checkable));
        assert!(set.holds(Rung::Ticketed));
    }

    #[test]
    fn a_row_without_a_template_is_not_branch_addressable() {
        let tmp = tempfile::tempdir().unwrap();
        let set = CapabilitySet::resolve("widget", Some(&placement(tmp.path(), None)), &bindings());
        assert!(!set.holds(Rung::BranchAddressable));
        assert!(set
            .reason(Rung::BranchAddressable)
            .unwrap()
            .contains("branch_root_template"));

        let with = placement(tmp.path(), Some("{project_root}/{branch}"));
        let set = CapabilitySet::resolve("widget", Some(&with), &bindings());
        assert!(set.holds(Rung::BranchAddressable));
    }

    #[test]
    fn a_bound_checkout_command_buys_the_rung_with_nothing_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let set = CapabilitySet::resolve("widget", Some(&placement(tmp.path(), None)), &bindings());
        assert!(set.holds(Rung::CheckoutAble));

        // Without one, and with no checkout to grow a working tree from, the
        // rung is missing and names both halves of why.
        let unbound = Bindings {
            checkout: None,
            ..bindings()
        };
        let set = CapabilitySet::resolve("widget", Some(&placement(tmp.path(), None)), &unbound);
        let reason = set.reason(Rung::CheckoutAble).unwrap();
        assert!(reason.contains("no checkout command is bound"), "{reason}");
        assert!(reason.contains("no checkout on disk"), "{reason}");
    }

    #[test]
    fn the_runtime_rung_names_the_runner_it_looked_for() {
        let tmp = tempfile::tempdir().unwrap();
        let unbound = CapabilitySet::resolve(
            "widget",
            Some(&placement(tmp.path(), None)),
            &Bindings {
                runner: None,
                ..bindings()
            },
        );
        assert!(unbound
            .reason(Rung::Workable)
            .unwrap()
            .contains("no runtime is bound"));

        let absent = CapabilitySet::resolve(
            "widget",
            Some(&placement(tmp.path(), None)),
            &Bindings {
                runner: Some("no-such-runner-anywhere"),
                ..bindings()
            },
        );
        assert_eq!(
            absent.reason(Rung::Workable),
            Some(
                "no-such-runner-anywhere is not on PATH; ephor writes the tickets but the \
                 runtime runs them."
            )
        );

        // Something every machine has, to prove the held case is real.
        let present = CapabilitySet::resolve(
            "widget",
            Some(&placement(tmp.path(), None)),
            &Bindings {
                runner: Some("sh"),
                ..bindings()
            },
        );
        assert!(present.holds(Rung::Workable));
    }

    #[test]
    fn no_source_is_not_observable_and_no_gate_reported_is_not_gated() {
        let tmp = tempfile::tempdir().unwrap();
        let set = CapabilitySet::resolve(
            "widget",
            Some(&placement(tmp.path(), None)),
            &Bindings {
                sources: 0,
                gate_reported: false,
                ..bindings()
            },
        );
        assert!(!set.holds(Rung::Observable));
        assert!(!set.holds(Rung::Gated));
        assert!(set
            .reason(Rung::Gated)
            .unwrap()
            .contains("no gate verbs are bound"));
    }

    #[test]
    fn a_project_the_registry_does_not_describe_holds_nothing() {
        let set = CapabilitySet::unknown("ghost");
        assert!(set.held().is_empty());
        assert!(set
            .reason(Rung::Observable)
            .unwrap()
            .contains("no registry row"));
    }

    #[test]
    fn a_refusal_is_the_first_missing_rung_the_feature_named() {
        let tmp = tempfile::tempdir().unwrap();
        let set = CapabilitySet::resolve("widget", Some(&placement(tmp.path(), None)), &bindings());
        // Placed holds, Workable does not: the refusal is Workable's sentence.
        let refusal = set.refusal(&[Rung::Placed, Rung::Workable]).unwrap();
        assert!(refusal.contains("no runtime is bound"), "{refusal}");
        // Order is the feature's: whichever it names first and is missing.
        assert_eq!(
            set.refusal(&[Rung::Checkable, Rung::Workable]),
            set.reason(Rung::Checkable).map(String::from)
        );
        // Everything it needs holds: nothing to say, which is the offer.
        assert_eq!(set.refusal(&[Rung::Placed]), None);
        assert_eq!(set.unavailable(&[Rung::Placed]), None);
        assert!(set
            .unavailable(&[Rung::Workable])
            .unwrap()
            .starts_with("(unavailable: "));
    }
}
