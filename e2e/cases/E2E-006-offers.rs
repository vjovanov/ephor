//! E2E-006-offers: a project offers a menu entry, and a rung it needs decides its fate.
//!
//! The scenario is a project that ships a menu entry with itself: an action in
//! its `ephor.json` that anyone watching the project gets, without any of them
//! configuring it (§FS-006-project-interface.9). What the entry needs, it names
//! in `requires`, and the answer comes from the one capability table — so a
//! project's offer is refused in exactly the words a person's own action is,
//! and the reason is the ladder's sentence rather than each surface's guess
//! (§FS-006-project-interface.10, §AR-005-capabilities.2).
//!
//! Three things the scenario holds ephor to. A missing rung leaves the entry
//! visible with its reason, never removed — an entry that vanished teaches
//! nothing (§REQ-001-boundary.1). A requirement that is not a rung at all is
//! named rather than quietly treated as met. And a row that narrows its trust
//! in the checkout keeps what the project says about itself and runs none of it
//! (§FS-006-project-interface.2).
//!
//! Assembling the rows is the inbox's (§FS-004-quick-actions.3 orders them);
//! everything the offer itself decides happens below that, and that is what
//! runs here.

#[path = "../support.rs"]
mod support;

use ephor::capabilities::{Bindings, CapabilitySet, Rung};
use ephor::feed::config::ActionConfig;
use ephor::manifest::{Manifest, Trust};
use ephor::seams::summons::{Mode, Outcome, Place, Site, Summons};

use support::*;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// A project offering the one thing only it knows how to do, gated on the one
/// thing only the machine can answer: whether this checkout can be verified.
fn offering(world: &World) {
    world.manifest(serde_json::json!({
        "actions": [{
            "id": "rebuild",
            "icon": "🔁",
            "description": "rebuild the docs site",
            "command": "./tools/rebuild.sh",
            "requires": ["checkable"],
            "when": { "kinds": ["pr"] }
        }]
    }));
    world.script(
        "tools/rebuild.sh",
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'rebuilt in %s\\n' \"$PWD\"\n",
    );
}

fn offer_of(world: &World, trust: Trust) -> ActionConfig {
    let manifest = Manifest::read(&world.forest(), trust)
        .expect("the manifest reads")
        .expect("the project wrote one");
    manifest
        .offers
        .first()
        .map(ephor::manifest::Offer::action)
        .expect("the project offers an entry")
}

fn capabilities(world: &World) -> CapabilitySet {
    let placement = ephor::branches::Placement::load(&world.registry_doc(), PROJECT)
        .expect("the registry describes the project");
    let manifest = Manifest::read(&world.forest(), placement.trust).expect("the manifest reads");
    CapabilitySet::resolve(
        PROJECT,
        Some(&placement),
        &Bindings {
            sources: 1,
            manifest: manifest.as_ref(),
            ..Bindings::default()
        },
    )
}

#[test]
fn an_offer_whose_rung_is_missing_is_shown_with_the_ladders_own_sentence() {
    let world = World::new();
    offering(&world);

    // The forest holds no check verb, so the *checkable* rung does not hold,
    // and the entry the project offered is refused with the reason that rung
    // gives — the same text `ephor` would print anywhere else.
    let offer = offer_of(&world, Trust::Full);
    let (rungs, unknown) = offer.rungs();
    assert_eq!(rungs, vec![Rung::Checkable]);
    assert!(unknown.is_empty());

    let can = capabilities(&world);
    let refusal = can.refusal(&rungs).expect("a missing rung refuses");
    assert!(refusal.contains("holds none of"), "{refusal}");
    assert!(refusal.contains("check.sh"), "{refusal}");
    assert!(refusal.contains("nothing to verify with"), "{refusal}");
    // How a surface renders it: the row stays, marked with why.
    assert_eq!(
        can.unavailable(&rungs),
        Some(format!("(unavailable: {refusal})"))
    );

    // The project adds the thing the rung was about, and the same offer is
    // simply available — resolved from the world as it is now, not from
    // anything written down (§AR-005-capabilities.1).
    world.script("check.sh", "#!/usr/bin/env bash\nexit 0\n");
    let can = capabilities(&world);
    assert!(can.holds(Rung::Checkable));
    assert_eq!(can.refusal(&rungs), None);
    assert_eq!(can.unavailable(&rungs), None);

    // And it is invoked the way every command ephor runs is: one summons, in
    // the place the entry named (§AR-002-summons, §FS-006-project-interface.3).
    let summons = Summons::new(&offer.description, &offer.command).at(Place::Workspace);
    let answer = ephor::seams::summons::run(
        &summons,
        &Site::workspace(world.forest(), world.forest()),
        Mode::Captured(TIMEOUT),
    )
    .expect("the offer runs");
    assert_eq!(answer.outcome, Outcome::Done);
    assert!(answer
        .output
        .as_deref()
        .expect("captured output")
        .contains("rebuilt in"));
}

/// A requirement ephor does not recognize is refused by name, and refused
/// early: the manifest is structure crossing the interface, so it is held to
/// the published schema before anything reads a field of it
/// (§FS-006-project-interface.11). A project learns here rather than by an
/// entry that quietly never appears.
#[test]
fn a_requirement_that_is_not_a_rung_is_named_rather_than_treated_as_met() {
    let world = World::new();
    world.manifest(serde_json::json!({
        "actions": [{
            "id": "rebuild",
            "description": "rebuild the docs site",
            "command": "./tools/rebuild.sh",
            "requires": ["buildable"]
        }]
    }));

    // What the project runs in its own CI, needing nothing but the checkout
    // (§FS-009-shipped-actions.2).
    world
        .ephor()
        .args(["validate", "--manifest", &world.forest().to_string_lossy()])
        .assert()
        .failure()
        .stderr(predicates::str::contains("actions/0/requires/0"))
        .stderr(predicates::str::contains("buildable"))
        .stderr(predicates::str::contains("checkable"));

    // A person's own action is not schema-checked — it is their configuration,
    // not a stranger's — so there the unknown word is caught when the entry is
    // gated, and named rather than treated as met.
    let mine: ActionConfig = serde_json::from_value(serde_json::json!({
        "icon": "🔁",
        "description": "rebuild the docs site",
        "command": "./tools/rebuild.sh",
        "requires": ["buildable"]
    }))
    .expect("a site action reads");
    let (rungs, unknown) = mine.rungs();
    assert!(rungs.is_empty());
    assert_eq!(unknown, vec!["buildable".to_string()]);

    // And the icon is the one thing ephor fills in for a project's offer: an
    // offer that named none still has to look like the entries beside it.
    world.manifest(serde_json::json!({
        "actions": [{
            "id": "rebuild",
            "description": "rebuild the docs site",
            "command": "./tools/rebuild.sh"
        }]
    }));
    assert_eq!(offer_of(&world, Trust::Full).icon, "▸");
}

/// The row is the authority on how much of a checkout to believe: a project
/// that is trusted to describe itself and not to run anything offers nothing
/// (§FS-006-project-interface.2).
#[test]
fn a_row_that_narrowed_its_trust_gets_no_offers_at_all() {
    let world = World::new();
    offering(&world);
    let narrowed = Manifest::read(&world.forest(), Trust::Descriptions)
        .expect("the manifest reads")
        .expect("the project wrote one");
    assert!(narrowed.offers.is_empty());

    // Which is what the registry row asks for by saying so — the same read
    // ephor does everywhere it resolves this project.
    world.register(serde_json::json!({ "manifest_trust": "descriptions" }));
    let placement = ephor::branches::Placement::load(&world.registry_doc(), PROJECT)
        .expect("the registry describes the project");
    assert_eq!(placement.trust, Trust::Descriptions);
    assert!(Manifest::read(&world.forest(), placement.trust)
        .expect("the manifest reads")
        .expect("the project wrote one")
        .offers
        .is_empty());
}
