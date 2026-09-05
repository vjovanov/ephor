# FS-006-project-interface: a project and ephor meet over one interface, in three homes

ephor requires capabilities of a project, never artifacts in it
([§REQ-001-boundary.3](../requirements/REQ-001-boundary.md#3-requirements-on-a-project-are-capabilities-never-artifacts)): a project is fully watchable exactly as it stands, and
everything beyond watching is something the project *can do* or *chooses to
offer*. This section is the whole of how a project and ephor speak — where
each fact lives, how a command is invoked, what it may answer, and what its
absence degrades to — so that tracking a new project costs minutes and
touches nothing in it ([§GOAL-005-costless](../goals.md#goal-005-costless-watching-costs-the-watched-nothing)). Whatever crosses this interface
in structure is validated against a published schema; nothing crosses it as
linked code.

## 1. The three homes

Every fact the interface carries lives in exactly one of three places.
**Description and identity** live in the registry row: where the forest is,
how a branch becomes a workspace, and the signals by which the project's
matters are recognized ([§FS-008-attribution.1](FS-008-attribution.md#1-identity-is-declared-and-the-row-has-the-last-word)). **Operational bindings**
live in site configuration: which command fills which verb, which runtime
runs work, the person's own actions and recipes. **Conventions** are probed
in the checkout: well-known names a project carries for its own sake. One
precedence resolves them all — *site configuration over manifest over
probe* ([§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)) — so a probe is a default, a manifest is a
declaration, and the person always has the last word.

## 2. The manifest is offered, never required

A project that chooses to speak places one file, `ephor.json`, at its forest
root (decided in [§DF-001-manifest-offered](../decisions/functional/DF-001-manifest-offered.md#df-001-manifest-offered-the-manifest-is-offered-never-required)). It may declare identity hints,
the forest's own layout, check verbs, gate verbs, task stores, and
offers — every field optional, an empty
manifest valid, and nothing in it able to gate a capability that probing or
site configuration could not establish alone. Identity fields are hints the
registry row adopts unless it overrides: **the row is authoritative, because
attribution keys must not be forgeable by a checkout.** Manifest commands
run with exactly the trust a person extends to running the project's own
build, and the row can narrow it — honoring the manifest fully, reading only
its descriptions, or ignoring it — for checkouts trusted less. Offers are
menu entries a person invokes; what spends agent time on its own match (a
recipe) is site configuration only, because a repository does not get to
spend it.

## 3. A summons: environment in, exit code and answer out

Every command the interface names — a check verb, a gate verb, a checkout, a
task store's CLI, an offer — is invoked one way. It runs in the resolved
place: the item's branch workspace where one resolves, the forest root
otherwise, a manifest-designated repository of the forest where the entry
says so. It receives the dossier as `EPHOR_*` environment — one vocabulary,
identical to what a shell action and a state-machine script receive
([§FS-005-dispatch.8](FS-005-dispatch.md#8-the-ticket-carries-the-item-as-data-not-only-as-prose)). It answers first with its exit code — `0` for done,
non-zero for failed, `75` for *parked*: not applicable now, ask again later —
and optionally in structure, written to the file named by `$EPHOR_ANSWER`.
Standard output and error remain the command's own, streamed to the person
or the log; a contract that parsed them would make every honest build log a
protocol violation.

**Paths in that environment are spelled so the shell can read them.** The
command is invoked through a shell, so `$EPHOR_ANSWER` and the rest are
strings that shell parses before anything opens them — and where a platform's
native spelling separates directories with the shell's own escape character, a
path handed over verbatim stops being a path: the redirect lands somewhere
else, or nowhere, and the answer comes back empty with nothing saying why,
which is the silence [§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy) refuses. Path-valued variables use
`/` between their segments on every platform. This costs nothing where the two
spellings already agree, and a command that only passes a path on to another
program sees no difference either way; the place the command runs in is set by
ephor rather than spelled to the shell, so it is not involved.

## 4. The answer envelope

Structured answers share one envelope, speaking the model's nouns
([§FS-007-matters](FS-007-matters.md#fs-007-matters-the-feed-is-made-of-matters-and-a-matter-knows-why-it-is-there)): `matters`, `discussions`, and `events` for sources with
something to report; `summary`, `url`, and `needs_response` for the common
one-line cases; `failures` and `features` as verb-level conveniences that
ephor normalizes into events and facts; `data` as free passthrough that
returns wherever the dossier's metadata goes. Each verb's contract names the
fields it reads and ignores the rest, and unknown fields are ignored
everywhere — the envelope evolves by addition, and an incompatible change is
a version bump with a changelog entry (§11). Paths in an answer resolve
against the summons's working directory.

## 5. Checks are verbs, and every script is self-contained

A project's checks are three well-known names probed at the forest root —
`./check.sh` the aggregate, `./check-style.sh` the fast style pass,
`./smoke-test.sh` the smoke — or the same three declared in the manifest
under whatever paths the project prefers. Each is self-contained: a smoke
test that needs a build performs its build, because how a project builds is
the project's knowledge and stays there. Smoke may enumerate **features** —
`--list` printing one id per line, or a static list in the manifest — and a
feature id given as an argument runs that feature's smoke alone; without
enumeration, smoke is one opaque verb and that is a complete
implementation. Which verbs run, and in what order, is policy above the
interface: a verify step sequences them from site configuration, one
summons each.

## 6. The gate is the project's, in three verbs

How to ask a project's CI what it is doing is project truth — the same for
every person who works on it — so its home is the manifest, with site
configuration overriding where credentials or variants demand. Three verbs:
**status** answers the gate's counts per repository of the forest;
**failures** answers what actually failed, as the failing job, its log, and
the error where it can be had — the expensive question, asked on demand
([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)); **restart** re-runs the gate,
committing nothing, at the scope it is asked for — everything the gate covers,
or only what is not green
([§FS-004-quick-actions.9](FS-004-quick-actions.md#9-a-gate-is-offered-the-restart-in-two-shapes)) —
and, where only what is not green is asked for, that means the failing gate
and every gate downstream of it, under the semantics of [§FS-005-dispatch.11](FS-005-dispatch.md#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed).
The scope crosses the seam as part of the ask, because the cheap re-run and
the expensive one are different questions and only the caller knows which was
meant. A forge-hosted gate needs no manifest at all: the
provider's own gate capability is the shipped default binding. A project
with an internal gate binds three commands, and nothing above the seam can
tell the difference.

## 7. The project's own tasks are read where they live

A project may keep its own work in its checkout — a plan directory, a
git-backed issue store — and a **task store** ephor recognizes is read
through the store's own files and CLI, as matters with their discussions
([§FS-007-matters](FS-007-matters.md#fs-007-matters-the-feed-is-made-of-matters-and-a-matter-knows-why-it-is-there)), into the same feed under the same rules as anything a
forge reported. Recognition is by probed convention or manifest
declaration; attribution is the checkout's own project; and the stores are
project-native things that exist without ephor — a store's presence is a
capability rung, never an obligation.

The word is **task**, and it is one name for one thing
([§FS-001-forge-interface.3](FS-001-forge-interface.md#3-policy-lives-above-the-interface-never-in-an-implementation)): a *ticket* is what a remote tracker keys, an
*issue* is what a forge files, and these are neither — they are the
project's own work, written down in the project's own checkout. So the row
they land on is **Tasks** ([§FS-003-feed-categories.1](FS-003-feed-categories.md#1-the-categories)), the rung is *tasks*
(§10), and the manifest key is `tasks`. What is not one of these keeps its
own name: the ticket ephor writes to dispatch work ([§FS-005-dispatch.3](FS-005-dispatch.md#3-one-rhei-per-item-one-ticket-per-dispatch)) and
the ticket keys a forge is asked for ([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)) are other
things and are called what they are.

**A task in a final state is not read.** Final is the store's own word: what
its state machine declares final, or — where the store declares no machine —
what the runtime's built-in default machine declares, since that is what the
store's own tasks actually run under. Such a task is history the store keeps,
not news the feed carries: it has no activity time of its own beyond its
file's, so under [§FS-003-feed-categories.2](FS-003-feed-categories.md#2-recent) every finished task in a plan would
resurface each time the plan was touched, and a store that is the record of a
project's work would drown the feed in its record. The store keeps the finished
work and answers for it; the feed shows what is open. A store whose machine
cannot be read is a store that did not answer, exactly like a plan that cannot
be read ([§FS-001-forge-interface.6](FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).

**Where they live is per branch, where a project has branch workspaces.**
Work about a change belongs in that change's working tree
([§FS-005-dispatch.3](FS-005-dispatch.md#3-one-rhei-per-item-one-ticket-per-dispatch)), so a branch-addressable project keeps a store per
workspace rather than one at the forest root. Both places are read: the root,
for a project whose checkout is its root, and every branch workspace on disk
that holds one, for a project whose branches have trees of their own. Reading
only the root leaves such a project writing its work into a place it never
looks again — work dispatched, plans on disk, and a feed that shows none of
it.

Verified on disk rather than derived from the registry row. The row names the
branches somebody wrote down; the stores are wherever branches were actually
checked out, and the two are not the same list.

**A workspace ephor makes gets a store.** Where ephor creates a branch
workspace ([§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)) it initializes one there, so the first
dispatch into that branch has somewhere to land and what is under way is
visible from the moment the tree exists. This is not an artifact required of
the project and does not bend [§REQ-001-boundary.3](../requirements/REQ-001-boundary.md#3-requirements-on-a-project-are-capabilities-never-artifacts): the store ignores itself,
so what it holds is ephor's own planning state that happens to live in a
checkout, never content the project carries. A project cloned without ephor
is byte-for-byte what it was.

**The runtime makes its own project; ephor says where.** What a runtime
project consists of is the runtime's answer, so ephor asks the runner for one
rather than writing the runner's files out of a copy of that answer compiled
into ephor — two answers to one question drift the first time the runner
changes its mind, and the reader who then runs the runner by hand is the one
who finds out. What ephor supplies is the directory: the work root it already
resolves for this workspace, so a person who moved the work root gets the
project there rather than wherever the runner would have put it left to
itself. Ephor's own state machine ([§FS-005-dispatch.6](FS-005-dispatch.md#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)) is installed beside
what the runner wrote, and the self-ignore above stays ephor's whatever the
runner's project would say about version control — the paragraph above is a
promise about the checkout, and the runner has made none. For the same reason
the runner is asked not to leave its own discovery note beside the project: a
note in the checkout's `AGENTS.md` is a change to the branch, tracked or
untracked, and ephor said there would be none. Where the runner is not on the
machine, ephor writes the store it can and says what it could not do: a
checkout that failed because a convenience did is a checkout that did not need
to fail.

## 8. The checkout contract

A project may bind one **checkout** command; its contract is to make
`$EPHOR_WORKSPACE` exist, and ephor verifies the directory afterwards
rather than trusting the exit code alone. Where none is bound, ephor
supplies the git checkout itself ([§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)). Everything that
needs a workspace — offers marked as requiring one, work whose recipe edits
the change — is gated on this contract and degrades by naming it.

## 9. Offers: the project's actions

A manifest may offer actions: entries for the same menu configured actions
occupy, in the same shape, selected by the same `when` language recipes use,
and gated by the same capability requirements. Provenance orders the menu —
what ephor itself recognized first ([§FS-004-quick-actions.3](FS-004-quick-actions.md#3-quick-actions-come-first-and-configuration-adds-to-them)), then the
project's offers, then the person's own — and where two entries share an id,
the person's beats the project's beats the shipped one. An offer is invoked
by a person, runs as a summons (§3), and is refused with its reason where
its requirements do not hold ([§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)). It takes the reader's
terminal while it runs, which is what lets an offer be a pager or an editor —
and an offer that needs none of that says so, and runs beneath the screen as a
job instead ([§FS-005-dispatch.17](FS-005-dispatch.md#17-a-move-that-needs-nobody-runs-beneath-the-screen)). An offer that is such a program and should
not take the terminal says `window` instead, and runs in a window of the
reader's own where one is bound — still something that takes a person, now
beside ephor rather than in its place ([§FS-005-dispatch.22](FS-005-dispatch.md#22-a-window-of-the-readers-own-where-one-is-bound)).

**An offer may name a workflow instead.** Where the runtime carries whole
workflows of its own — parameterized plans that lay down tasks of their own —
an entry naming one is the same entry, in the same menu, under the same
provenance, and what it answers that workflow's inputs with is written beside
the rest of it
([§FS-005-dispatch.19](FS-005-dispatch.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)).
The manifest is one of the three places such an entry may be written, for the
reason it holds the others: which of the runtime's workflows are worth offering
here, and about what, is the project's to say.

**Who does an action is the project's to default.** Work that needs judgment
goes to a hand ([§FS-005-dispatch.14](FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)), and `work.hands` maps an action's id to
the one that does it — `{ "default": "sonnet", "rebase": "luna:high",
"fix-gate": "gpt-5:high" }` — with `default` answering for every id the table
does not name. The id is the menu's own, so an offer, a configured action and
a recipe are all named the same way and a project learns no second vocabulary.
The table exists per project and at site level, because the alternative — one
hand for everything — is either the deep hand on every trivial replay or the
cheap one on the conflict that actually needed judgment, and that choice is
being made today anyway, silently, by whatever the runtime would pick.

**An entry may name an ordered list instead of one hand.** Every place this
table writes a hand — an action's id, `default`, the pin a recipe carries, and
`--hand` on the command line, which writes the same grammar the tables do —
takes either one name or an ordered list of them, and one name is the list
with a single member ([§FS-005-dispatch.14](FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)). In configuration a list is a JSON
array of the same names the scalar form takes, which is the only thing an
array of hands can mean here: the long form below is an *object* naming its
halves, so reading `["a", "b"]` as a pair spelled out positionally would let a
list of two alternates arrive as one hand nobody wrote, and it would do it
silently. On the command line a list is the names separated by commas, because
a flag that can be given twice would make the order depend on how the shell
was typed; an empty member is refused with what was written, since a trailing
comma is a typo far more often than it is a name.

**Permission is checked against every member, and a list is refused whole.**
Where a project narrows the roster, one unpermitted name in a list refuses the
whole list, that name is in the refusal, and no member of it is used — never
filtered down to the members that are permitted. It is the same argument this
section already makes about a narrowing that fails quietly, and it holds
harder here: a list is where a name can be dropped without the person noticing
they wrote it, and a policy that quietly used the second choice is
indistinguishable from a policy that was never asked. Which member finally
does the work is decided after this, from evidence rather than from policy,
and it may pass a permitted member over but may never reach past the list for
one that was refused ([§FS-005-dispatch.29](FS-005-dispatch.md#29-headroom-is-reported-to-ephor-and-vetoes-a-member-it-never-reorders)).

A hand is named `<hand-id>[:<effort>]`, both the roster's own words
([§FS-005-dispatch.14](FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)). The long form `{ "agent", "model", "effort" }` stays
legal for a pair the runtime's registry never enumerated — a proxy serving a
model it does not list — and is **accepted with a note, never refused**: ephor
cannot prove such a pair invalid, and refusing it would make ephor's
configuration a smaller world than the runtime's, which sends the person back
to configuring the runtime directly and defeats the table. A name the roster
does list is checked against it: an effort the hand does not declare is
refused with the ones it does, and a name carrying no effort is settled by
what the hand declares — completed, refused, or asked plainly
([§FS-005-dispatch.14](FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).

The project's table is read before the site's, each narrow before broad: this
action's id, then `default`, then the site's the same way. That is the middle
of the seven steps [§FS-005-dispatch.14](FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project) sets, and neither end moves — what the
reader picked for this dispatch alone still displaces every table, and what
nobody chose at all is still the runtime's to pick unasked.

**A project may narrow the roster.** `work.permitted_hands` lists the hands
that may be used on it at all, which is what a repository under a policy about
which models may see its code needs. A hand outside the list is refused with
that reason wherever it was named — the project's own table, the site's, the
pin an action carries, the reader's choice at the moment of asking — and never
silently dropped or silently replaced, because a policy that fails quietly is
indistinguishable from a typo and the person would learn which it was from the
other side. The check is against the name, not against the roster, so it holds
with no runtime bound too. A hand spelled out in full is refused under a
narrowing for the same reason a name outside it is: nothing in the list
authorized it. What a narrowing cannot bind is the runtime's own unasked pick,
so a project that narrows and names no `default` is told that much.

Absence is the ordinary case: with no table anywhere nobody is named and the
runtime picks exactly as it does now. With no runtime bound there is no roster
to name a hand from, so a configured hand resolves to nothing and says so in
the *workable* rung's own words (§10) rather than failing the dispatch — the
ticket is written as it would have been, because who does the work is not what
makes a ticket ([§FS-005-dispatch.4](FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)).

## 10. Capability, rung by rung

What a project can do is resolved into a ladder, and every feature names
the rungs it needs: *observable* (a registry row and at least one source
answering) buys the watch; *placed* (the forest root on disk) buys actions
and update; *branch-addressable* (a workspace template) buys resolution of
matters to workspaces; *checkout-able* (§8) buys work that edits;
*checkable* (§5) buys verification that means something; *gated* (§6) buys
failure dossiers and the restart; *tasks* (§7) buys the project's
own tasks as matters — a `requires` naming either of its older spellings,
*ticketed* or *local-issues*, goes on meaning it;
*workable* (a bound runtime, [§FS-005-dispatch](FS-005-dispatch.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)) buys the loop. A missing
rung degrades exactly the features that named it, with the reason stated
where the feature would have appeared — never an error, never silence
([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)).

## 11. The interface is versioned

The manifest, the envelope, and the registry schema are published schemas,
embedded in the binary and printable on demand, so a project can validate
what it says without ephor present. They evolve by addition: an optional
field costs nothing, unknown fields are ignored, and any incompatible
change bumps the schema version with a changelog entry per
[§FS-002-release.1](FS-002-release.md#1-changelog). The schemas are the interface's stability surface — what
a release may change is answerable by diffing them.


