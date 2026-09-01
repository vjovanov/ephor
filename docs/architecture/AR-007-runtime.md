# AR-007-runtime: the runtime adapter, and rhei as its shipped binding

Everything runtime-specific lives in one module — `work/runtime/` in the
target layout — and the module realizes the boundary of the
[§FS-005-dispatch](../../requirements.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime) lead: the plan language, the runner, and the read-back are
the whole coupling, and no `rhei` literal exists outside this module,
shipped assets, examples, and documentation ([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).

## 1. What the module owns

Writing: the plan file per matter, tickets appended in order, the dossier
as prose and the identifiers as metadata ([§FS-005-dispatch.2](../../requirements.md#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it), .8), id
normalization to the plan language's grammar, and the state-machine asset
installed into an empty work root — never over an existing one
([§FS-005-dispatch.6](../../requirements.md#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)). Running: the invocation of the bound runner as a
summons ([§AR-002-summons](AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world)), with `work.runner` from site configuration
defaulting to the shipped runtime. Reading: work state from the plan, the
verdict line from results, the parked states a machine will not leave on
its own ([§FS-005-dispatch.9](../../requirements.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)) — and a result carrying a **proposed answer**
([§FS-005-dispatch.13](../../requirements.md#13-a-communication-is-work-too-and-its-answer-comes-back-as-a-proposal)), read back and attached to the matter beside the
discussion it answers, for the surfaces to offer where the channel can
post it. Rostering: the enumeration of who can be asked
([§FS-005-dispatch.14](../../requirements.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)) — each hand read from the binding's own registry of
agents and model profiles with its efforts and its availability
([§DA-004-roster-is-asked-not-configured](../decisions/architectural/DA-004-roster-is-asked-not-configured.md#da-004-roster-is-asked-not-configured-the-roster-is-asked-of-the-binding-never-kept-by-ephor)), and the rendering of a chosen
hand into the binding's own words, here and nowhere else, so that above
this module a hand is an opaque id. That rendering has two grammars,
because the choice binds in one of two spellings ([§FS-005-dispatch.14](../../requirements.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)): a
hand carrying a model becomes the plan language's execution-target line on
the ticket, and a hand naming an agent and no model — which the plan
language cannot spell — becomes the runner's own agent flags on the run
invocation, declared beside the plan flag as part of the same coupling. Watching:
whether a run is live on an execution root, read from the binding's own
artifacts and never from a process table ([§FS-005-dispatch.15](../../requirements.md#15-every-operation-is-visible-in-one-place)) — the
per-root run lock, probed with a non-blocking try-lock because the runner
takes it with a blocking acquire and a waiting probe would queue ephor
behind the very run it asks about; the live run's own event stream for what
that run has taken up and let go, read from its own beginning and resumed by
the sequence a reader has already seen, with the transition journal and the
newest matching agent log as the floor beneath it where a runner writes no
such stream ([§FS-005-dispatch.15.2](../../requirements.md#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)) — one task having many logs across
states and visits; the dashboard address a live run publishes,
per run rather than per ticket; and the advance and release commands in the
runner's own words ([§FS-005-dispatch.10](../../requirements.md#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)). Cancelling: the transition of
one ticket into the abandonment state, composed in the runner's own
words — plan, ticket, expected state, target, and the reader's reason as
the result — and run as a captured summons from the work root, with the
runner's own refusal lifted from what it printed ([§FS-005-dispatch.16](../../requirements.md#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so),
[§DA-005-cancel-is-the-runtimes-move](../decisions/architectural/DA-005-cancel-is-the-runtimes-move.md#da-005-cancel-is-the-runtimes-move-cancelling-a-ticket-asks-the-runtime-and-never-rewrites-the-state-line)); the abandonment state's name is the
plan language's and is spelled here and nowhere else, so above this
module a surface asks only whether the machine in force declares one and
whether a ticket sits in it. Where the binary itself is
present, its own plan listing (`rhei list --json`) may sharpen state and
assignee read-back — the binding's own stdout, honored by this one binding
the way custom-status's is ([§AR-002-summons.3](AR-002-summons.md#3-the-answer)) — while the direct plan read
of §3 stays the floor and is never removed ([§FS-005-dispatch.15](../../requirements.md#15-every-operation-is-visible-in-one-place)). Also
this module's: the recognition of a plan on disk. The plan-file suffix and
the directory-workspace shape are the binding's grammar, so a surface that
enumerates a work root's plans ([§FS-005-dispatch.15](../../requirements.md#15-every-operation-is-visible-in-one-place)) asks this module what
the directory holds and gets back plan ids and paths — the suffix is never
spelled above it ([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).

Instantiating: the binding's own **workflows** — what it offers and what
each one takes ([§FS-005-dispatch.19](../../requirements.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)). Enumerating them asks the binding, for
the reason the roster does ([§DA-004-roster-is-asked-not-configured](../decisions/architectural/DA-004-roster-is-asked-not-configured.md#da-004-roster-is-asked-not-configured-the-roster-is-asked-of-the-binding-never-kept-by-ephor)): the
listing is the binding's own stdout (`rhei templates --json`), honored by this
one binding the way custom-status's is ([§AR-002-summons.3](AR-002-summons.md#3-the-answer)), and what goes up
from here is an id, a description, and typed inputs with none of the binding's
grammar on them. Two things are read beside a workflow the binding keeps as a
directory: the input properties its own listing leaves out — which input is an
execution target, which is the principal one — scanned out of the manifest the
way a states document is scanned in §3, right enough to fill an input and never
the authority on anything else; and the entry that makes the workflow an
action, handed up as bytes, because this module knows where such a file sits
and never what it means. Rendering one is a summons like any other: the
values supplied by the reader's YAML or JSON files and explicit answers
resolved into the binding's own values file, the output directory named under
the work root, and the workspace left behind recognized
by the plan reading below — so a workflow's plan is an operation the moment it
exists, and asks nothing further of this module. The hand grammar stays here as
everywhere: an input the workflow declares an execution target for is filled
with a chosen hand rendered into the binding's selector, the same rendering a
ticket's execution line takes ([§FS-005-dispatch.14](../../requirements.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).

Detaching and attaching: the run started beneath the screen and the surface
put on it afterwards ([§FS-005-dispatch.20](../../requirements.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)). The detached invocation is the
runner's own flag beside the plan flag (`rhei run --headless`), part of the
same coupling, and what the launcher prints is read for the run's id; the
run's **identity** — id, pid, and the control address it serves while it
serves one — is read from the descriptor the binding writes beside its lock
(`runtime/run.json`), probed with the lock and never remembered, so a run
ephor did not start has the same identity as one it did. The attach verb
(`rhei attach <id>`) is composed here as a summons the reader types into and
handed up as one — the executor decides whether it gets the terminal or a
window ([§AR-002-summons.6](AR-002-summons.md#6-windowed-the-readers-own-window)) — and the stop command is composed here in the
runner's own words and only ever shown ([§FS-005-dispatch.20](../../requirements.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)), as the release
command is. Whether the binding *can* detach on this platform is this
module's answer too, so the surfaces ask it before offering: where it cannot,
the run is the attached invocation of before, and the outcome line says so.

## 2. What stays outside

The ledger is the engine's and runtime-agnostic: it records what was
dispatched and fingerprints, never work state ([§FS-005-dispatch.4](../../requirements.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)).
Recipes, briefs, and selectors are configuration. The dossier is
materialized by core ([§AR-006-matters.3](AR-006-matters.md#3-the-dossier-is-a-view)). A second runtime is a second
binding for this module's verbs — plan writing in its language, its runner
command, its read-back — selected by configuration, with no change above
the module.

## 3. Degrade

With no runner bound or installed, writing and reading work unchanged —
tickets accumulate on disk, readable and hand-editable — and running
refuses with the configured runner named ([§FS-006-project-interface.10](../../requirements.md#10-capability-rung-by-rung),
the workable rung). The refusal is the capability table's sentence
([§AR-005-capabilities.2](AR-005-capabilities.md#2-features-declare-needs)), not a spawn error. The roster is empty then, in
the same rung's own sentence ([§FS-005-dispatch.14](../../requirements.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)): who can be asked is
the runtime's knowledge, and nothing else in ephor changes. The workflows
are empty then for the same reason — what the binding offers is the
binding's to say ([§FS-005-dispatch.19](../../requirements.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)) — and an entry naming one is refused
in that rung's sentence rather than dropped from the menu; a plan some
workflow laid down before is unaffected, since reading plans never depended
on the binary. The operations
board is then the refresh row alone ([§FS-005-dispatch.15](../../requirements.md#15-every-operation-is-visible-in-one-place)) — an operation is
a run, and where nothing can run there are none. Enumerating a work root's
plans is part of reading, not of running: a directory listing against the
binding's own naming, with no runner asked — so every plan is still found
and still readable with no runner installed. A runner that is bound and
present but cannot detach — an older one, or a platform with no detached
shape — runs attached, the terminal handed over, and a run's identity is
whatever descriptor it leaves; where it leaves none there is no id to show
and no surface to attach, and the board says *live* from the lock alone
([§FS-005-dispatch.20](../../requirements.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)).
