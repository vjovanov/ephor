# DA-008-a-run-follows-the-ticket: autorun is a sweep that starts a run, never a runner kept alive

**Status:** Accepted
**Date:** 2026-08-23

Work a recipe marks as needing nobody to start it has to get a run without a
reader pressing anything ([§FS-005-dispatch.24](../../functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)). Two shapes give that: keep a
runner alive per branch, waiting for work to appear in the root it watches;
or leave the runtime exactly as it is — started, finite, over when nothing
is schedulable — and make *starting* it reflexive. This record fixes the
second and names what the first would have cost.

## 1. The decision

Autorun is a **sweep**: `work run --due` asks which roots hold a ticket that
is open, unclaimed, not gating, and from a recipe that asked to run itself,
and where no run is live in the checkout that root's work runs in it starts
the detached run
[§FS-005-dispatch.20](../../functional-spec/FS-005-dispatch.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching) already describes. Nothing about the run changes — the
same launcher, the same identity beside the same lock, the same attach and
the same stop in the runner's own words. The only new thing is who says go.

The sweep is **idempotent by construction and stateless about its own past**.
It re-reads the world every time ([§FS-005-dispatch.15](../../functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)), and the live-run
check is the runtime's own lock, so running it twice in a second starts one
run, and running it on a root whose working tree is already busy starts none. That is what
lets every trigger be the same call: the dispatch that just wrote an
autorun ticket, the timer, and a reader typing it all invoke one verb, and
none of them has to know what the others did.

Triggers are therefore cheap and layered rather than exact. Dispatch runs the
sweep in the same breath as writing the ticket, because that is the moment
the work exists and the latency the reader would otherwise feel is the whole
point. The periodic sync runs it after reopening what moved, catching work
born anywhere else — a hand-written ticket, another ephor, a run that died.
Neither is load-bearing alone: the sweep's own correctness does not depend on
being called at the right moment, only on being called.

The one thing it must remember is **failure**, and it remembers it in the
ledger, which is ephor's record of what ephor did and never the truth about
the work ([§FS-005-dispatch.4](../../functional-spec/FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)). A root whose start failed is skipped for a
back-off that grows with consecutive failures, so a runner that refuses
cannot turn every sweep into a spawn. The record is about ephor's own act;
the work's state stays where it has always been read from.

## 2. The rejected alternative

One runner per branch, kept alive, reacting to work appearing in its root —
the shape the request began as, and it is the intuitive one: the thing that
runs work should be the thing that watches for work.

It fails on what it would have to become. The bound runner is finite by
design — it computes a ready set, works it, and exits when nothing is
schedulable — and its own design record rejects a daemon outright. So "one
per branch, always up" means either a runner ephor asks for a mode it does
not have, or a supervisor ephor writes and keeps alive per branch workspace:
a process per branch on every project, each holding its root's lock for its
whole lifetime rather than for the length of a run. That lock is the
liveness signal every surface here reads ([§FS-005-dispatch.15](../../functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)). A supervisor
holding it while idle would make *live* mean *supervised*, and the board —
which exists to say what is actually going on — would report a machine full
of runs that are doing nothing. Every reading downstream of the lock would
need a second question to recover the fact it used to get for free.

And it would put ephor in the business of process supervision at exactly the
altitude it has refused to: restarts, orphan reaping, one lifetime per branch
workspace against a set of workspaces that changes when someone makes a
checkout. Against that, the sweep costs a directory read and a lock probe per
candidate root, on triggers that already exist.

The **latency** the daemon would buy over the sweep is the difference between
reacting to a ticket appearing and being told about it. Because dispatch
itself calls the sweep, that difference only exists for work ephor did not
write — and for that case the periodic trigger is the same answer the rest of
this design gives about the world changing behind ephor's back.

## 3. The cost

Work born outside a dispatch waits for a trigger. A ticket a person appends
by hand to an autorun recipe's plan starts on the next sweep, not on the
keystroke that saved the file — the sweep is not a file watcher, and the
timer's period is the bound. That is accepted: the case is rare, the reader
who wrote the ticket can start it themselves, and a watcher over every work
root is the walk [§FS-005-dispatch.15.1](../../functional-spec/FS-005-dispatch.md#151-the-board-keeps-itself-current) rules out on every tick.

A run started by a sweep has nobody watching its first moments. Where the
launcher itself fails this is reported ([§FS-005-dispatch.24](../../functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)), but a run that
starts and then goes wrong is seen when the reader next looks, exactly as a
detached run somebody started by hand is. Nothing regressed here — a
detached run never had a watcher — but autorun makes it the common case
rather than the exception.

The back-off is ephor remembering something, and every remembered thing can
be wrong about the world: a root that failed for a reason since fixed waits
out its interval before being tried again. The interval is bounded and a
reader can always start the run themselves, which is the escape hatch that
keeps the memory from being load-bearing.
