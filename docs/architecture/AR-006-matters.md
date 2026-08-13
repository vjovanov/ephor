# AR-006-matters: the core types of the watch

The nouns of §FS-007-matters as data. These types are core-layer
(§AR-001-layers.1): no source, seam, or surface adds fields of its own —
what a provider knows beyond the model rides in `raw` passthrough and comes
back out in `EPHOR_RAW`.

## 1. The types

- `Matter { key, kind, placement, state, links, discussions, events,
  fingerprint, seen }` — `key` is the stated subject
  (§FS-007-matters.1): `gh:owner/repo#123`, `ticket:GR-73955`, a store's
  own id, `topic:<digest>`. `placement` is project and branch or
  unattributed-with-candidates. `links` are referenced keys
  (§FS-007-matters.2).
- `Discussion { channel, messages, needs_response }`;
  `Message { author, time, text, reactions, task }` — task state carried
  where a channel tracks one (§FS-003-feed-categories.4).
- `Channel { id, capabilities }` — react, tick, reply
  (§FS-007-matters.4).
- `Event { kind, time, payload }` — gate counts per repository, state
  transitions, check results (§FS-007-matters.5).

## 2. Merging and fingerprints

Merge happens on identical `key` at the pipeline's merge stage, the richer
report surviving and the loser's unique facts carried over
(§FS-003-feed-categories.5). The fingerprint digests state, each
discussion's (last activity, message count, task states), and the event
tail; comparing fingerprints is how sync finds moved matters, and the
differing component is how the row names its reason for resurfacing
(§FS-005-dispatch.5, §FS-007-matters.5).

## 3. The dossier is a view

A dossier is materialized from a matter on demand — state, placement,
forest and workspace, gate breakdown, discussions quoted under the bounds
of §FS-005-dispatch.2 — in two renderings from one source: prose for the
ticket, `EPHOR_*` identifiers for programs (§FS-005-dispatch.8), the
materials that cross the seam (§REQ-001-boundary.1). It is never stored: a
stored dossier would be a second truth about the matter.

## 4. The cache is a cache

The feed store under the state directory persists matters between
refreshes and nothing else is derived from it that cannot be re-fetched.
Model changes rebuild it; `seen` state (read, done) is the one part carried
across rebuilds, keyed by matter key, because it is the reader's and not
the world's.
