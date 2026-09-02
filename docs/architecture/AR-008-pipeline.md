# AR-008-pipeline: the engine is seven stages, legible as one function

`fetch → attribute → merge → offer → summon → record → resurface` — the
whole product is this loop, and the engine keeps it readable as one
function. The first three run on refresh; offering runs when a surface
asks; summoning, recording, and resurfacing run as the person or the sync
drives them.

## 1. Fetch

Remote sources fetch per the [§FS-001-forge-interface](../functional-spec/FS-001-forge-interface.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface) capability set —
shared sources (a mailbox, a notification stream) once per site, per-repo
sources per project — and checkout sources read the forests: git probes,
custom-status, task stores, each a summons ([§AR-002-summons](AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world)). Fetch
normalizes everything to discussions and events with their evidence
attached, and fails per [§FS-001-forge-interface.6](../functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not): explicitly, visibly,
with last-good kept and marked stale.

## 2. Attribute and merge

The engine of [§AR-003-attribution](AR-003-attribution.md#ar-003-attribution-one-matching-engine-evidence-against-identity-at-two-scopes) places what fetch produced; merge folds
same-key reports into one matter under [§FS-003-feed-categories.5](../functional-spec/FS-003-feed-categories.md#5-one-subject-is-one-row-however-many-sources-reported-it), links
referenced matters ([§FS-007-matters.2](../functional-spec/FS-007-matters.md#2-same-subject-one-matter-related-subjects-linked-matters)), recomputes fingerprints, and
persists the store ([§AR-006-matters.4](AR-006-matters.md#4-the-cache-is-a-cache)). Unattributed remainders persist
too — the bucket is part of the store, not a log line
([§FS-008-attribution.4](../functional-spec/FS-008-attribution.md#4-unattributed-is-a-place-not-a-fate)).

Attribution runs here and nowhere else. It is the expensive stage — an
item's evidence is its whole recorded conversation joined into one string,
matched against a table — so a surface asking it again is asking a
question that was already answered. A screen therefore settles every count
and placement a row shows when it is rebuilt, and drawing reads what is
already decided: a cursor moving does not rebuild, so anything left in the
draw path is paid once per keystroke against the whole feed.

## 3. Offer and summon

Offering is pure: matters × (quick actions, manifest offers, site
configuration) filtered through selectors and the capability table
([§AR-005-capabilities.2](AR-005-capabilities.md#2-features-declare-needs)), provenance-ordered
([§FS-006-project-interface.9](../functional-spec/FS-006-project-interface.md#9-offers-the-projects-actions)). A chosen offer, verb, or dispatch is a
summons; dispatch additionally writes through the runtime module
([§AR-007-runtime](AR-007-runtime.md#ar-007-runtime-the-runtime-adapter-and-rhei-as-its-shipped-binding)) and the ledger records what was handed over with the
matter's fingerprint at that moment ([§FS-005-dispatch.4](../functional-spec/FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)).

## 4. Record and resurface

Seen state (read, done) is the reader's and survives cache rebuilds.
Resurfacing compares stored fingerprints against the freshly merged world:
a dispatched matter that moved is offered reopening with the changed
component named ([§FS-005-dispatch.5](../functional-spec/FS-005-dispatch.md#5-an-item-that-moved-reopens-its-work), [§FS-007-matters.5](../functional-spec/FS-007-matters.md#5-an-event-moves-state-and-resurfacing-names-its-reason)); a done matter
that moved returns unread with the same reason. Nothing resurfaces as a
side effect of merely looking ([§FS-005-dispatch.5](../functional-spec/FS-005-dispatch.md#5-an-item-that-moved-reopens-its-work)).

## 5. Failure is data here too

Every stage reports what it dropped: a source that failed, an item that
would not parse, an ambiguity attribution refused to guess at — absence
and failure visible, per the degrade rule of [§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy). The
pipeline's own health is presented with the feed — the header naming
failed providers, the non-zero exit of a lossy refresh
([§FS-001-forge-interface.6](../functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)) — because a watch that cannot be believed
about itself cannot be believed about the world ([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)).
