# Why this project exists

## GRUND-001-overseer: one watch over every project, and none of the governing

A person who works on several projects at once keeps the watch in their head.
ephor exists to keep it for them — to be the ephor of Sparta's constitution:
a board that observed everything, governed nothing, and could summon anyone.
Observation without governance is not a limitation of the tool; it is the
reason for it, and every boundary in the design is that stance made law.

### 1. The problem

Work scatters. Every project has its own forge, its own gate, its own
tickets, and its own places where people talk — pull request threads, issues,
mail, chat. Each of those tools shows its own slice to everyone equally;
none of them watches one person's whole estate. So the watch is kept by
hand: the morning sweep across forges and inboxes, the noticing of what
moved, the remembering of which branch trails its main, which gate is red,
who has been waiting on an answer since Tuesday. Nearly every row on that
mental list has an obvious next move, and both halves of the ritual — the
reading and the routine move — spend the one resource that cannot be bought
back, attention.

When the watch is automated, the automation rots along a familiar seam:
knowledge of the projects leaks into the tool (an employer's repositories,
one gate's commands), the tool leaks into the projects (files only it
understands), and a vendor or runtime gets fused in rather than chosen. The
result is one person's script — unpublishable, unshareable, dead when its
site dies.

### 2. What this project does about it

ephor keeps one watch over every registered project: every matter, wherever
it is discussed, in one place, with what changed and why it resurfaced. And
where watching is not enough, it summons rather than governs — the project's
own check, the gate's own restart, an agent runtime handed a complete
dossier — and then keeps the ledger of what came of it. Everything ephor
cannot do itself lives across a boundary that is law rather than habit:
nothing project-specific upstream, nothing ephor-specific demanded of a
project, and every default — the forge, the runtime — shipped and
replaceable rather than fused. That is what lets the same tool watch an
employer's poly-repo estate and a weekend project without either one
noticing the other exists.

### 3. Who it is for

First, the person keeping too many watches: a developer or maintainer across
several projects — poly-repo estates like GraalVM among them — who works
with agents and wants the routine moves handed over without handing over
judgment. Second, their projects, which stay clean: fully trackable with
nothing added, and able to offer more — their checks, their gate, their
actions — only when they choose to speak.
