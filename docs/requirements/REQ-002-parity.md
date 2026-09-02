# REQ-002-parity: every ability is reachable without the screen, and every answer has a machine form

The interactive interface is a convenience over the watch, never the place
the watch lives. Every ability it offers is therefore also a command, and
every reading a command gives back is also available as JSON.

This is what keeps ephor usable by the thing it exists to hand work to
([§GOAL-004-handover](../goals.md#goal-004-handover-routine-moves-leave-the-persons-hands)). A runtime that can read a feed but cannot post the
reply its own run drafted holds half a tool, and the missing half is the one
that finishes the move. It is also what keeps the watch trustworthy
([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)): a fact that can only be seen by a person sitting
at a terminal is a fact nothing can check, alert on, or carry into the next
machine.

The law binds every screen, every key, and every surface added later. Work
that adds a key cites it.

## 1. An ability is a key that reveals a fact or changes the world

Parity is claimed over **abilities**, not over keystrokes. A key that
reveals something the reader could not otherwise know, or that changes
anything outside the process, is an ability and owes a command. Everything
else the screen does is presentation and owes nothing:

- moving a cursor, scrolling, paging, switching modes, folding a section
- opening a pager, an editor, or a browser on something the command line
  already names — the path or the URL is the ability; spawning the reader's
  own program on it is not
- drawing: colour, width, truncation, the footer that teaches keys

The test is what a second person could do with the answer. If pressing the
key is the only way to learn a fact or to make a change, the fact and the
change belong to the command line too.

## 2. Parity runs both ways

Neither surface leads. A move added to the interface lands with its command
in the same change, and a command that reveals or changes something the
reader would look for on a screen is offered there. Two surfaces that drift
become two products with one name, and the second one is always the one
nobody maintains.

Where a surface genuinely cannot carry an ability, the reason is stated at
the place it is missing — not left as an absence, which reads exactly like
an oversight ([§REQ-001-boundary.1](REQ-001-boundary.md#1-the-anatomy)). A prompt that must be typed into is an
argument on the command line; a list that must be chosen from is an argument
too, and the list itself is a reading.

## 3. Every reading answers a program

Any command that prints a reading takes `--json` and prints that same
reading as JSON on standard output, alone — no progress, no notes, no
warnings, which go to the error stream where they narrate the run without
becoming part of it ([§FS-010-doctor.3](../functional-spec/FS-010-doctor.md#3-two-passes-the-site-and-ephor-itself)). A command that changes something
prints, under `--json`, what it changed: the same outcome its prose
describes, in the shape a program can act on.

The two forms are one answer rendered twice. The prose form may summarise;
it may never *know* something the JSON form does not, because then a script
would be the degraded surface, and the whole point is that it is not
([§GRUND-001-overseer.2](../grund.md#2-what-this-project-does-about-it)).

## 4. The machine form is a contract, not a dump

What `--json` prints is a declared shape with a published schema, not
whatever a struct happened to serialize to ([§GOAL-005-costless](../goals.md#goal-005-costless-watching-costs-the-watched-nothing)). Fields are
added freely; a field is renamed or removed only as a release notes it. A
surface that printed its internals would make every refactor a breaking
change for whoever automated against it, which is the opposite of being
scriptable.

The schema is held to what the commands print, not merely to what they are
called. A check that asks whether a shape *has* an entry passes a shape whose
entry describes something else entirely — an array published as an object, a
field declared a string that is printed as null — and every one of those is a
program that crashes on the first answer it reads. So the build runs the
commands and validates their output against their own entry, in both
directions: a shape nothing validates fails as loudly as a shape nothing
declares. A contract nobody checks is a comment.

## 5. The parity list is checked, not remembered

Which abilities exist and which command carries each one is written down
where the build can read it, and the check fails when a screen offers a key
no command answers, when the command carrying an ability has no `--json`, or
when a `--json` anywhere in the tree prints a shape nothing declared. A
convention nobody can run is a convention that has already drifted — the same
reasoning that makes [§REQ-001-boundary.5](REQ-001-boundary.md#5-no-product-literal-outside-its-adapter) a build failure rather than a review
comment.

A check is owed the same honesty as a surface. What it cannot read it reports
rather than passes: a binding whose key is decided by a constant, a screen
region it does not know how to scan. A check that skips what it does not
understand is worse than no check, because it reports green over exactly the
cases nobody looked at.
