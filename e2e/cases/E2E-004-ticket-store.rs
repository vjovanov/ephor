//! E2E-004-ticket-store: tickets a project keeps in its own checkout become matters.
//!
//! The scenario is a project that tracks its work in files rather than on a
//! forge: a plan directory in the checkout, kept for the project's own sake and
//! existing whether or not ephor ever runs (§FS-006-project-interface.7). ephor
//! recognizes the store by its own name, reads it where it lives, and the
//! tickets arrive in the feed as matters under the store's own ids — nothing is
//! renamed, nothing is written back, and no configuration was needed to say the
//! tickets belong to this project: they are in its checkout
//! (§FS-007-matters.1, §FS-008-attribution.2).

#[path = "../support.rs"]
mod support;

use ephor::capabilities::{Bindings, CapabilitySet, Rung};

use predicates::prelude::*;

use support::*;

/// A plan as the store itself writes one: tickets are task headings, and the
/// state line under each is the store's, not ephor's.
const PLAN: &str = "# Rhei: the retry window\n\n\
## Tasks\n\n\
### Task 1: Widen the retry window\n**State:** pending\n\n\
The window resets per attempt, which is not what the docs say.\n\n\
### Task 2: Document the reset\n**State:** completed\n\n\
Done.\n";

#[test]
fn a_store_in_the_checkout_is_read_where_it_lives_and_keeps_its_own_ids() {
    let world = World::new();
    // The probed convention: a directory the project keeps for itself, found
    // by its own name (§REQ-001-boundary.2).
    world.file("panta/window.rhei.md", PLAN);

    world.ephor().args(["refresh", PROJECT]).assert().success();

    // Two tickets, one matter each, keyed as the store keys them: the store
    // named them and ephor does not get to rename them.
    let open = world.matter("rhei:window.1");
    assert_eq!(open["title"], "Widen the retry window");
    assert_eq!(open["state"], "pending");
    assert_eq!(open["kind"], "issue");
    // Attribution is the checkout's project: a store in a checkout is about
    // that checkout, and nothing has to guess (§FS-008-attribution.2).
    assert_eq!(open["placement"]["on"]["project"], PROJECT);
    // A local ticket waits on whoever keeps the store; nothing about it says
    // someone is waiting on an answer.
    assert_eq!(open["needs_response"], false);

    let finished = world.matter("rhei:window.2");
    assert_eq!(finished["state"], "completed");

    // And it is in the feed a person reads, beside whatever the forges said.
    world
        .ephor()
        .args(["feed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Widen the retry window"));
}

/// A project that keeps its store somewhere else says so in its manifest, and
/// declaring one does not hide the other — a project may keep two
/// (§FS-006-project-interface.7).
#[test]
fn a_declared_store_and_a_probed_one_are_both_read() {
    let world = World::new();
    world.file("panta/window.rhei.md", PLAN);
    world.file(
        "docs/plans/release.rhei.md",
        "# Rhei: the release\n\n## Tasks\n\n### Task 1: Cut the tag\n**State:** pending\n",
    );
    world.manifest(serde_json::json!({
        "tickets": [{ "kind": "rhei", "path": "docs/plans" }]
    }));

    world.ephor().args(["refresh", PROJECT]).assert().success();

    assert_eq!(
        world.matter("rhei:window.1")["title"],
        "Widen the retry window"
    );
    assert_eq!(world.matter("rhei:release.1")["title"], "Cut the tag");
}

/// Work about a change belongs in that change's working tree
/// (§FS-005-dispatch.3), so a project whose branches have workspaces of their
/// own keeps a store per workspace rather than one at the forest root — and
/// both places are read (§FS-006-project-interface.7).
///
/// Looking only at the root is how such a project came to write its plans into
/// a place nothing read again: dispatch put them in the branch workspace,
/// where the default work root is, and the feed showed none of them.
#[test]
fn a_store_in_a_branch_workspace_is_read_as_readily_as_one_at_the_root() {
    let world = World::new();
    let workspace = world.path().join("trees/you-ABC-42");
    std::fs::create_dir_all(&workspace).expect("the branch workspace");
    // The row names one branch. The disk has two, and the one it does not name
    // is where most of the work is — which is the situation this exists for.
    world.register(serde_json::json!({
        "branch_root_template": world.path().join("trees/{branch}").to_string_lossy(),
        "branches": [{ "id": "current", "branch": "you-ABC-42", "active": true }]
    }));
    std::fs::create_dir_all(workspace.join("panta")).expect("the store");
    std::fs::write(workspace.join("panta/window.rhei.md"), PLAN).expect("a plan in the branch");

    let unnamed = world.path().join("trees/you-XYZ-9/panta");
    std::fs::create_dir_all(&unnamed).expect("a store on a branch the row never named");
    std::fs::write(
        unnamed.join("old.rhei.md"),
        "# Rhei: old\n\n## Tasks\n\n### Task 1: Something else\n**State:** pending\n",
    )
    .expect("a plan on a branch the row never named");

    world.ephor().args(["refresh", PROJECT]).assert().success();

    // Both are read: the stores are verified on disk, because the row names
    // the branches somebody wrote down and the work is wherever branches were
    // actually checked out (§FS-006-project-interface.7).
    assert_eq!(
        world.matter("rhei:window.1")["title"],
        "Widen the retry window"
    );
    assert_eq!(world.matter("rhei:old.1")["title"], "Something else");

    // And the rung holds on the strength of the workspace, with nothing at the
    // forest root at all.
    let placement = ephor::branches::Placement::load(&world.registry_doc(), PROJECT)
        .expect("the registry describes the project");
    let set = CapabilitySet::resolve(
        PROJECT,
        Some(&placement),
        &Bindings {
            sources: 1,
            ..Bindings::default()
        },
    );
    assert!(set.holds(Rung::LocalIssues));
}

/// Finding a store is a capability, never an obligation
/// (§FS-006-project-interface.10): a project without one is watched exactly as
/// before, and the rung says in one sentence what it looked for.
#[test]
fn a_project_without_a_store_is_watched_all_the_same_and_says_what_it_looked_for() {
    let world = World::new();
    world.ephor().args(["refresh", PROJECT]).assert().success();
    assert!(world.matters().is_empty());

    let placement = ephor::branches::Placement::load(&world.registry_doc(), PROJECT)
        .expect("the registry describes the project");
    let bare = CapabilitySet::resolve(
        PROJECT,
        Some(&placement),
        &Bindings {
            sources: 1,
            ..Bindings::default()
        },
    );
    assert!(!bare.holds(Rung::LocalIssues));
    let reason = bare
        .reason(Rung::LocalIssues)
        .expect("a missing rung says why");
    // Every place a store may live is named, not just the forest root: a
    // branch-addressable project keeps one per workspace
    // (§FS-006-project-interface.7), and a reason that named only the root
    // would send a reader to look in the wrong place.
    assert!(reason.contains("keeps no issues of its own"), "{reason}");
    assert!(
        reason.contains(&world.forest().display().to_string()),
        "{reason}"
    );

    // The store appears, and so does the rung — resolved from the world as it
    // is now rather than from anything written down (§AR-005-capabilities.1).
    world.file("panta/window.rhei.md", PLAN);
    let ticketed = CapabilitySet::resolve(
        PROJECT,
        Some(&placement),
        &Bindings {
            sources: 1,
            ..Bindings::default()
        },
    );
    assert!(ticketed.holds(Rung::LocalIssues));
}
