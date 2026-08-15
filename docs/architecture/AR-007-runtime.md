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
post it. Rostering: the enumeration of who can be asked
(§FS-005-dispatch.14) — each hand read from the binding's own registry of
agents and model profiles with its efforts and its availability
(§DA-004-roster-is-asked-not-configured), and the rendering of a chosen
hand into the binding's own words, here and nowhere else, so that above
this module a hand is an opaque id. That rendering has two grammars,
because the choice binds in one of two spellings (§FS-005-dispatch.14): a
hand carrying a model becomes the plan language's execution-target line on
the ticket, and a hand naming an agent and no model — which the plan
language cannot spell — becomes the runner's own agent flags on the run
invocation, declared beside the plan flag as part of the same coupling. Watching:
whether a run is live on an execution root, read from the binding's own
artifacts and never from a process table (§FS-005-dispatch.15) — the
per-root run lock, probed with a non-blocking try-lock because the runner
takes it with a blocking acquire and a waiting probe would queue ephor
behind the very run it asks about; the transition journal and the newest
matching agent log for which tickets a live run holds, one task having many
logs across states and visits; the dashboard address a live run publishes,
per run rather than per ticket; and the advance and release commands in the
runner's own words (§FS-005-dispatch.10). Where the binary itself is
present, its own plan listing (`rhei list --json`) may sharpen state and
assignee read-back — the binding's own stdout, honored by this one binding
the way custom-status's is (§AR-002-summons.3) — while the direct plan read
of §3 stays the floor and is never removed (§FS-005-dispatch.15).

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
(§AR-005-capabilities.2), not a spawn error. The roster is empty then, in
the same rung's own sentence (§FS-005-dispatch.14): who can be asked is
the runtime's knowledge, and nothing else in ephor changes. The operations
board is then the refresh row alone (§FS-005-dispatch.15) — an operation is
a run, and where nothing can run there are none.
