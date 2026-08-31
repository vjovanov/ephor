# Goals

## GOAL-001-fewest-moves: the most frequent response is the cheapest one

Resolving a matter usually takes one of a small set of moves: read it, answer
it, react to it, tick its box, mark it done, rebase the branch, restart the
gate, hand the fix over. The goal is that each of those is a couple of
keystrokes from the row it applies to, without leaving the screen and without
retyping context the tool already holds — and that the move a matter most
likely needs is offered first, because a problem ephor recognizes arrives
with the action for it ([§GRUND-001-overseer.2](grund.md#2-what-this-project-does-about-it)). Success is observable at the
fingertips: for the most frequent responses, the path from feed row to
resolved is short enough that reaching for another tool would be slower.

## GOAL-002-glance: one glance answers "what needs me now"

Matters are visualized for action, not for completeness: unread first,
grouped where the person acts — organization, project, branch — and every
row carries exactly the facts that pick its next move: whether it awaits a
response, the gate counts per repository, how far the branch trails, and why
the row resurfaced. Success is observable on the first screen: the answer to
"what needs me right now" is the top of the tree, a returning user reads the
delta rather than the world, and invoking an action never requires opening
the matter anywhere else first ([§GRUND-001-overseer.1](grund.md#1-the-problem)).

## GOAL-003-nothing-lost: the watch is trusted enough to retire the sweep

The hand-kept morning sweep ([§GRUND-001-overseer.1](grund.md#1-the-problem)) stops only when the tool
provably misses nothing. Every conversation lands on a row or in a visible
unattributed bucket — never silently dropped; done stays done until the
matter actually moves; what resurfaces names its reason; a failing source
degrades to stale last-good items instead of a blank. Success is observable
as a habit change: a person who stops sweeping forges and inboxes by hand
catches everything they would have caught, and knows it.

## GOAL-004-handover: routine moves leave the person's hands

A matter whose next move is mechanical — fix the red gate, answer from the
logs, rebase the trailing branch — is handed over rather than performed:
to the project's own script where an algorithm can finish it, to an agent
runtime with a complete dossier where judgment is cheap but typing is not
([§GRUND-001-overseer.2](grund.md#2-what-this-project-does-about-it)). What came of the handover returns to the row.
Success is observable in cost: dispatching a covered matter is as cheap as
marking it read, and the verdict arrives on the same screen the matter did.

## GOAL-005-costless: watching costs the watched nothing

Any project is trackable exactly as it stands — a registry row away, no file
added, no forge or runtime assumed beyond git itself — and a project that
chooses to speak (its checks, its gate, its offers) gets more, with
everything it says validated against a published schema
([§GRUND-001-overseer.3](grund.md#3-who-it-is-for)). Success is observable at both edges: tracking a new
project takes minutes and touches nothing in it, deleting ephor's traces
leaves a clean checkout, and ephor itself stays publishable — no employer,
site, or fused vendor named in what ships ([§GRUND-001-overseer.2](grund.md#2-what-this-project-does-about-it)).
