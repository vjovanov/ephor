# AR-007-runtime: the runtime adapter, and rhei as its shipped binding

Everything runtime-specific lives in one module — `work/runtime/` in the
target layout — and the module realizes the boundary of the
§FS-005-dispatch lead: the plan language, the runner, and the read-back are
the whole coupling, and no `rhei` literal exists outside this module,
shipped assets, examples, and documentation (§REQ-001-boundary.5).

## 1. What the module owns

Writing: the plan file per matter, tickets appended in order, the dossier
as prose and the identifiers as metadata (§FS-005-dispatch.2, .8), id
normalization to the plan language's grammar, and the state-machine asset
installed into an empty work root — never over an existing one
(§FS-005-dispatch.6). Running: the invocation of the bound runner as a
summons (§AR-002-summons), with `work.runner` from site configuration
defaulting to the shipped runtime. Reading: work state from the plan, the
verdict line from results, the parked states a machine will not leave on
its own (§FS-005-dispatch.9) — and a result carrying a **proposed answer**
(§FS-005-dispatch.13), read back and attached to the matter beside the
discussion it answers, for the surfaces to offer where the channel can
post it.

## 2. What stays outside

The ledger is the engine's and runtime-agnostic: it records what was
dispatched and fingerprints, never work state (§FS-005-dispatch.4).
Recipes, briefs, and selectors are configuration. The dossier is
materialized by core (§AR-006-matters.3). A second runtime is a second
binding for this module's verbs — plan writing in its language, its runner
command, its read-back — selected by configuration, with no change above
the module.

## 3. Degrade

With no runner bound or installed, writing and reading work unchanged —
tickets accumulate on disk, readable and hand-editable — and running
refuses with the configured runner named (§FS-006-project-interface.10,
the workable rung). The refusal is the capability table's sentence
(§AR-005-capabilities.2), not a spawn error.
