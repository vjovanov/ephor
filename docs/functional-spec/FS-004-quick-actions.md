# FS-004-quick-actions: a problem ephor recognizes arrives with the action for it

The action menu starts empty: every entry in it is something the reader wrote
down first. Yet what a person wants on a problem is nearly always the same
thing — the failing log, the diff, the conversation — and making them
configure that is ephor knowing what is wrong and sending them to fetch it
anyway. The reader would have to leave, find the repository, remember the
tool's flags, and come back with an answer ephor could have handed them.

So ephor offers those itself. A **quick action** is a menu entry ephor has
without being told, on an item where it already knows what the problem is —
the most frequent response made the cheapest one ([§GOAL-001-fewest-moves](../goals.md#goal-001-fewest-moves-the-most-frequent-response-is-the-cheapest-one)).

## 1. A quick action belongs to the source that found the problem

The source that produced an item is the one place that knows which forge it
came from and which tool reaches it — and naming a forge or a vendor CLI
anywhere above that is what
[§FS-001-forge-interface](FS-001-forge-interface.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
forbids. A quick action is therefore offered by the source; ephor's core only
merges it into the menu and runs it, exactly as it runs a configured action —
the same checkout resolution, the same `EPHOR_*` environment, the same
handover of the terminal while it runs, one crossing in the seam's materials
([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)). A quick action is an ordinary menu
entry that nobody had to write, and a source that offers none is complete.

## 2. Offered only where it would work

A quick action appears only when running it would do something: the item has
the problem the action addresses, the identifiers the command needs are
known, and the tool it runs is installed. A menu that lists an action which
cannot work is worse than one that lists nothing, because the reader believes
it and spends a keystroke and a screen of errors finding out.

A key a screen advertises is under the same rule, and it is measured against
what the key would act on rather than against the screen. Message keys are
offered on the message the reader has selected: the key to react appears where
the forge would accept a reaction, the key to tick a task where there is an
unresolved task to tick. A footer that offers the same keys everywhere teaches
the reader a key that does nothing on most of what they select, and the answer
they get for pressing it — a refusal in one line at the bottom of a full
screen — is the one place they were not looking.

## 3. Quick actions come first, and configuration adds to them

They are listed above the configured actions, so that the obvious thing is
the first key. Configuration never replaces them: a reader whose own action
does the same job gets both, because ephor cannot tell that two commands mean
the same thing and silently dropping either one is the failure that matters.

## 4. Failing CI answers what failed and why

The quick action on a pull request whose gate is red shows what failed: the
check list as the forge reports it — which failed, which passed, which are
still running — and then the failures themselves, each with its log, paged.
That is the whole question a red gate asks, and reaching it by hand is several
commands and a browser tab.

The condition is the red gate, not the source that reported it. Every item
carrying a failing gate is offered the action, whichever source produced it,
and a source that cannot say what failed offers nothing. Hanging the action off
one kind of item instead leaves the reader looking at a number with nothing
behind it on every forge that reports its gate on the pull request itself —
which is most of them, since a gate is a property of the change, not a separate
piece of work.

Where ephor renders the failures itself rather than handing over a log,
identical ones are reported once, with the number of jobs that hit them. A gate
fans one error across every job that compiled the same file, and six copies of
one compile error is a worse answer than one copy that says six.

## 5. A task is ticked where it is read

A task renders as what it is — a box, ticked or not — beside the message that
carries it, and the reader ticks it from there. The whole of that interaction
is reading the sentence and agreeing with it, so sending the reader to a
browser to click the same box is the trip this section exists to save.

Ticking goes back through the source that reported the task, like every other
write ([§1](#1-a-quick-action-belongs-to-the-source-that-found-the-problem)) —
ephor knows a task has a state and a way to transition it, and nothing about
how that forge spells either. A forge that reports task state without offering
to write it renders its boxes and offers no key, which is
[§2](#2-offered-only-where-it-would-work) and not a degraded mode.

A ticked box is an answer ([§FS-003-feed-categories.4](FS-003-feed-categories.md#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)),
so the thread stops awaiting the reader as soon as the forge accepts the
transition, without waiting for the next refresh to say so.

## 6. A branch that trails its main branch is offered the rebase

ephor already measures how far every checked-out branch has fallen behind its
project's main branch, and says so on the branch row — `3 behind`. Then it
leaves the reader to go elsewhere and do something about it. Knowing what is
wrong and not offering the move is the whole of what this section exists to
stop, and here ephor is not even relying on a forge to tell it: the fact is on
disk, in the reader's own checkout.

The fact on disk is only as fresh as the last fetch. Nothing under the watch
fetches — reading a row costs a handful of local reads and runs nothing — so
every distance stated here is measured against the copy of the base this
machine last pulled down, and that copy can be weeks old. Two things follow,
and together they are the shape of the offer.

So **a checkout that trails its project's main branch is offered the rebase
onto that branch — and so is one that measured level**. The reading that says
*level* is exactly the reading the replay would refresh: replaying onto main
begins by fetching ([§FS-005-dispatch.12](FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)),
so a branch that went stale without this machine hearing about it is the branch
the offer is needed on most, and withholding the entry there hides the one move
that would correct the reading. A branch genuinely level replays onto nothing
at no cost, and is told so in the register a current repository is always told
it in ([§2](#2-offered-only-where-it-would-work)) — a cheaper thing to be wrong
about than a branch nineteen days behind being shown as current. What the offer
needs is a base to name, not a distance to it.

Which base this is about has to be said now that there are two: this one
replays onto the branch the project declares as its main, and it is offered
only where the project declares one — an entry has to name what it is about to
replay onto, and where nothing names a main branch there is no answer to put in
it. The other rebase
([§8](#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it))
resolves its ref inside each repository and so needs no base named anywhere:
the two are gated apart, and a project that declares no main branch is still
offered the replay onto its own published copy.

What carries the offer is a branch on disk, not the kind of row that mentions
it. **Any item that resolves to a branch workspace** is offered it — a pull request, an issue, a status a source filed about the same
change — because the fact being acted on belongs to the checkout, and the item
is only how the reader arrived at it. Restricting it to pull requests said that
a branch is stale only when a forge has an opinion about it, which is the
inverse of this section's whole argument: the fact is on disk.

**And the branch rows carry it themselves**, with no item behind them at all.
The row saying `13 behind` is where a reader looking at a stale branch is
actually standing, and an offer reachable only by first finding something a
source filed about that branch is an offer most readers never reach — including
on every branch nothing has been filed about yet.

**And every statement of the distance says how fresh it is.** The row and the
entry alike read `13 behind as of Jul 28` — the count, and the day the local
copy of the base last moved — because `13 behind` on its own is a claim about
now that nothing here is in a position to make. A branch that measured level
says the same thing the same way: `level as of Jul 28`. *Up to date* is retired
wherever it stood, on rows and in entries both: it announced that the branch was
current when all that was measured was that it matched a copy fetched at some
unstated time, and it was the wording that made a nineteen-day-old reading look
like news.

The day is the base's own: the last time the local copy of the base moved on
this disk, which is the last fetch that brought anything down for it — read
from that copy's ref, never from a record of fetches having been attempted. So
it never over-claims. A fetch that found nothing new leaves the copy, and the
day, where they were; a fetch that failed to connect leaves both untouched
rather than stamping today onto a comparison it never refreshed. It is measured
per repository, so a checkout of several reports the oldest of them — a
comparison is only as fresh as its stalest half. And where a measured
repository has no such day at all, which is what a fresh clone that has never
fetched looks like, there is no day to report and the qualifier is left off
entirely: the row reads `13 behind`, and nothing is invented to fill the gap.

Offered only where it would work ([§2](#2-offered-only-where-it-would-work)):
something that resolves to no branch has nowhere to rebase, and a workspace
that is not there is a checkout question
([§7](#7-a-workspace-that-is-not-there-is-offered-the-checkout)) rather than a
rebase one. A branch measured level is no longer one of these refusals, but a
branch nothing could be measured on — no base named, or no ref to compare with
— still is: the entry would have nothing to name. Where a branch cannot be
resolved to a checkout on disk the offer is withheld rather than made and left
to fail on the keystroke.

It is git and nothing else. Fetch, replay the branch on the base, say what
moved — no forge, no vendor CLI, and no knowledge of what the project is built
with. A poly-repo workspace is several repositories sharing one branch name, so
every repository under the checkout is rebased and the answer is given per
repository, one already current being reported as current rather than silently
skipped.

Two things it will not do. It does not stash: a rebase that quietly pockets
uncommitted work and replays it is a good trick right up until it conflicts,
and the reader is then holding a conflict in a change they had not finished
writing — a repository with uncommitted work is reported and left alone. And it
does not decide. A rebase that stops in a conflict has arrived at a question
about the code, which is
[§FS-005-dispatch.12](FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model).

## 7. A workspace that is not there is offered the checkout

A branch ephor watches, on a project whose checkouts are one per branch, is
either on disk or it is not — and ephor is the thing that knows which, because
it computes the directory from the project's own template and looks. Where it
is not there, everything else stops: every action that needs a checkout is
refused, and no work can be dispatched at all, since a ticket about a change
has to run in the change.

Sending the reader to a configuration file for the command that fixes that is
the same mistake as sending them for the failing log. It is one operation, the
same on every project ephor watches, and ephor is already holding every input
it takes: which repositories the project has, where each goes under the
checkout, which branch, and what that branch is grown from. So **a missing
workspace is offered the checkout, and ephor supplies the command**. A project
that wants its own — a bare mirror, a filesystem snapshot, a `gh pr checkout` —
configures one and that wins ([§3](#3-quick-actions-come-first-and-configuration-adds-to-them)),
but nothing has to be configured for the offer to exist.

What it does is git and nothing else, and it has the rebase's shape. A
poly-repo workspace is several repositories sharing one branch name, so each
gets a working tree under the new directory: the branch itself where the forge
has it, a new branch of that name grown from the main branch where it does
not, and — where the repository already has the branch but the forge does
not — the branch as it stands, checked out and reported as published nowhere,
naming what its tracking configuration records, if anything (a tracking
configuration naming the base is where the branch was cut, not where it is
published, [§DA-003-upstream-is-the-published-copy](../decisions/architectural/DA-003-upstream-is-the-published-copy.md#da-003-upstream-is-the-published-copy-a-branchs-upstream-is-its-published-copy-not-its-tracking-config)). That third case changes no
tracking configuration and pushes nothing: the reader is told the fact rather
than handed a claim. The answer is per repository, and one that was already
there is reported as already there rather than silently skipped.

Two things it will not do. It will not move a branch another working tree is
holding — git refuses that, and it is right to; the repository is reported and
left alone rather than worked around. And it will not decide where to put the
workspace: the directory is the project's template applied to the branch, the
same one every other part of ephor resolves, because a checkout that landed
somewhere else would be a checkout nothing else could find.

Like the rebase, it is one implementation for every caller
([§FS-005-dispatch.12](FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)):
the key the reader presses, the command a state machine runs, and the dispatch
that makes the workspace a `branch` template named
([§FS-005-dispatch.25](FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs))
are the same operation, since two of them would eventually disagree about what
a checked-out workspace is.

### 7.1 A workspace that is there is still owed its store

A directory that is there stops the offer: ephor reports the workspace as
already checked out and does nothing more. But *already checked out* answers
the question about repositories, not the one about work. A workspace made
before ephor made stores at all, or made by a project's own checkout command
([§3](#3-quick-actions-come-first-and-configuration-adds-to-them)), holds every
repository it should and still has nowhere for a plan to land — and the reader
who asks for the checkout again is asking exactly the question that would fix
it.

So the checkout makes whatever is missing and stops only where nothing is: the
repositories where those are absent, and the store
([§FS-006-project-interface.7](FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live))
in either case, since a store is part of what makes a directory a workspace
rather than a pile of repositories. Asking twice is how a half-made workspace
is repaired, not a no-op the reader has to work around.

### 7.2 The offer is a key on the row that says the workspace is missing

The row already says *not checked out* — the matter's own row, the branch's
row, and the row of a matter nothing could place, which is where a matter whose
branch has no workspace usually is, since a branch nothing checked out is a
branch the reader has no row for. Where a fact is shown is where the move
about it belongs
([§6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)), so the
checkout is a key on that row and not only an entry in a menu opened over it.
One key, on every row that carries a branch, running the one operation above.

The menu keeps its entry — the key is a second way to the same move, not a
replacement for the first. But a menu is where a reader goes to choose among
things, and there is nothing to choose among here: a workspace that is not
there has exactly one thing that can be done about it, and the row already
says so.

## 8. A branch that trails its own published copy is offered the rebase onto it

Main moving under a branch is one thing that happens to it. The branch moving
under the reader is another: a teammate pushes to it, a second machine of their
own does, the forge writes something onto it — and the checkout on this disk is
behind the copy everybody else can see. ephor measures that distance too, per
repository, and shows it on the branch row beside the first one. Then, again, it
leaves the reader to go elsewhere and do something about it.

So **a branch whose published copy carries commits its checkout does not is
offered the rebase onto that copy**, beside the rebase onto main
([§6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)). Two
facts, two entries, two operations: one replays what the reader has onto where
the project went, the other onto where their own branch already is.

It follows [§6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)
in both of that section's consequences, for the same reason: this distance is
measured against what was last fetched too. So the entry is offered wherever
there is a copy to replay onto, whether the checkout measured behind it or
level, because the replay is what would refresh the reading — and it says how
fresh the reading is, `2 behind as of Jul 28` or `level as of Jul 28`, from the
copy's own ref rather than the base's. The two rarely last moved on the same
day: main moves when the project lands something, a branch's copy when somebody
pushes to it, and a fetch only dates the refs it actually brought down.

A branch's published copy is what was last pushed of it, read per repository
from that repository's own `HEAD` rather than from the name of the directory the
workspace sits in — a repository need not be on the branch its workspace is
named for. Where git records where the branch is published, that is the copy;
where it does not, the remote's branch of the same name is; and tracking
configuration naming the repository's base records where the branch was *cut*,
not where it is published, so it publishes nothing
([§DA-003-upstream-is-the-published-copy](../decisions/architectural/DA-003-upstream-is-the-published-copy.md#da-003-upstream-is-the-published-copy-a-branchs-upstream-is-its-published-copy-not-its-tracking-config)) — and a base nobody could resolve
cannot clear the record of naming it, so there too only a pushed copy of the
branch's own name counts. The last rule is what keeps the two
entries from being one entry twice, and the middle one is what makes the offer
worth having at all: a branch that was pushed and has no tracking configuration
is exactly what `git worktree add -b` leaves behind, and in such a checkout bare
`git rebase` refuses to run.

Offered only where it would do something
([§2](#2-offered-only-where-it-would-work)), which here is four refusals. An
item linked to no branch has nowhere to rebase, and a workspace that is not on
disk is a checkout question
([§7](#7-a-workspace-that-is-not-there-is-offered-the-checkout)). A branch
never pushed has no copy at all — nothing to name in the entry, nothing to
replay onto, and no reading a fetch would correct — and *nothing published* is
an answer, given in the same register as a repository already current and never
as a failure: the reader is told what was found, not what went wrong. And a
repository whose published copy **is** the base carries nothing of its own here
— a branch parked on the main branch and tracking it
has one distance wearing two names, and two menu entries counting one distance
is the duplication the resolution above exists to prevent — so its distance
belongs to the rebase onto main alone, and a checkout of nothing but such
repositories is offered only that. The offer stands where some repository
actually trails a copy that is not its base — one on a change's branch while
another sits parked on the base — counting those repositories alone, and the
answer says what happened to each, because a forest is not one branch.

That per-repository answer is the difference this makes to the fold. The rebase
onto main is one branch name for the whole checkout; the rebase onto the
published copy is a different ref in every repository, resolved from each
repository's own `HEAD`, and a repository that has published nothing is reported
as such while the rest replay. Everything else is the rebase onto main's and
unchanged: git and nothing else, an answer per repository, uncommitted work
reported and left alone, a conflict handed over rather than decided.

One property has to be said plainly, because it is why this is not simply
*pull*. Replaying a branch onto its own published copy rewrites commits that are
already published, so that copy can no longer be fast-forwarded and landing the
result means a force push under a lease. The rebase onto main has exactly the
same property, and the same answer: the replay itself never pushes, and the push
is a decision belonging to whoever makes it.

## 9. A gate is offered the restart, in two shapes

Reading what failed ([§4](#4-failing-ci-answers-what-failed-and-why)) answers
the question a red gate asks. It does not answer the one a reader most often
has about it, which is *was that even me*. A runner died, a mirror was
unreachable, a dependency shipped something broken, the same flake landed on
the same job for the third day: nothing about the change is wrong, and what
the change needs is another run. Knowing that and sending the reader to a
browser tab to click it is the same failure this section exists to stop, and
it is worse here than elsewhere, because the browser tab is where a restart is
usually one click away and where finding *which* thing to click takes five
minutes.

So a gate is offered the restart. It is two entries, not one, and the
difference between them is the reader's own diagnosis:

**Restart what failed.** One job died on infrastructure while everything
around it passed, and what that needs is the work back rather than a whole
gate re-run — an hour of a shared machine pool to answer a question that was
about one build. This is the ordinary case and the cheaper one.

How much cheaper is the forge's to say, not this section's. What is asked for
is *what is not green, and everything downstream of it*
([§FS-006-project-interface.6](FS-006-project-interface.md#6-the-gate-is-the-projects-in-three-verbs)): on a forge that re-runs job by job that is the
failed jobs and nothing else, and on one that starts a gate as a whole it is
the failing part of the tree and what hangs off it, which can be most of the
tree. **The entry says which it is**, because a row promising one job over a
hundred rebuilt ones is the label lying about the cost — and a reader who
cannot tell the two apart cannot choose between the two entries at all, which
is the whole point of there being two.

**Restart everything.** The merge commit itself is suspect: the base moved
under the change, a cache was poisoned, the green results are as untrustworthy
as the red ones. Then re-running only what failed proves nothing, because the
part that would have caught it is the part that is being kept. This is the
expensive one, and it is a separate keystroke precisely so it is a decision
rather than a default.

**Where they are offered follows from what each of them means.** A red gate
gets both. A gate that is not red — green, still running, blocked on an
approval — gets *restart everything*, which is exactly the entry that still
has something to do there, and not *restart what failed*, which would be a key
that runs and reports that there was nothing to restart
([§2](#2-offered-only-where-it-would-work)). An item carrying no gate at all
gets neither: there is nothing to restart, and the fact is the item's, not the
project's.

**Restarting is the gate's verb, not ephor's idea of one.** Which command
re-runs a gate is project truth exactly as reading it is
([§FS-006-project-interface.6](FS-006-project-interface.md#6-the-gate-is-the-projects-in-three-verbs)), and the scope — everything, or only what is not
green — crosses that seam as part of the ask. A forge that hosts its own gate
answers it as the shipped default and needs no manifest, so restarting works
on the common case with nothing configured
([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)). Where the forge cannot re-run
something — an external status somebody else's system wrote — the entry says
so rather than reporting a restart that never happened.

**It runs beneath the screen** ([§FS-005-dispatch.17](FS-005-dispatch.md#17-a-move-that-needs-nobody-runs-beneath-the-screen)). A restart asks nothing,
decides nothing, and is answered by the gate minutes later; taking the
interface for it would be paying the screen for a command that never needed
it, and the log is where what it asked for and what came back are kept.

**It commits nothing, and it is the same move the loop makes**
([§FS-005-dispatch.11](FS-005-dispatch.md#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed)). A reader pressing this key and a state machine deciding
the failure was never the change's are asking for the same thing, so they ask
for it the same way — one verb, one scope, one answer — and cannot drift into
two ideas of what restarting means.

