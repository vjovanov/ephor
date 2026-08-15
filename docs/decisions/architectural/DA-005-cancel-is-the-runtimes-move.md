# DA-005-cancel-is-the-runtimes-move: cancelling a ticket asks the runtime, and never rewrites the state line

**Status:** Accepted
**Date:** 2026-08-15

A ticket that should not go on is cancelled by moving it into the machine's
abandonment state (§FS-005-dispatch.16). Two hands could make that move:
ephor's own — the ticket is a heading and a `**State:**` line ephor wrote,
in a file ephor already appends to and whose dossier it already rewrites —
or the runtime's, through its own transition verb, run as a captured
summons. This record fixes the second and names what the first would have
cost.

## 1. The decision

Cancelling is a transition, and the transition is asked of the bound
runner in its own words: the runtime module (§AR-007-runtime.1) composes
the runner's transition command — the plan, the ticket, the state it is
expected to be in, the abandonment state, and the reader's reason as the
ticket's result — and runs it captured, from the work root, with a short
timeout (§AR-002-summons). What the runner answers is what the reader is
told: the ticket cancelled, or the runner's own refusal, its first lines
lifted from the captured output because that is where the runner puts them.
The name of the abandonment state is the plan language's, spelled once in
the runtime module beside the plan flag and the agent flags, and above the
module a surface asks only whether the machine in force declares one.

Before the runner is asked, ephor refuses on what it can see for itself
(§FS-005-dispatch.16): a ticket a live run holds, read from the lock and
the journal exactly as the board reads them (§FS-005-dispatch.15); a
ticket already in a final state; a machine that declares no abandonment
state at all. Each is one sentence, and none touches the plan.

## 2. The rejected alternative

Rewriting the ticket's `**State:**` line in place, under a lock on the plan
file, with the reason appended to the ticket's body. It is less code and it
works with no runner installed — which is the whole of its appeal, and it
was tempting because ephor's every other write is by its own hand in the
plan language. It fails on the plan language's own terms: after a ticket is
authored, its state is the runtime's to move, and a state edited by hand
skips the compare-and-swap that keeps two movers from crossing, the
artifact checks, the callbacks a machine may hang on leaving a state, the
counted-visit metadata, and the result and audit trail a terminal move
writes. A plan that has been cancelled by hand looks finished to every
reader and is vouched for by none — a watch that has quietly become the
thing it reports on (§FS-005-dispatch.4). And the concurrency the lock
would buy is only half of it: the runtime persists by writing beside the
plan and renaming over it, so a lock taken on the file a moment before the
rename guards the wrong inode.

## 3. The cost

Cancelling needs the runner on `PATH`, where every other write in dispatch
does not — accepted, because a cancel is a move on work the runtime owns
and there is nobody else to make it; with no runner bound the refusal is
the workable rung's sentence, and the plan stays hand-editable for a
reader who wants to make the move themselves. And the move is only as
capable as the runner's verb: what the runner refuses — a state it will
not leave, a result it insists on — ephor reports and does not work
around. The shipped runtime enforces a state's declared outputs on every
transition out of it, the wildcard edge into `cancelled` included, so today
a ticket parked in a state whose artifacts were never written cannot be
cancelled through the verb until the runtime treats abandonment as the
failure routes it already exempts; that is the runtime's to fix, and until
it is fixed ephor says the runner's sentence rather than editing around it.
