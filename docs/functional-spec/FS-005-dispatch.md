# FS-005-dispatch: what ephor watches, it can hand to an agent runtime

A watch that only watches hands its reader a list. Nearly every row on that
list has an obvious next move — the gate is red so the failures need reading
and fixing, a reviewer asked a question so it needs answering, an issue was
filed so it needs doing — and every one of those moves is the same shape: read
a change in a checkout, do something small, say what was done. That is work an
agent can be asked to do, and asking it is the boring half of the day.

ephor does not do that work. It **dispatches** it: it turns an item into a
ticket in an agent runtime, hands over what it already knows, and then keeps
the ledger — which items have work under way, what that work reached, and
whether the item has moved since. Watching and working are one loop, ephor is
the half that remembers, and the routine moves leave the reader's hands
([§GOAL-004-handover](../goals.md#goal-004-handover-routine-moves-leave-the-persons-hands)).

The runtime is a binding with [rhei](https://github.com/vjovanov/rhei) as the
shipped default ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy), decided with its tradeoff recorded in
[§DA-001-runtime-bound-default](../decisions/architectural/DA-001-runtime-bound-default.md#da-001-runtime-bound-default-the-runtime-is-a-bound-default-not-a-named-coupling)). What ephor writes is a plan file in a
documented plain-text language, and that language — together with the runner
command configured to execute it and the verdict read back from its results —
is the entire coupling: a contract in files, never a linked process. Choosing
a runtime remains a property of how a person works, which is why one ships
wired and ready; requiring it would be something else. Nothing in ephor's
core names the default runner, and with no runner installed every part of
dispatch except the running still holds — tickets are written, read, and
reopened, staying readable, diffable, and hand-editable on disk — while
running refuses with the configured runner named.

## 1. A recipe decides which items deserve work, and what to ask for

A **recipe** is a named piece of configuration with a selector and a brief: the
selector says which items it applies to — kind, role, whether the gate is red,
whether a response is owed, which source reported it — and the brief is what
the ticket asks for, in the reader's own words.

Recipes are how the same watch serves different projects: what to do about a
red gate in one repository is not what to do about it in another, and neither
is ephor's to decide. A recipe is therefore configuration first. ephor ships
the few that are true everywhere — the red gate, the unanswered conversation,
the review, the issue, the branch that has fallen behind — for the same reason
it ships quick actions
([§FS-004-quick-actions](FS-004-quick-actions.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)):
a problem ephor already recognizes should not need to be described to it
before anything can be done about it. Configuration adds recipes, and a
configured recipe that reuses a shipped one's name replaces it.

The shipped `implement` recipe is the exception to the otherwise branch-neutral
defaults: it carries `"branch": "fix/issue-{number}"`. With no configured
replacement, an issue with no branch on a project that has
`branch_root_template` is dispatched inside the deterministically minted
`fix/issue-<number>` workspace; on a project without branch workspaces it is
refused by name with the configuration needed to proceed. An issue or pull
request that already has a forge branch, or a registry branch of its own —
never the project's configured main branch, which is the trunk every
workspace is grown from and not a matter's own
([§25](#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs))
— keeps that branch, and a configured recipe named `implement` replaces this
default with its own branch semantics.

**A recipe is an action.** The recipes and the quick actions are one menu, not
two lists behind two keys: *what can I do about this row* has one answer, and
which half of it the reader sees does not depend on which key they happened to
learn. A recipe stands among the entries a source, a project and the reader
wrote
([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)),
selected by the same language, ordered by the same provenance, and refused in
the same sentence — marked as work to hand over and saying who would get it
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project))
before the key is pressed, because that is the difference the reader is
choosing between: an entry that runs something here, and an entry that opens a
ticket asking somebody else.

It runs both ways. An entry may carry a **brief instead of a command** — the
same selector, the same brief, the same hand — which is how a project offers
agent work of its own without writing a separate list; such an entry is a
recipe under another name, and is dispatched as one. And an entry is offered
only where it would work
([§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)):
work about a change is offered where the change is on the machine, and never
about an item that is finished
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)).

Where an entry already in the menu carries a recipe's name, that recipe is
what the entry hands over when it cannot finish, not a second thing to do
about the row: the key that replays a branch and the ticket about the conflict
it stops at are one operation under one name
([§FS-004-quick-actions.6](FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase),
[§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)), and a
menu offering both would be asking the reader to tell two spellings of one
thing apart. Because they are one name, they are gated as one: what a recipe
applies to and what the entry that dispatches it is offered on cannot be
different sets, or the entry hands over work its own recipe says does not
apply here.

Handing work over from the menu is the same handing-over the work screen does
— one plan, one ticket, one ledger entry
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch),
[§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)) —
because where the reader pressed is not a fact about the work. And with no
runner bound the entries are still there: a ticket is written whether or not
anything can run it, and where the entry would say who gets it, it says
instead that nobody can be asked
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)).

## 2. The ticket carries what ephor knows, not a link to it

A ticket that says "look at pull request 42" has handed back the whole job. The
watch already holds what the work needs: the title and state, the branch and
the checkout it lives in, the gate's counts per repository and the forge's own
reasons for refusing the merge, and the conversation as messages with their
authors and times. All of it was fetched already, and it is on disk.

So the ticket carries it — a **dossier** written into the plan, and the ask
written under it. Two things follow. The work starts from what a person would
have read first, instead of spending its opening move re-fetching what ephor
had. And the dossier is a record: it says what the item looked like when the
work was asked for, which is the only way to read the result of that work
later.

A dossier is bounded. A conversation of two hundred messages is not evidence,
it is a transcript; what is quoted is bounded per thread and in total, and
where anything was dropped the ticket says so and links to the whole.

## 3. One rhei per item, one ticket per dispatch

An item's work lives in one plan named after the item — so its whole history is
one file, and dispatching a second recipe on the same item adds a ticket to it
rather than starting a rival copy of the same work somewhere else.

The plan is created in the project the item belongs to, in the checkout the
item's branch resolves to — the same resolution actions already use
([§FS-004-quick-actions.1](FS-004-quick-actions.md#1-a-quick-action-belongs-to-the-source-that-found-the-problem)).
Work about a branch belongs in that branch's working tree: it is where the
change is, where the tools run, and where the runtime will put the agent. Where
the branch is not checked out, dispatch says so and offers the checkout,
because writing a ticket about code that is not on the machine only moves the
problem.

A project that keeps a single checkout for every branch is not exempt from
that. Its root is the branch's working tree only while it is standing on the
branch; a root standing on another one is a checkout of different code, and a
directory existing is not the same fact as the change being in it. Dispatch
refuses there too, naming the branch the root is actually on — and it offers no
checkout, because there is none to make: the remedies are to put the branch in
that root or to give the project branch workspaces of its own, and both are the
reader's to choose between.

## 4. The ledger is ephor's record, and never the truth about the work

ephor keeps a ledger of what it dispatched: the item, the recipe, the plan, and
what the item looked like at that moment. The ledger is what makes the second
question answerable — has this already been handed over? — and it is written
where ephor's other state lives, not in the reader's repositories.

But the work's state belongs to the runtime and is read from the plan, never
cached in the ledger. A ledger that remembers "running" when the plan says
"done" is worse than no ledger: it is a watch reporting on itself instead of on
the world, which is the one thing this tool must never do. A ledger entry whose
plan has been deleted is reported as missing rather than repaired.

## 5. An item that moved reopens its work

Work asked about a pull request is answered against the pull request as it was.
New comments arrive; the gate turns red again; the state changes. The ticket
that was finished is now finished about something that no longer exists.

So ephor **fingerprints** the item at dispatch — its last activity, its state,
its gate, how much conversation it had — and a change to any of those makes the
work **stale**. Stale work is reopened by appending a ticket to the same plan
that says what changed since the last one and asks for the difference, ordered
after it — after the last ticket that was not cancelled
([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)),
since a cancelled prior is one nothing waits out. Not by opening a second plan: the point of the record is that one
item's work reads in one place, in order.

What is asked for is chosen against the item as it now is, preferring what was
asked last while that still applies. A change moves between categories as it
goes: the pull request whose gate was red is, two hours later, one whose jobs
pass and whose reviewer has asked a question. Reopening it under the recipe it
was first dispatched with would hand the work a ticket about a problem that is
no longer there. Where nothing applies any more — it merged, it closed — the
work is not reopened at all, and the ledger goes on saying that the item moved
past it.

Reopening is a decision, not a reflex. It is offered where it applies and
performed when asked for — by a person or by whatever runs the sync — and never
as a side effect of merely looking at the feed.

## 6. Dispatch is offered where it would work, and refuses where it would not

The rules of [§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)
hold here and cost more when broken, because a ticket that cannot run is not a
wasted keystroke but a piece of work that looks scheduled and never happens. A
recipe is offered only when it matches the item, the item's project has a root,
and the checkout can be resolved — and where the work edits the change rather
than reading it, only when that change is actually on the machine. Where the
runtime's setup in that checkout cannot run what ephor would write — a state
machine already there that does not declare the state a recipe starts in —
dispatch refuses and names both, rather than writing a ticket that will sit
there unrunnable. It refuses the mirror image too: where the reader's own plans
are already in that directory under no declared machine, ephor does not install
one, because a state machine governs every plan in a project and theirs were
there first.

Matching is on what a gate is doing, not on how red it looks. Jobs that failed
are work for a checkout; a forge that refuses to merge an otherwise green
change is usually waiting on a person, and dispatching an agent at it spends a
pass to be told so.

Finished work is never dispatched. An item under Recent
([§FS-003-feed-categories.2](FS-003-feed-categories.md#2-recent)) is news, and asking an agent to fix a
merged pull request is asking it to invent something to do.

### 6.1 The work root is a template, and it may reach above the project

Where a plan is written is configuration rather than a constant. `work.root` is
a template, rendered from the vocabulary the ticket itself is rendered from
([§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it)) — the item's
own fields, the checkout it resolves to, the project root — and it is read at
three scopes, the same nesting the autorun ceilings are read at
([§24](#24-work-nobody-has-to-start-starts-itself)): `projects.<id>.work.root`
first, then `organizations.<org-id>.work.root`, then the site's `work.root`.
The innermost one written answers and the others are not consulted; none of
them merges with another, because a path is one answer and a half-overridden
one is nobody's.

**Two of the names reach above the project.** `{org}` is the organization the
project's registry row places it in and `{org_root}` is where that organization
is rooted — the registry has always known both, and it was the placement that
could not reach them. With them, an organization tier writing
`"root": "{org_root}/panta"` gives a whole organization one work root, which is
where work that belongs to no single repository goes: a release that moves
several projects' gates, a sweep across all of them. Membership and the root
are the registry's own facts, read here and never written back
([§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)); only the tier that names them is
configuration's.

**A name with no answer refuses, and says which name and which
organization.** A project the registry places in no organization has no
`{org}` to render, and an organization that declares no `root` has no
`{org_root}`. Dispatch refuses both, by name — *organization acme declares no
root* — because what it would otherwise write is a directory literally called
`{org_root}`, or a path with a segment missing where the answer should have
been, and either one is work laid down somewhere nobody meant. The refusal is
about the *path*: the dossier and a recipe's brief are prose, and carry an
empty organization the way they carry any other field a matter has not got
([§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it)).

**Which scope a plan belongs in is a different question from which tier may
answer it**, and [§FS-014-work-root-scopes](FS-014-work-root-scopes.md#fs-014-work-root-scopes-a-plan-lives-in-the-smallest-scope-that-can-see-everything-it-touches) is the rule for it.

## 7. Handing over work is the reader's move, and stays inside the machine

Dispatch writes files and nothing else. It opens no pull request, posts no
comment, and pushes no branch — those are the runtime's to do, if a recipe asks
for them, and a recipe that does asks in the ticket's own words where a reader
can see it. What ships asks for none of them: the shipped recipes end at a
local change, and closing the loop out to the forge is one line of
configuration that a person turns on deliberately.

Bulk dispatch — every matching item in a project, in one command — is the same
guarantee at scale: it writes tickets, reports each one, and can be asked what
it would do without doing it.

## 8. The ticket carries the item as data, not only as prose

The dossier is written for a reader — a person or an agent — and a program
cannot read it. Yet the useful thing to put in front of an agent working a
failing gate is usually what a *script* can fetch: the log, the failing job,
the forge's analysis. A state machine can run that script before the agent, but
only if the script is told which item it is about, and prose is not an input.

So every ticket also carries the item's identifiers as **structured metadata**,
under the same names its context takes in a shell action
([§FS-004-quick-actions.1](FS-004-quick-actions.md#1-a-quick-action-belongs-to-the-source-that-found-the-problem)):
project and source, kind and item id, repository and number, branch and ticket,
url and state, and the checkout the work belongs to. One vocabulary, whether
the thing reading it is a shell command in a menu or a program in a state
machine.

Two consequences. A ticket that is appended to a plan **adds** its metadata
rather than replacing what is there, because the runtime keeps its own
bookkeeping in the same place and a ticket writing over it would break the
plan. And what is written is identifiers only — the prose stays in the dossier,
which is where a reader is looking.

## 9. Work that stops for a person says so where the person is looking

Work handed to a runtime is autonomous until it meets a question that is not
its to answer: a product decision, a trade-off between two things it cannot
weigh, an instruction it cannot read. The honest move then is to stop and ask —
and a machine that can only finish or fail will instead guess, because guessing
is the only move it has.

A runtime that can park work pending a person is therefore something ephor
reads, not something it invents: where the state a ticket sits in is one the
runtime will not leave on its own, that ticket is **waiting on the reader**, and
it is shown that way — ahead of anything else its work is doing, since it is
the one part of it nobody else will move.

The question and its answer stay in the plan. A ticket that asked something
carries the question in the artifact it wrote, and the answer belongs beside it
rather than in a chat window, a comment, or somebody's memory: the plan is the
record of what was decided about this item, and a decision taken anywhere else
is one the next round cannot read.

## 10. What ephor offers is not a limit on what can be asked

Recipes are for the work that repeats. Most work does not: a reader looks at a
change and knows the one thing they want done to it, and that thing has never
come up before and will not come up again. A tool where every ask must first be
written down as a rule, in a configuration file, in another window, has made
the common case the expensive one — and the reader will do it by hand instead,
which is what they were trying to stop doing.

So an item can be asked for **anything, in the reader's own words, where they
are standing**. What that produces is an ordinary ticket: the same dossier, the
same plan, the same place in the order, the same runtime. Only the brief is
different, in that nobody wrote it in advance.

Two things follow from its being asked for rather than offered:

1. **It is never refused for not matching.** Selectors say what ephor
   *volunteers*; they say nothing about what a person may ask for. Finished
   work, an item no recipe covers, a second ask on work already under way — each
   is somebody's deliberate request, and ephor's job is to write it down
   accurately rather than to have an opinion about it.
2. **The same holds for a command.** The action menu
   ([§FS-004-quick-actions](FS-004-quick-actions.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it))
   is configuration plus what a source offers; a reader who wants to run
   something once should not have to add it to a file first. A command typed
   into the menu runs exactly as a configured one does — the same checkout, the
   same `EPHOR_*` environment, the same handover of the terminal — because the
   only difference between the two is whether anyone expects to want it again.

## 11. A failure that is not the change's fault is restarted, not fixed

Not every red gate is a broken change. A runner dies, a mirror is unreachable,
a dependency ships a bad artifact, the same flake lands on the same job for the
third day running — and what the item needs then is not a fix but another run.
A loop that cannot tell the two apart pays for the difference twice: it spends
a model on diagnosing something that was never wrong, and then it lands a
commit whose only purpose was to make the gate start again.

So **restarting is a move the loop has**, and it is a different move from
fixing. Four things follow from its being different.

It is **decided by a program, not by prose**. Recognizing infrastructure is a
judgement, and a judgement nobody acts on is the same as no judgement at all:
what the model concluded has to reach a transition. So the verdict is a marker
on a line of its own, the way a question for a person is
([§FS-005-dispatch.9](FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)),
and a program reads that line and picks the state. An agent that is asked to
notice something, and given nowhere to put the answer, has been asked for
nothing.

It **opens a ticket of its own**. A restart is work done to the item, and the
plan is the record of that work — what was hit, when, and whether the round
after it came back green. A restart that happened as a side effect of some
other state would leave the next round unable to see that this failure has been
retried already, which is the one fact that separates a flake from a gate that
is simply broken.

It **restarts the gate and every gate downstream of it**. A gate that spans
several repositories fails downward: the repository whose job died takes the
rest of the tree with it, and the gates below never ran at all. Re-running only
what failed leaves every one of them exactly as red, so what is restarted is
the failing gate and everything under it that is not green. Nothing is
committed — the change was never the problem.

And it is **bounded**. An unhealthy runner pool answers a restart with the same
failure, and a loop that restarts every round is one that never stops and never
says why. Past a small number of restarts on one item the work stops for a
person, because at that point the infrastructure is the thing that is wrong and
no amount of retrying is going to be the fix.

## 12. Work an algorithm can finish does not start with a model

Not everything the watch turns up is a judgment call. Replaying a branch onto
its main branch is a fetch and a rebase: it either applies or it stops at a
conflict, and it does the same thing every time. Handing that to a model is
paying a pass to have two commands typed, slower and less predictably than the
commands would have run themselves — and the pass that matters is the one after
it, on the part no algorithm can do.

So the deterministic move runs first, and the work starts where it stopped.
Where it finished, nothing is dispatched at all: a clean rebase is a done
thing, not a ticket. Where it stopped, that is the ticket, and what is handed
over is the situation rather than the request to reproduce it — the repository
is left where the algorithm left it, mid-rebase with the conflict in the
working tree, because that is the state resolving it needs, and the ticket says
which repository, which files, and which two sides
([§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it)).

A move that costs no model costs no screen either: the replay runs beneath
the interface as a job, and what the reader would have watched is in its log
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)).

The rebase is the first of these, not the shape of the only one. Any recipe
whose opening move is deterministic makes that move before it costs a model,
and dispatches what is left over — which is also why the move has to be
runnable on its own ([§FS-004-quick-actions.6](FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)):
the same rebase the reader presses a key for is the one a state machine runs,
and two implementations of it would eventually disagree about what a clean
rebase is.

## 13. A communication is work too, and its answer comes back as a proposal

Not every matter's next move is a change in a checkout; often it is a reply.
A matter owing a response — a question in a review thread, a mail asking for
a decision, a mention carrying a request
([§FS-003-feed-categories.4](FS-003-feed-categories.md#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)) — is dispatched like any other: the ticket
carries the discussions as its dossier, and asks the runtime for a
**proposed answer**, drafted in the matter's context. The shipped answer
recipe is this shape, and an ask in the reader's own words (§10) may request
one for anything.

Three things distinguish it. **It needs no checkout**: the work is about the
conversation, so the plan is written where the matter resolves — the branch
workspace where one exists, because sitting in the change makes a better
answer, and the project root otherwise — and the checkout-able rung is not
required ([§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung)). **The proposal is a file, never a
post**: §7 holds — the runtime writes the proposed reply into the plan's
results, ephor reads it back and surfaces it beside the discussion it
answers, and nothing reaches the channel by itself. **Posting is one
deliberate move, where the channel can carry it**: on a channel that
declares reply ([§FS-007-matters.4](FS-007-matters.md#4-a-channel-says-what-it-can-do)) the surfaced proposal is offered for
posting, edited or as it stands, exactly as a reaction is posted today; on
a channel that does not, the proposal is what the person copies — a stated
degrade ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)), not a failure.

## 14. Who does the work is chosen, and defaulted per project

Work is handed to an agent carrying a model at an effort, and two of those
three follow from the first: which models an agent can carry and which
efforts it declares are facts about the agent, not free choices beside it.
A chooser built as `agent × model × effort` would be mostly cells nobody can
run, and ephor has no way to know which — the runtime does. So the choice has
one axis the reader picks and one dependent on it, and the set of choices is
the **roster**: the runtime binding's own enumeration of who can be asked,
read from the binding at the moment of asking rather than kept as a list in
ephor's configuration, because a copy is wrong the first time an agent or a
model is added on the other side
([§DA-004-roster-is-asked-not-configured](../decisions/architectural/DA-004-roster-is-asked-not-configured.md#da-004-roster-is-asked-not-configured-the-roster-is-asked-of-the-binding-never-kept-by-ephor)). Every id on the roster is unique —
the runtime's model and agent namespaces are separate, and a model profile
may claim an agent's very name, in which case the profile holds it and the
agent stands alone under a marked spelling of its own — because one name
over two rows can address only one of them.

One entry is a **hand**: a name, the agent it summons and the model that
agent will carry — shown together, because a reader choosing the name is
choosing both — the **efforts** the entry declares, and whether it is
available. An entry may declare no efforts at all, which is an answer rather
than a gap: such a hand is simply asked plainly, in either spelling, because
it has no effort an ask could drop. A hand that does declare efforts is
always asked at one of them. A choice that names none is **completed** where
the hand declares exactly one — a single declared effort is a fact about the
hand, not a choice left open, and the completion is said in a note — and
**refused** where it declares several, with the efforts listed, before
anything is written: the runtime's two spellings do not agree on what an
effort-less ask would mean — one drops the effort silently, the other lets
the state machine's own choice fall in and fails outright where the hand
does not declare it — and neither answer is the reader's choice.
Configuration names a hand by its id and nothing else; the binding's own
spelling of agent, mode, provider and model is the binding's, so a
configuration written under one runtime reads unchanged under another
([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)).

An unavailable hand is **shown with its reason, never hidden**. This is the
opposite of what a menu does ([§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)), and deliberately so:
a menu entry is an offer, and an offer that cannot work costs a keystroke —
but the roster is the answer to "who could I ask", and a hand that silently
vanished because its agent left `PATH` looks exactly like a hand that never
existed, which is the one confusion a reader debugging a dispatch cannot
resolve from the screen. The reason is computed where the roster is read —
the agent's command is looked for, never spawned to fail — and it is one
sentence beside the entry, the same sentence everywhere that hand appears.
The roster is reportable before anything depends on it: `ephor doctor` and
`ephor capabilities` print each hand, what it resolves to, and why an
unavailable one is unavailable ([§FS-010-doctor.2](FS-010-doctor.md#2-the-ladder-is-answerable-on-its-own)).
When that roster contains only agent-default hands, `capabilities` also says
that no model-carrying hands are configured and points to model profiles in
the shipped binding's `models` settings registry as the way to create
nameable model-carrying hands. A choice naming an id absent from the roster is
still refused before anything is written, with both the requested id and the
current roster retained in the refusal; the refusal additionally says that a
model profile bearing that id and an agent carrier in the same registry makes
it a nameable model-carrying hand. A missing name is never passed through as
an ordinary string.

**The hand for a piece of work resolves in seven steps**, each displacing
the ones after it: what the reader picked for this dispatch alone; the pin
the action or recipe itself carries; the project's hand for this action id;
the project's default for everything; the site's hand for this action id;
the site's default; and, where nobody chose at all, whatever the binding
would pick unasked. The order mirrors the
binding's own resolution deliberately, so ephor's answer and the runtime's
cannot come to disagree about what one configuration means. A project may
also narrow the roster — say which hands may be used on it at all, which is
what a repository under a policy about which models may see its code needs —
and a hand outside the narrowing is refused with that reason, never silently
dropped.

**A pin may name an ordered list of hands, and the steps still answer with
one.** Everywhere a hand is named — a table's entry, the pin an action or a
recipe carries, what the reader picked — the value may be an ordered list of
names instead of a single one, and a single name *is* that list with one
member in it. Nothing written before this means anything different, because a
list of one resolves and dispatches exactly as the bare name did. The seven
steps themselves are untouched: they displace one another in the same order
and still answer exactly once, and what the answering step hands on is the
list it carried rather than a name. Choosing among that list is a later stage
with a different question in front of it (§29), and the two are kept apart on
purpose — a step answers *whose work this is*, which is the author's judgment
about the matter, and the stage below answers *which of them can be reached
right now*, which is a fact about the world and no judgment at all. So a step
that answered has answered: no later step is consulted because a member of its
list turned out to be unreachable, and the fallback lives inside the list the
author wrote rather than across the precedence order, where it would silently
promote a table nobody meant to reach.

The order inside a list is the author's, and it ranks by **fitness rather than
by equality**: the first name is first because it is the right hand for this
work, and each name after it is what to do when the one before it cannot be
had. That is the whole reason the stage below may veto a member and may never
reorder the survivors (§29) — a rule that sorted this list by anything else
would be overruling the only judgment in it.

The first step is **made at the moment of dispatch and spent by it**. In the
interface it is a picker over the menu's entry: the roster's hands in one
column and, beside a hand that declares efforts, those efforts in a second —
absent where it declares none, which is every hand on a machine whose
runtime settings declare no model profiles, and a dead column would teach an
axis that is not there. On the command line it is `--hand
<hand-id>[:<effort>]` on the command that dispatches, the same grammar the
tables write ([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)), so the key and the flag are one
operation ([§FS-005-dispatch.12](FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)). The pick lives exactly as long as the one
dispatch it was made for: nothing records it, and the next dispatch of the
same action resolves from the second step down — a pick that outlived its
dispatch would be a configuration layer nothing wrote down. The picker never
assembles a choice the resolution would refuse — a hand that declares
efforts is picked at one of them — and it shows an unavailable hand with its
reason without letting it be chosen. A hand the project's narrowing excludes
does not appear in it at all: the narrowing is the project's policy rather
than a state of the hand, and what is refused loudly is a *named* choice —
offering the name only to refuse it would teach the policy one wasted
keystroke at a time. With an empty roster there is no picker, and the entry
dispatches exactly as if nothing had been picked.

**A chosen hand binds in one of two spellings, never both.** A hand that
carries a model is written onto its ticket, at dispatch, in the runtime's
own per-ticket execution line ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)) — each ticket carrying
its own choice, so two tickets in one plan can go to two hands and the
choice survives every later run. A hand that names an agent and no model of
its own — which is every hand on a machine whose runtime settings declare
no model profiles — has no line in the plan language, so it rides the run
instead: the run invocation carries the choice as the runtime's own per-run
agent flags, the agent and the effort where one was settled, resolved again
at the moment the run is invoked — the same moment the runtime reads its
own configuration, so the two answers cannot drift apart. The two per-ticket
lines rank differently against a run's flags, and ephor follows the
runtime's own ranking exactly. A ticket carrying the full execution line
cannot be re-aimed: the runtime resolves such a ticket from its line alone,
and the run's agent flags are invisible to it — only a per-run model choice
reaches past that line. A ticket carrying a model alone can be: the run's
agent flags supply its carrier, and one run advances several tickets,
including one a person pinned by hand. So the flags ride a run only where
they can re-aim nothing — every ticket the run would advance and that has
no line of its own resolves to the same spelling, and none pins a bare
model. Where one plan's open tickets do not agree, that plan runs with no
flags and the reader is told the hand went unbound for that run; plans that
agree differently are run separately, each under its own spelling. A ticket
somebody has claimed is not the run's to advance at all — a claim makes the
runtime skip it ([§FS-005-dispatch.15](FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)) — and enters none of this. The
cheaper spelling is always available to the reader: a
model profile declared in the runtime's own settings turns an agent-only
hand into a model hand, and the ticket line then carries it everywhere,
with no flags involved at all.

**A run started from the interface is the same run.** The key that hands
one item's plan to the runtime resolves the hand exactly as the command
line does and carries the same flags — which surface a reader started a
run from is not a fact about who did the work, and two resolutions of it
would eventually disagree ([§FS-005-dispatch.12](FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)). Such a run names one
plan and advances no other, so that plan's own open tickets settle its
flags and no other plan's can contradict them. What the resolution has to
say — a hand nothing resolves, an effort completed, a hand left unbound —
is said where the reader can still read it when the run returns: a surface
that cedes the terminal keeps the note for its own message rather than
printing it into a screen the run is about to take, and a refusal is
answered before the terminal is ceded at all, never after
([§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)).

With no runtime bound, or a bound one not on `PATH`, the roster is empty and
says so in the *workable* rung's own words ([§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung)):
who can be asked is the runtime's knowledge, and where nobody can run work
there is nobody to ask. A runtime settings file that exists and does not
parse empties the roster too, in a sentence of its own that names the file:
a roster read around it would be a list missing whatever the person just
added, which is worse than no list. Nothing else changes — every other rung
resolves, and tickets are still written and read on disk
([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)).

## 15. Every operation is visible in one place

The watch can say what is being done about any one item — the lines its work
stands on beneath its row
([§23](#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)), the
work screen behind `w` — but "what is ephor doing right now" should
not require visiting every row that might hold a piece of the answer. So
there is an **operations board**: one screen, reachable from anywhere in the
interface, holding every operation beneath the reading — each live run, each
claim somebody holds, each ticket waiting on a person, and the refresh
itself, which already reports in the header of the screen being read and
appears here *additionally* ([§FS-001-forge-interface.7](FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)). It is where
[§FS-005-dispatch.9](FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking) pays off at the scale of the whole watch: work that
stopped for a person is one glance away wherever the person happens to be
looking — and within one operation, what asks something of the reader is
listed ahead of anything else its work is doing — a parked question first,
then what a dead run left holding — then what runs, then claims, then the
queue.

**The rows are found by looking, never by remembering.** What ephor
dispatched is in the ledger, but "every operation" is a claim about the
world, not about the ledger: the work roots themselves are enumerated —
the place a project's work is configured to live, resolved at the
project's own checkout and again in each branch workspace on disk, since
the work root is per branch workspace and each one is its own execution
root, and again at its organization's root where the template reaches
above the project
([§6.1](#61-the-work-root-is-a-template-and-it-may-reach-above-the-project))
— and every plan found in one is watched, whoever wrote it. A plan
written by hand, a project's own planning tickets, and a run somebody
started in another terminal on a root ephor never dispatched into are
operations exactly as dispatched work is, judged row-worthy by the same
artifacts; the ledger still says which matter a plan is about
([§FS-005-dispatch.4](FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)), it just no longer decides what exists. And an
operation ephor never dispatched has no matter behind it by construction —
that is the common case for a foreign plan, not an edge — so `Enter`,
which goes to the matter where the feed still carries one, opens the plan
itself there: the same reading `e` offers wherever work is shown, and no
row on the board leads nowhere. Enumerating is a reading of the plan files
on disk and asks nothing of the bound runner: with no runner installed the
plans are still found and still readable — it is only operations that
cannot exist then.

**Liveness is read from the runtime's artifacts, never from a process
table.** The same reasoning that keeps work state out of the ledger
([§FS-005-dispatch.4](FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)) applies to whether work is running at all: the runtime
leaves the truth on disk. It holds a lock on an execution root for exactly
as long as a run is live there, and the operating system lets go of that
lock when a run dies, however it dies. The board probes the lock without
ever waiting on it — the runtime acquires it blockingly, so a probe that
queued would park the watch behind the very run it is asking about. Which
tickets a live run holds is read from what that run itself writes as it
works ([§15.2](#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)),
and where a runner writes no such stream, from the journal and the logs it
leaves behind.

**A row is an execution root, not a ticket.** The runtime schedules one run
per root, and ephor's work root is per branch workspace — so two items whose
work lives in one workspace are one operation, and a ticket written into a
root a run already holds is shown as **queued**, never as running: a second
run there would wait for the first. And a ticket is a ticket at whatever
depth the runtime nests it — a subtask parked three headings deep is as much
an operation as the ticket it was split from, run or no run.

**A claim is not a run.** An assignee on a ticket is written when somebody
takes it, and its effect is that the runtime skips it: it says *claimed and
unschedulable*, never *live*. A ticket with an assignee on a root where no
run holds the lock is its own flavour of row — **claimed, not scheduled** —
shown with the bound runner's own command for releasing the claim. The board
reports it; it does not act on it. And under a live run a claim stays a
claim: the run skips it, so *queued* would promise a turn that never comes.

**Parked work outlives the run that parked it.** The usual end of a run
that parks a ticket is the run exiting: nothing else was schedulable, so
the lock goes free — and the runtime wrote no claim on the way out, since
parking is a transition, not a taking. The ticket is waiting on the reader
all the same ([§FS-005-dispatch.9](FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)), and it keeps its row — **waiting**,
ahead of anything else its operation is doing — whether the run that parked
it is still live, exited, or died. A root holding such a ticket is an
operation with nothing running in it, exactly as a root holding a claim is.

**Silence is a badge, not a verdict.** A long tool call is legitimately
quiet, and the lock — not the last write — is the liveness signal. A live
run that has written nothing for a while carries a **quiet** badge and
nothing more; a run that died has released its lock, and its root simply
stops being a live row — though not always a row: a ticket a dead run was
still holding mid-slot is read out of the journal that run left behind and
keeps a row of its own, **dropped by a run that died** — beside a parked
ticket, deliberately not as one: nobody else will move either, but a parked
ticket asks a question about the work, and a dropped one asks for the run
back. The artifacts tell the two apart without guessing — parked is the
machine's gating word on the ticket's own state, dropped is the journal's
unreleased slot under a lock nobody holds. The journal
outlives every run, so what it says is held is believed only while the
world still agrees: an assignment no run ever released stops counting the
moment the ticket's own state says it moved on, and is never read as
running under a run that came later.

**Watching only, and deliberately so.** The board starts nothing, stops
nothing, and intervenes in nothing — and it stays that way now that something
*can* be started beneath the screen
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)): what starts a
job is the menu entry the reader pressed, and the board is only where it is
then seen. Interfering with a live run remains out — it needs a channel to
the run that exists only while a run serves one, and a board that hinted at
it would be promising what it cannot do.

**With no runtime bound, the board holds the refresh row and ephor's own
jobs** — and that is the board being right, not broken ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)):
the operations a *runtime* has are runs, and where nothing can run there are
none, said in the workable rung's own words ([§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung)). A
job needs no runtime to exist
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)) — it is ephor
running a command, which is the one thing that never depended on a binding —
so it keeps its row while the runtime's half of the board says why it is
empty. The tickets themselves stay readable
exactly as everywhere else in dispatch: work state is read from the plan
files on disk, that reading is the floor and is never removed, and where the
bound runner itself can be asked for a sharper listing, its answer may
refine what the files said — it never replaces them. And a plan whose state
machine cannot be read is reported at the same altitude: liveness, running,
claims, and what a dead run dropped are facts the lock, the journal, and the
plans carry on their own, and the board still says them — but nothing in
that plan is called queued or waiting and nothing of it counted finished,
because those are the machine's words and the machine is not there to say
them. The row itself says the machine could not be read, **naming the plans
it happened to**, in so many words: a count left silently at zero would read
as nothing done, which is exactly the guess the withholding exists to avoid.
Which machine that was is the plan's own question
([§28](#28-a-workflow-entry-can-ask-for-the-same-thing-a-recipe-can)), so a
plan judged by a machine of its own is never among them — a row that said
otherwise would deny a count the work beside it really earned.

### 15.1 The board keeps itself current

Nothing here is something the reader has to ask for twice: work that moves
on disk — a ticket advanced, a question parked, a verdict written, a run
starting or dying — surfaces on its own within moments, not at the next
refresh. This is not the refresh ([§FS-001-forge-interface.7](FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)) wearing a new
name: a refresh asks the world's forges and costs what they cost, while this
watches files ephor already knows by name and asks nothing of any forge
([§GOAL-005-costless](../goals.md#goal-005-costless-watching-costs-the-watched-nothing)). It is cheap by construction — nothing is re-read while
nothing has changed, a timestamp answers that, and the timestamps asked
are a fixed handful per root: each plan file the last enumeration found,
the root's own directory — a plan appearing or vanishing is a directory
event — and the artifacts the runtime moves as it works: the one it writes
on every slot it takes or releases, and the live run's own stream
([§15.2](#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)), which
is one more name in the same fixed handful and never a sweep of everything
the runtime ever wrote; the bound runner is
asked to list its plans only about a root that holds an operation, and
nothing is ever read while a frame is being put on screen — and it holds
everywhere work is shown, not only on the board: a ticket the runtime
parks resurfaces on the reader's rows when it parks ([§FS-005-dispatch.9](FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)),
instead of waiting for a refresh that was never going to be about it.

Finding the roots ([§FS-005-dispatch.15](FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)) is the one walk in the design, and
it is not the tick's: the work roots are enumerated when the rows are
built — the board opened over the reading, rebuilt because the glance saw
something move, or a refresh landing — and never merely because time
passed. The walk is bounded by where work is configured to live, not by
what the disk holds: it visits the project checkouts and the branch
workspaces ephor already resolves, to the fixed depth a branch name can
nest, and — where the template names an organization
([§6.1](#61-the-work-root-is-a-template-and-it-may-reach-above-the-project))
— the root of the organization the project belongs to, which lies inside
no checkout and would otherwise be written to and never looked at again.
It costs one directory listing per candidate work root — it never
descends into a repository, and it never reads a plan to find it. What
has no answer here is skipped rather than refused, the way a template
naming a field only an item can fill is: an organization placeholder
nothing answers is a template no dispatch could have written through
either, so there is nothing under it to have missed.

### 15.2 What a run is doing is read from the run's own stream

The lock says a run is live and the plan says what state a ticket is in.
Between those two facts sits everything the reader actually wants from a
live row — which ticket the run has in hand right now, what it just
finished, whether it is still moving — and it was reconstructed from a
journal that outlives every run. That journal is the wrong witness for a
question about *this* run: it is append-only across all of them, so a run
that died mid-slot leaves an assignment nobody ever released, and every
later reading has to argue that entry down from evidence elsewhere — the
ticket's own state, the age of a log against the birth of the lock. The
answer is right and the reasoning is a chain of inferences about a file
that was never about one run.

So where the runtime keeps a **record of the run itself** — what that run
took up and let go, in order, from its own beginning — that record is what
a live row is read from. Its properties are the ones the inference chain
was standing in for: it belongs to one run, so nothing in it can be another
run's leavings; it is ordered and numbered, so a reader can say exactly how
much of it it has seen; it says how each slot ended rather than leaving the
end to be deduced; and it says when the run ended, so a reader knows the
difference between a run that finished and a run that stopped existing.

**The journal stays the floor.** A runner that writes no such record is
read exactly as before, with the same inferences and the same caution
([§FS-005-dispatch.15](FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)): this sharpens the reading where the artifact is
there and is never a thing a runtime has to provide to be bound at all
([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)). Where both exist the run's own record answers,
because a witness to one run beats a witness to all of them.

**Liveness is still the lock.** A record that ends saying the run finished
agrees with a lock that is free; a record that simply stops does not mean a
run is gone, only that it has written nothing lately, which is the quiet
badge's business and not a verdict ([§FS-005-dispatch.15](FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)). Nothing here is
consulted to decide *whether* a run is live — only what the live one is
doing, and what the last one did.

**And it is read where work is shown, not only on the board.** The reader
looking at a matter's rows sees the same word the board would show, because
this is one reading narrowed to one matter rather than a second one of its
own ([§23](#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)).

## 16. Work that should not go on is cancelled, and the plan says so

Not every ask should run to its end. The same recipe was pressed twice; a
ticket was asked for on the wrong item; the item moved past the question
before anyone picked it up. What is wanted then is not a fix and not a
reopen but a **cancel** — and a watch that can hand work over and cannot
take it back leaves the reader with two runs of the same fix in one
checkout, or with an editor open on the plan, guessing at what the runtime
would have written.

So a ticket can be cancelled: from the work screen, on the ticket, and from
the command line. Cancelling moves the ticket into the machine's
**abandonment state** — spelled `cancelled` in the plan language, the one
final state that satisfies no `**Prior:**` — carrying the reader's reason
as the ticket's result. Two things follow from its being a state.

**It is the runtime's move, made in the runtime's own words.** The plan
language reserves a ticket's state, after it is written, to the runtime's
own verbs — the compare-and-swap, the artifact checks, the callbacks, the
audit trail — and a state line ephor rewrote by hand would be a plan the
runtime can no longer vouch for
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work),
[§DA-005-cancel-is-the-runtimes-move](../decisions/architectural/DA-005-cancel-is-the-runtimes-move.md#da-005-cancel-is-the-runtimes-move-cancelling-a-ticket-asks-the-runtime-and-never-rewrites-the-state-line)). So ephor asks the runtime for the
transition, captured rather than watched, and what comes back is what the
reader is told: the ticket cancelled, or the runtime's own refusal in its
own sentence. It follows that with no runner bound cancelling is refused in
the workable rung's words like running is
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)): the
plan is still readable and hand-editable, and nobody is there to make the
move.

**The record keeps it.** A cancelled ticket stays in the plan, in its place
in the order, marked as cancelled with its reason beneath it — the same
reading a finished ticket gets. Nothing is deleted: the plan is the record
of what was decided about this item
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)), and taking an ask
back is a decision too.

Cancelling refuses where the move would be wrong, and says why in one
sentence before anything is asked of the runtime. A ticket a **live run
holds** is that run's to finish: pulling the state out from under an agent
is interfering with a run, which is the later section
([§15](#15-every-operation-is-visible-in-one-place)), and the refusal names
the run. A **finished** ticket has nothing left to cancel. A machine that
declares **no abandonment state** is refused with the machine and its file
named, exactly as a recipe naming a state the machine lacks is
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)):
a ticket moved into a state the machine does not have leaves a plan the
runtime refuses to run at all. The machine ephor ships and the examples
beside it declare the state and a transition into it from anywhere, so a
work root ephor made can cancel from the start; a machine of the reader's
own says whether it can, and the refusal tells them what to add. Everything
else is fair: a ticket that is queued, parked on a question, dropped by a
run that died, or claimed and unscheduled is somebody's to cancel, and the
runtime's own checks are the last word.

Order follows from the state's name. A ticket **ordered after** one now
cancelled will not start — the abandonment state satisfies no `**Prior:**`,
which is what its spelling is for
([§11](#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed))
— so cancelling says which open tickets those are, and cancelling them too
is one more keystroke; ephor does not decide for them. And a ticket ephor
appends afterwards — a reopen, a second ask — is ordered after the last
ticket that is **not** cancelled
([§5](#5-an-item-that-moved-reopens-its-work)), so ephor's own chain never
hangs off abandoned work.

## 17. A move that needs nobody runs beneath the screen

Pressing a key for a deterministic move costs the whole interface for as long
as the move takes. Replaying a forest onto its main branch is a fetch and a
rebase per repository, and on a checkout weeks behind that is minutes of
output — output that asks nothing, decides nothing, and is read afterwards if
it is read at all. Meanwhile the watch is gone: the reader cannot look at the
next item, cannot start the second move, cannot even see that the first one is
still going, because the screen that would say so has been given away. Then it
asks for a keypress to hand the screen back. The reader paid the interface for
a command that never needed it
([§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).

So a menu entry that does not need the reader runs as a **job**: started
beneath the screen, with the interface staying exactly where it was, and the
job taking a row of its own among the operations
([§15](#15-every-operation-is-visible-in-one-place)). What was a takeover
becomes a line saying the work started and a row saying it is going.

**Which entries these are, the entry says.** ephor's own deterministic moves
are jobs by construction — the rebase is the whole argument of
[§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model), and a
move that costs no model has no reader to cost either. Everything a person or
a project wrote keeps the terminal unless it asks otherwise, because a menu
entry has always been allowed to *be* the reader's session — `lazygit`, an
editor, a pager ([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)) — and starting one of those
beneath the screen would leave a program nobody can type into, waiting on a
terminal it does not have. Which way an entry runs is therefore the entry's
own word, and its default is the one that was always safe.

**A job outlives the interface that started it.** It is its own process in its
own process group, so quitting ephor does not take it down, and neither does
closing the terminal: a move that needed nobody watching does not suddenly
need somebody staying. This is the same property from the other side — work
handed to the runtime already survives the screen ([§FS-005-dispatch.4](FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)), and a
move ephor runs itself should not be the fragile one.

**Everything the reader would have watched is kept.** The job's output —
what it wrote and what it complained about, whole and in order — goes to a
log, and the log is the inspection: from the job's row on the board, from the
work screen of the item it was started about, and from the command line. A
job that is going says what it is doing right now, because "still running" and
"stuck" are the same three words on a row, and the log is the difference.

**Liveness is the lock, exactly as it is for a run.** A job holds one for as
long as it runs, and the operating system releases it when the job dies,
however it dies ([§15](#15-every-operation-is-visible-in-one-place)); the
board probes it and never waits on it. Nothing consults a process table, and
nothing believes a record that says a job started: a job ephor started and
then crashed alongside is not running, and the lock says so without being
asked.

**The rows are found by looking, here too.** Jobs are read from where they are
written, not from anything that remembers having written them, so a job
started from another ephor — a second terminal, an earlier session that has
since exited — is a row like any other, and a record with no job under it is
history rather than a claim.

**The chain travels with the job.** An entry that needs the item's branch
workspace still gets the checkout first ([§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)), inside the
job and against the same verification: the directory is checked for rather
than trusted ([§FS-006-project-interface.8](FS-006-project-interface.md#8-the-checkout-contract)), and a checkout that did not make
it ends the job there with what it said. A job is a sequence because the move
was a sequence; it is not a way to run two unrelated things.

**The outcome comes back to the row it was about.** A job ending is news
exactly as a ticket parking is ([§FS-005-dispatch.9](FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)), and news read at the top
of the screen is news about nothing in particular: a header line saying a
replay went through names no branch, so a reader with three of them going has
to guess which row just moved. So the line lands **under the subject the job
ran on** — the branch it replayed ([§FS-004-quick-actions.6](FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)), or the matter it
was started about — beside the facts it changed, and it stays there until the
reader opens that row or a later job about the same subject replaces it. Where
that row is not on the screen at all, the project's own row carries it: news
with nowhere to land is news that is lost.

**Only what has ended lands there.** A job still going is already marked
running where it could be started again
([§21](#21-what-is-already-going-is-shown-where-it-could-be-started-again))
and holds a row among the operations
([§15](#15-every-operation-is-visible-in-one-place)), and a third live mark
on the tree would be one fact said three times. A job that has ended is no
longer an operation and leaves the board — an inbox that accumulated every
finished thing would be the pile this section exists to avoid — while its
record stays with the item it was about, log and all, until it is old enough
to be swept.

**What the move hands over, it still hands over.** A rebase that stops in a
conflict dispatches its ticket exactly as it did when the reader was watching
([§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)), and
that ticket is a run with a row of its own. The job ends; the work does not.

## 18. The work screen says when, and folds away what is over

What was asked for and what ephor ran are both lists of things that already
happened, and a list of things that happened without times answers half of
what the reader brought to it. "Did I already press this?", "is that the job I
started a minute ago, or the one from yesterday?" — those are questions about
time, and a screen that will not answer them sends the reader to the ledger
and the job directory to read timestamps that were on disk the whole while
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work),
[§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)).

So **every row that already happened says when**, in the age the watch
already spells on its rows rather than a clock time the reader has to
subtract from: a screen read at a glance is read in "12m ago", not in
"14:22". A ticket says when it was **asked for** — the ledger's record of the
dispatch, not anything the plan holds, since the plan is the runtime's and
tracks what the work reached rather than when it was handed over
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)). A
job says when it **ran**, and how long it took; a job still going says how
long it has been going, which is the same question asked of a thing that has
not finished, and the only answer there is.

**An age nothing can source is left off.** A ticket somebody wrote into a
plan by hand was never dispatched, so nothing knows when it was asked for,
and that row simply carries no age. The plan file's own modification time
would be ephor inventing a fact about work it did not start, which is the
same refusal the board makes about rows it did not write
([§15](#15-every-operation-is-visible-in-one-place)).

**What is over folds away.** Tickets accumulate, and are meant to: a reopen,
a second ask, a cancel, months of work about one long-lived change, all of it
kept ([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)).
But a reader opening this screen is looking at what is still going, and the
finished and the cancelled push it down the screen a row at a time until it is
off it. So the screen leads with the tickets that are still open and collects
the ones that are over behind a single line saying how many there are and how
to see them; one key unfolds them in place, in their order in the plan, ages
and all. Nothing is hidden that a keystroke does not show, and nothing is
deleted: this is a reading of the record, and the record is unchanged
([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)).

**A plan with nothing open is not folded to nothing.** Where every ticket is
over, they are all shown: folding the whole list away would leave a heading
above an empty space and a reader wondering where the work went. The fold is
for what is over *beside* what is not.

## 19. A workflow the runtime offers is an action, and its inputs are answered here

A binding brings more than a place to put tickets. It carries **workflows** —
named, parameterized plans that lay down tasks of their own, under a machine of
their own, with fan-out and gates ephor never wrote — and a reader who has one
is in exactly the position
[§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)
describes: the thing worth doing about this row is already written down, and
doing it means leaving, remembering a vocabulary, and coming back. So a
workflow is an entry in the same menu, selected by the same language, ordered
by the same provenance, refused in the same sentence.

Above the binding a workflow is an id, a description, and its **inputs**: each
one a name, a type, whether it is required, and what it stands at when nobody
says. Everything else about it — where workflows live, what a plan rendered
from one looks like, how it is rendered at all — is the binding's, and is
spelled there ([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)). Which workflows there are, and what each
one takes, is the binding's own knowledge too: it is asked of the binding at
the moment of asking rather than kept as a list of ephor's, for the reason the
roster is ([§DA-004-roster-is-asked-not-configured](../decisions/architectural/DA-004-roster-is-asked-not-configured.md#da-004-roster-is-asked-not-configured-the-roster-is-asked-of-the-binding-never-kept-by-ephor)) — a copy is wrong the first
time a workflow is added on the other side. So with no runtime bound there are
no workflows, in the *workable* rung's own words
([§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung)), and nothing else
changes: the plans a workflow already laid down go on being found by looking
and read from disk, like every other plan there
([§15](#15-every-operation-is-visible-in-one-place)).

**An entry names a workflow, and that is what makes it an action.** The entry
is the one the menu already has
([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)) — an id, an
icon, a description, a `when`, its capability requirements — and where it would
carry a command or a brief it carries a workflow's name and the answers to its
inputs instead. It is written in any of three places: beside the workflow
itself, where the binding keeps one somewhere a reader can put a file — there
it travels and versions with the workflow; in the project's manifest; in the
person's own configuration. Narrow beats broad, as everywhere else. A
workflow the binding ships is ranked with what ephor ships, one the person
keeps with the person's own, one the project keeps with the project's offers —
the provenance the menu already orders by, with nothing new to learn, and the
same rule settling two entries that share an id.

**A workflow no entry names is still asked for.** Requiring configuration
before a workflow can be used once is the cost
[§10](#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked) exists to
refuse — but a menu carrying every workflow the machine can find, on every row,
is [§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work) broken at
scale: most of them have nothing to do with the item, and a reader who has to
read past twenty of them to reach the two that do has lost the menu. So the
named ones stand in the menu where they apply, and every other workflow is
behind one entry that opens the list of them, where it is picked, answered, and
run once. What was answered there can be kept as an entry, which is how a
workflow becomes an action without anybody opening a configuration file first.

**The inputs are answered in six steps**, each displacing the ones after it:
what the reader answered explicitly for this instantiation alone; what the
reader supplied in the values files for this instantiation; what the entry
says; what ephor answers for an input about who does the work; the workflow's
own default; and, where an input is required and still unanswered, the reader
— asked, or refused by name where nobody is there to ask. Explicit `--set`
answers therefore displace values-file answers, and both displace the entry,
hand, and workflow defaults. The order is
[§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)'s, deliberately,
so that one resolution order covers everything a dispatch has to settle.

The command-line laying surface accepts repeatable `--values <file>` options.
Each named file is a YAML or JSON mapping, read relative to the directory from
which the command was invoked, and mappings are merged left to right: a later
file replaces an earlier value for the same input. Arrays, records, numbers,
and flags remain their structured types; they are not flattened into words.
Values supplied this way are reader answers and appear as such in the human
and JSON answer accounts, distinct from an explicit `--set` answer. A
non-empty value for an input naming who does the work still resolves as a hand
id through ephor's roster, narrowing, and permitted-hand checks; a values file
never provides a way around that policy.

What the entry says is data, not prose: a string is rendered with the item's
fields where it names them, exactly as a brief is
([§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)), and
anything that is not a string is passed on as it stands, because an input that
wants a number, a flag, or a list of them is not served by a sentence. Where a
list or a record holds strings, those are rendered too — the fields an item
carries are as useful inside a structure as beside one.

**What ephor knows reaches a workflow as files, not only as words.** The
dossier and the identifiers are already written for exactly this
([§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it),
[§8](#8-the-ticket-carries-the-item-as-data-not-only-as-prose)), and a workflow
has no place in its plan for either — so an entry may answer an input with the
dossier or with the item's identifiers, and what the workflow is given is a
path to a file ephor has already written, or that file's contents where the
input wants the text itself. It is written before the workflow is, so an input
that insists its file exists is answered truthfully, and it is written where a
work root's own reading will not mistake it for a plan.

**Every input is answered on one screen, with its answer already standing in
it.** Answering a workflow is not filling in the blanks. The answers that were
resolved are as likely to be the wrong ones as the missing ones — the
workflow's own defaults most of all, which are a stranger's habits about which
model reviews and how many passes it takes — so what the reader is given is
every input the workflow declares, each carrying the answer the five steps
reached and the name of the step it came from, and each one changeable there.
A screen that showed only the holes would be asking the reader to accept
everything else unseen, which is the opposite of what showing an account is
for.

**What has a known set is chosen from it; the rest is typed.** An input is
edited in the shape it actually has. Where the values it can take are known,
one is picked from them rather than spelled: a flag has two, an input that
names who does the work has the roster, already narrowed and already saying
who is unavailable and why
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project),
[§DA-006-hands-fill-a-workflows-targets](../decisions/architectural/DA-006-hands-fill-a-workflows-targets.md#da-006-hands-fill-a-workflows-targets-who-a-workflows-agents-are-is-ephors-answer-not-the-workflows)),
and an input whose own check is a plain set of words has those words. An input
wanting several of them is answered several at a time, from the same set.
Everything else is one line typed on its own row, which is
[§10](#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)'s ask with
the input's name already on it. What ephor reads out of a check is a
convenience and never a second authority: a value the offered set does not
hold can still be typed, and the binding remains the one thing that validates
its own inputs. What no row can carry — a record, or a list of them — is the
reader's editor, on that input alone or over the whole set at once, which is
the same handover an offer already takes.

**What is still unanswered blocks the laying and says so, or refuses by name.**
A required input nobody has answered leaves the screen open, naming what is
missing, rather than laying a workflow down with a hole in it. Where nobody is
there to ask — a dispatch of every matching item at once — the entry refuses
and names the inputs it could not answer, because a workflow written with a
hole in it is a piece of work that looks scheduled and never happens
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)).

**Who does the work is ephor's answer, not the workflow's.** A workflow's
inputs are mostly its agents: which one reviews, which one adjudicates, which
one writes — each defaulted to a model its author happened to have. Left alone,
those defaults are a hole in everything
[§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project) and
[§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions) settle: the
project's table stops applying, the reader's pick stops applying, and a project
that narrowed which hands may see its code is narrowed right up to the moment a
workflow is instantiated, which is the moment it mattered. So an input that
names who does the work resolves through the seven steps like any other hand,
and is rendered into the binding's words in the one place that renders them
([§DA-006-hands-fill-a-workflows-targets](../decisions/architectural/DA-006-hands-fill-a-workflows-targets.md#da-006-hands-fill-a-workflows-targets-who-a-workflows-agents-are-is-ephors-answer-not-the-workflows)); an
input wanting several is answered with several. A hand a narrowing does not
permit is refused with that reason wherever it was named — the workflow's own
default included, since a default is a naming — and never quietly replaced.
An empty answer, including an empty element in an input wanting several,
resolves to nobody: it passes to the workflow as written, no hand is rendered
into it, and no narrowing binds it because nothing was chosen. The workflow's
own fallback can therefore stand. This is the input-side spelling of the
roster's `Choice::Unasked`, which keeps nobody choosing apart from a refusal.

**What it writes is a plan beside the item's, not a ticket inside it.** A
workflow lays down a plan of its own; it cannot be appended to the item's
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)), and pretending otherwise
would mean rewriting somebody else's workflow into ephor's shape and losing
what made it worth having. It is written into the item's own work root all the
same, and everything that follows from that is the point: the operations board
finds it by looking, like every other plan there
([§15](#15-every-operation-is-visible-in-one-place)); it shares the root's one
run, so a workflow and a ticket about the same change queue rather than edit
the same tree at once; and the ledger records the dispatch against the plan it
made, which is the fact that was missing — the record says the item, the entry,
the plan, and what the item looked like
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)), and
gains a plan of its own beside a ticket of its own, which is an addition and
costs nothing to what is already written
([§FS-006-project-interface.11](FS-006-project-interface.md#11-the-interface-is-versioned)). An item that
moved is offered the workflow again rather than a ticket appended to it
([§5](#5-an-item-that-moved-reopens-its-work)), ordered after what came before
and named apart from it, because two runs of one workflow about one item are
two records and not a correction of the first.

**Instantiating writes files; running is the move after it.** Everything
[§7](#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)
guarantees holds here and holds for the same reason: what a key press does is
write a plan, and what runs it is the reader, from the board where every other
operation is run. A workflow lays down more than a ticket does — a directory,
a machine, sometimes settings — so what is about to be written is shown before
it is, in the binding's own account of it beside ephor's own account of every
input it answered and where the answer came from. A workflow that fails to be
written leaves nothing behind, so the board is never given half of one to
report on, and the refusal read back is the binding's own.

**A values-file refusal leaves no partial laying.** A missing, unreadable,
malformed, or non-mapping values file is reported before a plan or workspace
is written. The same is true when the runtime rejects the effective values:
ephor validates them before creating the destination and reports the runtime's
refusal without leaving a partial workflow workspace. With no `--values`, the
existing laying behavior is unchanged, including a dry run's guarantee that
nothing is written.

**What comes back is what the plan says, and no more.** A ticket ephor wrote
carries the shape a verdict and a proposed answer are read out of
([§13](#13-a-communication-is-work-too-and-its-answer-comes-back-as-a-proposal));
a workflow answers to its author, not to ephor, and reading a verdict out of
one would be ephor inventing a fact about work it did not shape. So workflow
work is read at the altitude every plan is read at — its states, what is
waiting, what is finished — and nothing is attached to the matter that the
matter did not get. In the same spirit, where a workflow brings settings of its
own into a work root, those settings are the root's from then on and govern
every plan in it, ephor's tickets included: what changed is reported when it is
written, because a workflow quietly re-answering how another workflow's agents
run is exactly the kind of fact a watch exists to say out loud.

## 20. A run of the runtime starts beneath the screen, and is watched by attaching

Pressing the key that runs the runtime hands it the whole interface for as
long as the run takes — and a run takes as long as the work does. Everything
[§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen) says about a
replay holds here with more force: the work was handed over precisely so that
nobody had to stay
([§7](#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)),
ephor is the half that remembers, and a screen given away to one run cannot
watch the other items, cannot start the next move, and cannot even say that
the first one is still going. A run that the reader *wants* to watch is the
exception that was made the rule.

So **a run starts detached**: where the binding can, the runtime is started in
a session of its own, outliving the screen and the terminal that started it,
and what the reader gets is one line saying the run began and what it is
called. The root turns live on the board
([§15](#15-every-operation-is-visible-in-one-place)) from the lock, as every
run does — nothing new is watched, because nothing about "is a run live here"
changed. A move that needs nobody does not suddenly need somebody staying,
and a run is the longest such move ephor makes.

**A run has an identity, and it is the binding's.** A live run names itself —
an id, and while it serves one, the address of its control — and both are
read from the artifacts the binding leaves beside its lock, never from
anything ephor remembers having started: a run somebody started in another
terminal, on a root ephor never dispatched into, has the same identity and is
reached the same way
([§15](#15-every-operation-is-visible-in-one-place)). An id is how the reader
and the runtime agree on which run they mean, so the board says it on the
row, the work screen says it on the operation, and the command line prints it
with the rest ([§FS-011-command-line.8](FS-011-command-line.md#8-what-is-going-is-said-and-the-way-in-is-printed)).

**Watching is attaching.** The binding's own surface for a run it did not
start — a reader of the run's files and a client of its control — is opened
on the run, and leaving that surface detaches and never stops the run: the
reflex that ends a foreground command must not end a run another screen may
also be watching. The surface is something the reader types into, so by
[§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)'s own rule it
takes the reader's terminal — or a window of the reader's own, where one is
bound ([§22](#22-a-window-of-the-readers-own-where-one-is-bound)). What the
surface can do — answer a question the run parked, release a gate, intervene
— is the binding's, unchanged, and ephor adds nothing to it and takes nothing
from it.

**Stopping stays out of the screen.** The board starts nothing and stops
nothing ([§15](#15-every-operation-is-visible-in-one-place)), and a detached
run does not change that: where a run can be stopped, the row carries the
runner's own command for stopping it, in the runner's own words, exactly as a
claim carries the command that releases it
([§10](#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)). A key that
stopped a run would be a channel to the run ephor promised never to hold.

**A question a detached run asks still reaches the reader.** A run with
nobody at its terminal waits at a human gate rather than exiting — that is
the binding's own contract, and it is the right one, because the person who
releases the gate is expected to arrive later. ephor reads the wait exactly
as it reads a parked ticket
([§9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)):
the ticket says *waiting on you* wherever the reader is looking, and the way
to answer is to attach.

**Where the binding cannot detach, the run is watched as it was.** A runner
with no detached shape, or a platform that has none, runs attached — the
terminal handed over, the reader watching — and the one line says so rather
than pretending. With no runtime bound there is no run to start, in the
workable rung's own words ([§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung)).

## 21. What is already going is shown where it could be started again

The menu says what can be done about a row; the board says what is being
done. Kept apart, they forget each other: a reader who opens the menu on an
item whose rebase is already replaying is shown the rebase as something to
start, presses it, and is either refused by a lock or has started a second
one — and has to visit the board to learn which. That is the watch knowing a
fact and not saying it where the reader is looking ([§GOAL-002-glance](../goals.md#goal-002-glance-one-glance-answers-what-needs-me-now)).

So **every entry that has work going about its subject is marked running,
and set apart**: the running entries stand first, under a line that says so,
indented a step further than the rest, in one colour reserved for what is
going and used for nothing else on that screen. Each says how long it has
been going and what it is at right now — the job's own last line, the ticket
a run holds and the state it is in, *waiting on you* where the ticket it
opened is parked, with a run still on the root or without one, *queued* where
the root's run will reach it — in the words the board already uses
([§15](#15-every-operation-is-visible-in-one-place),
[§18](#18-the-work-screen-says-when-and-folds-away-what-is-over)), because
this is the board's reading narrowed to one row, not a second reading.

**What counts as going is found by looking, here too.** A command entry is
running where a job started from that entry, about this subject, still holds
its lock — so a job records which entry it came from and, on a branch row,
which branch, or nothing could ever match it back
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)); the checkout
row is running where the job that is making the workspace is. An entry that
hands work over is running where the ticket it would open, or the plan it
would lay, is open and its root is live or will reach it — the very facts the
work's own lines beneath the row are made of
([§9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking),
[§23](#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)) —
and a ticket the run parked counts, live root or not, because a question
standing on this subject is exactly what a second dispatch must not be laid
beside.
An entry whose program runs in a window of the reader's own is running while
that window holds it
([§22](#22-a-window-of-the-readers-own-where-one-is-bound)). Nothing here is
remembered from the keypress: a second ephor opening the same menu sees the
same rows, and a job that died is not running, whatever started it.

**Pressing a running entry opens it; it never starts it again.** The key on a
row that says *running* goes to the thing that is running: a job's log,
followed as it writes ([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen));
a run of the runtime, attached
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching));
a program in its own window, that window brought forward
([§22](#22-a-window-of-the-readers-own-where-one-is-bound)). A second copy is
not what a reader pressing a row that says *running* meant, and where
somebody does mean it the command line starts it and the refusal is the
lock's own sentence. The footer says *open* on such a row, not *run*, because
it is built from the row and not from the key ([§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)).

**Both surfaces say it.** The list `ephor actions` prints carries the same
mark with the same facts — what is running, since when, and the way in
([§FS-011-command-line.8](FS-011-command-line.md#8-what-is-going-is-said-and-the-way-in-is-printed)) — so that a program reading the menu cannot start
what a person reading it would have opened ([§REQ-002-parity.2](../requirements/REQ-002-parity.md#2-parity-runs-both-ways)).

## 22. A window of the reader's own, where one is bound

Two kinds of thing above want a terminal: the surface that attaches to a run
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)),
and the program an entry *is* — an editor, a pager, a coding agent's own
session ([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)). ephor has one terminal and is sitting
in it. Handing it over works everywhere, and stays the floor: ephor leaves,
the program runs, the reader comes back. But a reader inside a multiplexer,
or in a terminal that opens windows on request, has a better move available
— the program in a window of its own, ephor still on screen, and "open" from
then on meaning *bring that window forward* — and a tool that cannot make
that move sends them to make it by hand, which is the sweep this project
exists to retire ([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)).

So **the window is a seam** ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy), decided with its tradeoff
recorded in [§DA-007-window-is-a-bound-opener](../decisions/architectural/DA-007-window-is-a-bound-opener.md#da-007-window-is-a-bound-opener-a-window-of-the-readers-own-is-a-bound-opener-with-the-terminal-as-the-floor)), with the anatomy every seam
has. The contract is in materials: one command that opens a window
running a given command and prints a handle for the window it made, and one
that brings a handle forward. The binding is configured — `window` in site
configuration names which — and ephor ships bindings for the common shapes
(a terminal multiplexer, and terminals that take remote commands), chosen
unasked when the environment ephor is running in says which one the reader
is sitting inside, and never by spawning one to find out. The degrade rule is
the floor: no binding and no recognized environment means no window, and
the terminal is handed over as it always was. A window is the reader's: ephor
opens it and brings it forward, and never closes it or ends what is in it
([§15](#15-every-operation-is-visible-in-one-place)).

**An entry may ask for a window.** An offer or a configured action says
`window` as it already says `background`
([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)): its program runs in a window of its own
instead of taking the terminal, and ephor stays where it was. Such a program
is an operation while it runs — it holds a lock as a job does
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)), its record
keeps the window's handle, and the window is its inspection where a log
would have been, because what it writes is on that screen and nowhere else.
That is what makes a coding agent started from the menu a row that says
*running* and opens to the agent
([§21](#21-what-is-already-going-is-shown-where-it-could-be-started-again)),
rather than a program ephor handed the terminal to and forgot. Where no
window can be opened, the entry takes the terminal as it always did, and
says so.

**Attaching goes to a window where one is bound.** The surface on a run
opens in a window when there is one to open, and in the terminal otherwise;
either way leaving it detaches and the run goes on
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)).

## 23. Work stands on rows of its own, beneath the row it is about

A matter's work used to ride on the end of the matter's own line — one
phrase, after the title, the state and the gate: *⚙ fix-gate · fix*. It said
the true thing and left the reader nowhere to go with it. The line is not a
row, so the cursor cannot reach it; the keys on the row it rides belong to
the matter, so none of them are the work's; and the phrase is cut to what is
left of the width after everything else on the line has taken its share. A
reader who reads *⚙ fix-gate · fix* and wants to take it back has to open a
second screen to find the thing they were already looking at, which is the
sweep this project exists to retire ([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)).

So **the work comes off the matter's line and stands on lines of its own,
beneath it** — indented a step, selectable like any other row, one per ticket
the plan holds open. The matter's line goes back to being about the matter,
and the work is where a key can reach it.

**What each line says** is what the board and the work screen already say, in
their words, because this is that reading narrowed to one matter and not a
third one of its own
([§15](#15-every-operation-is-visible-in-one-place),
[§18](#18-the-work-screen-says-when-and-folds-away-what-is-over)): the ticket's
recipe and the state it is in, and how long since it was asked for, where the
ledger knows — a ticket nobody dispatched carries no age rather than a guessed
one. A ticket the runtime parked says *waiting on you* and stands first among
them, since it is the one part nobody else will move
([§9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)).

**A ticket a run has in hand says so, and says it at a glance.** *Open* and
*being worked on right now* are different facts, and a row that spelled them
the same way left the reader to guess which of the two they were looking at:
the ticket an agent is inside of and the ticket nothing has picked up wore one
marker and one colour, and the only way to tell them apart was to leave for
another screen — which is the sweep this project exists to retire
([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)). So a ticket a live run holds is marked apart from one
merely open, read from the run's own record of itself
([§15.2](#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)) and
phrased as the board phrases it, because this is that reading narrowed to one
matter and not a third one of its own
([§15](#15-every-operation-is-visible-in-one-place)). A ticket on a root whose
run is live but busy elsewhere is *queued*, for the same reason the board says
so: it will get its turn without anyone doing anything. And a live run that has
gone quiet carries the badge it carries there — a long tool call is legitimately
quiet, so it is a badge and never a verdict.

Nothing here is a new question asked of the world: it is the liveness the watch
already probes and the record the run already writes, said on the row the
reader is already looking at.

**What is over is one line, not many.** Tickets accumulate and are all kept
([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)), and
a tree that grew a line per finished ticket would bury the matters between
them. So where a plan holds nothing open, its work is one line for what the
last ticket decided — the verdict, or *cancelled* — and where it holds
something open, what is over is not on the tree at all: the work screen is
where the whole record is read
([§18](#18-the-work-screen-says-when-and-folds-away-what-is-over)). An item
that has moved under its work says so on a line of its own, in the words §5
gives it, because that is a fact about the work and not about the matter
([§5](#5-an-item-that-moved-reopens-its-work)).

**The keys are the work's, on the row the work is on.** On such a line,
cancel takes *that* ticket back — named, with no second screen to choose it on
([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)) —
attach watches the run holding it
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)),
and the plan opens for reading. What the line is *about* is still the matter,
so the keys that go to the matter — its thread, its gate is not among them,
its work screen, its menu — go there from here too. A key means one thing at
a time and the footer says which, measured against the row the cursor is on
rather than the screen it is in ([§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)): where the cursor
stands on work, the footer offers the work's keys and not the ones they
displaced.

**A line is offered only where the move behind it would work.** Cancelling is
the runtime's move and is refused in the runtime rung's words with nothing
bound ([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so));
attaching needs a run actually holding the root, read at the keypress from the
lock and never remembered
([§15](#15-every-operation-is-visible-in-one-place)); a line for work that is
over has no ticket to take back. Each says so in one sentence rather than
appearing to act.

## 24. Work nobody has to start starts itself

A ticket that needs a person to press a key before anything happens to it is
a ticket waiting on a person, whatever its state says. For the work a reader
has already decided about — *the gate is red, collect what failed and fix
it* — that key is a formality standing between the dispatch and the run, and
the interval it costs is not the reader's attention being spent well: it is
work sitting in a plan on disk while the person who asked for it does
something else, and the row saying *⚙ fix-gate · collect* the whole time,
which is true and reads as *going* when nothing is going at all.

So **a recipe may say that the work it asks for needs nobody to start it**,
and a ticket written from such a recipe gets its run without anyone pressing
anything. The reader's deliberate act moves one step earlier and is made
once: adopting the recipe, rather than starting each of its tickets
([§7](#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)).
Everything a recipe already decides — which items deserve work, what to ask
for, whose hand does it — is the same decision, and *and do not wait for me*
belongs beside it.

**Nothing autoruns unasked.** Silence means the key, exactly as before: a
recipe that says nothing about this is started by the reader, and so is
every menu entry, every workflow entry that did not say it, and every plan
somebody wrote by hand. The setting is written on the thing that hands work
over — a recipe, or an entry that lays a workflow down
([§28](#28-a-workflow-entry-can-ask-for-the-same-thing-a-recipe-can)) — and
nowhere else, because the reader who trusts one kind of work to start itself
has said nothing about the rest.

**Starting is a sweep, and the sweep reads the world.** What starts a run is
not a memory of having dispatched something — that would be the ledger
deciding what exists, which is the one thing it never does
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)) —
but the same looking every other reading here does
([§15](#15-every-operation-is-visible-in-one-place)): a root is **due** when
a plan in it holds a ticket that is open, unclaimed, not parked on a
question, and from a recipe that asked to run itself — and no run is live in
the checkout that root's work would run in. A ticket a hand wrote into such a
plan is due exactly as a dispatched one is; the recipe is a fact about the
ticket, not about who appended it.

**A dispatch may give its autoruns arguments, for that invocation alone.**
`ephor work dispatch -- <RUNNER_ARGS>...` supplies the trailing vector to
every runtime run started by that dispatch's own sweep, unchanged and in the
order it was given. Ephor neither interprets the arguments nor stores them in
the plan, ledger, or configuration: a later due sweep does not inherit them.
Omitting the vector is the empty vector and preserves the behaviour dispatch
had before this form existed. The passthrough already accepted by `work run`
is unchanged, and `work sync` and interface actions still supply no runner
arguments. A dry run still starts no runtime, whether or not a trailing vector
was supplied.

**One live run per checkout, because a run is an agent editing a working
tree.** A root a run already holds is left alone: the runtime schedules one
run per root, the live run reaches a ticket written beneath it, and a second
run there would only wait for the first
([§15](#15-every-operation-is-visible-in-one-place)). But the root is not what
is really being shared. A run is made *in a checkout*
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)), and two work roots over
one tree — a second panta beside the first, or a root somebody pointed
elsewhere — are two agents editing the same files. So the invariant is over
the checkout, not the root: a root is left alone when a run holds it **or**
when a live run holds the tree its work would run in, whichever root that run
was started from. The trees are compared as the file system resolves them
rather than as somebody spelled them, so a symbolic link or a relative
spelling does not defeat the guard.

**The guard is where runs start, never where plans are written.** Laying a
ticket into a busy checkout's work root stays legal — writing a file is all
ephor does there, and the ticket simply waits in the plan until the tree is
free. That is what makes handing one down safe: a conflict written into the
very tree that is stopped mid-rebase is a note for whoever gets there next,
not a second agent in it. Nothing refuses at dispatch, at lay, or at sync for
this reason.

**A run the reader asks for by name is refused by name.** `ephor work run` on
a plan whose checkout a live run holds starts nothing and says *a run is live
in this checkout*, naming the run — its own id where it published one
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)),
the root holding it where it did not — so the reader is sent to the run that
is in the way rather than to a guess. A run this same invocation has just
started counts: one command over two work roots in one tree starts one of
them and refuses the rest, naming the run it made a moment ago, because a
tree is as busy from a run one second old as from one that was there all
along. `--force` starts anyway, for the reader who knows what that other run
is doing; it lifts this refusal and nothing else, on the run asked for by
name — the sweep below is not a run anybody asked for by name and is never
forced. A refusal is a non-launch outcome of a command that was understood,
not a failure of it.

**The key in the interface is a place a run starts, so it is guarded there
too.** Pressing it on a matter whose checkout a live run holds starts nothing
and says the same sentence, naming the same run: the invariant is over the
tree the agent edits, and it cannot depend on which surface the reader
reached for. There is no forcing it from the screen — the reader who means
to start a second run in a busy tree says so on the command line, where the
flag is.

This is why the sweep can be run as often as anything cares to run it and asks
nothing about what it did last time — a due root gets a run, a root whose
checkout is busy gets nothing, and running the sweep twice in a second is the
same as running it once. Inside one sweep the same holds without a second
look at the world: a checkout a launch has just taken is taken for the rest of
that sweep, and a later due root over the same tree is passed over with the
reason, which is a successful non-launch outcome and not a failed start.

**A root held back by another root's run is passed over, not dropped.** The
sweep says so in the same words and in the same kind of row as the tree it
took mid-sweep: a reader told only that nothing started would go looking for
a ceiling that is not full, and an empty sweep is exactly what a quiet
machine looks like. The root whose *own* run is live is the one exception and
stays silent — it has its run, that run reaches the tickets written beneath
it, and saying so every sweep would report on the ordinary case for as long
as the work takes.

**Autorun may be bounded at three nested scopes without changing the manual
key.** The site's `work.max_concurrent` is the aggregate ceiling on live runs
across all work roots; `organizations.<org-id>.work.max_concurrent` is a
ceiling on the live roots of every project that organization holds, inside
the site's; and `projects.<id>.work.max_concurrent` is a ceiling on that one
project's live roots, inside both. The site holds every organization, an
organization holds its projects, and a project holds its roots — nesting the
registry already declares, because which organization a project belongs to is
the `organization` field on its registry row and nothing else, read here and
never written
([§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)).
Leaving any of the three out leaves that ceiling unlimited; writing `0`
admits no new autorun starts under it. An absent `organizations` map is an
omitted ceiling for every organization, so a configuration that has never
heard of this tier starts exactly the runs it started before; a project whose
registry row names no organization is under no organization ceiling. These
limits apply only to this sweep: a run a reader explicitly starts keeps being
the reader's move. `ephor work run --due --max-concurrent N` replaces the
site's configured aggregate ceiling for that invocation, including when `N`
is zero; every organization and project ceiling still applies inside the
command-line ceiling.

**The organization tier carries more than a ceiling.** `work.root` is read at
these same three scopes and by the same registry membership
([§6.1](#61-the-work-root-is-a-template-and-it-may-reach-above-the-project)),
so an organization block is where both the budget its projects share and the
place their work goes are written. The two are read differently and have to
be: every ceiling is evaluated and the outermost full one refuses, while a
root is a single answer, so the innermost scope that writes one is the whole
answer and nothing above it is asked.

**Every ceiling is evaluated, and the outermost full one is the reason.** A
start is refused by whichever is full first, asked outermost scope inward —
site, then organization, then project — and within one scope by roots in
flight before working roots. So no root begins because an inner ceiling had
room while an outer one did not, the reason a reader is given names the widest
thing that was actually full, and a scope that declared no working ceiling
answers exactly as it did before there was one. None of them replaces another
and none clamps another: they are questions asked of the same start, and the
answer is the first *no*.

**A ceiling written the wrong way round is named, not corrected.** An
organization's number is expected to be the larger of the pair, because it is
the budget its projects share, so a project ceiling above its organization's
— or above the site's — is almost always a mistake. It is warned about by
name at each sweep that reads the ceilings — `ephor work run --due`,
`ephor work dispatch` and `ephor work sync` — and the warning says which
project, which ceiling it is above, and both numbers, so it is seen when it
bites rather than only when someone runs a check over the file. It is not
refused and the project's number is not rewritten: whoever configured the one
project is closer to it than whoever set the number above it, and a tool that
silently clamped them would be deciding something it does not know. Nor is
the warning a licence — the organization's total still binds, because a pair
written the wrong way round is not permission to exceed a ceiling somebody
set on purpose. The site number a pair is measured against is the configured
one: `--max-concurrent` narrows a single sweep deliberately, so a project
ceiling above it is the reader's own choice rather than a configuration
contradicting itself, and nothing is said about it. A ceiling of `0` above is
not a pair at all: it admits no new starts under it, so it is a pause the
reader wrote on purpose rather than a budget a project could be above, and
the numbers beneath a paused site or organization are said nothing about
either — a configuration that pauses the site and leaves its project numbers
where they were is not thereby a configuration that contradicts itself.

**A ceiling over nobody is said out loud.** An `organizations.<org-id>` key
bounds nothing exactly when no registry row places a project inside that
organization — the same reading the ceiling itself binds through, so a key
that is refusing starts is never announced as bounding nobody. Two
configurations arrive at that emptiness: an id no registry row names at all,
which is the typo removing the bound its author believes they set, and an
organization the registry declares that no project has yet joined. They are
one condition and are named the same way, because membership is the
`organization` field on a project's registry row and nothing else. Bounding
nothing is the one thing a ceiling may never quietly be, so it is named — by
`ephor doctor`, in the words an unknown project id is named in, and at the
sweep where the missing bound would have been read. It is not an error and
nothing is refused for it: the runs that would have happened without the key
still happen, and the reader is told why the key they wrote is not the one
biting.

**A second ceiling bounds the work an agent is actually doing.** The three
above bound roots in flight — worktrees, processes, and the burst when several
resume at once — which is not the question *how much may be spent at once*. So
`work.max_active` is a second aggregate ceiling over the live roots that are
**active**, and `projects.<id>.work.max_active` is that project's ceiling
inside it. Both read exactly as their `max_concurrent` counterparts: omitted is
unlimited, `0` admits no new autorun starts under it, the project number sits
inside the site one, and a run the reader started by hand is outside both. The
organization tier bounds roots in flight only — an organization is a budget of
machine, and the working ceiling is deliberately left to the site and the one
project until a reader asks for the middle of it. Omission is the default
everywhere, so a configuration naming only `max_concurrent` is bounded exactly
as it was, in behaviour and in wording, and `--max-concurrent N` replaces the
site's roots-in-flight ceiling alone.

**A live root is parked when what it waits for is a person.** Every live root
counts toward the flight ceilings — it exists, and that is what those numbers
count — and toward `max_active` too *unless* nothing in it is being worked and
something in it is somebody's turn: no open ticket witnessed held by the run,
and at least one open ticket in a state the machine in force calls gating
([§9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)) or
whose poll declares whose answer it waits for — one fact, differing only in who
resumes it. It is judged per plan by the machine answering for that plan, as
every reading of what a ticket is doing is
([§15](#15-every-operation-is-visible-in-one-place)); a plan whose machine
cannot be read makes its root active, because a misreading must cost a slot
rather than hand one out. And the ceilings gate starts, not what is under way
already: parked roots resuming together can carry active work above
`max_active` until one finishes, capacity here being live work, not attempts.

**Capacity is live work, not attempts.** One root snapshot supplies both the
due candidates and the live counts. Every already-live root consumes one
aggregate slot, one slot in each project whose plan it holds, and one slot in
each organization holding such a project, whether or not that root is due or
selected by a command's `--project`. A root holding plans from two projects
of one organization spends one of that organization's slots rather than two:
the slot is the live run, and there is one of those. A successful start
consumes those same slots, as active work, only while its runtime lock remains
held. A start
that fails, reports itself finished during the launch handshake, or has
already released its lock leaves the slot for the next candidate in the same
sweep. The failed-start back-off still applies independently.

**The highest-ranked due roots get the available slots.** Where
`work.ranking` names item ids, due roots for those items are considered in the
file's order before the rest; roots the ranking does not distinguish retain
the deterministic root-path order the sweep already had. The ranking orders
and never filters. Every otherwise eligible root not started solely because
an aggregate, organization, or project ceiling is full is returned as one
`passed-over` outcome whose reason names the key it refused on, in prose and
`--json`; and the reading says how many roots are live and how many of those
are parked, so a full ceiling never hides that a person holds one of the
slots. Passing a root over is not a failed launch and does not increase the
reading's `failed` count.

**The sweep is safe where the key was.** Everything dispatch refuses before
writing a ticket, starting refuses before running one
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)):
a branch that is not in the working tree the plan is about is not run in,
whatever the directory holds
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)). And what it starts is
the run [§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)
already describes — detached, identified by the binding, watched by
attaching. Nothing about how a run is seen, stopped, or answered changes
because nobody pressed the key that began it.

**A start that fails is not tried again immediately.** A root that cannot
start a run — a runner that refuses, a workspace that has gone wrong — would
otherwise be retried by every sweep for as long as the ticket stays open,
which is a loop nobody asked for and the most expensive kind of quiet. So a
failed start is remembered as ephor's own record of what ephor did (never as
work state — that is still the plan's
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work))),
and that root is left alone for a while, longer each time it fails. The
failure itself is not swallowed: it lands under the row it was about, the
way a job's outcome does
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)), so a reader
who never pressed anything still learns that the thing they did not press
did not happen.

**The reader keeps the key.** Starting a run by hand is unchanged and is
never refused on the grounds that a sweep would have got there — a reader
who wants it now says so, and a recipe that autoruns is still work whose run
can be watched, attached to, and stopped in the runner's own words. The
sweep only removes the requirement that somebody be present, and the board
stays what it is: it starts nothing itself, and what it shows is the run,
whoever asked for it
([§15](#15-every-operation-is-visible-in-one-place)).

## 25. Work about a matter with no branch can mint the branch it needs

A pull request arrives with a branch, and everything above resolves from it:
the workspace it is checked out in, the work root inside that workspace, the
`{branch}` the ticket carries. An issue arrives with none — which is not a gap
in what ephor knows, because an issue *has* no branch until somebody cuts one.
But it leaves the kind of work that most needs a checkout, *do this issue*,
with nowhere to be done: a project whose checkouts are one per branch has no
workspace for a branch nobody has cut, and its root is a directory holding
those workspaces rather than a checkout of anything.

So **an entry that hands work over may say which branch that work belongs on**,
as a template rendered against the matter: `"branch": "fix/issue-{number}"`. It
is written on an entry that asks for a ticket or lays down a workflow — a
configured action carrying `agent` or `workflow`, a project's own offer naming
a workflow, the entry beside a workflow, and a recipe
([§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for),
[§19](#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here),
[§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)) — and never on
one that runs a command here: those say what they need on disk with
`requires_checkout`, and the workspace they need is one somebody else has
already made ([§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)).

The shipped `implement` recipe (§1) carries this template by default. Therefore
an unconfigured dispatch for a branch-less issue mints `fix/issue-<number>` and
writes its plan inside that workspace when the project has
`branch_root_template`; without that registry field it refuses by name and
explains the configuration required, leaving no work root or partial workspace.
An existing matter branch still wins, and a configured `implement` recipe still
replaces the shipped recipe and chooses its own branch behavior.

**The template is rendered like a brief**, from the same fields — `{number}`,
`{repo}`, `{kind}`, `{title}`, `{ticket}`, and the rest of
[§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it)'s vocabulary —
and three of them it may not name, because they are what it produces:
`{branch}`, `{workspace}` and `{reply}`.

**A template that is wrong for every matter is refused by name**, where it is
read and rather than turned into a directory nobody meant, and the refusal
says which of the three things is wrong with it: it names one of the three
fields it decides; it names something that is no field of a matter at all, and
the refusal lists the ones it may name; or what it renders is not a name git
will take as a branch. The last of those is answered here rather than left to
the checkout: git's own refusal arrives from inside the making, by which time
the directories leading to the workspace are there, so a template git will
not take is held to that before anything is made.

**A template that names a field this matter has not got means the entry does
not serve this matter.** It is withheld from the menu and its readings, and
from dispatch selection — including unattended sweeps — rather than selected
and refused, because another matter can carry the field and render the same
template correctly. The `work offers` reading names the excluded entry and
the field it needed, as §27 requires; a silent disappearance would leave the
reader unable to distinguish an incompatible matter from no configured work.

**The matter's own branch always wins — but the project's main branch is
never a matter's own.** A pull request keeps the branch the forge recorded
and a matter the registry placed keeps the branch it matched; the template
applies only where the matter has no branch at all. The project's configured
`main_branch` is the trunk every workspace is grown from, not a branch any
issue or pull request owns, so a registry match to it counts as no branch
here: the template mints exactly as it would for a matter matched to
nothing, and the refusal below fires the same way where none applies. "Here"
is every write that places work: the workspace an edit is made in, and the
work root a dispatch or a lay writes its plan into — laying a ticket is a
write to a directory, not a reading of the change, so it resolves this way
even where the work it lays down only reads, and no plan about such a matter
is written inside the main checkout. What keeps answering from the registry
match itself, main branch included, is the reading: where the matter's code
lives right now — the directory a command about it runs in, and what the
work is told about where it is — and anything a reading shows about the
matter's own branch and checkout. Neither of those places anything. A
forge-recorded branch that happens to equal `main_branch` is the forge's own
fact and keeps winning — only the registry-matched arm is carved out.
Rendering it *is* the resolution, and nothing is written down: a second
dispatch about the same matter renders the same name, resolves the same
workspace, and lands beside the first, and a workspace that is already on
disk is worked in as it stands.

**Saying it means the work needs the checkout.** An entry carrying a `branch`
is work about a change and belongs inside that change's own workspace;
`needs_checkout` and `requires_checkout` go on meaning exactly what they meant
for every entry that says nothing.

**The workspace is made by the one checkout operation**
([§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout),
[§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)): the same
source checkout, the same directory template, the same trees grown from the
project's main branch, the same task store — the third caller of one
implementation, so a workspace dispatch makes and a workspace the reader's key
makes cannot be two different things. Nothing is written to the registry: a
workspace ephor made is found on disk like every other
([§FS-008-attribution.2](FS-008-attribution.md#2-two-stages-one-engine)). Nothing is pushed either
— publishing the branch is the work's move, not ephor's
([§7](#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)).
And a project with no directory template for its branches is refused by name,
the way a single-checkout project standing on other code already is
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)): nothing is minted into a
root that is itself the checkout.

**It is made after every refusal and before the first write.** Who does the
work is chosen, the machine is vetted, the workflow's inputs are answered —
and only then does the workspace appear, so a refusal still leaves nothing
behind
([§19](#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)).
The machine vetted is the one in force where the work root is already there,
and the one ephor would install where it is not: a workspace that does not
exist declares nothing, and minting one in order to read its machine back is
the leaving-behind this rule forbids.
One case escapes it, and is the only one: a runtime that installs a machine of
its own when it is asked to make the store
([§FS-006-project-interface.7](FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live))
installs it inside the workspace the mint has just made, and ephor leaves what
the runner left standing. That machine is read after the workspace exists, so a
refusal on it is the one refusal that outlives the mint — and it says so,
naming the workspace it made, because a workspace nothing mentioned is one
nobody knows to look at. Nothing further is made behind it: dispatching into
that workspace again refuses on the same machine, now the one the work root
declares.
A run asked what it would do makes nothing at all: not the workspace, not the
work root inside it, not the files a runtime would be shown — those have
nowhere to go until the workspace exists — and it says the branch and the
directory it would make and the plan path inside it instead. A repository the
checkout refuses is the checkout's own refusal, reported in the checkout's own
words, and nothing is dispatched behind it.

**What the work sees is the minted branch.** `{branch}` and `{workspace}`
render with it, the ticket's identifiers carry it
([§8](#8-the-ticket-carries-the-item-as-data-not-only-as-prose)), the ledger
records it, and the work root resolves inside the workspace — so the plan lands
in the tree the work will edit rather than beside it. A surface asking about
that work before it is dispatched asks about the same workspace: the hand an
entry would go to and the roster a picker offers are read against the work root
the dispatch will use
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)), which for an
entry carrying a `branch` is the root inside the workspace that entry names and
not the project's own.

**Offers follow.** An entry carrying a `branch` is offered on a matter with no
branch, in the *will check out first* shape rather than blocked as *the
matter's branch is unknown*
([§FS-004-quick-actions.2](FS-004-quick-actions.md#2-offered-only-where-it-would-work)), on the menu
and in every reading of the same menu ([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)).

**And without a template, the command line refuses what the menu refuses.**
Work that edits the change, about a matter no branch could be found for, on a
project whose checkouts are one per branch, is refused — naming `branch` as the
way out — where it used to be written at the project root. That fallback was
the defect: work about a change, written into a directory that holds no change,
which [§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)
does not allow and the menu has always blocked. This is the two surfaces coming
to agree ([§REQ-002-parity.2](../requirements/REQ-002-parity.md#2-parity-runs-both-ways)), and it is the one thing here that changes for a
configuration written before it.

**The same refusal covers a matter matched only to the main branch.** Since
that match counts as no branch above, work that edits the change is refused
exactly as it is for a matter truly unmatched — but the refusal names the
main branch it declined rather than calling the matter's branch unknown, so
the reader is told what was passed over rather than only that nothing was
found.

## 26. An ordering already made can be read, and a limit bounds what runs

A sweep dispatches every eligible item in one order — today, newest
`updated_at` first — and that order decides which items get a ticket first.
Ephor has no opinion of its own about which item matters more: it does not
compute a rank from labels, from reactions, or from anything else, and it
does not ask a project to compute one either
([§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)).
What it can do is read one a project already wrote.

**The ranking arrives as a file: an ordered list of item ids, one per line,
most important first.** The id is the one `ephor feed` prints and `--item`
already takes everywhere else — `github-issues:vjovanov/rhei#95` — never a
URL, because a matter without one (a project's own task,
[§FS-006-project-interface.7](FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live))
would otherwise be permanently unrankable. Order in the file *is* the rank:
there are no scores, no bands, and nothing here interprets a tie.

**It is named two ways, the second good for one run.** The `work` block of
site configuration takes an optional `"ranking": "<path>"`, and
`ephor work dispatch --ranking <path>` displaces it for that invocation alone
— the same displacement `--hand` already gives a single dispatch
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)).

**Ranked items dispatch first, in the file's own order; everything the file
does not name follows, in the order it already had.** The file orders — it
never filters. An item the file is silent about is still eligible, still
offered a recipe, and still dispatched; it merely sorts after every item the
file did name.

**`--limit N` bounds how many items are dispatched — opened, or would-open
under `--dry-run` — taken from the top of that order.** It is the reader's
own number, not the file's: nothing a ranking names causes an item to be
dispatched that a recipe would not already have matched. A recipe decides
which items deserve work at all; a rank only orders the work the reader
already chose to do
([§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)). An
item skipped for another reason — it already has work, it fails `--kind` or
`--updated-within`, no recipe applies — costs nothing against the bound; only
an item actually dispatched does. An item whose deterministic opening move
finishes with nothing to hand over
([§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)) opened
nothing either, and for the same reason costs nothing against the bound.

**A file that is absent, empty, or unreadable is not an error.** The sweep
falls back to the order it always used, and says which of the three happened
rather than failing silently. With no ranking configured and no `--limit`,
nothing about the sweep's behaviour or its output changes: this is a
capability turned on by naming it, not a default anyone pays for unasked.

**An id in the file that matches no eligible item is skipped and named, not
fatal.** Eligible here means the sweep's own project and recency-filtered
set — after `--project` and the feed's own recency window, before `--kind`,
`--item`, or `--updated-within` narrow it further — so an id excluded only by
one of those still matches. A ranking outliving the matter it names is
ordinary — an issue closes, a pull request merges — and the sweep continues
past it exactly as it does past everything else it cannot use.

**The reading says which file it used and how old it is, in prose and in
`--json` alike**
([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)),
so a ranking nobody has refreshed in a while is visible rather than
mysterious, and every id the file named that matched nothing is said the same
way.

**The producer of the file is out of scope.** Whatever writes it — a script
over a label, a person with an editor — is not ephor's concern here, and
nothing ephor ships makes one.

## 27. An offer that a selector refused says why

A recipe a selector refused and a recipe nobody wrote are not the same fact,
though an empty offers list has always told them apart with the one sentence
"nothing matches this matter". The first is something a reader can act on —
loosen the selector, or learn that the matter simply does not carry the field
it asked about — and the second is nothing to act on at all.

So **a recipe considered for the matter and refused by its selector or its
branch template is named, beside what refused it**, in the reading `ephor work
offers` returns: which of `roles`, `gate`, `needs_response`, or `sources` did
not hold and what the matter carried instead of what the selector asked for,
or which field its branch template needed and this matter did not carry.
"Considered" is narrower than every recipe the project has. A recipe whose
`kinds` refused was never about a matter of this shape at all — a `pr`
recipe has nothing to say about a task — so it names nothing, the same as a
recipe nobody wrote; `kinds` is what decides whether a recipe is considered,
not a reason reported once it is. And `behind` or `behind_upstream` refusing
alone names nothing either: whether a branch trails is a fact about a
checkout on this machine, already its own concept on the menu — the rebase
entries, and the `needs_checkout` gate — and every item with no local
checkout would otherwise report the same "could not be measured" line,
which is noise on nearly every reading rather than the one thing worth
saying. Both forms of the reading carry what is named, in the same words —
the JSON gains it as an additive field beside `offers`, the prose names it
under the same "nothing matches this matter" a reader already looks under —
because neither may know something the other does not
([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)).

The role-less case is why this exists. A project's own tasks
([§FS-003-feed-categories.1](FS-003-feed-categories.md#1-the-categories)) carry no role
at all — there is no forge reviewer to be one — and a `roles` selector, being
non-empty by definition once it is written, matches a role-less item only
when it is empty
([§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)).
That rule does not change here: a `roles: [author]` recipe still refuses
every task, and correctly. What changes is that the refusal stops being
silent — a recipe that plainly covered issues and pull requests no longer
looks, without explanation, like it covers nothing about a project's own
tasks. This is a reading only: dispatch itself, and what it hands over, are
unaffected — the exclusion is `ephor work offers`' own diagnosis of one
matter, not a second thing the selector decides.

## 28. A workflow entry can ask for the same thing a recipe can

[§24](#24-work-nobody-has-to-start-starts-itself) removed the key from the
front of a recipe's work and left it in front of every workflow's, which is
where the unattended loop actually stops. A workflow is what fixes a matter
end to end — implement, review, ship — and it is exactly the shape of work
nobody should have to be present for. Yet it took two deliberate acts per
matter: laying the plan down, and then starting the run. Both are the
formality [§24](#24-work-nobody-has-to-start-starts-itself) already refuses to
charge a reader for.

**So a workflow entry may say `autorun`, and it means what it means on a
recipe.** The entry is the one
[§19](#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)
already describes, in any of its three homes, and this is one more thing it
says beside its `when` and its `inputs`. Nothing else may say it: an entry
that runs a command here has no work to start, and an entry that asks for a
ticket says it inside the recipe it already is — two spellings of one fact
would drift, so the second is refused where it is written.

**The sweep lays it, where nothing else would.** `ephor work dispatch`
already walks the matters that deserve work and hands each one to the first
recipe that applies. A matter no recipe applies to and that has no work at
all is where the entry gets its turn: the first workflow entry that both
matches and asked to run itself is laid down about that matter, through the
one path [§19](#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)
already writes plans through. Recipes keep their priority — a matter a recipe
covers is a ticket, exactly as before — and a matter that already has work is
left alone, `--again` included: a second plan about one matter is something
`ephor work lay` is asked for, never something a sweep decides.

**It counts, reports, and refuses like a dispatch.** Laying is one of the
things `--limit` bounds
([§26](#26-an-ordering-already-made-can-be-read-and-a-limit-bounds-what-runs)),
taken in the ranking's own order like everything else that sweep does. Under
`--dry-run` everything is resolved and nothing is written — not the plan, not
the record of it, and not the files a real laying would put beside it. An
entry that cannot be laid down — a required input nobody answered, a hand a
narrowing refuses — is reported as a refusal with nothing written, the way a
dispatch that could not open a ticket already is, and the sweep goes on to
the next matter. Both forms of the reading carry it in the same words
([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)).

**And what it laid is due like anything else.** A plan a workflow wrote is
work in a work root, so
[§24](#24-work-nobody-has-to-start-starts-itself)'s sweep is what starts it:
its root is **due** when that plan holds a task that is open, unclaimed and
not parked on a question, and no run is live on the root. Two things are the
plan's own rather than the root's, and both are read where the plan is. Its
tasks are wherever the runtime wrote them — a plan rendered as a directory
keeps them in files beside its index, and those are as much the plan's tasks
as one written inside it. And they run under **the machine in force for that
plan**, which is the machine beside it where it declares one, because a task's
state means whatever the machine in force for its own store says it means
([§FS-006-project-interface.7](FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).
Where such a plan declares no machine of its own, **the root's own answers**:
that is the machine the runtime resolves it against — a plan that names a
machine names the project's — and reading it under a default nobody chose
would call its finished work unfinished. A machine that is there and will not
read is neither: nothing in that plan is judged at all, rather than judged by
a machine that answers for other work
([§15](#15-every-operation-is-visible-in-one-place)).
A root's own machine answers for the plans the root holds directly, as it
always did.

**What asked for it is what the ledger says asked for it.** A recipe is a
fact about a ticket; the entry is the fact about a laid plan, and ephor's
record of the laying is where it is read from
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)) —
not from the plan, which is the runtime's and says nothing about who asked.
A plan nothing in the record laid — one a reader laid by hand, one that was
simply found in the root — asked for nothing and is nobody's to start, which
is [§24](#24-work-nobody-has-to-start-starts-itself)'s silence again. That
holds however its tasks are named: the ticket id that says which recipe wrote
it ([§24](#24-work-nobody-has-to-start-starts-itself)) is a fact about the
tickets ephor itself wrote into the root's own plan, and a store of its own
names its tasks in the runtime's language, where the same spelling means
nothing of the kind.

**Everything else about the sweep is untouched.** The aggregate and project
ceilings, the cross-process reservation, the ranking, the failed-start
back-off, the one-run-per-root rule and the refusal to run in a working tree
standing on another branch all apply to a root a workflow laid exactly as
they apply to one a recipe wrote: it is the same root, reached the same way.
An entry that says nothing about this is a menu entry and only a menu entry,
laid by the reader and started by the reader, as it was.

## 29. Headroom is reported to ephor, and vetoes a member it never reorders

A list of alternates is only worth writing if something can say that the first
of them cannot be had. That something is a **quota**, and a quota is the
provider's fact rather than ephor's: ephor observes and summons, and it never
governs ([§REQ-001-boundary](../requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-lacks-crosses-a-seam-and-the-seam-has-one-anatomy)). So it consumes a *report* about capacity and
never derives one, and the report is evidence over a list somebody else
ordered — never an ordering of its own ([§DA-009-headroom-vetoes](../decisions/architectural/DA-009-headroom-vetoes.md#da-009-headroom-vetoes-headroom-is-a-report-ephor-is-given-and-it-vetoes-a-member-rather-than-ranking-the-list)).

**The unit is a pool, and a pool is who serves the model.** A hand's pool is
its provider where the roster gives it one, and its agent id where it does
not. That is the smallest thing a window is actually bought against: two hands
served by one provider spend one allowance whichever of them is asked, so
evidence about either is evidence about both, while a hand whose profile names
no provider has nothing smaller than the agent that carries it to be limited
by. Keying evidence any finer would make a refusal that has already happened
look like it happened to somebody else.

**Two channels report it, and one of them costs nothing.** This is a seam, so
it has the four parts [§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy) requires of every seam, and the
first channel is the shipped default that keeps it working unbound.

The first is **the ledger**, which needs no configuration because ephor
already writes it. The one authoritative thing a provider says about its own
window is a refusal, and a refusal names the instant it lifts; it arrives as a
start that failed, which ephor records in its own words already
(§4). Where those words carry an instant ephor can read, that start is a
**refusal on that hand's pool**, held until the instant and cleared by any
observed success on the pool. Where they do not, nothing about a pool is
claimed and the failure stays exactly what it was — one root's own back-off
(§24) — because a failure ephor cannot date is not a window it may guess at.
The record is site data, kept where ephor's other state is and never written
into a project ([§REQ-001-boundary.4](../requirements/REQ-001-boundary.md#4-the-footprint-rule)); it stays ephor's record of ephor's own
act and never becomes the truth about the work (§4). A count of what ephor
spawned may be shown beside it and may never be read into the rule: counting
one's own spawns is deriving a quota under another name, and it is wrong the
first moment anything else — a session at a terminal, another machine — spends
from the same credential.

The second is **a bound verb**, richer and optional. `work.headroom` binds one
command per pool at the site ([§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)), summoned exactly as every
other command this interface names — environment in, exit code out, structure
written to the file `$EPHOR_ANSWER` names ([§FS-006-project-interface.3](FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out)) — and
its payload rides `data`, the envelope's own free passthrough
([§FS-006-project-interface.4](FS-006-project-interface.md#4-the-answer-envelope)). Not standard output: stdout is the command's
own and a contract that parsed it would make an honest log a protocol
violation ([§FS-006-project-interface.3](FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out)). The payload is a list of **windows**,
each with a name, how much of it is left as a fraction, and when it resets.
Probing is fetching, so it runs where fetching runs — beneath the reading, on
the same freshness discipline as every other source ([§FS-001-forge-interface.7](FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it))
— and never in front of a dispatch, which would put a network call between a
reader and a ticket. How any one vendor is asked is volatile and stays outside
ephor: what ships is a worked example per vendor beside it, never a literal
inside it ([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).

**A number nobody reported is unknown, and unknown is never zero.** A window
with no `remaining` is unknown; a pool with no readable window at all is
unknown. This is the load-bearing rule, and it is load-bearing because absent
is the *ordinary* case: the credentials that reach these providers show their
usage to a person and publish no number a program may ask for, so a rule that
read silence as exhaustion would veto every pool on the machine and stop the
loop it exists to keep moving. ephor already keeps this distinction where the
same mistake was available — an implementation with no notion of assignment
has nothing counted as unclaimed, because nobody's claim is not a claim that
nobody has it ([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)) — and the rule below makes it
structural rather than advisory. The rule is a veto over what is *known
spent*, never a ranking over what is known remaining, so there is no place in
it for an unknown to be sorted against anything: no number cannot demote a
member, because demotion is not something the rule can do at all.

**A pool's effective remaining is the least of its known windows.** A window
that is spent is spent whatever the others say — the shortest leash is the
leash — and an unknown window is simply not among them, so one unreadable
window never lowers a pool that another window has reported healthy.

**The rule: known spent is passed over, and everything else keeps its place.**
A member is passed over exactly when its pool is known spent — an unexpired
refusal in the ledger, or an effective remaining at or under a floor, which is
`0` unless the site names another. Every surviving member keeps the order its
author wrote (§14). Where *every* member is vetoed the first still gets the
ticket, carrying a note that names the earliest instant any of their pools
resets: a ticket that is written and waits is work a person can see, start by
hand, and reason about, and work that silently never dispatched is none of
those things: who does the work was never what makes a ticket (§4).

**Absence degrades to unknown, out loud.** A pool with no verb bound, a verb
that exits non-zero, output that will not parse, and an answer holding no
window ephor can read all read as unknown, with the reason shown beside the
pool. None of them is an error that stops a dispatch, and none of them is
silent: this is the degrade rule [§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy) requires, and both halves
of it are the requirement. A seam whose absence failed the dispatch would make
an optional verb mandatory; one whose absence said nothing would leave a
reader looking at a choice they cannot explain.

**The choice is recorded where the work is.** Selection runs at every write
ephor makes — a dispatch and a laying — and the member it chose, with whatever
the choosing had to say, is written onto the ticket in ephor's own words,
beside the dossier it already writes there (§2) rather than as a field in the
runtime's plan language, which is the runtime's ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)).
So the plan itself says who and why, and a reader who was not there can read
both. Mid-plan exhaustion needs nobody steering: the spawn fails carrying the
instant, the sync that reads the failure writes the ledger, and the next write
ephor makes reads it and answers afresh. A pin already on a step is **never
silently replayed** onto it — a hand changing under a reader who is still
reading the plan is the one thing worse than a hand that waited — so where a
pin is answered again it is a **recorded plan edit** a reader can see.

**What is reported grows by addition.** `status` and `capabilities` gain a
line per pool — the pool, its effective remaining or *unknown* with the reason
it is unknown, and any unexpired refusal with the instant it lifts — and
`capabilities --json` gains the matching fields. That is the JSON form that
grows: `status --json` prints the matters a source reported, somebody else's
document rather than ephor's, with nowhere site-wide a pool belongs
([§FS-011-command-line.7](FS-011-command-line.md#7---json-is-the-same-answer-not-a-second-one)). Nothing already printed changes, which is what the
interface's own versioning asks of any growth ([§FS-006-project-interface.11](FS-006-project-interface.md#11-the-interface-is-versioned)).
