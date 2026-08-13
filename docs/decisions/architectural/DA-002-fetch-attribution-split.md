# DA-002-fetch-attribution-split: fetch normalizes, attribution places

**Status:** Accepted
**Date:** 2026-08-13

Sources used to place their own items: each provider decided which project
its findings belonged to, branch matching lived beside fetching, and
`github-notifications` — the one source whose job is to be exhaustive —
hand-rolled placement for items no configuration had scoped. The split
makes placement one job done once: fetch normalizes everything to
discussions and events with their evidence attached (§AR-008-pipeline.1),
and one pure engine — evidence against identity, no IO — places them in
the attribute stage (§AR-003-attribution), realizing §FS-008-attribution.
No source places its own items.

## 1. Why one engine

Placement rules multiply when each source owns them; they converge when
one engine does. The engine is a function of evidence against the compiled
identity table, so a misplacement is debugged by looking at data on the
item rather than rereading a provider (§AR-003-attribution.1), ambiguity
lands in the visible unattributed bucket instead of a silent guess
(§FS-008-attribution.4), and a new source is a normalizer, not an
architecture change — after the split, mail and Slack are providers.

## 2. The accepted cost: status.json restructures

A source shared by many projects — a mailbox, a notification stream —
cannot stay declared per project once no source places its own items.
`status.json` restructures: **sources move to site level, shared; what
remains under `projects` is per-project bindings** — the shape the three
homes prescribe (§REQ-001-boundary.2). This is the design's one real
breaking change to configuration, accepted deliberately: ephor reads the
legacy shape with a deprecation note for a release or two, and the feed
cache is a cache — it is rebuilt, never migrated.
