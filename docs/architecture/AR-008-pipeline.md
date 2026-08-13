# AR-008-pipeline: the engine is seven stages, legible as one function

`fetch → attribute → merge → offer → summon → record → resurface` — the
whole product is this loop, and the engine keeps it readable as one
function. The first three run on refresh; offering runs when a surface
asks; summoning, recording, and resurfacing run as the person or the sync
drives them.

## 1. Fetch

Remote sources fetch per the §FS-001-forge-interface capability set —
shared sources (a mailbox, a notification stream) once per site, per-repo
sources per project — and checkout sources read the forests: git probes,
custom-status, ticket stores, each a summons (§AR-002-summons). Fetch
normalizes everything to discussions and events with their evidence
attached, and fails per §FS-001-forge-interface.6: explicitly, visibly,
with last-good kept and marked stale.

## 2. Attribute and merge

The engine of §AR-003-attribution places what fetch produced; merge folds
same-key reports into one matter under §FS-003-feed-categories.5, links
referenced matters (§FS-007-matters.2), recomputes fingerprints, and
persists the store (§AR-006-matters.4). Unattributed remainders persist
too — the bucket is part of the store, not a log line
(§FS-008-attribution.4).

## 3. Offer and summon

Offering is pure: matters × (quick actions, manifest offers, site
configuration) filtered through selectors and the capability table
(§AR-005-capabilities.2), provenance-ordered
(§FS-006-project-interface.9). A chosen offer, verb, or dispatch is a
summons; dispatch additionally writes through the runtime module
(§AR-007-runtime) and the ledger records what was handed over with the
matter's fingerprint at that moment (§FS-005-dispatch.4).

## 4. Record and resurface

Seen state (read, done) is the reader's and survives cache rebuilds.
Resurfacing compares stored fingerprints against the freshly merged world:
a dispatched matter that moved is offered reopening with the changed
component named (§FS-005-dispatch.5, §FS-007-matters.5); a done matter
that moved returns unread with the same reason. Nothing resurfaces as a
side effect of merely looking (§FS-005-dispatch.5).

## 5. Failure is data here too

Every stage reports what it dropped: a source that failed, an item that
would not parse, an ambiguity attribution refused to guess at — absence
and failure visible, per the degrade rule of §REQ-001-boundary.1. The
pipeline's own health is presented with the feed — the header naming
failed providers, the non-zero exit of a lossy refresh
(§FS-001-forge-interface.6) — because a watch that cannot be believed
about itself cannot be believed about the world (§GOAL-003-nothing-lost).
