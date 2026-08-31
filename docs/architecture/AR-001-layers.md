# AR-001-layers: five layers, one binary, and the core depends on nothing

ephor is one binary in five layers, and the whole of [§REQ-001-boundary](../requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-lacks-crosses-a-seam-and-the-seam-has-one-anatomy) is
enforced by where code is allowed to sit. Dependencies point inward only:
surfaces → engine → seams and sources → core, and core depends on nothing —
no IO, no vendor, no product.

## 1. The layers

- **core** — the domain: Matter, Discussion, Channel, Event
  ([§AR-006-matters](AR-006-matters.md#ar-006-matters-the-core-types-of-the-watch)), Project, Identity, Forest ([§AR-004-forest](AR-004-forest.md#ar-004-forest-git-is-the-substrate-and-a-project-is-a-forest-folded-over)),
  CapabilitySet ([§AR-005-capabilities](AR-005-capabilities.md#ar-005-capabilities-availability-is-computed-once-and-consulted-everywhere)), Dossier, Summons and Answer
  ([§AR-002-summons](AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world)), the ledger's types, fingerprints. Pure data and pure
  functions; testable without a filesystem.
- **sources** — what fetches: remote providers behind the
  [§FS-001-forge-interface](../../requirements.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface) capability set, and checkout sources (the git
  prober, custom-status, task-store readers). Each adapter owns its vendor
  literal; nothing else does.
- **seams** — what binds and runs: binding resolution
  (site configuration over manifest over probe, [§FS-006-project-interface.1](../../requirements.md#1-the-three-homes)),
  the summons executor and answer reader, its detached form — the job
  ([§AR-002-summons.5](AR-002-summons.md#5-detached-the-job)) — and the verb modules: checks, gate, checkout, task
  stores, runtime ([§AR-007-runtime](AR-007-runtime.md#ar-007-runtime-the-runtime-adapter-and-rhei-as-its-shipped-binding)).
- **engine** — the pipeline of [§AR-008-pipeline](AR-008-pipeline.md#ar-008-pipeline-the-engine-is-seven-stages-legible-as-one-function), the cache, refresh, and the
  work ledger's operations.
- **surfaces** — CLI, TUI, status widget, JSON output. Presentation only;
  a surface never talks to a source or a seam except through the engine.

## 2. Where literals live

`gh`, `rhei`, `beads`, a mail host, a chat vendor: each appears in exactly
one adapter under sources or seams, in shipped assets and examples, and in
documentation — never in core, engine, or surfaces ([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).

This is a build failure, not a review comment. `scripts/check_boundary.py`
runs in `just check` and in CI, beside the site-word check that guards
[§FS-001-forge-interface.5](../../requirements.md#5-no-site-specific-data-in-the-repository), and it holds the two halves of this page at once:

1. **No product name outside its adapter.** Every name is declared with the
   files allowed to spell it, and the Rust sources under `src/` are read with
   comments and `#[cfg(test)]` bodies left out — documentation and examples
   are what the law permits. A migration that has not happened yet is one
   pinned entry on the script's ledger, naming the file and the one spelling
   it excuses; an entry that stops matching fails the check, so the debt list
   only shrinks.
2. **Core reaches nothing above it.** The `crate::` paths a core module names
   must themselves be core, and no filesystem, process, or network API may
   appear outside a test body — which is what makes "core compiles without the
   rest" a property of the tree rather than a claim in a comment.

## 3. From today's tree

The layers are a target the existing modules migrate toward, not a rewrite:
`src/feed/providers/` becomes sources; the action-menu spawn in the TUI, the
custom-status runner, and `src/work/commands.rs`'s process code converge on
the one executor; `src/registry.rs` splits its descriptive types (core) from
its file reading (engine); `src/work/` divides between the ledger (engine),
the plan language ([§AR-007-runtime](AR-007-runtime.md#ar-007-runtime-the-runtime-adapter-and-rhei-as-its-shipped-binding)), and its types (core). The migration
order is the implementation plan's; this page only fixes where each piece
belongs when it lands.

What §2 enforces today is the part of core that is already pure: the matter
model, attribution, ticket keys, the feed's item and gate types. `Forest` is
core by §1 and not yet core by structure — it asks the git prober what is on
disk — and it joins the enforced list when that prober moves to sources. The
list is in the script and it never shrinks: a module that has been made pure
stays pure.
