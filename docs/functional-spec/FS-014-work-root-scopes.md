# FS-014-work-root-scopes: a plan lives in the smallest scope that can see everything it touches

Ephor addresses the world at three scopes — an organization, a project, a
checkout — and the runtime it hands work to addresses its memory at one. A work
root is where those two meet, and nothing has said which of the three scopes a
given plan belongs in. The mechanism has been configurable for a while: the root
is a template read at three tiers, and two of its names reach above the project
([§FS-005-dispatch.6.1](FS-005-dispatch.md#61-the-work-root-is-a-template-and-it-may-reach-above-the-project)). But a template that can point anywhere is not a rule for
where to point it, and without one the answer falls out of whichever tier
somebody happened to write rather than out of the work.

So there is a rule, and it is one sentence: **a plan goes in the smallest scope
that can see everything the work touches.** Everything below is that sentence's
consequences. It is a placement rule because placement is a handover question —
what is handed over goes to a runtime that will edit something, and where the
plan sits is the same fact as what the work it holds is allowed to reach
([§GOAL-004-handover](../goals.md#goal-004-handover-routine-moves-leave-the-persons-hands)).

## 1. Three scopes, one shape

A work root exists at each of the three scopes ephor already addresses:

- an **organization** root, which lives as long as the organization does, and
  holds work belonging to no single repository — a release that moves several
  projects' gates, a sweep across all of them;
- a **project** root, which lives as long as the project, and holds work about
  one repository that is not about one of its branches;
- a **checkout** root, which dies with its branch, and holds work being done in
  that working tree.

They differ in reach and in nothing else. Each is an ordinary flat runtime
project — the same plans, the same states, the same artifacts, read by the same
commands — because a scope is a fact about the work, not a second kind of
container. Whoever can work one can work all three, and a plan can move between
them without being rewritten.

Which organization a project belongs to and where that organization is rooted
are the registry's own facts, read when a root is rendered and never written
back ([§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)). This rule says which scope work belongs in; it does
not say where the three scopes live, because the registry already does.

## 2. Reach places, and nothing else does

The scope is read off what the work touches. A fix sees one tree, so it goes in
that checkout's root. A sweep that rebases every branch of one project sees that
project's checkouts, so it goes in the project's root. A release that moves
several projects' gates at once sees the organization, so it goes in the
organization's.

**Nothing else decides it.** Not who filed the matter, not how long the work
will take, not who or what will do it, not which directory the reader was
standing in when they asked. Each of those is a true fact about a piece of work
and none of them says what that work may reach, so each one, used as the
placement rule, puts plans in roots that do not describe them — which is how
project-scope work ends up parked in one checkout and org-scope work ends up
with nowhere to be written down at all.

**Smallest, not merely sufficient.** An organization root can see every tree
beneath it, so every plan those trees hold would be *legal* there. What the
smallest scope buys is that the placement is itself a claim a reader can check:
a plan in a checkout root says this work is confined to this tree, and a root
that held everything would say nothing. The rule is only worth having in its
strict form.

## 3. Upper scopes decide, the checkout scope executes

Work above a checkout surveys, decides and reports. Where it finds something
that needs one working tree changed, it does not reach into that tree: it hands
a ticket **down** into that tree's own work root, and a run there does the
editing.

The reason is not tidiness. A run is an agent editing a working tree, and the
invariant that keeps two of them out of one tree is held over the checkout
rather than over the root ([§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)). A sweep that edited each tree
it walked would be one agent in every tree at once, under no such guard, and the
guard cannot be extended to cover it — there is one process, holding nothing.
Handing down is what makes the same work safe: laying a ticket into a busy
checkout's root is writing a file, and the ticket waits in that plan until the
tree is free.

So an upper-scope plan's own work is reading and deciding, and what it produces
is a report plus, wherever a tree must change, a ticket in that tree's root. The
division is not a matter of taste about who is trusted with what. It is the only
arrangement in which the concurrency guard that already exists still means
something.

## 4. A handed-down ticket names where it came from, and the trail runs both ways

Placing by reach scatters one piece of work across several roots on purpose: the
sweep is in one, and each conflict it found is in another. A trail that ran one
way would leave half its readers stuck, because the two ends are found by
different people standing in different directories.

So both ends carry the other. **A handed-down ticket names the work that handed
it down** — the plan and the ticket it came from — so a reader who opens a
checkout root and finds work nobody in that tree asked for can see what asked
for it. **And the handing-down work's result names the tickets it spawned**, so
a reader of the sweep can see what came of it without walking every checkout to
find out.

## 5. Nothing durable lives in a checkout work root

A checkout root dies with its branch. That is not a defect to be worked around:
it is what makes a checkout root cheap to create and honest about its reach.

What follows is a rule about what happens next, not a rule that keeps work out
of a checkout root — work in the tree belongs there, by [§2](#2-reach-places-and-nothing-else-does). **Nothing is left in
a checkout root that anybody will want after the branch is gone.** What is worth
keeping is folded up into a longer-lived scope before the tree is reclaimed.

The rule that places and the rule that preserves are separate, and neither may
be spent to buy the other. A plan pushed up a scope so that it will survive is a
plan whose placement has stopped saying what it may touch; a tree kept alive so
that its record survives is a checkout that has stopped being a checkout.
Durability is the archiver's job.

## 6. What this is not

Three things this rule is mistaken for often enough to be worth refusing by
name.

- **The runtime does not learn to nest.** Its work root is flat by design, and
  three scopes are three flat projects rather than one tree of them. The roll-up
  is a reading, not a container: ephor is the aggregator, and asking the runtime
  to become one would buy nothing this rule needs.
- **An intake project is not a work root.** A project that collects findings and
  triages them has its own state machine and its own reason to exist, and may
  sit beside an organization root without being one. Folding the two together
  would put intake and work under one machine that suits neither.
- **No tree is kept alive in order to keep a record.** The answer to [§5](#5-nothing-durable-lives-in-a-checkout-work-root) is
  folding up before the tree is reclaimed, never declining to reclaim it.
  Checkouts nobody is working in, held open for their logs, are the cost this
  rule was supposed to remove.

## 7. What is not yet held

Three parts of the rule above are this program's to enforce and are not yet
enforced. Saying so costs a paragraph; a reader discovering it from behaviour
costs more.

- **Placement is chosen per project, not per entry.** One project's dispatches
  are placed by one answer, so a project cannot send fixes to minted checkouts
  and sweeps to its own root at the same time ([§FS-005-dispatch.25](FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs) mints the
  branch, [§FS-005-dispatch.6.1](FS-005-dispatch.md#61-the-work-root-is-a-template-and-it-may-reach-above-the-project) renders the root, and neither varies by entry).
  Until placement is per entry, [§2](#2-reach-places-and-nothing-else-does) is a rule a person applies by configuring one
  scope at a time.
- **A mutating verb above a checkout does not yet report by default.** [§3](#3-upper-scopes-decide-the-checkout-scope-executes) says
  upper scopes decide and hand down; nothing refuses a verb that would act at
  organization or project scope instead, so today the division holds only
  because whoever configured the work kept to it.
- **A handed-down ticket does not yet name where it came from.** [§4](#4-a-handed-down-ticket-names-where-it-came-from-and-the-trail-runs-both-ways) is owed in
  both directions: neither the origin on the ticket nor the spawned ids on the
  result is written by anything.

A fourth is owed, and not from here: the fold-up [§5](#5-nothing-durable-lives-in-a-checkout-work-root) asks for is the
archiver's, so what is missing for it is outside this program rather than in it.

The roadmap carries what each of these is waiting on, here and elsewhere. What
this declaration carries is the rule they are measured against, so that work
naming this doctrine has something in the tree to name.
