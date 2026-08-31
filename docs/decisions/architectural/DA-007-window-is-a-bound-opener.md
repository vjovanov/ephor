# DA-007-window-is-a-bound-opener: a window of the reader's own is a bound opener, with the terminal as the floor

**Status:** Accepted
**Date:** 2026-08-22

A run of the runtime starts detached and is watched by attaching
([§FS-005-dispatch.20](../../../requirements.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)); a program an entry *is* may run beside ephor rather
than in its place ([§FS-005-dispatch.22](../../../requirements.md#22-a-window-of-the-readers-own-where-one-is-bound)). Both need a terminal that is not
the one ephor is drawing in, and where that terminal comes from is a choice
ephor cannot make for the reader. This record fixes it as a seam
([§AR-002-summons.6](../../architecture/AR-002-summons.md#6-windowed-the-readers-own-window)) and names what the alternatives would have cost.

## 1. The decision

A window is opened and brought forward by a **bound opener**: two commands
— `open`, printing a handle; `focus`, taking one — selected by `window` in
site configuration, or recognized from the environment ephor was started in
when nothing is configured, and never discovered by spawning. Three bindings
ship: a terminal multiplexer's new window, and the remote-control spawn of two
terminals that offer one; the person this was designed with works in one of
the terminals, and all three are supported because the next person will not.
Each binding's focus verb is the product's own, since "bring this window
forward" is a different command in every one of them and the handle `open`
printed is whatever that product calls a window.

The fallback is the terminal ephor is in, handed over as every interactive
summons always was ([§AR-002-summons.2](../../architecture/AR-002-summons.md#2-the-invocation)). It is the floor because it needs
nothing: no multiplexer, no remote control, no configuration, and no
platform. An environment ephor does not recognize gets the floor and a line
saying so, not a guess.

A windowed program is recorded as a job ([§AR-002-summons.5](../../architecture/AR-002-summons.md#5-detached-the-job)) because the
thing the menu needs — is it still going, and how do I get to it
([§FS-005-dispatch.21](../../../requirements.md#21-what-is-already-going-is-shown-where-it-could-be-started-again)) — is exactly what a job already answers, and the only
change is that the inspection is a handle rather than a log. Attaching to a
run is windowed the same way but is *not* a job: the run is the operation,
and a surface on it is not a second one.

## 2. The rejected alternatives

**Always hand the terminal over.** What exists today. It works everywhere,
and it is the floor. Alone, it makes a running coding agent invisible — ephor
is not on screen while the agent has the terminal — and it makes watching a
detached run cost the whole interface again, which undoes the point of
detaching. It stays as the degrade rule, not as the design.

**One multiplexer, assumed.** The shape every hand-rolled script takes:
`tmux new-window` and be done. It is one person's environment compiled in
([§REQ-001-boundary.2](../../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)), it is wrong for a reader in a terminal with its own
windows, and "is tmux there" answered by spawning it is discovery by failure
([§AR-002-summons.4](../../architecture/AR-002-summons.md#4-refusal-is-computed-not-discovered)).

**A second screen inside ephor.** Splitting ephor's own terminal and running
the program — the runtime's attach surface, the agent — in one half. It
keeps everything in one process and needs no binding, and that is its
appeal. It fails on what it would have to be: a terminal emulator inside a
terminal, re-drawing somebody else's full-screen program through ephor's
own frame, with resize, key forwarding, and the program's own colours all
ephor's to get right. The runtime already has a surface; the agent already
has one; a window is where such things run.

## 3. The cost

The seam is only as good as the binding's handle: a multiplexer that
renumbers windows, or a terminal restarted between `open` and `focus`, makes
`focus` miss, and what the reader gets is the opener's own error, reported
as the command's output rather than as ephor's. A windowed program leaves no
log in the job directory, so a window closed before it was read is gone —
accepted, because duplicating a screen the reader was watching into a file
is a recording, and the reader asked for a window precisely to watch. And
the environment recognition reads a handful of variables the products set
for this purpose; a reader who unsets them, or starts ephor from a shell
that never had them, gets the floor and the line that says why.
